//! Synchronisation operations (ADR-0031): fetch, fast-forward-only pull, reviewed commit, reviewed push, clone and
//! restore. Every operation is recorded in the history, one at a time per workspace; commits and pushes need the
//! token of the review the person saw (the state must not have changed since), and are refused when the inspection
//! blocks. Failures leave the repository as it was (staging and fast-forward are rolled back).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use super::history;
use super::inspect::{self, Context, Direction, Finding, Inspection, Severity};
use crate::git::{self, DiffEntry, LogEntry};
use crate::workspace::{self, Workspace};
use crate::{CoreError, Result};

/// The tree of an empty repository (to diff a first push against).
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Everything an operation needs.
pub struct Sync<'a> {
    pub conn: &'a Connection,
    pub ws: &'a Workspace,
    pub data_dir: &'a Path,
    pub home: &'a Path,
    pub host_id: &'a str,
}

impl Sync<'_> {
    fn root(&self) -> &Path {
        &self.ws.root_path
    }
    fn ctx(&self) -> Context<'_> {
        Context { home: self.home }
    }
    fn require_repo(&self) -> Result<()> {
        if !super::own_repository(self.root()) {
            return Err(CoreError::invalid("the workspace folder is not a Git repository of its own — `git init` it (or open a workspace from Git)"));
        }
        Ok(())
    }
    /// The remote to talk to: the upstream's, else `origin`, else the only remote.
    fn remote(&self) -> Option<String> {
        if let Some((r, _)) = git::upstream_of_head(self.root()) {
            return Some(r);
        }
        let all = git::remotes(self.root());
        if all.iter().any(|r| r == "origin") { Some("origin".into()) } else if all.len() == 1 { all.into_iter().next() } else { None }
    }
}

fn refused(conn: &Connection, id: &str, kind: &str, message: String) -> CoreError {
    let _ = history::finish(conn, id, "refused", None, 0, 0, Some(kind), Some(&message));
    CoreError::Conflict(message)
}

fn failed(conn: &Connection, id: &str, kind: &str, message: String) -> CoreError {
    let _ = history::finish(conn, id, "failed", None, 0, 0, Some(kind), Some(&message));
    CoreError::External { program: "git".into(), message }
}

fn first_blocker(i: &Inspection) -> String {
    let blockers: Vec<&Finding> = i.findings.iter().filter(|f| f.severity == Severity::Block).collect();
    let first = blockers.first().map(|f| format!("{}{}: {}", f.path, f.line.map(|l| format!(":{l}")).unwrap_or_default(), f.message)).unwrap_or_default();
    format!("{} blocking finding(s) — {first}", blockers.len())
}

// ---- fetch ----------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Fetched {
    pub remote: String,
    pub incoming: u32,
    pub outgoing: u32,
}

pub fn fetch(s: &Sync<'_>) -> Result<Fetched> {
    s.require_repo()?;
    let st = git::status(s.root())?;
    let id = history::begin(s.conn, &s.ws.id, "fetch", s.host_id, st.branch.as_deref(), st.head_oid.as_deref())?;
    let Some(remote) = s.remote() else {
        return Err(refused(s.conn, &id, "no-remote", "no remote is configured — add one with your Git tool (`git remote add origin <url>`)".into()));
    };
    if let Err(e) = git::fetch_remote(s.root(), &remote) {
        let kind = serde_json::to_value(e.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "other".into());
        return Err(failed(s.conn, &id, &kind, e.explain()));
    }
    let after = git::status(s.root())?;
    history::finish(s.conn, &id, "ok", after.head_oid.as_deref(), after.behind as usize, 0, None, Some(&format!("{} incoming, {} outgoing commit(s)", after.behind, after.ahead)))?;
    Ok(Fetched { remote, incoming: after.behind, outgoing: after.ahead })
}

// ---- pull (fast-forward only) ------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NeedsApproval {
    pub id: String,
    pub title: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pulled {
    pub commits: u32,
    pub files: Vec<String>,
    pub head_before: Option<String>,
    pub head_after: Option<String>,
    /// Synced rules, constitutions and workflows whose new version waits for approval here.
    pub needs_approval: Vec<NeedsApproval>,
    /// AI Kits in the workspace whose files changed — they need a new review before sessions get them.
    pub kits_changed: Vec<String>,
    /// Incoming findings that did not block (e.g. a teammate committed a file that never syncs).
    pub warnings: Vec<Finding>,
}

pub fn pull(s: &Sync<'_>) -> Result<Pulled> {
    s.require_repo()?;
    let root = s.root();
    let st = git::status(root)?;
    let id = history::begin(s.conn, &s.ws.id, "pull", s.host_id, st.branch.as_deref(), st.head_oid.as_deref())?;
    if st.conflicted > 0 {
        return Err(refused(s.conn, &id, "conflicts", format!("{} file(s) have unresolved conflicts — resolve them in your Git tool first", st.conflicted)));
    }
    if let Some(op) = super::in_progress(root) {
        return Err(refused(s.conn, &id, "in-progress", format!("{op} — finish it in your Git tool first")));
    }
    if st.detached || st.branch.is_none() {
        return Err(refused(s.conn, &id, "detached", "HEAD is detached — check out a branch first".into()));
    }
    let Some((_, up)) = git::upstream_of_head(root) else {
        return Err(refused(s.conn, &id, "no-upstream", "the branch has no upstream yet — push it once (Sync Center → Push) or set one in your Git tool".into()));
    };
    if st.behind == 0 {
        history::finish(s.conn, &id, "ok", st.head_oid.as_deref(), 0, 0, None, Some("already up to date (as of the last fetch)"))?;
        return Ok(Pulled { commits: 0, files: vec![], head_before: st.head_oid.clone(), head_after: st.head_oid, needs_approval: vec![], kits_changed: vec![], warnings: vec![] });
    }
    if st.ahead > 0 {
        return Err(refused(
            s.conn,
            &id,
            "diverged",
            format!("the branch has diverged ({} local, {} incoming commit(s)) — NEXUS only fast-forwards; integrate (rebase or merge) in your Git tool", st.ahead, st.behind),
        ));
    }
    let incoming = inspect::inspect_incoming(root, "HEAD", &up)?;
    if incoming.blocked {
        return Err(refused(s.conn, &id, "unsafe-incoming", format!("the incoming changes cannot be checked out safely: {}", first_blocker(&incoming))));
    }
    let entries = git::diff_raw(root, "HEAD", Some(&up))?;
    let paths: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
    let local: BTreeSet<String> = git::changed_paths(root)?.into_iter().map(|(_, p)| p).collect();
    let overlap: Vec<&String> = paths.iter().filter(|p| local.contains(*p)).collect();
    if !overlap.is_empty() {
        let list = overlap.iter().take(5).map(|p| p.as_str()).collect::<Vec<_>>().join(", ");
        return Err(refused(s.conn, &id, "local-changes", format!("you have local changes to {} incoming file(s) ({list}) — commit or set them aside first", overlap.len())));
    }
    let head_before = st.head_oid.clone();
    let out = git::merge_ff_only(root, &up)?;
    if !out.success() {
        // Leave the repository as it was: incoming paths were clean, so restoring them is safe.
        let mut note = String::new();
        if git::head(root) == head_before {
            let existing: Vec<String> = entries.iter().filter(|e| e.status != "A").map(|e| e.path.clone()).collect();
            if git::restore_to_head(root, &existing).is_err() {
                note.push_str(" — some files could not be restored; check the Sync Center");
            }
            for e in entries.iter().filter(|e| e.status == "A") {
                let p = root.join(&e.path);
                if p.is_file() && !local.contains(&e.path) {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
        let msg = out.stderr_str().lines().next().unwrap_or("the fast-forward failed").trim().to_string();
        return Err(failed(s.conn, &id, "merge", format!("{msg}{note}")));
    }
    let head_after = git::head(root);
    let mut warnings: Vec<Finding> = incoming.findings.into_iter().filter(|f| f.severity == Severity::Warn).collect();
    if let Err(e) = crate::knowledge::service::refresh(s.conn, s.data_dir, s.ws) {
        warnings.push(Finding { path: String::new(), line: None, kind: "index", severity: Severity::Warn, message: format!("knowledge index not refreshed: {e}") });
    }
    if let Err(e) = workspace::reconcile(s.conn, &s.ws.id) {
        warnings.push(Finding { path: String::new(), line: None, kind: "manifest", severity: Severity::Warn, message: format!("workspace manifest not applied: {e}") });
    }
    let needs_approval = unapproved_in(s.conn, s.ws, &paths)?;
    let kits_changed = kits_changed_in(s.conn, s.data_dir, root, &paths)?;
    if !needs_approval.is_empty() || !kits_changed.is_empty() {
        let mut body = Vec::new();
        if !needs_approval.is_empty() {
            body.push(format!("{} rule(s), constitution(s) or workflow(s)", needs_approval.len()));
        }
        if !kits_changed.is_empty() {
            body.push(format!("{} AI Kit(s)", kits_changed.len()));
        }
        let mut e = crate::notify::NewEvent::new(crate::notify::Category::ReviewRequired, "sync", "Synced changes wait for your review".to_string())
            .body(format!("{} changed in the pull — they stay inactive until you approve them.", body.join(" and ")))
            .target("sync", s.ws.id.clone())
            .dedupe(format!("sync-review:{}", s.ws.id));
        e.workspace_id = Some(s.ws.id.clone());
        crate::notify::publish(s.conn, e)?;
    }
    history::finish(s.conn, &id, "ok", head_after.as_deref(), st.behind as usize, paths.len(), None, Some(&format!("fast-forwarded {} commit(s), {} file(s)", st.behind, paths.len())))?;
    Ok(Pulled { commits: st.behind, files: paths, head_before, head_after, needs_approval, kits_changed, warnings })
}

/// Rules (incl. constitutions) and workflows among `paths` whose current version is not approved on this machine.
fn unapproved_in(conn: &Connection, ws: &Workspace, paths: &[String]) -> Result<Vec<NeedsApproval>> {
    let mut out = Vec::new();
    for p in paths {
        let full = ws.root_path.join(p.replace('/', std::path::MAIN_SEPARATOR_STR));
        let row: Option<(Option<String>, String, String, String)> = rusqlite::OptionalExtension::optional(conn.query_row(
            "SELECT k.id, k.title, k.kind, k.content_hash FROM knowledge_items k WHERE k.workspace_id = ?1 AND k.path = ?2 AND k.valid = 1 AND k.kind IN ('rule', 'workflow')",
            rusqlite::params![ws.id, full.to_string_lossy()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        ))?;
        let Some((id, title, kind, hash)) = row else { continue };
        let approved: Option<String> = rusqlite::OptionalExtension::optional(conn.query_row("SELECT content_hash FROM knowledge_approvals WHERE path = ?1", [full.to_string_lossy()], |r| r.get(0)))?;
        if approved.as_deref() != Some(hash.as_str()) {
            out.push(NeedsApproval { id: id.unwrap_or_default(), title, kind });
        }
    }
    Ok(out)
}

/// Locally registered AI Kits whose folder is inside the workspace and was touched by `paths` and that are no
/// longer approved (their content hash changed — the existing kit review applies).
fn kits_changed_in(conn: &Connection, data_dir: &Path, root: &Path, paths: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for v in crate::kits::list(conn, data_dir)? {
        if v.kit.source_kind != "local" || v.status == "approved" {
            continue;
        }
        let dir = crate::kits::content_dir(data_dir, &v.kit);
        let Ok(rel) = dunce::canonicalize(&dir).unwrap_or(dir.clone()).strip_prefix(dunce::canonicalize(root).unwrap_or(root.to_path_buf())).map(|r| r.to_string_lossy().replace('\\', "/")) else {
            continue;
        };
        if paths.iter().any(|p| p.starts_with(&format!("{rel}/")) || *p == rel) {
            out.push(v.kit.name.clone());
        }
    }
    Ok(out)
}

// ---- commit (reviewed) ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFile {
    /// A new, M modified, D deleted, U conflicted.
    pub status: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitReview {
    pub branch: Option<String>,
    pub files: Vec<ReviewFile>,
    pub inspection: Inspection,
    /// Git has a name and e-mail for commits (NEXUS never invents one).
    pub identity: bool,
    /// Pass to [`commit`]: proves the commit is of exactly this reviewed state.
    pub token: String,
}

pub(super) fn status_of(xy: &str) -> &'static str {
    match xy {
        "??" => "A",
        _ if xy.contains('U') || xy == "AA" || xy == "DD" => "U",
        _ if xy.contains('D') => "D",
        _ if xy.starts_with('A') => "A",
        _ => "M",
    }
}

pub fn review_commit(s: &Sync<'_>, paths: Option<&[String]>) -> Result<CommitReview> {
    s.require_repo()?;
    let root = s.root();
    let changed: Vec<(String, String)> = git::changed_paths(root)?.into_iter().map(|(xy, p)| (status_of(&xy).to_string(), p)).collect();
    let selected: Vec<(String, String)> = match paths {
        None => changed,
        Some(want) => {
            let mut out = Vec::new();
            for w in want {
                let w = w.replace('\\', "/");
                let Some(c) = changed.iter().find(|(_, p)| *p == w) else {
                    return Err(CoreError::invalid(format!("{w} has no changes to commit")));
                };
                out.push(c.clone());
            }
            out
        }
    };
    let mut inspection = inspect::inspect_worktree(root, &s.ctx(), &selected)?;
    for (st, p) in &selected {
        if st == "U" {
            inspection.findings.push(Finding { path: p.clone(), line: None, kind: "conflict", severity: Severity::Block, message: "unresolved conflict — resolve it in your Git tool".into() });
            inspection.blocked = true;
        }
    }
    let head = git::head(root).unwrap_or_default();
    let mut fingerprint = head.clone();
    for (st, p) in &selected {
        let content = std::fs::read(root.join(p)).map(|b| crate::knowledge::content_hash(&b)).unwrap_or_else(|_| "-".into());
        fingerprint.push_str(&format!("\0{st}\0{p}\0{content}"));
    }
    Ok(CommitReview {
        branch: git::status(root)?.branch,
        files: selected.into_iter().map(|(status, path)| ReviewFile { status, path }).collect(),
        inspection,
        identity: git::identity_configured(root),
        token: crate::knowledge::content_hash(fingerprint.as_bytes()),
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Committed {
    pub oid: String,
    pub files: usize,
}

pub fn commit(s: &Sync<'_>, message: &str, paths: Option<&[String]>, token: &str) -> Result<Committed> {
    s.require_repo()?;
    let message = message.trim();
    if message.is_empty() || message.len() > 4000 || message.contains('\0') {
        return Err(CoreError::invalid("write a commit message (up to 4000 characters)"));
    }
    let st = git::status(s.root())?;
    let id = history::begin(s.conn, &s.ws.id, "commit", s.host_id, st.branch.as_deref(), st.head_oid.as_deref())?;
    let review = match review_commit(s, paths) {
        Ok(r) => r,
        Err(e) => return Err(refused(s.conn, &id, "review", e.to_string())),
    };
    if review.token != token {
        return Err(refused(s.conn, &id, "changed", "the changes are not the ones you reviewed any more — review them again".into()));
    }
    if review.files.is_empty() {
        return Err(refused(s.conn, &id, "nothing", "there is nothing to commit".into()));
    }
    if review.inspection.blocked {
        return Err(refused(s.conn, &id, "blocked", format!("the security inspection blocks this commit: {}", first_blocker(&review.inspection))));
    }
    if !review.identity {
        return Err(refused(s.conn, &id, "identity", "Git has no name and e-mail for commits — set them with `git config user.name` / `user.email` (NEXUS does not invent an identity)".into()));
    }
    let all: Vec<String> = review.files.iter().map(|f| f.path.clone()).collect();
    let new_files: Vec<String> = review.files.iter().filter(|f| f.status == "A").map(|f| f.path.clone()).collect();
    match git::commit_only(s.root(), message, &all, &new_files) {
        Ok(oid) => {
            history::finish(s.conn, &id, "ok", Some(&oid), 1, all.len(), None, Some(&format!("committed {} file(s)", all.len())))?;
            Ok(Committed { oid, files: all.len() })
        }
        Err((msg, added)) => {
            let rolled_back = git::unstage(s.root(), &added).is_ok();
            Err(failed(
                s.conn,
                &id,
                "commit",
                format!("{msg} — nothing was committed{}", if rolled_back { "; NEXUS undid its staging" } else { "; check the staged files" }),
            ))
        }
    }
}

// ---- push (reviewed) ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushReview {
    pub remote: String,
    pub branch: String,
    /// The branch does not exist on the remote yet: the whole history travels.
    pub first_push: bool,
    pub commits: Vec<LogEntry>,
    pub files: Vec<DiffEntry>,
    pub behind: u32,
    pub inspection: Inspection,
    pub token: String,
}

pub fn review_push(s: &Sync<'_>) -> Result<PushReview> {
    s.require_repo()?;
    let root = s.root();
    let st = git::status(root)?;
    let branch = st.branch.clone().filter(|_| !st.detached).ok_or_else(|| CoreError::invalid("HEAD is detached — check out a branch first"))?;
    let remote = s.remote().ok_or_else(|| CoreError::invalid("no remote is configured — add one with your Git tool (`git remote add origin <url>`)"))?;
    let tracking = git::upstream_of_head(root).map(|(_, up)| up).or_else(|| {
        let candidate = format!("{remote}/{branch}");
        git::rev_parse(root, &candidate).map(|_| candidate)
    });
    let (range, base, first_push) = match &tracking {
        Some(up) => (format!("{up}..HEAD"), up.clone(), false),
        None => ("HEAD".to_string(), EMPTY_TREE.to_string(), true),
    };
    let commits = git::log_range(root, &range, 200)?;
    let files = git::diff_raw(root, &base, Some("HEAD"))?;
    let inspection = if commits.is_empty() { Inspection::default() } else { inspect::inspect_history(root, &s.ctx(), &range)? };
    let head = git::head(root).unwrap_or_default();
    let tracked_oid = tracking.as_deref().and_then(|t| git::rev_parse(root, t)).unwrap_or_else(|| "none".into());
    let token = crate::knowledge::content_hash(format!("{head}\0{tracked_oid}\0{remote}\0{branch}").as_bytes());
    Ok(PushReview { remote, branch, first_push, commits, files, behind: if first_push { 0 } else { st.behind }, inspection, token })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pushed {
    pub remote: String,
    pub branch: String,
    pub commits: usize,
}

pub fn push(s: &Sync<'_>, token: &str) -> Result<Pushed> {
    s.require_repo()?;
    let st = git::status(s.root())?;
    let id = history::begin(s.conn, &s.ws.id, "push", s.host_id, st.branch.as_deref(), st.head_oid.as_deref())?;
    let review = match review_push(s) {
        Ok(r) => r,
        Err(e) => return Err(refused(s.conn, &id, "review", e.to_string())),
    };
    if review.token != token {
        return Err(refused(s.conn, &id, "changed", "the outgoing commits are not the ones you reviewed any more — review them again".into()));
    }
    if review.commits.is_empty() {
        history::finish(s.conn, &id, "ok", st.head_oid.as_deref(), 0, 0, None, Some("nothing to push"))?;
        return Ok(Pushed { remote: review.remote, branch: review.branch, commits: 0 });
    }
    if review.behind > 0 {
        return Err(refused(s.conn, &id, "diverged", format!("the remote has {} commit(s) you do not have — fetch and pull first (NEXUS never force-pushes)", review.behind)));
    }
    if review.inspection.blocked {
        return Err(refused(s.conn, &id, "blocked", format!("the security inspection blocks this push: {}", first_blocker(&review.inspection))));
    }
    let set_upstream = git::upstream_of_head(s.root()).is_none();
    if let Err(e) = git::push_branch(s.root(), &review.remote, &review.branch, set_upstream) {
        let kind = serde_json::to_value(e.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "other".into());
        return Err(failed(s.conn, &id, &kind, e.explain()));
    }
    history::finish(s.conn, &id, "ok", st.head_oid.as_deref(), review.commits.len(), review.files.len(), None, Some(&format!("pushed {} commit(s) to {}/{}", review.commits.len(), review.remote, review.branch)))?;
    Ok(Pushed { remote: review.remote, branch: review.branch, commits: review.commits.len() })
}

// ---- onboarding: clone and restore ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreProject {
    pub id: String,
    pub name: String,
    pub remote_url: Option<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Restore {
    pub workspace_id: String,
    pub name: String,
    pub projects: Vec<RestoreProject>,
    pub pending_approvals: Vec<NeedsApproval>,
    /// Folders in the workspace that contain an AI Kit (`.claude-plugin/plugin.json`), relative; `registered` = known.
    pub kit_folders: Vec<(String, bool)>,
    /// Gate names the workspace's workflows use — gate commands are machine-local and defined per project here.
    pub gates_needed: Vec<String>,
    /// What still needs doing on this machine, in order.
    pub steps: Vec<String>,
}

/// What restoring this workspace on this machine still needs (portable configuration vs machine-local state).
pub fn restore(conn: &Connection, data_dir: &Path, ws: &Workspace) -> Result<Restore> {
    let _ = crate::knowledge::service::refresh(conn, data_dir, ws);
    let projects: Vec<RestoreProject> = crate::project::list(conn, &ws.id)?
        .into_iter()
        .map(|p| RestoreProject { id: p.id, name: p.name, remote_url: p.remote_url, available: p.available })
        .collect();
    let pending_approvals: Vec<NeedsApproval> = conn
        .prepare(
            "SELECT k.id, k.title, k.kind FROM knowledge_items k LEFT JOIN knowledge_approvals a ON a.path = k.path
             WHERE k.workspace_id = ?1 AND k.valid = 1 AND k.kind IN ('rule', 'workflow') AND (a.content_hash IS NULL OR a.content_hash <> k.content_hash)
             ORDER BY k.kind, k.title",
        )?
        .query_map([&ws.id], |r| Ok(NeedsApproval { id: r.get::<_, Option<String>>(0)?.unwrap_or_default(), title: r.get(1)?, kind: r.get(2)? }))?
        .collect::<std::result::Result<_, _>>()?;
    let registered: Vec<PathBuf> = crate::kits::list(conn, data_dir)?
        .into_iter()
        .filter(|k| k.kit.source_kind == "local")
        .map(|k| dunce::canonicalize(&k.content_dir).unwrap_or(k.content_dir))
        .collect();
    let mut kit_folders = Vec::new();
    find_kits(&ws.root_path, &ws.root_path, 0, &mut kit_folders);
    let kit_folders: Vec<(String, bool)> = kit_folders
        .into_iter()
        .map(|rel| {
            let abs = dunce::canonicalize(ws.root_path.join(&rel)).unwrap_or_else(|_| ws.root_path.join(&rel));
            let known = registered.contains(&abs);
            (rel, known)
        })
        .collect();
    let mut gates: BTreeSet<String> = BTreeSet::new();
    for w in crate::workflow::list(conn, data_dir, ws)? {
        if let Ok(v) = crate::workflow::get(conn, data_dir, ws, &w.id) {
            for step in v.definition.steps {
                gates.extend(step.gates.into_iter().filter(|g| g != "constitution"));
            }
        }
    }
    let mut steps = Vec::new();
    let missing: Vec<&RestoreProject> = projects.iter().filter(|p| !p.available).collect();
    if !missing.is_empty() {
        steps.push(format!("Locate or clone {} project folder(s) on this machine (Projects → Locate…)", missing.len()));
    }
    if !pending_approvals.is_empty() {
        steps.push(format!("Review and approve {} synced rule(s), constitution(s) or workflow(s) — they are inactive until then", pending_approvals.len()));
    }
    let unregistered = kit_folders.iter().filter(|(_, k)| !k).count();
    if unregistered > 0 {
        steps.push(format!("Register and review {unregistered} AI Kit folder(s) from the workspace (AI Kits → Register)"));
    }
    if !gates.is_empty() {
        steps.push(format!("Define and approve the quality gates the workflows use ({}) for each project (Workflows → Gates)", gates.iter().cloned().collect::<Vec<_>>().join(", ")));
    }
    steps.push("Add your Claude profiles on this machine (profiles, sessions, usage and notifications are machine-local and never synced)".into());
    Ok(Restore { workspace_id: ws.id.clone(), name: ws.name.clone(), projects, pending_approvals, kit_folders, gates_needed: gates.into_iter().collect(), steps })
}

fn find_kits(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
    if depth > 3 || out.len() >= 50 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(t) = e.file_type() else { continue };
        if !t.is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if depth == 0 && matches!(name.as_str(), "repos" | ".git" | ".nexus-local" | "node_modules") || name == ".git" {
            continue;
        }
        let p = e.path();
        if p.join(".claude-plugin").join("plugin.json").is_file() {
            if let Ok(rel) = p.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
            continue;
        }
        find_kits(root, &p, depth + 1, out);
    }
}

/// Onboard a workspace from an existing Git repository: clone it into `dest` (new or empty folder), check it can be
/// checked out safely and is a NEXUS workspace, register it, and report what restoring it still needs. On any
/// failure the folder is left as it was before (removed if NEXUS created it).
pub fn clone_workspace(conn: &Connection, data_dir: &Path, host_id: &str, url: &str, dest_raw: &str) -> Result<(Workspace, Restore)> {
    let url = url.trim();
    if !git::clone_url_allowed(url) {
        return Err(CoreError::invalid("only https://, ssh://, git@host:path and file:// Git URLs are supported"));
    }
    let dest = crate::paths::validate_user_path(dest_raw, crate::paths::Expect::DirOrCreatable)?;
    let existed = dest.exists();
    if existed && std::fs::read_dir(&dest).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(CoreError::invalid("choose a new or empty folder for the workspace"));
    }
    let id = history::begin(conn, "", "clone", host_id, None, None)?;
    let cleanup = |dest: &Path| {
        if existed {
            if let Ok(rd) = std::fs::read_dir(dest) {
                for e in rd.flatten() {
                    let p = e.path();
                    let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
                }
            }
        } else {
            let _ = std::fs::remove_dir_all(dest);
        }
    };
    if let Err(e) = git::clone_full(url, &dest) {
        cleanup(&dest);
        let kind = serde_json::to_value(e.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "other".into());
        return Err(failed(conn, &id, &kind, e.explain()));
    }
    let tree = git::ls_tree(&dest, "HEAD").unwrap_or_default();
    let unsafe_files: Vec<Finding> = tree.iter().flat_map(|(mode, p)| inspect::path_findings(p, mode, Direction::Incoming)).filter(|f| f.severity == Severity::Block).collect();
    if let Some(f) = unsafe_files.first() {
        cleanup(&dest);
        return Err(refused(conn, &id, "unsafe-incoming", format!("the repository cannot be checked out safely ({} finding(s)) — {}: {}", unsafe_files.len(), f.path, f.message)));
    }
    if !dest.join(workspace::MANIFEST_FILE).is_file() {
        cleanup(&dest);
        return Err(refused(conn, &id, "not-a-workspace", format!("the repository has no {} — it is not a NEXUS workspace", workspace::MANIFEST_FILE)));
    }
    // The same workspace already open here: never move its registration silently (restoring a missing folder is fine).
    if let Ok(m) = workspace::read_manifest(&dest)
        && let Ok(existing) = workspace::get(conn, &m.id)
        && existing.available
    {
        cleanup(&dest);
        return Err(refused(
            conn,
            &id,
            "already-open",
            format!("this workspace is already open on this machine at {} — fetch and pull there instead", existing.root_path.display()),
        ));
    }
    let ws = match workspace::import(conn, &dest.to_string_lossy()) {
        Ok(ws) => ws,
        Err(e) => {
            cleanup(&dest);
            return Err(refused(conn, &id, "import", e.to_string()));
        }
    };
    let ws = workspace::activate(conn, &ws.id)?;
    history::set_workspace(conn, &id, &ws.id)?;
    let r = restore(conn, data_dir, &ws)?;
    history::finish(conn, &id, "ok", git::head(&ws.root_path).as_deref(), 0, tree.len(), None, Some(&format!("cloned {} file(s) from {}", tree.len(), git::redact_url(url))))?;
    Ok((ws, r))
}

// ---- file diffs for review -----------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    Working,
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub text: String,
    pub truncated: bool,
    pub binary: bool,
}

const DIFF_BYTES: usize = 256 << 10;

/// The diff of one file of a change the Sync Center lists (bounded; external diff tools never run).
pub fn file_diff(ws: &Workspace, side: Side, path: &str) -> Result<FileDiff> {
    let root = &ws.root_path;
    if inspect::unsafe_path_reason(path).is_some() {
        return Err(CoreError::invalid("not a workspace-relative path"));
    }
    let listed = |entries: Vec<DiffEntry>| entries.iter().any(|e| e.path == path);
    let (text, truncated) = match side {
        Side::Working => {
            let changed = git::changed_paths(root)?;
            let Some((xy, _)) = changed.iter().find(|(_, p)| p == path) else {
                return Err(CoreError::not_found("the file has no local changes"));
            };
            if xy == "??" {
                let full = root.join(path);
                let bytes = std::fs::read(&full).map_err(|e| CoreError::io("cannot read", &full, e))?;
                if bytes.iter().take(8000).any(|b| *b == 0) {
                    return Ok(FileDiff { path: path.into(), text: String::new(), truncated: false, binary: true });
                }
                let truncated = bytes.len() > DIFF_BYTES;
                let body = String::from_utf8_lossy(&bytes[..bytes.len().min(DIFF_BYTES)]).lines().map(|l| format!("+{l}")).collect::<Vec<_>>().join("\n");
                (format!("new file {path}\n{body}"), truncated)
            } else {
                git::diff_text(root, &["HEAD", "--", path], DIFF_BYTES)?
            }
        }
        Side::Incoming => {
            let (_, up) = git::upstream_of_head(root).ok_or_else(|| CoreError::invalid("the branch has no upstream"))?;
            if !listed(git::diff_raw(root, "HEAD", Some(&up))?) {
                return Err(CoreError::not_found("the file is not part of the incoming changes"));
            }
            git::diff_text(root, &["HEAD", &up, "--", path], DIFF_BYTES)?
        }
        Side::Outgoing => {
            let base = git::upstream_of_head(root).map(|(_, up)| up).unwrap_or_else(|| EMPTY_TREE.to_string());
            if !listed(git::diff_raw(root, &base, Some("HEAD"))?) {
                return Err(CoreError::not_found("the file is not part of the outgoing changes"));
            }
            git::diff_text(root, &[&base, "HEAD", "--", path], DIFF_BYTES)?
        }
    };
    let binary = text.contains("Binary files ") && text.lines().count() < 8;
    Ok(FileDiff { path: path.into(), text, truncated, binary })
}

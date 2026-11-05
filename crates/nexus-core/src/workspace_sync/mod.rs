//! Git-based workspace synchronisation (ADR-0031, M29): the person's own repository is the transport and history —
//! no sync service. This module reports readiness and status (read-only); [`ops`] fetches, fast-forwards, commits and
//! pushes — commits and pushes only after a review the person confirmed and an inspection that did not block
//! ([`inspect`]); [`history`] records every operation, one at a time per workspace.
//!
//! What travels: the portable files of the workspace folder (manifest, tasks, notes, decisions, specs, rules,
//! constitutions, workflows, kit folders the team keeps there). What never travels: everything machine-local —
//! sessions, usage, process information, notifications, gate commands, approvals, profiles, credentials (SQLite in the
//! NEXUS data folder, which must not be inside the workspace) and local-only knowledge (`.nexus-local/`).
//! Incoming documents that activate behaviour (rules, constitutions, workflows, AI Kits) need approval here.

pub mod history;
pub mod inspect;
pub mod ops;

#[cfg(test)]
mod tests;

pub use inspect::sensitive_reason;

use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::workspace::Workspace;
use crate::{Result, git};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// `error` blocks syncing; `warning` needs a decision; `info` explains.
    pub severity: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SensitiveFile {
    /// Relative to the workspace folder.
    pub path: String,
    /// Already committed (it is in the history) rather than only present and not ignored.
    pub tracked: bool,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Readiness {
    pub workspace_id: String,
    pub repo: bool,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    /// Remote URL with credentials removed.
    pub remote: Option<String>,
    /// As of the last fetch (this check does not contact the remote).
    pub ahead: u32,
    pub behind: u32,
    /// Changed or new portable files that an outgoing commit would contain.
    pub changed: Vec<String>,
    pub conflicted: u32,
    pub sensitive: Vec<SensitiveFile>,
    pub data_dir_inside: bool,
    /// Synced rules, constitutions and workflows whose current version is not approved on this machine.
    pub pending_approvals: usize,
    /// No `error` issue.
    pub ready: bool,
    pub issues: Vec<Issue>,
}

pub fn readiness(conn: &Connection, ws: &Workspace, data_dir: &Path) -> Result<Readiness> {
    let root = &ws.root_path;
    let mut r = Readiness {
        workspace_id: ws.id.clone(),
        repo: false,
        branch: None,
        upstream: None,
        remote: None,
        ahead: 0,
        behind: 0,
        changed: Vec::new(),
        conflicted: 0,
        sensitive: Vec::new(),
        data_dir_inside: false,
        pending_approvals: pending_approvals(conn, &ws.id)?,
        ready: false,
        issues: Vec::new(),
    };
    let canon = |p: &Path| dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    r.data_dir_inside = canon(data_dir).starts_with(canon(root));
    if r.data_dir_inside {
        r.issues.push(Issue { severity: "error", message: "the NEXUS data folder is inside the workspace folder — machine-local state would travel with it; move one of them".into() });
    }
    // Only a repository rooted at the workspace folder counts (a parent repository is someone else's).
    let top = git::toplevel(root).map(|t| canon(&t));
    if !own_repository(root) {
        r.issues.push(Issue {
            severity: "info",
            message: if top.is_some() {
                "the workspace folder is inside another Git repository — sync needs a repository of its own (`git init` in the workspace folder)".into()
            } else {
                "the workspace folder is not a Git repository yet — `git init`, commit, and add a remote to sync it".into()
            },
        });
        return Ok(r); // not ready: there is nothing to sync through yet
    }
    r.repo = true;
    let st = git::status(root)?;
    r.branch = st.branch.clone();
    r.upstream = st.upstream.clone();
    r.ahead = st.ahead;
    r.behind = st.behind;
    r.conflicted = st.conflicted;
    r.remote = git::remote_url(root);
    let tracked = git::tracked_files(root)?;
    let changed = git::changed_paths(root)?;
    for path in &tracked {
        if let Some(reason) = sensitive_reason(path) {
            r.sensitive.push(SensitiveFile { path: path.clone(), tracked: true, reason });
        }
    }
    for (xy, path) in &changed {
        if xy == "??"
            && let Some(reason) = sensitive_reason(path)
        {
            r.sensitive.push(SensitiveFile { path: path.clone(), tracked: false, reason });
            continue;
        }
        r.changed.push(path.clone());
    }
    r.changed.sort();
    r.changed.dedup();
    // Local-only knowledge must stay on this machine.
    let local_only = local_only_paths(conn, &ws.id, root)?;
    let exposed: Vec<&String> = local_only.iter().filter(|p| tracked.contains(p) || changed.iter().any(|(_, c)| c == *p)).collect();
    if !exposed.is_empty() {
        r.issues.push(Issue { severity: "error", message: format!("{} local-only knowledge file(s) are not ignored by Git — they would leave this machine", exposed.len()) });
    }
    if st.detached || st.branch.is_none() {
        r.issues.push(Issue { severity: "error", message: "HEAD is detached — check out a branch first".into() });
    }
    if r.conflicted > 0 {
        r.issues.push(Issue { severity: "error", message: format!("{} file(s) have unresolved merge conflicts — resolve them in Git first", r.conflicted) });
    }
    let tracked_sensitive = r.sensitive.iter().filter(|s| s.tracked).count();
    if tracked_sensitive > 0 {
        r.issues.push(Issue {
            severity: "error",
            message: format!("{tracked_sensitive} sensitive file(s) are committed — remove them from the repository (and its history) before pushing"),
        });
    }
    let loose = r.sensitive.len() - tracked_sensitive;
    if loose > 0 {
        r.issues.push(Issue { severity: "warning", message: format!("{loose} sensitive file(s) are not ignored — add them to .gitignore so they are never committed") });
    }
    if r.remote.is_none() {
        r.issues.push(Issue { severity: "warning", message: "no remote configured — add one (`git remote add origin …`) to sync between machines".into() });
    } else if r.upstream.is_none() {
        r.issues.push(Issue { severity: "warning", message: "the branch has no upstream — push it once with `git push -u` from your Git tool".into() });
    }
    if r.behind > 0 && r.ahead > 0 {
        r.issues.push(Issue { severity: "warning", message: format!("the branch has diverged ({} ahead, {} behind as of the last fetch) — NEXUS will only fast-forward; integrate in your Git tool", r.ahead, r.behind) });
    }
    if r.pending_approvals > 0 {
        r.issues.push(Issue { severity: "info", message: format!("{} synced rule(s), constitution(s) or workflow(s) wait for your approval on this machine", r.pending_approvals) });
    }
    r.ready = !r.issues.iter().any(|i| i.severity == "error");
    Ok(r)
}

/// Rules (incl. constitutions) and workflows of the workspace whose current version is not approved here.
fn pending_approvals(conn: &Connection, workspace_id: &str) -> Result<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM knowledge_items k LEFT JOIN knowledge_approvals a ON a.path = k.path
         WHERE k.workspace_id = ?1 AND k.valid = 1 AND k.kind IN ('rule', 'workflow')
           AND (a.content_hash IS NULL OR a.content_hash <> k.content_hash)",
        [workspace_id],
        |r| r.get(0),
    )?;
    Ok(n.max(0) as usize)
}

/// Local-only knowledge files inside the workspace folder, relative with forward slashes.
fn local_only_paths(conn: &Connection, workspace_id: &str, root: &Path) -> Result<Vec<String>> {
    let rows: Vec<String> = conn
        .prepare("SELECT path FROM knowledge_items WHERE workspace_id = ?1 AND privacy = 'local-only'")?
        .query_map([workspace_id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|p| Path::new(&p).strip_prefix(root).ok().map(|rel| rel.to_string_lossy().replace('\\', "/")))
        .collect())
}

/// Is the workspace folder the root of a Git repository of its own (not a folder inside someone else's)?
pub fn own_repository(root: &Path) -> bool {
    let canon = |p: &Path| dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    git::toplevel(root).map(|t| canon(&t)).as_deref() == Some(canon(root).as_path())
}

/// A Git operation left unfinished in the repository (by a person's Git tool or an interrupted program).
pub fn in_progress(root: &Path) -> Option<String> {
    let dir = git::git_dir(root)?;
    for (f, what) in [
        ("MERGE_HEAD", "a merge is in progress"),
        ("rebase-merge", "a rebase is in progress"),
        ("rebase-apply", "a rebase or patch is in progress"),
        ("CHERRY_PICK_HEAD", "a cherry-pick is in progress"),
        ("REVERT_HEAD", "a revert is in progress"),
    ] {
        if dir.join(f).exists() {
            return Some(what.into());
        }
    }
    let lock = dir.join("index.lock");
    if lock.exists() {
        return Some("Git's index is locked (index.lock) — another Git program is running, or one stopped; if none runs, remove the lock file".into());
    }
    None
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkingFile {
    pub path: String,
    /// added, modified, deleted, conflicted
    pub status: &'static str,
    /// Name/path rules that apply (contents are inspected in the commit review).
    pub findings: Vec<inspect::Finding>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub readiness: Readiness,
    /// not-repo, conflicts, in-progress, no-remote, no-upstream, diverged, behind, ahead, up-to-date
    pub state: &'static str,
    pub remote_name: Option<String>,
    pub last_fetch_at: Option<i64>,
    pub incoming: Vec<git::LogEntry>,
    pub outgoing: Vec<git::LogEntry>,
    pub incoming_files: Vec<git::DiffEntry>,
    pub outgoing_files: Vec<git::DiffEntry>,
    pub working: Vec<WorkingFile>,
    pub in_progress: Option<String>,
    /// Git has a name and e-mail for commits.
    pub identity: bool,
    pub history: Vec<history::Entry>,
}

/// Everything the Sync Center shows — read-only, no network (incoming/outgoing as of the last fetch).
pub fn status(conn: &Connection, ws: &Workspace, data_dir: &Path) -> Result<SyncStatus> {
    let root = &ws.root_path;
    let readiness = readiness(conn, ws, data_dir)?;
    let history = history::list(conn, &ws.id, 30)?;
    let mut s = SyncStatus {
        state: "not-repo",
        remote_name: None,
        last_fetch_at: history::last_ok(conn, &ws.id, "fetch")?,
        incoming: vec![],
        outgoing: vec![],
        incoming_files: vec![],
        outgoing_files: vec![],
        working: vec![],
        in_progress: None,
        identity: false,
        history,
        readiness,
    };
    if !s.readiness.repo {
        return Ok(s);
    }
    s.identity = git::identity_configured(root);
    s.in_progress = in_progress(root);
    if s.last_fetch_at.is_none() {
        s.last_fetch_at = git::git_dir(root).and_then(|d| std::fs::metadata(d.join("FETCH_HEAD")).ok()).and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64);
    }
    for (xy, path) in git::changed_paths(root)? {
        let status = match ops::status_of(&xy) {
            "A" => "added",
            "D" => "deleted",
            "U" => "conflicted",
            _ => "modified",
        };
        let mode = if std::fs::symlink_metadata(root.join(&path)).is_ok_and(|m| m.file_type().is_symlink()) { "120000" } else { "100644" };
        let findings = inspect::path_findings(&path, mode, inspect::Direction::Outgoing);
        s.working.push(WorkingFile { path, status, findings });
    }
    let upstream = git::upstream_of_head(root);
    s.remote_name = upstream.as_ref().map(|(r, _)| r.clone()).or_else(|| {
        let all = git::remotes(root);
        if all.iter().any(|r| r == "origin") { Some("origin".into()) } else { all.into_iter().next() }
    });
    if let Some((_, up)) = &upstream {
        s.incoming = git::log_range(root, &format!("HEAD..{up}"), 100)?;
        s.outgoing = git::log_range(root, &format!("{up}..HEAD"), 100)?;
        s.incoming_files = git::diff_raw(root, "HEAD", Some(up))?;
        s.outgoing_files = git::diff_raw(root, up, Some("HEAD"))?;
    } else if s.readiness.branch.is_some() && git::head(root).is_some() {
        s.outgoing = git::log_range(root, "HEAD", 100)?;
    }
    let r = &s.readiness;
    s.state = if r.conflicted > 0 {
        "conflicts"
    } else if s.in_progress.is_some() {
        "in-progress"
    } else if s.remote_name.is_none() {
        "no-remote"
    } else if upstream.is_none() {
        "no-upstream"
    } else if r.ahead > 0 && r.behind > 0 {
        "diverged"
    } else if r.behind > 0 {
        "behind"
    } else if r.ahead > 0 {
        "ahead"
    } else {
        "up-to-date"
    };
    Ok(s)
}

//! Push requests (ADR-0043; owner feedback 2026-11-06: "handle it via NEXUS — approve in a popup to push").
//!
//! A Claude session that may not push itself (Claude Code's permissions deny `git push`) asks NEXUS with the MCP tool
//! `request_push`. NEXUS records the request — the project, the checked-out branch and its commit, Claude's one-line
//! reason — and notifies the person. The person sees exactly what would travel (the outgoing commits, the files, the
//! same security inspection as a workspace push) and decides: NEXUS pushes that commit to the branch's remote — never
//! forced, never another commit than the one reviewed — or declines. The MCP server only writes the request (no
//! network, no Git writes, ADR-0009); the push runs in the NEXUS host when the person approves.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::workspace_sync::inspect::{self, Inspection};
use crate::{git, new_id, now_ms, project, session};

const MAX_REASON: usize = 300;
const MAX_COMMITS: usize = 50;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushRequest {
    pub id: String,
    pub session_id: Option<String>,
    pub project_id: String,
    pub branch: String,
    /// The commit the session asked to push.
    pub head: String,
    /// Claude's reason (its own words, one line, bounded).
    pub reason: String,
    /// `pending`, `pushed`, `declined` or `failed`.
    pub status: String,
    pub message: Option<String>,
    /// Why a push failed (ADR-0047): `rejected`, `auth`, `offline`, `remote-missing`, `timeout` or `other`.
    pub failure_kind: Option<String>,
    pub created_at: i64,
    pub decided_at: Option<i64>,
    /// The person set a failed request aside (it no longer shows on the project page).
    pub dismissed_at: Option<i64>,
}

fn from_row(r: &Row<'_>) -> rusqlite::Result<PushRequest> {
    Ok(PushRequest {
        id: r.get(0)?,
        session_id: r.get(1)?,
        project_id: r.get(2)?,
        branch: r.get(3)?,
        head: r.get(4)?,
        reason: r.get(5)?,
        status: r.get(6)?,
        message: r.get(7)?,
        created_at: r.get(8)?,
        decided_at: r.get(9)?,
        failure_kind: r.get(10)?,
        dismissed_at: r.get(11)?,
    })
}

const COLS: &str = "id, session_id, project_id, branch, head, reason, status, message, created_at, decided_at, failure_kind, dismissed_at";

pub fn get(conn: &Connection, id: &str) -> Result<PushRequest> {
    conn.query_row(&format!("SELECT {COLS} FROM push_requests WHERE id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| CoreError::not_found(format!("push request {id} not found")))
}

/// Pending requests (of one project, or all), newest first.
pub fn pending(conn: &Connection, project_id: Option<&str>) -> Result<Vec<PushRequest>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM push_requests WHERE status = 'pending' AND (?1 IS NULL OR project_id = ?1) ORDER BY created_at DESC LIMIT 50"
    ))?;
    let rows = stmt.query_map([project_id], from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Requests that still need the person (of one project, or all), newest first: pending ones, and failed ones until they
/// are dismissed or a newer request for the same branch supersedes them (ADR-0047) — a failure never just disappears.
pub fn open(conn: &Connection, project_id: Option<&str>) -> Result<Vec<PushRequest>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM push_requests r WHERE (?1 IS NULL OR project_id = ?1)
           AND (status = 'pending' OR (status = 'failed' AND dismissed_at IS NULL
                AND NOT EXISTS (SELECT 1 FROM push_requests n WHERE n.project_id = r.project_id AND n.branch = r.branch AND n.created_at > r.created_at)))
         ORDER BY created_at DESC LIMIT 50"
    ))?;
    let rows = stmt.query_map([project_id], from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn failed_key(r: &PushRequest) -> String {
    format!("push-failed:{}:{}", r.project_id, r.branch)
}

/// The person sets a failed request aside: it leaves the project page and its notification is resolved.
pub fn dismiss(conn: &Connection, id: &str) -> Result<PushRequest> {
    let r = get(conn, id)?;
    if r.status != "failed" {
        return Err(CoreError::invalid(format!("only a failed request can be dismissed — this one is {}", r.status)));
    }
    conn.execute("UPDATE push_requests SET dismissed_at = ?2 WHERE id = ?1 AND dismissed_at IS NULL", params![id, now_ms()])?;
    crate::notify::resolve_dedupe(conn, &failed_key(&r))?;
    get(conn, id)
}

/// What to do next about a failed push, by kind — the same words for the person (dialog, banner) and for Claude (MCP).
pub fn next_step(kind: Option<&str>) -> &'static str {
    match kind {
        Some("rejected") => "The remote has commits this branch does not have. Integrate them (git pull --rebase or merge), resolve any conflicts, run the tests, then ask to push again. NEXUS never force-pushes.",
        Some("auth") => "The remote refused the credentials. Sign in with your Git tool or credential manager on this computer (NEXUS never asks for or stores them), then review the push again.",
        Some("offline") | Some("timeout") => "The remote could not be reached. Check the network, then review the push again — nothing was sent.",
        Some("remote-missing") => "The remote repository was not found, or these credentials cannot see it. Check the remote URL and your access.",
        _ => "Git reported an error (see the message). Check the repository in your Git tool before trying again.",
    }
}

fn repo_of(conn: &Connection, project_id: &str) -> Result<PathBuf> {
    let p = project::get(conn, project_id)?;
    p.local_path.filter(|_| p.available).ok_or_else(|| CoreError::invalid(format!("{} is not on this computer", p.name)))
}

/// Where the branch goes: its upstream, else `origin` (or the only remote) with the same branch name.
fn target(repo: &Path, branch: &str) -> Result<(String, String, bool)> {
    if let Some((remote, up)) = git::upstream_of_head(repo) {
        // `upstream_of_head` names the upstream as `remote/branch`.
        let b = up.strip_prefix(&format!("{remote}/")).unwrap_or(&up).to_string();
        return Ok((remote, b, false));
    }
    let remotes = git::remotes(repo);
    let remote = if remotes.iter().any(|r| r == "origin") { "origin".to_string() } else { remotes.into_iter().next().ok_or_else(|| CoreError::invalid("the repository has no remote to push to"))? };
    Ok((remote, branch.to_string(), true))
}

/// The session asks to push its project's checked-out branch (MCP `request_push`). Asking again for the same branch
/// returns the latest request: still pending (updated to the current commit), or decided for this very commit — that is
/// how Claude learns the outcome.
pub fn request(conn: &Connection, session_id: &str, reason: &str) -> Result<PushRequest> {
    let s = session::get(conn, session_id)?;
    let project_id = s.project_id.clone().ok_or_else(|| CoreError::invalid("this session has no project — push requests belong to a project"))?;
    let repo = repo_of(conn, &project_id)?;
    let st = git::status(&repo)?;
    let branch = st.branch.clone().ok_or_else(|| CoreError::invalid("check out a branch first (detached HEAD)"))?;
    let head = st.head_oid.clone().ok_or_else(|| CoreError::invalid("the branch has no commits yet"))?;
    let reason: String = reason.chars().filter(|c| !c.is_control()).take(MAX_REASON).collect::<String>().trim().to_string();
    let latest: Option<PushRequest> = conn
        .query_row(&format!("SELECT {COLS} FROM push_requests WHERE project_id = ?1 AND branch = ?2 ORDER BY created_at DESC LIMIT 1"), params![project_id, branch], from_row)
        .optional()?;
    match latest {
        Some(r) if r.status == "pending" => {
            conn.execute("UPDATE push_requests SET head = ?2, reason = ?3, session_id = ?4 WHERE id = ?1", params![r.id, head, reason, session_id])?;
            return get(conn, &r.id);
        }
        // The outcome for this very commit (a failure the person set aside no longer answers: ask afresh).
        Some(r) if r.head == head && r.dismissed_at.is_none() => return Ok(r),
        _ => {}
    }
    if st.upstream.is_some() && st.ahead == 0 {
        return Err(CoreError::invalid(format!("{branch} has nothing to push — it is not ahead of {}", st.upstream.unwrap_or_default())));
    }
    // A newer request supersedes a failed one for the branch: its failure notification is no longer the open question.
    crate::notify::resolve_dedupe(conn, &format!("push-failed:{project_id}:{branch}"))?;
    let id = new_id();
    conn.execute(
        "INSERT INTO push_requests(id, session_id, project_id, branch, head, reason, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7)",
        params![id, session_id, project_id, branch, head, reason, now_ms()],
    )?;
    let p = project::get(conn, &project_id)?;
    let mut e = crate::notify::NewEvent::new(crate::notify::Category::ReviewRequired, "push", format!("Claude asks to push {branch} of {}", p.name))
        .body(if reason.is_empty() { "Review the outgoing commits and push — or decline.".to_string() } else { format!("“{reason}” — review the outgoing commits and push, or decline.") })
        .target("push", id.clone())
        .dedupe(format!("push:{project_id}:{branch}"));
    e.workspace_id = Some(p.workspace_id.clone());
    e.project_id = Some(project_id.clone());
    e.session_id = Some(session_id.to_string());
    crate::notify::publish(conn, e)?;
    get(conn, &id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushReview {
    pub request: PushRequest,
    pub project_name: String,
    pub remote: String,
    /// The remote URL without credentials.
    pub remote_url: Option<String>,
    pub remote_branch: String,
    pub first_push: bool,
    /// The commit that would be pushed now (the branch may have moved since the request).
    pub head: String,
    pub moved: bool,
    pub commits: Vec<git::LogEntry>,
    pub inspection: Inspection,
    /// Pass to [`decide`]: the push is of exactly this reviewed commit.
    pub token: String,
    /// The review fetched the remote branch first (ADR-0047); `false` when it could not (see `fetch_error`).
    pub fetched: bool,
    /// Why the fetch failed (offline, auth …): the counts below are then as of the last fetch.
    pub fetch_error: Option<git::RemoteError>,
    /// Commits the push would add to the remote branch.
    pub ahead: u32,
    /// Commits the remote branch has that this branch does not — a push would be rejected while this is above 0.
    pub behind: u32,
    /// Both sides have commits the other lacks.
    pub diverged: bool,
    /// The remote's commits this branch does not have (newest first, bounded).
    pub incoming: Vec<git::LogEntry>,
}

/// What the push would send now: the commits not on the remote branch, their files and the inspection findings.
pub fn review(conn: &Connection, home: &Path, id: &str) -> Result<PushReview> {
    let request = get(conn, id)?;
    let repo = repo_of(conn, &request.project_id)?;
    let st = git::status(&repo)?;
    if st.branch.as_deref() != Some(request.branch.as_str()) {
        return Err(CoreError::invalid(format!("{} is no longer checked out — switch back to it to push it, or decline the request", request.branch)));
    }
    let head = st.head_oid.clone().ok_or_else(|| CoreError::invalid("the branch has no commits"))?;
    let (remote, remote_branch, first_push) = target(&repo, &request.branch)?;
    // Fetch first: a remote that moved on (a teammate pushed) must be visible before the person decides, not only when
    // Git rejects the push. Offline is said, and a reviewed attempt stays possible.
    let (fetched, fetch_error) = match git::fetch_branch(&repo, &remote, &remote_branch) {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e)),
    };
    let base = format!("refs/remotes/{remote}/{remote_branch}");
    let has_base = git::rev_parse(&repo, &base).is_some();
    // A branch the remote does not have yet sends only the commits none of the remote's branches has (a feature branch
    // of `main` sends its own commits, not the whole history); into an empty remote that is every commit.
    let (range, not_on_remote) = (format!("{base}..HEAD"), format!("--remotes={remote}"));
    let revs: Vec<&str> = if has_base { vec![&range] } else { vec!["HEAD", "--not", &not_on_remote] };
    let commits = git::log_revs(&repo, &revs, MAX_COMMITS)?;
    let (ahead, behind) = if has_base { git::ahead_behind(&repo, "HEAD", &base).unwrap_or((commits.len() as u32, 0)) } else { (commits.len() as u32, 0) };
    let incoming = if behind > 0 { git::log_range(&repo, &format!("HEAD..{base}"), MAX_COMMITS)? } else { Vec::new() };
    let inspection = inspect::inspect_history(&repo, &inspect::Context { home }, &revs)?;
    let project_name = project::get(conn, &request.project_id)?.name;
    Ok(PushReview {
        fetched,
        fetch_error,
        ahead,
        behind,
        diverged: ahead > 0 && behind > 0,
        incoming,
        moved: head != request.head,
        remote_url: git::remote_url_of(&repo, &remote),
        request,
        project_name,
        remote,
        remote_branch,
        first_push: first_push || !has_base,
        token: head.clone(),
        head,
        commits,
        inspection,
    })
}

/// The person's decision. Push: exactly the reviewed commit (`token`), never forced, refused when the inspection blocks.
pub fn decide(conn: &Connection, home: &Path, id: &str, approve: bool, token: &str) -> Result<PushRequest> {
    let request = get(conn, id)?;
    // A failed request can be decided again (the person reviews it again after signing in, or once online).
    if request.status != "pending" && !(request.status == "failed" && request.dismissed_at.is_none()) {
        return Err(CoreError::Conflict(format!("this request was already {}", request.status)));
    }
    let finish = |status: &str, message: &str, kind: Option<&str>| -> Result<PushRequest> {
        conn.execute(
            "UPDATE push_requests SET status = ?2, message = ?3, failure_kind = ?4, decided_at = ?5 WHERE id = ?1",
            params![id, status, message, kind, now_ms()],
        )?;
        crate::notify::resolve_dedupe(conn, &format!("push:{}:{}", request.project_id, request.branch))?;
        if status != "failed" {
            crate::notify::resolve_dedupe(conn, &failed_key(&request))?;
        }
        get(conn, id)
    };
    if !approve {
        return finish("declined", "declined by the person", None);
    }
    let r = review(conn, home, id)?;
    if r.head != token {
        return Err(CoreError::Conflict("the branch moved since you reviewed it — review it again".into()));
    }
    if r.inspection.blocked {
        return Err(CoreError::invalid("the security inspection blocks this push — fix the findings first (or push it yourself after checking)"));
    }
    // The remote moved on after the review (a teammate pushed): Git would reject the push — recorded as such, not tried.
    if r.behind > 0 {
        let message = format!("{}/{} has {} commit(s) this branch does not have", r.remote, r.remote_branch, r.behind);
        let done = finish("failed", &message, Some("rejected"))?;
        failure_notice(conn, &done, &r.project_name)?;
        return Ok(done);
    }
    let repo = repo_of(conn, &r.request.project_id)?;
    match git::push_branch(&repo, &r.remote, &r.remote_branch, r.first_push && git::upstream_of_head(&repo).is_none()) {
        Ok(()) => {
            // The request now names the commit that was pushed (the reviewed one), so asking again reports "pushed".
            conn.execute("UPDATE push_requests SET head = ?2 WHERE id = ?1", params![id, r.head])?;
            finish("pushed", &format!("pushed {} commit(s) to {}/{}", r.commits.len(), r.remote, r.remote_branch), None)
        }
        Err(e) => {
            // Recorded with its kind and announced; the request stays listed until dismissed or superseded (ADR-0047).
            let kind = serde_json::to_value(e.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "other".into());
            let done = finish("failed", &e.message, Some(&kind))?;
            failure_notice(conn, &done, &r.project_name)?;
            Ok(done)
        }
    }
}

/// An action notification for a failed push, aimed at the request (its dialog explains the kind and the next step).
fn failure_notice(conn: &Connection, r: &PushRequest, project_name: &str) -> Result<()> {
    let what = match r.failure_kind.as_deref() {
        Some("rejected") => "was rejected — the remote has new commits",
        Some("auth") => "failed — the remote refused the credentials",
        Some("offline") => "failed — the remote cannot be reached",
        Some("timeout") => "failed — the remote did not answer in time",
        Some("remote-missing") => "failed — the remote repository was not found",
        _ => "failed",
    };
    let p = project::get(conn, &r.project_id)?;
    let mut e = crate::notify::NewEvent::new(crate::notify::Category::ReviewRequired, "push", format!("Push of {} ({project_name}) {what}", r.branch))
        .priority(crate::notify::Priority::Action)
        .body(next_step(r.failure_kind.as_deref()))
        .target("push", r.id.clone())
        .dedupe(failed_key(r));
    e.workspace_id = Some(p.workspace_id);
    e.project_id = Some(r.project_id.clone());
    e.session_id = r.session_id.clone();
    crate::notify::publish(conn, e)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::NewSession;

    fn git_in(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@example.invalid", "-c", "core.autocrlf=false", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn a_push_is_requested_reviewed_and_made_only_for_the_reviewed_commit() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let (bare, repo) = (root.join("origin.git"), root.join("app"));
        std::fs::create_dir_all(&repo).unwrap();
        git_in(&root, &["init", "-q", "--bare", bare.to_str().unwrap()]);
        git_in(&repo, &["init", "-q"]);
        std::fs::write(repo.join("README.md"), "app\n").unwrap();
        git_in(&repo, &["add", "-A"]);
        git_in(&repo, &["commit", "-qm", "init"]);
        git_in(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git_in(&repo, &["push", "-q", "-u", "origin", "main"]);

        let conn = crate::db::open_in_memory().unwrap();
        let ws = crate::workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
        let p = project::register(&conn, &ws.id, repo.to_str().unwrap(), Some("app")).unwrap();
        let s = session::insert_starting(&conn, NewSession { workspace_id: Some(ws.id.clone()), project_id: Some(p.id.clone()), title: "t".into(), cwd: repo.clone(), program: "claude".into(), ..Default::default() }).unwrap();

        assert!(request(&conn, &s.id, "x").unwrap_err().to_string().contains("nothing to push"));
        std::fs::write(repo.join("auth.js"), "verify()\n").unwrap();
        git_in(&repo, &["add", "-A"]);
        git_in(&repo, &["commit", "-qm", "Fix the verify bypass"]);
        let r = request(&conn, &s.id, "Deploy the auth fix\u{7}").unwrap();
        assert_eq!((r.status.as_str(), r.branch.as_str(), r.reason.as_str()), ("pending", "main", "Deploy the auth fix"));
        assert_eq!(request(&conn, &s.id, "again").unwrap().id, r.id, "one pending request per branch");
        assert!(crate::notify::list(&conn, &Default::default()).unwrap().iter().any(|n| n.title.contains("asks to push main of app")));

        let home = root.join("home");
        let rv = review(&conn, &home, &r.id).unwrap();
        assert_eq!(rv.commits.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(), ["Fix the verify bypass"]);
        assert!(!rv.inspection.blocked && !rv.first_push && rv.remote == "origin");

        // The branch moved after the review: the reviewed token no longer matches, nothing is pushed.
        std::fs::write(repo.join("later.js"), "x\n").unwrap();
        git_in(&repo, &["add", "-A"]);
        git_in(&repo, &["commit", "-qm", "Later"]);
        assert_eq!(decide(&conn, &home, &r.id, true, &rv.token).unwrap_err().code(), "conflict");
        let rv = review(&conn, &home, &r.id).unwrap();
        assert!(rv.moved && rv.commits.len() == 2);
        let done = decide(&conn, &home, &r.id, true, &rv.token).unwrap();
        assert_eq!(done.status, "pushed", "{done:?}");
        let remote_head = std::process::Command::new("git").args(["--git-dir", bare.to_str().unwrap(), "rev-parse", "main"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&remote_head.stdout).trim(), rv.head);
        assert_eq!(decide(&conn, &home, &r.id, false, "").unwrap_err().code(), "conflict", "decided once");
        // Asking again for the same commit tells Claude the outcome.
        assert_eq!(request(&conn, &s.id, "").unwrap().status, "pushed");

        // A new commit, a new request — declined this time.
        std::fs::write(repo.join("more.js"), "y\n").unwrap();
        git_in(&repo, &["add", "-A"]);
        git_in(&repo, &["commit", "-qm", "More"]);
        let r2 = request(&conn, &s.id, "").unwrap();
        assert_ne!(r2.id, r.id);
        assert_eq!(decide(&conn, &home, &r2.id, false, "").unwrap().status, "declined");
        assert!(pending(&conn, Some(&p.id)).unwrap().is_empty());
    }

    /// The owner's 0.5.1 case: a new branch of `main` was blocked by findings in `main`'s history, which the remote has.
    #[test]
    fn a_new_branch_sends_and_inspects_only_the_commits_the_remote_does_not_have() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let ws = crate::workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
        let home = root.join("home");
        let pem_head = "-----BEGIN OPENSSH";
        let setup = |name: &str, push_main: bool| {
            let (bare, repo) = (root.join(format!("{name}.git")), root.join(name));
            std::fs::create_dir_all(&repo).unwrap();
            git_in(&root, &["init", "-q", "--bare", bare.to_str().unwrap()]);
            git_in(&repo, &["init", "-q"]);
            std::fs::write(repo.join("scanner.md"), format!("{pem_head} PRIVATE KEY-----\n")).unwrap();
            git_in(&repo, &["add", "-A"]);
            git_in(&repo, &["commit", "-qm", "Key-shaped text in main"]);
            git_in(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
            if push_main {
                git_in(&repo, &["push", "-q", "-u", "origin", "main"]);
            }
            git_in(&repo, &["switch", "-q", "-c", "feature"]);
            commit(&repo, "feature.js", "Feature");
            let p = project::register(&conn, &ws.id, repo.to_str().unwrap(), Some(name)).unwrap();
            let s = session::insert_starting(&conn, NewSession { workspace_id: Some(ws.id.clone()), project_id: Some(p.id), title: "t".into(), cwd: repo, program: "claude".into(), ..Default::default() }).unwrap();
            review(&conn, &home, &request(&conn, &s.id, "").unwrap().id).unwrap()
        };

        let rv = setup("app", true);
        assert!(rv.first_push && rv.remote_branch == "feature");
        assert_eq!(rv.commits.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(), ["Feature"], "main's commits are on the remote");
        assert_eq!((rv.ahead, rv.inspection.files), (1, 1));
        assert!(!rv.inspection.blocked && rv.inspection.findings.is_empty(), "{:?}", rv.inspection.findings);

        // Into an empty remote the whole history travels, and is inspected.
        let rv = setup("fresh", false);
        assert_eq!(rv.commits.len(), 2);
        assert!(rv.inspection.blocked && rv.inspection.findings.iter().any(|f| f.path == "scanner.md" && f.kind == "secret"));
    }

    fn commit(repo: &Path, file: &str, msg: &str) {
        std::fs::write(repo.join(file), format!("{msg}\n")).unwrap();
        git_in(repo, &["add", "-A"]);
        git_in(repo, &["commit", "-qm", msg]);
    }

    /// The anilist case (ADR-0047): a teammate pushed first. The review fetches and shows it, the push is recorded as
    /// rejected with a notification and stays listed; a push Git itself refuses is kept the same way; Claude learns the
    /// kind; a newer request supersedes the failure and a dismissal sets it aside.
    #[test]
    fn a_remote_that_moved_on_is_shown_before_the_push_and_a_rejection_never_disappears() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let (bare, repo, mate) = (root.join("origin.git"), root.join("app"), root.join("teammate"));
        std::fs::create_dir_all(&repo).unwrap();
        git_in(&root, &["init", "-q", "--bare", bare.to_str().unwrap()]);
        git_in(&repo, &["init", "-q"]);
        commit(&repo, "README.md", "init");
        git_in(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git_in(&repo, &["push", "-q", "-u", "origin", "main"]);
        git_in(&root, &["clone", "-q", bare.to_str().unwrap(), mate.to_str().unwrap()]);

        let conn = crate::db::open_in_memory().unwrap();
        let ws = crate::workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
        let p = project::register(&conn, &ws.id, repo.to_str().unwrap(), Some("anilist")).unwrap();
        let s = session::insert_starting(&conn, NewSession { workspace_id: Some(ws.id.clone()), project_id: Some(p.id.clone()), title: "t".into(), cwd: repo.clone(), program: "claude".into(), ..Default::default() }).unwrap();
        let home = root.join("home");

        // The teammate pushes first; this branch has not fetched it.
        commit(&mate, "theirs.txt", "Teammate change");
        git_in(&mate, &["push", "-q", "origin", "main"]);
        commit(&repo, "ours.txt", "Our change");
        let r = request(&conn, &s.id, "ship it").unwrap();

        let rv = review(&conn, &home, &r.id).unwrap();
        assert!(rv.fetched && rv.fetch_error.is_none(), "{:?}", rv.fetch_error);
        assert_eq!((rv.ahead, rv.behind, rv.diverged), (1, 1, true));
        assert_eq!(rv.incoming.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(), ["Teammate change"]);

        // Pushing anyway is recorded as rejected — nothing is sent, nothing is forced.
        let done = decide(&conn, &home, &r.id, true, &rv.token).unwrap();
        assert_eq!((done.status.as_str(), done.failure_kind.as_deref()), ("failed", Some("rejected")), "{done:?}");
        assert!(done.message.as_deref().unwrap_or("").contains("origin/main has 1 commit"), "{done:?}");
        let n = crate::notify::list(&conn, &Default::default()).unwrap();
        let notice = n.iter().find(|n| n.title.contains("Push of main (anilist) was rejected")).expect("an action notification");
        assert_eq!((notice.priority.as_str(), notice.target_kind.as_deref(), notice.project_id.as_deref()), ("action", Some("push"), Some(p.id.as_str())));
        assert_eq!(open(&conn, Some(&p.id)).unwrap().iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), [r.id.as_str()], "the failure stays listed");
        // Claude asking again for this commit learns the kind.
        let again = request(&conn, &s.id, "").unwrap();
        assert_eq!((again.id.as_str(), again.failure_kind.as_deref()), (r.id.as_str(), Some("rejected")));
        assert!(next_step(again.failure_kind.as_deref()).contains("git pull --rebase"));

        // Integrated: a new request supersedes the failure (and resolves its notification).
        git_in(&repo, &["pull", "-q", "--rebase", "origin", "main"]);
        let r2 = request(&conn, &s.id, "integrated").unwrap();
        assert_ne!(r2.id, r.id);
        assert_eq!(open(&conn, Some(&p.id)).unwrap().iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), [r2.id.as_str()]);
        assert!(!crate::notify::is_open(&conn, &format!("push-failed:{}:main", p.id)).unwrap());

        // A race the review cannot see: the remote refuses (a declining hook stands in for a push that lands first).
        let rv2 = review(&conn, &home, &r2.id).unwrap();
        assert_eq!((rv2.ahead, rv2.behind), (1, 0));
        std::fs::write(bare.join("hooks").join("pre-receive"), "#!/bin/sh\necho 'rejected by policy' >&2\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(bare.join("hooks").join("pre-receive"), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let failed = decide(&conn, &home, &r2.id, true, &rv2.token).unwrap();
        assert_eq!((failed.status.as_str(), failed.failure_kind.as_deref()), ("failed", Some("rejected")), "{failed:?}");
        assert!(open(&conn, Some(&p.id)).unwrap().iter().any(|x| x.id == r2.id));

        // Reviewed again once the remote accepts: the same request is pushed and its failure notice resolved.
        std::fs::remove_file(bare.join("hooks").join("pre-receive")).unwrap();
        let rv3 = review(&conn, &home, &r2.id).unwrap();
        assert_eq!(decide(&conn, &home, &r2.id, true, &rv3.token).unwrap().status, "pushed");
        assert!(open(&conn, Some(&p.id)).unwrap().is_empty());
        assert!(!crate::notify::is_open(&conn, &format!("push-failed:{}:main", p.id)).unwrap());

        // A failure the person sets aside leaves the page; only failed requests can be dismissed.
        commit(&repo, "more.txt", "More");
        let r3 = request(&conn, &s.id, "").unwrap();
        assert!(dismiss(&conn, &r3.id).is_err(), "pending requests are declined, not dismissed");
        git_in(&repo, &["remote", "set-url", "origin", root.join("gone.git").to_str().unwrap()]);
        let rv4 = review(&conn, &home, &r3.id).unwrap();
        assert!(!rv4.fetched && rv4.fetch_error.is_some(), "an unreachable remote is said, the review still works");
        let gone = decide(&conn, &home, &r3.id, true, &rv4.token).unwrap();
        assert_eq!((gone.status.as_str(), gone.failure_kind.as_deref()), ("failed", Some("remote-missing")), "{gone:?}");
        assert!(dismiss(&conn, &r3.id).unwrap().dismissed_at.is_some());
        assert!(open(&conn, Some(&p.id)).unwrap().is_empty());
        assert!(!crate::notify::is_open(&conn, &format!("push-failed:{}:main", p.id)).unwrap());
    }
}

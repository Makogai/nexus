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
    pub created_at: i64,
    pub decided_at: Option<i64>,
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
    })
}

const COLS: &str = "id, session_id, project_id, branch, head, reason, status, message, created_at, decided_at";

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
        Some(r) if r.head == head => return Ok(r),
        _ => {}
    }
    if st.upstream.is_some() && st.ahead == 0 {
        return Err(CoreError::invalid(format!("{branch} has nothing to push — it is not ahead of {}", st.upstream.unwrap_or_default())));
    }
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
}

/// What the push would send now: the commits not on the remote branch, their files and the inspection findings.
pub fn review(conn: &Connection, home: &Path, id: &str) -> Result<PushReview> {
    let request = get(conn, id)?;
    let repo = repo_of(conn, &request.project_id)?;
    let st = git::status(&repo)?;
    if st.branch.as_deref() != Some(request.branch.as_str()) {
        return Err(CoreError::invalid(format!("{} is no longer checked out — switch back to it to push", request.branch)));
    }
    let head = st.head_oid.clone().ok_or_else(|| CoreError::invalid("the branch has no commits"))?;
    let (remote, remote_branch, first_push) = target(&repo, &request.branch)?;
    let base = format!("refs/remotes/{remote}/{remote_branch}");
    let has_base = git::rev_parse(&repo, &base).is_some();
    let range = if has_base { format!("{base}..HEAD") } else { "HEAD".to_string() };
    let commits = git::log_range(&repo, &range, MAX_COMMITS)?;
    let inspection = inspect::inspect_history(&repo, &inspect::Context { home }, &range)?;
    let project_name = project::get(conn, &request.project_id)?.name;
    Ok(PushReview {
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
    if request.status != "pending" {
        return Err(CoreError::Conflict(format!("this request was already {}", request.status)));
    }
    let finish = |status: &str, message: &str| -> Result<PushRequest> {
        conn.execute("UPDATE push_requests SET status = ?2, message = ?3, decided_at = ?4 WHERE id = ?1", params![id, status, message, now_ms()])?;
        crate::notify::resolve_dedupe(conn, &format!("push:{}:{}", request.project_id, request.branch))?;
        get(conn, id)
    };
    if !approve {
        return finish("declined", "declined by the person");
    }
    let r = review(conn, home, id)?;
    if r.head != token {
        return Err(CoreError::Conflict("the branch moved since you reviewed it — review it again".into()));
    }
    if r.inspection.blocked {
        return Err(CoreError::invalid("the security inspection blocks this push — fix the findings first (or push it yourself after checking)"));
    }
    let repo = repo_of(conn, &r.request.project_id)?;
    match git::push_branch(&repo, &r.remote, &r.remote_branch, r.first_push && git::upstream_of_head(&repo).is_none()) {
        Ok(()) => {
            // The request now names the commit that was pushed (the reviewed one), so asking again reports "pushed".
            conn.execute("UPDATE push_requests SET head = ?2 WHERE id = ?1", params![id, r.head])?;
            finish("pushed", &format!("pushed {} commit(s) to {}/{}", r.commits.len(), r.remote, r.remote_branch))
        }
        Err(e) => {
            let _ = finish("failed", &e.message);
            Err(CoreError::External { program: "git push".into(), message: e.message })
        }
    }
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
}

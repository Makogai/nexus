//! Git-based workspace synchronisation — readiness only (ADR-0031). Read-only: this module never fetches, commits,
//! pushes or changes the repository. It answers "would syncing this workspace folder through Git be safe now?":
//!
//! * the folder is a Git repository with a branch and (optionally) an upstream; ahead/behind as of the last fetch;
//! * which portable files changed (they would be part of an outgoing commit);
//! * files that must never travel — machine-local databases, environment files, keys and credential files — are
//!   reported by *name* (never read) when tracked or not ignored;
//! * local-only knowledge inside the folder must be ignored; the NEXUS data dir must not be inside it;
//! * how many synced rules, constitutions and workflows wait for approval (incoming changes are untrusted).
//!
//! The planned sync (desktop review of the outgoing diff, explicit commit and push, fast-forward-only pull) builds on
//! this check; machine-local state (SQLite: sessions, approvals, gates, usage, notifications) never leaves the machine.

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

/// Why a file name must never be synced (checked by name only; contents are never read).
pub fn sensitive_reason(rel: &str) -> Option<&'static str> {
    let name = rel.rsplit(['/', '\\']).next().unwrap_or(rel).to_ascii_lowercase();
    if name == ".env" || name.starts_with(".env.") {
        return Some("environment file (may contain secrets)");
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if matches!(ext, "db" | "db-wal" | "db-shm" | "sqlite" | "sqlite3") || name.ends_with(".db-journal") {
        return Some("machine-local database");
    }
    if matches!(ext, "pem" | "key" | "p12" | "pfx" | "jks" | "keystore") || name.starts_with("id_rsa") || name.starts_with("id_ed25519") || name.starts_with("id_ecdsa") {
        return Some("private key or keystore");
    }
    if matches!(name.as_str(), ".credentials.json" | "credentials.json" | ".git-credentials" | ".netrc" | "_netrc" | ".npmrc" | ".pypirc") {
        return Some("credential file");
    }
    None
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
    if top.as_deref() != Some(canon(root).as_path()) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@example.invalid", "-c", "core.autocrlf=false", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
    }

    #[test]
    fn sensitive_names() {
        for n in [".env", "config/.env.production", "nexus.db", "x.db-wal", "id_rsa", "keys/server.pem", ".credentials.json", ".npmrc"] {
            assert!(sensitive_reason(n).is_some(), "{n}");
        }
        for n in ["README.md", "tasks/env-setup.md", "rules/keys.md", "nexus-workspace.json"] {
            assert!(sensitive_reason(n).is_none(), "{n}");
        }
    }

    #[test]
    fn readiness_reports_what_would_travel_and_what_must_not() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let ws = crate::workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        // Not a repository yet: guidance, not ready.
        let r = readiness(&conn, &ws, &data).unwrap();
        assert!(!r.repo && !r.ready && r.issues[0].message.contains("not a Git repository"));

        git(&ws.root_path, &["init", "-q"]);
        git(&ws.root_path, &["add", "-A"]);
        git(&ws.root_path, &["commit", "-qm", "init"]);
        std::fs::create_dir_all(ws.root_path.join("tasks")).unwrap();
        std::fs::write(ws.root_path.join("tasks").join("a.md"), "---\nnexus: 1\n---\n").unwrap();
        std::fs::write(ws.root_path.join("server.pem"), "not read").unwrap();
        std::fs::write(ws.root_path.join(".env"), "IGNORED=by-the-generated-gitignore").unwrap();
        let r = readiness(&conn, &ws, &data).unwrap();
        assert!(r.repo && r.branch.is_some());
        assert_eq!(r.changed, vec!["tasks/a.md".to_string()], "the new task would travel; .env is ignored by the generated .gitignore");
        assert_eq!(r.sensitive, vec![SensitiveFile { path: "server.pem".into(), tracked: false, reason: "private key or keystore" }]);
        assert!(r.ready, "warnings only: {:?}", r.issues);
        assert!(r.issues.iter().any(|i| i.severity == "warning" && i.message.contains("not ignored")));
        assert!(r.issues.iter().any(|i| i.message.contains("no remote")));

        // A committed key blocks; so does a data dir inside the workspace.
        git(&ws.root_path, &["add", "server.pem"]);
        git(&ws.root_path, &["commit", "-qm", "oops"]);
        let r = readiness(&conn, &ws, &ws.root_path.join(".nexus-data")).unwrap();
        assert!(!r.ready && r.data_dir_inside);
        assert!(r.sensitive.iter().any(|s| s.tracked && s.path == "server.pem"));
        assert!(r.issues.iter().any(|i| i.severity == "error" && i.message.contains("committed")));

        // Ahead of an upstream (as of the last fetch); no network is used by the check.
        let remote = root.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "-q", "--bare"]);
        git(&ws.root_path, &["remote", "add", "origin", &format!("file://{}", remote.display())]);
        git(&ws.root_path, &["push", "-q", "-u", "origin", "HEAD"]);
        git(&ws.root_path, &["add", "-A"]);
        git(&ws.root_path, &["commit", "-qm", "task"]);
        let r = readiness(&conn, &ws, &data).unwrap();
        assert_eq!((r.ahead, r.behind, r.upstream.is_some(), r.remote.is_some()), (1, 0, true, true));
    }
}

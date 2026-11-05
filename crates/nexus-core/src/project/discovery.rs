//! Project discovery and restoration (Portable Workspace P4): which Git repositories sit in the workspace's clone folder
//! (`projects/`, or `repos/` in older workspaces), which of them belong to known projects, which are new, and which
//! registered projects are missing on this computer — with a clone from the recorded remote for those.
//!
//! Scanning only reads (`.git` presence, the remote, manifest files for stack detection); nothing in a repository is
//! executed and nothing is registered automatically. Cloning uses `git clone --no-recurse-submodules` without prompts;
//! the clone is bound to the project's stable id.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::workspace::{self, Workspace};
use crate::{git, paths, project};

const MAX_FOUND: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub path: PathBuf,
    /// Workspace-relative, forward slashes.
    pub rel: String,
    pub name: String,
    pub remote_url: Option<String>,
    pub stacks: Vec<String>,
    /// `registered` (bound to a project), `known` (a missing project with the same remote — link it) or `new`.
    pub status: &'static str,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Missing {
    pub id: String,
    pub name: String,
    pub remote_url: Option<String>,
    /// Suggested clone destination (workspace-relative) when it can be cloned.
    pub clone_to: Option<String>,
    pub can_clone: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub projects_dir: String,
    pub found: Vec<Found>,
    pub missing: Vec<Missing>,
    /// Available projects that live outside the workspace folder (linked, not cloned into it).
    pub external: Vec<String>,
}

/// `host/path` without scheme, credentials, `.git` or trailing slash, lowercase host — to compare remotes.
pub fn normalize_remote(url: &str) -> String {
    let u = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let rest = match u.split_once("://") {
        Some((_, r)) => r.to_string(),
        None => u.replacen(':', "/", 1), // git@host:path
    };
    let rest = rest.rsplit_once('@').map(|(_, r)| r.to_string()).filter(|_| rest.find('@') < rest.find('/')).unwrap_or(rest);
    match rest.split_once('/') {
        Some((host, path)) => format!("{}/{}", host.to_lowercase(), path),
        None => rest.to_lowercase(),
    }
}

fn repos_in(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if out.len() >= MAX_FOUND {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if p.join(".git").exists() {
            out.push(p);
        } else if depth < 1 {
            repos_in(&p, depth + 1, out); // owner/repo layouts
        }
    }
}

/// Clone folders of the workspace: the manifest's, plus `repos/` when an older workspace still has it.
fn clone_dirs(root: &Path, m: &workspace::Manifest) -> Vec<String> {
    let mut v = vec![m.projects_dir().to_string()];
    for legacy in [workspace::LEGACY_PROJECTS_DIR, workspace::PROJECTS_DIR] {
        if !v.iter().any(|x| x == legacy) && root.join(legacy).is_dir() {
            v.push(legacy.to_string());
        }
    }
    v
}

pub fn scan(conn: &Connection, ws: &Workspace) -> Result<Scan> {
    let root = &ws.root_path;
    let m = workspace::read_manifest(root)?;
    let projects = project::list(conn, &ws.id)?;
    let missing_projects: Vec<&project::Project> = projects.iter().filter(|p| !p.available).collect();
    let mut found = Vec::new();
    for d in clone_dirs(root, &m) {
        let mut repos = Vec::new();
        repos_in(&root.join(&d), 0, &mut repos);
        for path in repos {
            let path = dunce::canonicalize(&path).unwrap_or(path);
            let remote = git::remote_url(&path).map(|u| git::redact_url(&u));
            let registered = projects.iter().find(|p| p.local_path.as_deref().is_some_and(|lp| paths::same_path(lp, &path)));
            let known = registered.is_none().then(|| {
                remote.as_deref().and_then(|r| missing_projects.iter().find(|p| p.remote_url.as_deref().is_some_and(|pr| normalize_remote(pr) == normalize_remote(r))))
            });
            let (status, project_id) = match (registered, known.flatten()) {
                (Some(p), _) => ("registered", Some(p.id.clone())),
                (None, Some(p)) => ("known", Some(p.id.clone())),
                _ => ("new", None),
            };
            found.push(Found {
                rel: paths::relative_slash_path(root, &path).unwrap_or_default(),
                name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                remote_url: remote,
                stacks: project::detect::detect(&path).stacks,
                status,
                project_id,
                path,
            });
        }
    }
    let taken: Vec<&str> = found.iter().filter(|f| f.status == "known").filter_map(|f| f.project_id.as_deref()).collect();
    let missing = missing_projects
        .iter()
        .filter(|p| !taken.contains(&p.id.as_str()))
        .map(|p| {
            let dest = format!("{}/{}", m.projects_dir(), crate::project::context::slug(&p.name));
            let (can_clone, reason) = match p.remote_url.as_deref() {
                None => (false, Some("no remote is recorded — locate a copy instead".to_string())),
                Some(u) if !git::clone_url_allowed(u) => (false, Some("the recorded remote is not a supported Git URL".to_string())),
                Some(_) if root.join(&dest).exists() => (false, Some(format!("{dest} already exists — link it or choose another folder"))),
                Some(_) => (true, None),
            };
            Missing { id: p.id.clone(), name: p.name.clone(), remote_url: p.remote_url.clone(), clone_to: can_clone.then_some(dest), can_clone, reason }
        })
        .collect();
    let external = projects
        .iter()
        .filter(|p| p.available && p.local_path.as_deref().is_some_and(|lp| paths::relative_slash_path(root, lp).is_none()))
        .map(|p| p.name.clone())
        .collect();
    Ok(Scan { projects_dir: m.projects_dir().to_string(), found, missing, external })
}

/// Clone a missing project from its recorded remote into the clone folder (or `dest_rel`), bind it to the project's
/// id and refresh its context. The destination must be inside the workspace and not exist yet.
pub fn clone_missing(conn: &Connection, data_dir: &Path, ws: &Workspace, project_id: &str, dest_rel: Option<&str>) -> Result<project::Project> {
    let root = &ws.root_path;
    let p = project::get(conn, project_id)?;
    if p.workspace_id != ws.id {
        return Err(CoreError::invalid("the project is not part of this workspace"));
    }
    if p.available {
        return Err(CoreError::Conflict(format!("{} is already on this computer", p.name)));
    }
    let url = p.remote_url.clone().ok_or_else(|| CoreError::invalid("no remote is recorded for this project — locate a copy instead"))?;
    let m = workspace::read_manifest(root)?;
    let rel = dest_rel.map(str::to_string).unwrap_or_else(|| format!("{}/{}", m.projects_dir(), crate::project::context::slug(&p.name)));
    let dest = paths::resolve_within(root, &rel)?;
    if dest.exists() && std::fs::read_dir(&dest).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(CoreError::Conflict(format!("{rel} already exists and is not empty")));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| CoreError::io("cannot create", parent, e))?;
    }
    if let Err(e) = git::clone_full(&url, &dest) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(CoreError::External { program: "git clone".into(), message: e.message });
    }
    let bound = project::locate(conn, project_id, &dest.to_string_lossy())?;
    let _ = project::refresh(conn, project_id);
    let _ = crate::project::context::refresh_state(conn, data_dir, ws, project_id);
    Ok(bound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
    }

    fn file_url(p: &Path) -> String {
        format!("file:///{}", dunce::canonicalize(p).unwrap().to_string_lossy().replace('\\', "/").trim_start_matches('/'))
    }

    /// A project repository with a README and a local bare remote.
    fn project_repo(base: &Path, name: &str) -> (PathBuf, String) {
        let bare = base.join(format!("{name}.git"));
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare"]);
        let work = base.join(name);
        std::fs::create_dir_all(work.join("src")).unwrap();
        std::fs::write(work.join("README.md"), format!("# {name}\n\nA small service that sends invoices.\n")).unwrap();
        std::fs::write(work.join("package.json"), r#"{"dependencies":{"vue":"^3"},"scripts":{"test":"vitest"}}"#).unwrap();
        std::fs::write(work.join("src").join("main.ts"), "export {}\n").unwrap();
        git(&work, &["init", "-q"]);
        git(&work, &["add", "-A"]);
        git(&work, &["commit", "-q", "-m", "init"]);
        let url = file_url(&bare);
        git(&work, &["remote", "add", "origin", &url]);
        git(&work, &["push", "-q", "origin", "main"]);
        (work, url)
    }

    #[test]
    fn a_second_computer_finds_clones_and_restores_missing_projects_by_id() {
        let t = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(t.path()).unwrap();
        // Computer A: a schema 2 workspace with one project that has a remote and a portable context.
        let conn = crate::db::open_in_memory().unwrap();
        let data = base.join("data-a");
        let ws = workspace::create(&conn, base.join("ws").to_str().unwrap(), "W", "").unwrap();
        let (work, url) = project_repo(&base, "invoices");
        let p = project::register_with_context(&conn, &data, &ws.id, work.to_str().unwrap(), None).unwrap();
        let ctx = crate::project::context::context_dir(&ws, &p.id).expect("context folder");
        assert!(ctx.ends_with("invoices"));
        for f in ["project.json", "overview.md", "architecture.md", "current-state.md"] {
            assert!(ctx.join(f).is_file(), "{f}");
        }
        let overview = std::fs::read_to_string(ctx.join("overview.md")).unwrap();
        assert!(overview.contains("A small service that sends invoices.") && overview.contains("_Source: README.md._"), "{overview}");
        assert!(std::fs::read_to_string(ctx.join("architecture.md")).unwrap().contains("status: draft"));
        let record: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ctx.join("project.json")).unwrap()).unwrap();
        assert_eq!((record["id"].as_str(), record["stacks"][0].as_str()), (Some(p.id.as_str()), Some("vue")));
        assert!(!std::fs::read_to_string(ctx.join("project.json")).unwrap().contains(&*base.to_string_lossy()), "no machine paths in the portable record");
        // Project documents go into the context folder.
        let task = crate::knowledge::service::create(
            &conn,
            &data,
            &ws,
            &crate::knowledge::service::CreateInput { kind: "task".into(), title: "Send reminders".into(), project_id: Some(p.id.clone()), ..Default::default() },
            crate::knowledge::service::Author::Human,
        )
        .unwrap();
        assert!(task.path.starts_with(ctx.join("tasks")), "{}", task.path.display());
        // Re-initialising keeps the person's files and only regenerates what NEXUS owns.
        std::fs::write(ctx.join("overview.md"), std::fs::read_to_string(ctx.join("overview.md")).unwrap().replace("## Repository", "## Repository (mine)")).unwrap();
        let again = crate::project::context::init(&conn, &data, &ws, &p.id).unwrap();
        assert!(again.kept.iter().any(|k| k.ends_with("overview.md")) && std::fs::read_to_string(ctx.join("overview.md")).unwrap().contains("(mine)"));
        assert!(again.updated.iter().any(|k| k.ends_with("current-state.md")), "the new task changes the generated state: {again:?}");

        // Computer B: same workspace folder (as after cloning it), fresh database, the project is not here.
        let conn_b = crate::db::open_in_memory().unwrap();
        let data_b = base.join("data-b");
        std::fs::rename(&work, base.join("invoices-elsewhere")).unwrap();
        let ws_b = workspace::import(&conn_b, ws.root_path.to_str().unwrap()).unwrap();
        let s = scan(&conn_b, &ws_b).unwrap();
        assert_eq!(s.missing.len(), 1);
        let m = &s.missing[0];
        assert!(m.can_clone && m.clone_to.as_deref() == Some("projects/invoices") && m.remote_url.as_deref() == Some(url.as_str()), "{m:?}");
        // A repository dropped into the clone folder by hand is offered for review, never registered automatically.
        let stray = ws_b.root_path.join("projects").join("scratch");
        std::fs::create_dir_all(&stray).unwrap();
        git(&stray, &["init", "-q"]);
        let s = scan(&conn_b, &ws_b).unwrap();
        assert_eq!(s.found.iter().map(|f| (f.rel.as_str(), f.status)).collect::<Vec<_>>(), [("projects/scratch", "new")]);
        assert_eq!(project::list(&conn_b, &ws_b.id).unwrap().len(), 1, "nothing was registered");

        let cloned = clone_missing(&conn_b, &data_b, &ws_b, &m.id, None).unwrap();
        assert_eq!(cloned.id, p.id, "the stable id is kept");
        assert!(cloned.available && ws_b.root_path.join("projects").join("invoices").join("README.md").is_file());
        assert!(clone_missing(&conn_b, &data_b, &ws_b, &m.id, None).is_err(), "already here");
        let s = scan(&conn_b, &ws_b).unwrap();
        assert!(s.missing.is_empty() && s.found.iter().any(|f| f.rel == "projects/invoices" && f.status == "registered"));

        // A clone made by hand is recognised by its remote and linked to the project's id.
        let conn_c = crate::db::open_in_memory().unwrap();
        let ws_c = workspace::import(&conn_c, ws.root_path.to_str().unwrap()).unwrap();
        let s = scan(&conn_c, &ws_c).unwrap();
        let known = s.found.iter().find(|f| f.rel == "projects/invoices").unwrap();
        assert_eq!((known.status, known.project_id.as_deref()), ("known", Some(p.id.as_str())));
        assert!(s.missing.is_empty(), "a known clone is offered for linking, not cloning again");
        assert!(clone_missing(&conn_c, &base.join("data-c"), &ws_c, &p.id, Some("../outside")).is_err(), "destinations stay in the workspace");
    }

    #[test]
    fn remotes_compare_across_url_forms() {
        let a = normalize_remote("https://GitHub.com/me/app.git");
        assert_eq!(a, "github.com/me/app");
        assert_eq!(normalize_remote("git@github.com:me/app.git"), a);
        assert_eq!(normalize_remote("ssh://git@github.com/me/app"), a);
        assert_eq!(normalize_remote("https://user@github.com/me/app/"), a);
        assert_ne!(normalize_remote("https://github.com/me/other"), a);
    }
}

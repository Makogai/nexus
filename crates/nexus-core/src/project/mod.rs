//! Project registry. A project is an independent Git repository registered in a workspace.
//! Portable fields go to the workspace manifest, the machine-local path to SQLite (ADR-0005).

pub mod context;
pub mod detect;
pub mod discovery;
pub mod environment;
pub mod explorer;
pub mod learn;

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

pub use detect::{ScriptHint, StackInfo};

use crate::error::{CoreError, Result};
use crate::paths::{self, Expect};
use crate::workspace::{self, ManifestProject};
use crate::{git, new_id, now_ms};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub local_path: Option<PathBuf>,
    pub remote_url: Option<String>,
    pub stack: StackInfo,
    pub created_at: i64,
    pub last_opened_at: Option<i64>,
    /// `local_path` is set and exists on this machine.
    pub available: bool,
}

/// Result of inspecting a folder before registering it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathInspection {
    pub path: PathBuf,
    pub suggested_name: String,
    pub git_root: Option<PathBuf>,
    pub remote_url: Option<String>,
    pub stack: StackInfo,
    /// Already registered in the given workspace (project id).
    pub registered_as: Option<String>,
    pub inside_workspace: bool,
}

const SELECT: &str = "SELECT id, workspace_id, name, local_path, remote_url, stack_json, created_at, last_opened_at
    FROM projects";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Project> {
    let local_path: Option<PathBuf> = r.get::<_, Option<String>>(3)?.map(PathBuf::from);
    let stack_json: String = r.get(5)?;
    let available = local_path.as_ref().is_some_and(|p| p.is_dir());
    Ok(Project {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        name: r.get(2)?,
        local_path,
        remote_url: r.get(4)?,
        stack: serde_json::from_str(&stack_json).unwrap_or_default(),
        created_at: r.get(6)?,
        last_opened_at: r.get(7)?,
        available,
    })
}

pub fn list(conn: &Connection, workspace_id: &str) -> Result<Vec<Project>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE workspace_id = ?1 ORDER BY COALESCE(last_opened_at, created_at) DESC, name"
    ))?;
    let rows = stmt.query_map([workspace_id], from_row)?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> Result<Project> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| CoreError::not_found(format!("project {id} not found")))
}

fn find_by_path(conn: &Connection, workspace_id: &str, path: &Path) -> Result<Option<String>> {
    for p in list(conn, workspace_id)? {
        if p.local_path.as_deref().is_some_and(|lp| paths::same_path(lp, path)) {
            return Ok(Some(p.id));
        }
    }
    Ok(None)
}

/// Inspect a folder (deterministic, read-only) so the UI can preview a registration.
pub fn inspect_path(conn: &Connection, workspace_id: &str, raw: &str) -> Result<PathInspection> {
    let ws = workspace::get(conn, workspace_id)?;
    let path = paths::validate_user_path(raw, Expect::ExistingDir)?;
    let git_root = git::toplevel(&path);
    let remote_url = git_root.as_deref().and_then(git::remote_url);
    let suggested_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    Ok(PathInspection {
        stack: detect::detect(&path),
        registered_as: find_by_path(conn, workspace_id, &path)?,
        inside_workspace: paths::relative_slash_path(&ws.root_path, &path).is_some(),
        path,
        suggested_name,
        git_root,
        remote_url,
    })
}

/// Register a local folder as a project of `workspace_id`. Writes the manifest entry and the DB row.
pub fn register(conn: &Connection, workspace_id: &str, raw: &str, name: Option<&str>) -> Result<Project> {
    let ws = workspace::get(conn, workspace_id)?;
    if !ws.available {
        return Err(CoreError::invalid(format!(
            "workspace folder {} is not available on this machine",
            ws.root_path.display()
        )));
    }
    let path = paths::validate_user_path(raw, Expect::ExistingDir)?;
    if paths::same_path(&path, &ws.root_path) {
        return Err(CoreError::invalid("the workspace folder itself cannot be a project"));
    }
    if let Some(existing) = find_by_path(conn, workspace_id, &path)? {
        return Err(CoreError::Conflict(format!("this folder is already registered (project {existing})")));
    }
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    workspace::validate_name(&name)?;

    let stack = detect::detect(&path);
    let remote_url = git::toplevel(&path).as_deref().and_then(git::remote_url);
    let id = new_id();
    let rel = paths::relative_slash_path(&ws.root_path, &path);

    let mut manifest = workspace::read_manifest(&ws.root_path)?;
    manifest.projects.push(ManifestProject {
        id: id.clone(),
        name: name.clone(),
        remote_url: remote_url.clone(),
        path: rel,
        context: None,
        extra: Default::default(),
    });

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO projects(id, workspace_id, name, local_path, remote_url, stack_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            ws.id,
            name,
            path.to_string_lossy(),
            remote_url,
            serde_json::to_string(&stack).unwrap_or_else(|_| "{}".into()),
            now_ms()
        ],
    )?;
    // Manifest first, commit second: if the manifest cannot be written nothing is registered.
    workspace::write_manifest(&ws.root_path, &manifest)?;
    // A repository inside the workspace (outside the clone folder) must never end up in the workspace's Git repository.
    if let Some(rel) = manifest.projects.last().and_then(|p| p.path.clone()) {
        workspace::ensure_ignored(&ws.root_path, &rel)?;
    }
    tx.commit()?;
    get(conn, &id)
}

/// Register a folder and, in a schema 2 workspace, set up its portable context (`project-context/<slug>/`). A context
/// that cannot be written does not undo the registration — the Workspace Doctor reports it and offers to retry.
pub fn register_with_context(conn: &Connection, data_dir: &std::path::Path, workspace_id: &str, raw: &str, name: Option<&str>) -> Result<Project> {
    let p = register(conn, workspace_id, raw, name)?;
    let ws = workspace::get(conn, workspace_id)?;
    if workspace::read_manifest(&ws.root_path).map(|m| m.schema_version >= 2).unwrap_or(false)
        && let Err(e) = context::init(conn, data_dir, &ws, &p.id)
    {
        tracing::warn!(project = %p.id, error = %e, "project context could not be created");
    }
    get(conn, &p.id)
}

/// Bind a manifest project that has no local path on this machine to a folder.
pub fn locate(conn: &Connection, id: &str, raw: &str) -> Result<Project> {
    let project = get(conn, id)?;
    let path = paths::validate_user_path(raw, Expect::ExistingDir)?;
    if let Some(other) = find_by_path(conn, &project.workspace_id, &path)?.filter(|o| o != id) {
        return Err(CoreError::Conflict(format!("this folder is already registered (project {other})")));
    }
    let stack = detect::detect(&path);
    conn.execute(
        "UPDATE projects SET local_path = ?2, stack_json = ?3 WHERE id = ?1",
        params![id, path.to_string_lossy(), serde_json::to_string(&stack).unwrap_or_else(|_| "{}".into())],
    )?;
    get(conn, id)
}

/// Re-run deterministic detection (after the user changed the repository).
pub fn refresh(conn: &Connection, id: &str) -> Result<Project> {
    let project = get(conn, id)?;
    let Some(path) = project.local_path.filter(|p| p.is_dir()) else {
        return Err(CoreError::invalid("project folder is not available on this machine"));
    };
    let stack = detect::detect(&path);
    let remote = git::toplevel(&path).as_deref().and_then(git::remote_url);
    conn.execute(
        "UPDATE projects SET stack_json = ?2, remote_url = COALESCE(?3, remote_url) WHERE id = ?1",
        params![id, serde_json::to_string(&stack).unwrap_or_else(|_| "{}".into()), remote],
    )?;
    get(conn, id)
}

/// Remove the project from the workspace manifest and this machine's registry. Never deletes files.
pub fn unregister(conn: &Connection, id: &str) -> Result<()> {
    let project = get(conn, id)?;
    let ws = workspace::get(conn, &project.workspace_id)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM projects WHERE id = ?1", [id])?;
    if ws.available {
        let mut manifest = workspace::read_manifest(&ws.root_path)?;
        manifest.projects.retain(|p| p.id != id);
        workspace::write_manifest(&ws.root_path, &manifest)?;
    }
    tx.commit()?;
    Ok(())
}

pub fn touch_opened(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("UPDATE projects SET last_opened_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Connection, workspace::Workspace) {
        let t = tempfile::tempdir().unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let ws = workspace::create(&conn, t.path().join("ws").to_str().unwrap(), "WS", "").unwrap();
        (t, conn, ws)
    }

    #[test]
    fn register_inside_workspace_writes_relative_path() {
        let (_t, conn, ws) = setup();
        let app = ws.root_path.join("repos").join("game");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("project.godot"), "").unwrap();
        let p = register(&conn, &ws.id, app.to_str().unwrap(), None).unwrap();
        assert_eq!(p.name, "game");
        assert!(p.available);
        assert!(p.stack.stacks.contains(&"godot".into()));
        let m = workspace::read_manifest(&ws.root_path).unwrap();
        assert_eq!(m.projects[0].path.as_deref(), Some("repos/game"));
        assert_eq!(m.projects[0].id, p.id);
    }

    #[test]
    fn register_outside_workspace_keeps_path_machine_local() {
        let (t, conn, ws) = setup();
        let ext = t.path().join("elsewhere");
        std::fs::create_dir_all(&ext).unwrap();
        let p = register(&conn, &ws.id, ext.to_str().unwrap(), Some("Backend")).unwrap();
        let m = workspace::read_manifest(&ws.root_path).unwrap();
        assert_eq!(m.projects[0].path, None, "absolute machine paths never enter the manifest");
        assert_eq!(p.name, "Backend");
        let raw = std::fs::read_to_string(ws.root_path.join(workspace::MANIFEST_FILE)).unwrap();
        assert!(!raw.contains("elsewhere"));
    }

    #[test]
    fn duplicates_and_workspace_root_rejected() {
        let (t, conn, ws) = setup();
        let ext = t.path().join("x");
        std::fs::create_dir_all(&ext).unwrap();
        register(&conn, &ws.id, ext.to_str().unwrap(), None).unwrap();
        assert_eq!(register(&conn, &ws.id, ext.to_str().unwrap(), None).unwrap_err().code(), "conflict");
        assert_eq!(
            register(&conn, &ws.id, ws.root_path.to_str().unwrap(), None).unwrap_err().code(),
            "invalid"
        );
    }

    #[test]
    fn unregister_removes_registration_but_not_files() {
        let (t, conn, ws) = setup();
        let ext = t.path().join("keep-me");
        std::fs::create_dir_all(&ext).unwrap();
        let p = register(&conn, &ws.id, ext.to_str().unwrap(), None).unwrap();
        unregister(&conn, &p.id).unwrap();
        assert!(list(&conn, &ws.id).unwrap().is_empty());
        assert!(workspace::read_manifest(&ws.root_path).unwrap().projects.is_empty());
        assert!(ext.is_dir());
    }

    #[test]
    fn locate_binds_missing_project() {
        let (t, conn, ws) = setup();
        let ext = t.path().join("orig");
        std::fs::create_dir_all(&ext).unwrap();
        let p = register(&conn, &ws.id, ext.to_str().unwrap(), None).unwrap();
        // New machine: manifest has the project, but no local path.
        let conn2 = crate::db::open_in_memory().unwrap();
        workspace::import(&conn2, ws.root_path.to_str().unwrap()).unwrap();
        let missing = get(&conn2, &p.id).unwrap();
        assert!(!missing.available);
        let found = locate(&conn2, &p.id, ext.to_str().unwrap()).unwrap();
        assert!(found.available);
    }
}

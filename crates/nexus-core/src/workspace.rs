//! Workspaces: a folder with a portable manifest (`nexus-workspace.json`) plus a machine-local
//! binding in SQLite (ADR-0005).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::paths::{self, Expect};
use crate::{new_id, now_ms, settings};

pub const MANIFEST_FILE: &str = "nexus-workspace.json";
pub const MANIFEST_SCHEMA: u32 = 1;
const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;

/// Portable workspace manifest. Unknown fields are preserved so newer NEXUS versions can add data
/// without older versions destroying it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub projects: Vec<ManifestProject>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManifestProject {
    pub id: String,
    pub name: String,
    /// Remote URL with any embedded credentials removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    /// Path relative to the workspace root (forward slashes) when the project lives inside it.
    /// Projects elsewhere on disk have no portable path; their location is machine-local (SQLite).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub description: String,
    pub root_path: PathBuf,
    pub created_at: i64,
    pub last_opened_at: Option<i64>,
    pub project_count: i64,
    /// False when the folder or its manifest is missing on this machine.
    pub available: bool,
}

const WORKSPACE_GITIGNORE: &str = "\
# NEXUS workspace
# Project repositories cloned into repos/ are independent Git repositories: never commit them here.
repos/

# Machine-local runtime data never belongs in a workspace repository.
*.db
*.db-wal
*.db-shm
.env
.env.*
";

fn workspace_readme(name: &str) -> String {
    format!(
        "# {name}\n\nThis folder is a NEXUS workspace.\n\n\
         * `{MANIFEST_FILE}` — portable workspace manifest (projects, settings).\n\
         * `repos/` — default location for cloned project repositories (ignored by Git).\n\n\
         The workspace can be a Git repository so it follows you to other machines. It never contains\n\
         credentials, session transcripts or the NEXUS database.\n"
    )
}

pub fn read_manifest(root: &Path) -> Result<Manifest> {
    let path = root.join(MANIFEST_FILE);
    let meta = std::fs::metadata(&path).map_err(|e| CoreError::io("cannot read", &path, e))?;
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err(CoreError::invalid(format!("{} is too large", path.display())));
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| CoreError::io("cannot read", &path, e))?;
    let manifest: Manifest = serde_json::from_str(&raw).map_err(|source| CoreError::Json {
        what: MANIFEST_FILE.to_string(),
        source,
    })?;
    if manifest.schema_version > MANIFEST_SCHEMA {
        return Err(CoreError::Unsupported(format!(
            "{MANIFEST_FILE} uses schema version {}; this build understands up to {MANIFEST_SCHEMA}",
            manifest.schema_version
        )));
    }
    validate_name(&manifest.name)?;
    uuid::Uuid::parse_str(&manifest.id)
        .map_err(|_| CoreError::invalid(format!("{MANIFEST_FILE} has an invalid workspace id")))?;
    Ok(manifest)
}

/// Write the manifest atomically (temp file in the same directory, then rename).
pub fn write_manifest(root: &Path, manifest: &Manifest) -> Result<()> {
    let mut json = serde_json::to_string_pretty(manifest).map_err(|source| CoreError::Json {
        what: MANIFEST_FILE.to_string(),
        source,
    })?;
    json.push('\n');
    write_atomic(&root.join(MANIFEST_FILE), json.as_bytes())
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or_else(|| CoreError::invalid("path has no parent"))?;
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        new_id()
    ));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp).map_err(|e| CoreError::io("cannot write", &tmp, e))?;
        // Durable before visible: data reaches the disk before the rename publishes it.
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| CoreError::io("cannot write", &tmp, e))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        CoreError::io("cannot replace", path, e)
    })
}

pub fn validate_name(name: &str) -> Result<()> {
    let n = name.trim();
    if n.is_empty() {
        return Err(CoreError::invalid("name must not be empty"));
    }
    if n.chars().count() > 80 {
        return Err(CoreError::invalid("name must be at most 80 characters"));
    }
    if n.chars().any(|c| c.is_control()) {
        return Err(CoreError::invalid("name must not contain control characters"));
    }
    Ok(())
}

/// Create a new workspace in `root` (created if missing; an existing folder must not already be a workspace).
pub fn create(conn: &Connection, root_raw: &str, name: &str, description: &str) -> Result<Workspace> {
    validate_name(name)?;
    let root = paths::validate_user_path(root_raw, Expect::DirOrCreatable)?;
    if root.join(MANIFEST_FILE).exists() {
        return Err(CoreError::Conflict(format!(
            "{} already contains a NEXUS workspace — import it instead",
            root.display()
        )));
    }
    ensure_not_registered(conn, &root)?;
    std::fs::create_dir_all(&root).map_err(|e| CoreError::io("cannot create", &root, e))?;
    let root = dunce::canonicalize(&root).map_err(|e| CoreError::io("cannot access", &root, e))?;

    let manifest = Manifest {
        schema_version: MANIFEST_SCHEMA,
        id: new_id(),
        name: name.trim().to_string(),
        description: description.trim().to_string(),
        projects: Vec::new(),
        extra: Default::default(),
    };
    write_manifest(&root, &manifest)?;
    write_if_missing(&root.join(".gitignore"), WORKSPACE_GITIGNORE)?;
    write_if_missing(&root.join("README.md"), &workspace_readme(&manifest.name))?;
    let repos = root.join("repos");
    std::fs::create_dir_all(&repos).map_err(|e| CoreError::io("cannot create", &repos, e))?;

    insert_row(conn, &manifest, &root)?;
    get(conn, &manifest.id)
}

/// Register an existing workspace folder (e.g. cloned from Git on a new machine).
/// If the same workspace id is already registered at another path, the binding is moved.
pub fn import(conn: &Connection, root_raw: &str) -> Result<Workspace> {
    let root = paths::validate_user_path(root_raw, Expect::ExistingDir)?;
    let manifest = read_manifest(&root)?;
    let existing: Option<String> = conn
        .query_row("SELECT root_path FROM workspaces WHERE id = ?1", [&manifest.id], |r| r.get(0))
        .optional()?;
    match existing {
        Some(_) => {
            conn.execute(
                "UPDATE workspaces SET root_path = ?2, name = ?3, description = ?4, archived_at = NULL WHERE id = ?1",
                params![manifest.id, root.to_string_lossy(), manifest.name, manifest.description],
            )?;
        }
        None => {
            ensure_not_registered(conn, &root)?;
            insert_row(conn, &manifest, &root)?;
        }
    }
    reconcile(conn, &manifest.id)?;
    get(conn, &manifest.id)
}

fn ensure_not_registered(conn: &Connection, root: &Path) -> Result<()> {
    for (id, path) in all_roots(conn)? {
        if paths::same_path(&path, root) {
            return Err(CoreError::Conflict(format!(
                "{} is already registered as workspace {id}",
                root.display()
            )));
        }
    }
    Ok(())
}

fn all_roots(conn: &Connection) -> Result<Vec<(String, PathBuf)>> {
    let mut stmt = conn.prepare("SELECT id, root_path FROM workspaces")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, PathBuf::from(r.get::<_, String>(1)?))))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn insert_row(conn: &Connection, m: &Manifest, root: &Path) -> Result<()> {
    conn.execute(
        "INSERT INTO workspaces(id, name, description, root_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![m.id, m.name, m.description, root.to_string_lossy(), now_ms()],
    )?;
    Ok(())
}

fn write_if_missing(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    std::fs::write(path, contents).map_err(|e| CoreError::io("cannot write", path, e))
}

const SELECT: &str = "SELECT w.id, w.name, w.description, w.root_path, w.created_at, w.last_opened_at,
    (SELECT COUNT(*) FROM projects p WHERE p.workspace_id = w.id) FROM workspaces w";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Workspace> {
    let root = PathBuf::from(r.get::<_, String>(3)?);
    let available = root.join(MANIFEST_FILE).is_file();
    Ok(Workspace {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        root_path: root,
        created_at: r.get(4)?,
        last_opened_at: r.get(5)?,
        project_count: r.get(6)?,
        available,
    })
}

pub fn list(conn: &Connection) -> Result<Vec<Workspace>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE w.archived_at IS NULL ORDER BY COALESCE(w.last_opened_at, w.created_at) DESC"
    ))?;
    let rows = stmt.query_map([], from_row)?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> Result<Workspace> {
    conn.query_row(&format!("{SELECT} WHERE w.id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| CoreError::not_found(format!("workspace {id} not found")))
}

/// Make `id` the active workspace and reconcile its manifest into the database.
pub fn activate(conn: &Connection, id: &str) -> Result<Workspace> {
    let ws = get(conn, id)?;
    if ws.available {
        reconcile(conn, id)?;
    }
    conn.execute("UPDATE workspaces SET last_opened_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
    settings::set(conn, settings::ACTIVE_WORKSPACE, &id)?;
    get(conn, id)
}

pub fn active(conn: &Connection) -> Result<Option<Workspace>> {
    let Some(id) = settings::get::<String>(conn, settings::ACTIVE_WORKSPACE)? else {
        return Ok(None);
    };
    match get(conn, &id) {
        Ok(ws) => Ok(Some(ws)),
        Err(CoreError::NotFound(_)) => {
            settings::remove(conn, settings::ACTIVE_WORKSPACE)?;
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// Remove the workspace registration from this machine. Never deletes any files.
pub fn forget(conn: &Connection, id: &str) -> Result<()> {
    let n = conn.execute("DELETE FROM workspaces WHERE id = ?1", [id])?;
    if n == 0 {
        return Err(CoreError::not_found(format!("workspace {id} not found")));
    }
    if settings::get::<String>(conn, settings::ACTIVE_WORKSPACE)?.as_deref() == Some(id) {
        settings::remove(conn, settings::ACTIVE_WORKSPACE)?;
    }
    Ok(())
}

/// Bring the SQLite project bindings in line with the manifest (the manifest is authoritative for
/// membership). New entries are added (with a local path when the manifest path exists on this machine),
/// names/remotes are refreshed, rows no longer in the manifest are removed. Files are never touched.
pub fn reconcile(conn: &Connection, id: &str) -> Result<()> {
    let ws = get(conn, id)?;
    let manifest = read_manifest(&ws.root_path)?;
    if manifest.id != ws.id {
        return Err(CoreError::Conflict(format!(
            "the manifest in {} belongs to a different workspace",
            ws.root_path.display()
        )));
    }
    let tx = conn.unchecked_transaction()?;
    let mut keep = Vec::new();
    for p in &manifest.projects {
        if uuid::Uuid::parse_str(&p.id).is_err() || validate_name(&p.name).is_err() {
            tracing::warn!(project = %p.id, "skipping invalid manifest project entry");
            continue;
        }
        keep.push(p.id.clone());
        let manifest_path = p
            .path
            .as_deref()
            .and_then(|rel| paths::resolve_within(&ws.root_path, rel).ok())
            .filter(|abs| abs.is_dir());
        let exists: bool = tx
            .query_row("SELECT 1 FROM projects WHERE id = ?1", [&p.id], |_| Ok(true))
            .optional()?
            .unwrap_or(false);
        if exists {
            tx.execute(
                "UPDATE projects SET name = ?2, remote_url = ?3,
                    local_path = COALESCE(?4, local_path), workspace_id = ?5 WHERE id = ?1",
                params![p.id, p.name, p.remote_url, manifest_path.as_ref().map(|x| x.to_string_lossy()), ws.id],
            )?;
        } else {
            tx.execute(
                "INSERT INTO projects(id, workspace_id, name, local_path, remote_url, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    p.id,
                    ws.id,
                    p.name,
                    manifest_path.as_ref().map(|x| x.to_string_lossy()),
                    p.remote_url,
                    now_ms()
                ],
            )?;
        }
    }
    let existing: Vec<String> = {
        let mut stmt = tx.prepare("SELECT id FROM projects WHERE workspace_id = ?1")?;
        stmt.query_map([&ws.id], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?
    };
    for pid in existing.iter().filter(|e| !keep.contains(e)) {
        tx.execute("DELETE FROM projects WHERE id = ?1", [pid])?;
    }
    tx.execute(
        "UPDATE workspaces SET name = ?2, description = ?3 WHERE id = ?1",
        params![ws.id, manifest.name, manifest.description],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Connection) {
        (tempfile::tempdir().unwrap(), crate::db::open_in_memory().unwrap())
    }

    #[test]
    fn create_writes_manifest_gitignore_and_row() {
        let (t, conn) = setup();
        let root = t.path().join("my-ws");
        let ws = create(&conn, root.to_str().unwrap(), "  Personal  ", "side projects").unwrap();
        assert_eq!(ws.name, "Personal");
        assert!(ws.available);
        let m = read_manifest(&ws.root_path).unwrap();
        assert_eq!(m.id, ws.id);
        assert_eq!(m.schema_version, MANIFEST_SCHEMA);
        let gi = std::fs::read_to_string(ws.root_path.join(".gitignore")).unwrap();
        assert!(gi.lines().any(|l| l == "repos/"), "repos/ must be ignored");
        assert!(ws.root_path.join("repos").is_dir());
        assert_eq!(list(&conn).unwrap().len(), 1);
    }

    #[test]
    fn create_refuses_existing_workspace_and_duplicates() {
        let (t, conn) = setup();
        let root = t.path().join("ws");
        create(&conn, root.to_str().unwrap(), "A", "").unwrap();
        let err = create(&conn, root.to_str().unwrap(), "B", "").unwrap_err();
        assert_eq!(err.code(), "conflict");
    }

    #[test]
    fn import_on_new_machine_and_relocation() {
        let (t, conn) = setup();
        let root = t.path().join("ws");
        let ws = create(&conn, root.to_str().unwrap(), "Work", "").unwrap();

        // Simulate another machine: fresh DB, same folder.
        let conn2 = crate::db::open_in_memory().unwrap();
        let imported = import(&conn2, root.to_str().unwrap()).unwrap();
        assert_eq!(imported.id, ws.id);

        // Move the folder: importing again rebinds the same id.
        let moved = t.path().join("ws-moved");
        std::fs::rename(&root, &moved).unwrap();
        let rebound = import(&conn2, moved.to_str().unwrap()).unwrap();
        assert_eq!(rebound.id, ws.id);
        assert_eq!(list(&conn2).unwrap().len(), 1);
    }

    #[test]
    fn import_rejects_bad_manifests() {
        let (t, conn) = setup();
        std::fs::write(t.path().join(MANIFEST_FILE), "{not json").unwrap();
        assert_eq!(import(&conn, t.path().to_str().unwrap()).unwrap_err().code(), "json");
        std::fs::write(
            t.path().join(MANIFEST_FILE),
            r#"{"schemaVersion": 99, "id": "8f0e6d34-5b4b-4b8a-9a43-5a3b0d0b3c11", "name": "x"}"#,
        )
        .unwrap();
        assert_eq!(import(&conn, t.path().to_str().unwrap()).unwrap_err().code(), "unsupported");
    }

    #[test]
    fn manifest_preserves_unknown_fields() {
        let (t, conn) = setup();
        let ws = create(&conn, t.path().join("w").to_str().unwrap(), "W", "").unwrap();
        let mut raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(ws.root_path.join(MANIFEST_FILE)).unwrap()).unwrap();
        raw["futureFeature"] = serde_json::json!({"enabled": true});
        std::fs::write(ws.root_path.join(MANIFEST_FILE), raw.to_string()).unwrap();
        let m = read_manifest(&ws.root_path).unwrap();
        write_manifest(&ws.root_path, &m).unwrap();
        let again = std::fs::read_to_string(ws.root_path.join(MANIFEST_FILE)).unwrap();
        assert!(again.contains("futureFeature"));
    }

    #[test]
    fn reconcile_follows_manifest_and_blocks_escapes() {
        let (t, conn) = setup();
        let ws = create(&conn, t.path().join("w").to_str().unwrap(), "W", "").unwrap();
        std::fs::create_dir_all(ws.root_path.join("repos").join("app")).unwrap();
        let mut m = read_manifest(&ws.root_path).unwrap();
        let good = new_id();
        let evil = new_id();
        m.projects.push(ManifestProject {
            id: good.clone(),
            name: "App".into(),
            remote_url: None,
            path: Some("repos/app".into()),
            extra: Default::default(),
        });
        m.projects.push(ManifestProject {
            id: evil.clone(),
            name: "Evil".into(),
            remote_url: None,
            path: Some("../../outside".into()),
            extra: Default::default(),
        });
        write_manifest(&ws.root_path, &m).unwrap();
        reconcile(&conn, &ws.id).unwrap();

        let path_of = |id: &str| -> Option<String> {
            conn.query_row("SELECT local_path FROM projects WHERE id = ?1", [id], |r| r.get(0)).unwrap()
        };
        assert!(path_of(&good).unwrap().ends_with("app"));
        assert_eq!(path_of(&evil), None, "escaping path must not bind");

        m.projects.retain(|p| p.id == good);
        write_manifest(&ws.root_path, &m).unwrap();
        reconcile(&conn, &ws.id).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn activate_and_forget() {
        let (t, conn) = setup();
        let ws = create(&conn, t.path().join("w").to_str().unwrap(), "W", "").unwrap();
        assert!(active(&conn).unwrap().is_none());
        activate(&conn, &ws.id).unwrap();
        assert_eq!(active(&conn).unwrap().unwrap().id, ws.id);
        forget(&conn, &ws.id).unwrap();
        assert!(active(&conn).unwrap().is_none());
        assert!(ws.root_path.join(MANIFEST_FILE).exists(), "forget must not delete files");
    }
}

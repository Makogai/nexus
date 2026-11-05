//! Workspace schema migrations (ADR-0036): an explained plan first, then a recoverable apply. Every file the migration
//! changes is copied to `<data dir>/workspace-backups/<workspace id>/<time>/` before anything is written; when a step
//! fails the copies are put back. Nothing is moved or deleted — older folders (`repos/`, top-level `tasks/`) keep
//! working after the migration.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use super::{
    LEGACY_PROJECTS_DIR, Layout, MANIFEST_FILE, MANIFEST_SCHEMA, Manifest, NEXUS_VERSION, WORKSPACE_GITIGNORE, Workspace, compat, read_manifest,
    write_atomic,
};
use crate::error::{CoreError, Result};
use crate::now_ms;

pub const BACKUP_DIR: &str = "workspace-backups";
/// Lines every schema 2 workspace ignores (appended to an existing `.gitignore`, never rewritten).
const REQUIRED_IGNORES: &[&str] = &["projects/", "repos/", ".nexus-local/"];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// Workspace-relative path, forward slashes.
    pub path: String,
    /// `update`, `append` or `create`.
    pub action: &'static str,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub workspace_id: String,
    pub from: u32,
    pub to: u32,
    pub changes: Vec<Change>,
    /// What stays as it is and what to know before applying.
    pub notes: Vec<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Migrated {
    pub plan: Plan,
    /// Where the previous versions of the changed files were copied (None when nothing changed).
    pub backup_dir: Option<PathBuf>,
}

fn missing_ignores(root: &Path) -> (bool, Vec<&'static str>) {
    let path = root.join(".gitignore");
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let have: Vec<&str> = text.lines().map(str::trim).collect();
            (true, REQUIRED_IGNORES.iter().copied().filter(|l| !have.contains(l) && !have.contains(&l.trim_end_matches('/'))).collect())
        }
        Err(_) => (false, REQUIRED_IGNORES.to_vec()),
    }
}

/// What migrating the workspace in `root` to the current schema would change. Read-only.
pub fn plan(root: &Path) -> Result<Plan> {
    let m = read_manifest(root)?;
    let c = compat(&m);
    if !c.writable {
        return Err(CoreError::Unsupported(c.reason.unwrap_or_default()));
    }
    let mut p = Plan { workspace_id: m.id.clone(), from: m.schema_version, to: MANIFEST_SCHEMA, changes: Vec::new(), notes: Vec::new() };
    if m.schema_version >= MANIFEST_SCHEMA {
        p.to = m.schema_version;
        return Ok(p);
    }
    p.changes.push(Change {
        path: MANIFEST_FILE.into(),
        action: "update",
        description: format!(
            "schema {} → {MANIFEST_SCHEMA}: records migratedWith {NEXUS_VERSION} and minNexusVersion {NEXUS_VERSION}; keeps the clone folder `{}/` (layout.projects)",
            m.schema_version,
            m.projects_dir()
        ),
    });
    match missing_ignores(root) {
        (true, missing) if !missing.is_empty() => p.changes.push(Change {
            path: ".gitignore".into(),
            action: "append",
            description: format!("adds {} so clones and local-only knowledge never enter the repository", missing.join(", ")),
        }),
        (false, _) => p.changes.push(Change { path: ".gitignore".into(), action: "create", description: "creates the standard NEXUS .gitignore".into() }),
        _ => {}
    }
    p.notes.push("Nothing is moved or deleted: repos/, tasks/, rules/, knowledge/ and workflows/ keep working as before.".into());
    p.notes.push("New project knowledge goes to project-context/<project>/ from now on; constitutions can live in constitutions/.".into());
    p.notes.push(format!("NEXUS versions before {NEXUS_VERSION} (schema 1) will no longer open this workspace — update NEXUS on your other computers first."));
    p.notes.push("The changed files are copied to the NEXUS data folder first; commit the migration through the Sync Center when you are ready.".into());
    Ok(p)
}

/// Migrate the workspace to the current schema as planned. A no-op when it is already current.
pub fn apply(conn: &Connection, data_dir: &Path, ws: &Workspace) -> Result<Migrated> {
    let root = &ws.root_path;
    let plan = plan(root)?;
    if plan.workspace_id != ws.id {
        return Err(CoreError::Conflict(format!("the manifest in {} belongs to a different workspace", root.display())));
    }
    if plan.is_empty() {
        return Ok(Migrated { plan, backup_dir: None });
    }
    let stamp = crate::knowledge::timestamp(now_ms()).replace(':', "");
    let backup = data_dir.join(BACKUP_DIR).join(&ws.id).join(format!("{stamp}-schema{}to{}", plan.from, plan.to));
    std::fs::create_dir_all(&backup).map_err(|e| CoreError::io("cannot create", &backup, e))?;
    let mut saved: Vec<(PathBuf, Option<PathBuf>)> = Vec::new(); // (workspace file, backup copy if it existed)
    for c in &plan.changes {
        let src = root.join(&c.path);
        let copy = if src.is_file() {
            let dst = backup.join(&c.path);
            std::fs::copy(&src, &dst).map_err(|e| CoreError::io("cannot back up", &src, e))?;
            Some(dst)
        } else {
            None
        };
        saved.push((src, copy));
    }
    let plan_json = serde_json::to_string_pretty(&plan).unwrap_or_default();
    std::fs::write(backup.join("migration.json"), plan_json).map_err(|e| CoreError::io("cannot write", &backup, e))?;

    let result = (|| -> Result<()> {
        let mut m: Manifest = read_manifest(root)?;
        let projects_dir = m.projects_dir().to_string();
        m.schema_version = MANIFEST_SCHEMA;
        m.migrated_with = Some(NEXUS_VERSION.to_string());
        m.min_nexus_version = Some(NEXUS_VERSION.to_string());
        let layout = m.layout.get_or_insert_with(Layout::default);
        layout.projects.get_or_insert(if projects_dir.is_empty() { LEGACY_PROJECTS_DIR.into() } else { projects_dir });
        super::write_manifest(root, &m)?;
        match missing_ignores(root) {
            (true, missing) if !missing.is_empty() => {
                let path = root.join(".gitignore");
                let mut text = std::fs::read_to_string(&path).map_err(|e| CoreError::io("cannot read", &path, e))?;
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str("\n# Added by the NEXUS schema 2 migration\n");
                for l in missing {
                    text.push_str(l);
                    text.push('\n');
                }
                write_atomic(&path, text.as_bytes())?;
            }
            (false, _) => write_atomic(&root.join(".gitignore"), WORKSPACE_GITIGNORE.as_bytes())?,
            _ => {}
        }
        Ok(())
    })();
    if let Err(e) = result {
        for (file, copy) in &saved {
            match copy {
                Some(c) => {
                    let _ = std::fs::copy(c, file);
                }
                None => {
                    let _ = std::fs::remove_file(file);
                }
            }
        }
        return Err(CoreError::invalid(format!("the migration failed and was rolled back (copies in {}): {e}", backup.display())));
    }
    super::reconcile(conn, &ws.id)?;
    Ok(Migrated { plan, backup_dir: Some(backup) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{self, ManifestProject};

    /// A schema 1 workspace as older NEXUS versions wrote it.
    fn schema1(conn: &Connection, root: &Path) -> Workspace {
        std::fs::create_dir_all(root.join("repos").join("app")).unwrap();
        std::fs::write(root.join(".gitignore"), "# mine\nrepos/\n*.db\n").unwrap();
        std::fs::write(
            root.join(MANIFEST_FILE),
            format!(
                r#"{{"schemaVersion":1,"id":"{}","name":"Old","projects":[{{"id":"{}","name":"App","path":"repos/app"}}],"futureField":7}}"#,
                crate::new_id(),
                crate::new_id()
            ),
        )
        .unwrap();
        workspace::import(conn, root.to_str().unwrap()).unwrap()
    }

    #[test]
    fn plan_explains_and_apply_migrates_with_a_backup() {
        let t = tempfile::tempdir().unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let ws = schema1(&conn, &t.path().join("ws"));
        assert!(ws.compat.as_ref().unwrap().needs_migration);
        let p = plan(&ws.root_path).unwrap();
        assert_eq!((p.from, p.to), (1, 2));
        assert_eq!(p.changes.iter().map(|c| (c.path.as_str(), c.action)).collect::<Vec<_>>(), [(MANIFEST_FILE, "update"), (".gitignore", "append")]);
        assert!(p.notes.iter().any(|n| n.contains("Nothing is moved")));
        let before = std::fs::read_to_string(ws.root_path.join(MANIFEST_FILE)).unwrap();

        let data = t.path().join("data");
        let done = apply(&conn, &data, &ws).unwrap();
        let backup = done.backup_dir.unwrap();
        assert_eq!(std::fs::read_to_string(backup.join(MANIFEST_FILE)).unwrap(), before, "the previous manifest is kept");
        assert!(backup.join("migration.json").is_file());
        let m = read_manifest(&ws.root_path).unwrap();
        assert_eq!(m.schema_version, 2);
        assert_eq!(m.projects_dir(), "repos", "the clone folder stays where it is");
        assert_eq!(m.migrated_with.as_deref(), Some(NEXUS_VERSION));
        assert!(m.extra.contains_key("futureField"), "unknown fields survive");
        assert_eq!(m.projects[0].path.as_deref(), Some("repos/app"));
        let gi = std::fs::read_to_string(ws.root_path.join(".gitignore")).unwrap();
        assert!(gi.starts_with("# mine\nrepos/\n*.db\n"), "the person's lines are kept: {gi}");
        assert!(gi.lines().any(|l| l == "projects/") && gi.lines().any(|l| l == ".nexus-local/"));
        assert_eq!(gi.matches("repos/").count(), 1, "no duplicates");
        // Idempotent.
        let again = apply(&conn, &data, &workspace::get(&conn, &ws.id).unwrap()).unwrap();
        assert!(again.plan.is_empty() && again.backup_dir.is_none());
        assert!(!workspace::get(&conn, &ws.id).unwrap().compat.unwrap().needs_migration);
    }

    #[test]
    fn a_failed_step_puts_everything_back() {
        let t = tempfile::tempdir().unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let ws = schema1(&conn, &t.path().join("ws"));
        let before = std::fs::read_to_string(ws.root_path.join(MANIFEST_FILE)).unwrap();
        // A directory where the .gitignore temp file must go cannot be replaced: make .gitignore a folder.
        std::fs::remove_file(ws.root_path.join(".gitignore")).unwrap();
        std::fs::create_dir(ws.root_path.join(".gitignore")).unwrap();
        let err = apply(&conn, &t.path().join("data"), &ws).unwrap_err();
        assert!(err.to_string().contains("rolled back"), "{err}");
        assert_eq!(std::fs::read_to_string(ws.root_path.join(MANIFEST_FILE)).unwrap(), before, "the manifest is restored");
    }

    #[test]
    fn newer_workspaces_open_read_only_and_refuse_writes() {
        let t = tempfile::tempdir().unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let root = t.path().join("future");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(MANIFEST_FILE), format!(r#"{{"schemaVersion":7,"id":"{}","name":"Future","newThing":{{"a":1}}}}"#, crate::new_id())).unwrap();
        let ws = workspace::import(&conn, root.to_str().unwrap()).unwrap();
        let c = ws.compat.clone().unwrap();
        assert!(!c.writable && c.reason.as_deref().unwrap().contains("read-only"), "{c:?}");
        assert_eq!(plan(&root).unwrap_err().code(), "unsupported");
        let mut m = read_manifest(&root).unwrap();
        m.projects.push(ManifestProject { id: crate::new_id(), name: "x".into(), remote_url: None, path: None, context: None, extra: Default::default() });
        assert_eq!(workspace::write_manifest(&root, &m).unwrap_err().code(), "unsupported");
        assert!(workspace::ensure_writable_at(&root.join("rules").join("a.md")).is_err());
        // A minimum version above this NEXUS has the same effect.
        std::fs::write(root.join(MANIFEST_FILE), format!(r#"{{"schemaVersion":2,"id":"{}","name":"Future","minNexusVersion":"99.0.0"}}"#, ws.id)).unwrap();
        assert!(!workspace::get(&conn, &ws.id).unwrap().compat.unwrap().writable);
    }

    #[test]
    fn versions_compare_numerically() {
        use std::cmp::Ordering::*;
        use workspace::version_cmp as v;
        assert_eq!(v("0.10.0", "0.9.9"), Greater);
        assert_eq!(v("1.0", "1.0.0"), Equal);
        assert_eq!(v("1.0.0-beta.1", "1.0.0"), Less);
        assert_eq!(v("v2.1.0", "2.1.0"), Equal);
        assert_eq!(v("0.1.0", "0.1.0+build.5"), Equal);
    }
}

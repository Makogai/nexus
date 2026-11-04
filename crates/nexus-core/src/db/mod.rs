//! SQLite storage: connection setup and forward-only migrations tracked by `PRAGMA user_version`
//! (ADR-0002).

use std::path::Path;
use std::time::Duration;

pub use rusqlite::Connection;

use crate::error::{CoreError, Result};

pub const DB_FILE: &str = "nexus.db";

/// Ordered migrations. Index `i` upgrades the schema from version `i` to `i + 1`.
/// Never edit a released migration; append a new one.
const MIGRATIONS: &[&str] = &[include_str!("migrations/0001_initial.sql")];

/// Schema version this build understands.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Open (creating if needed) the database at `path`, configure it and apply pending migrations.
/// A copy of an existing database is written to `<dir>/backups/` before it is upgraded.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| CoreError::io("cannot create", dir, e))?;
    }
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    let backup_dir = path.parent().map(|d| d.join("backups"));
    migrate(&mut conn, backup_dir.as_deref())?;
    Ok(conn)
}

/// In-memory database with the full schema (tests).
pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&mut conn, None)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<()> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // journal_mode returns a row; in-memory databases report "memory".
    let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

pub fn schema_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

fn migrate(conn: &mut Connection, backup_dir: Option<&Path>) -> Result<()> {
    let current = schema_version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(CoreError::Unsupported(format!(
            "the NEXUS database uses schema version {current}, but this build only understands up to \
             {SCHEMA_VERSION}. Update NEXUS to open it."
        )));
    }
    if current == SCHEMA_VERSION {
        return Ok(());
    }
    if current > 0 {
        if let Some(dir) = backup_dir {
            backup(conn, dir, current)?;
        }
    }
    for (idx, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let target = idx as i64 + 1;
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", target)?;
        tx.commit()?;
        tracing::info!(target, "applied database migration");
    }
    Ok(())
}

fn backup(conn: &Connection, dir: &Path, version: i64) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| CoreError::io("cannot create", dir, e))?;
    let file = dir.join(format!("nexus-v{version}-{}.db", crate::now_ms()));
    conn.execute("VACUUM INTO ?1", [file.to_string_lossy()])?;
    Ok(())
}

/// Monotonic counter that changes when *another* connection commits (cheap change detection, ADR-0009).
pub fn data_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA data_version", [], |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_has_current_schema() {
        let conn = open_in_memory().unwrap();
        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION);
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        for t in ["app_settings", "claude_profiles", "projects", "sessions", "workspaces"] {
            assert!(tables.iter().any(|x| x == t), "missing table {t}");
        }
        let fk: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn reopen_is_idempotent_and_uses_wal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        drop(open(&path).unwrap());
        let conn = open(&path).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION);
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
        assert!(!dir.path().join("backups").exists(), "no backup when nothing was migrated");
    }

    #[test]
    fn refuses_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        {
            let conn = open(&path).unwrap();
            conn.pragma_update(None, "user_version", SCHEMA_VERSION + 5).unwrap();
        }
        let err = open(&path).unwrap_err();
        assert_eq!(err.code(), "unsupported");
    }

    #[test]
    fn data_version_changes_on_foreign_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        let a = open(&path).unwrap();
        let b = open(&path).unwrap();
        let before = data_version(&a).unwrap();
        b.execute("INSERT INTO app_settings(key, value_json) VALUES ('k', '1')", []).unwrap();
        assert_ne!(before, data_version(&a).unwrap());
    }
}

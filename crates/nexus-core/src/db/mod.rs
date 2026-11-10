//! SQLite storage: connection setup, forward-only migrations tracked by `PRAGMA user_version`, backups and
//! recovery from unreadable databases (ADR-0002).

use std::path::{Path, PathBuf};
use std::time::Duration;

pub use rusqlite::Connection;
use rusqlite::ErrorCode;
use serde::Serialize;

use crate::error::{CoreError, Result};

pub const DB_FILE: &str = "nexus.db";
/// `PRAGMA application_id` of a NEXUS database ("NXUS"): another program's SQLite file is never migrated.
pub const APPLICATION_ID: i64 = 0x4E58_5553;

pub mod backup;
/// Pre-migration backups kept in `<data dir>/backups/`.
pub const BACKUPS_KEPT: usize = 5;

/// One schema step. Migration `i` upgrades version `i` to `i + 1`. Never edit a released migration; append one.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub name: &'static str,
    pub sql: &'static str,
    /// Table rebuilds must run with foreign keys off (otherwise `DROP TABLE` fires ON DELETE actions on
    /// referencing rows). The runner re-checks every constraint with `PRAGMA foreign_key_check` before committing.
    pub disable_foreign_keys: bool,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        name: "0001_initial",
        sql: include_str!("migrations/0001_initial.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0002_session_hosts",
        sql: include_str!("migrations/0002_session_hosts.sql"),
        disable_foreign_keys: true,
    },
    Migration {
        name: "0003_session_events",
        sql: include_str!("migrations/0003_session_events.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0004_session_activity",
        sql: include_str!("migrations/0004_session_activity.sql"),
        disable_foreign_keys: true,
    },
    Migration {
        name: "0005_knowledge_index",
        sql: include_str!("migrations/0005_knowledge_index.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0006_task_sessions_branches",
        sql: include_str!("migrations/0006_task_sessions_branches.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0007_profile_isolation",
        sql: include_str!("migrations/0007_profile_isolation.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0008_memory_proposals",
        sql: include_str!("migrations/0008_memory_proposals.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0009_kits",
        sql: include_str!("migrations/0009_kits.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0010_notifications",
        sql: include_str!("migrations/0010_notifications.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0011_context",
        sql: include_str!("migrations/0011_context.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0012_workflows",
        sql: include_str!("migrations/0012_workflows.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0013_gate_approval",
        sql: include_str!("migrations/0013_gate_approval.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0014_usage",
        sql: include_str!("migrations/0014_usage.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0015_sync",
        sql: include_str!("migrations/0015_sync.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0016_workspace_kits",
        sql: include_str!("migrations/0016_workspace_kits.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0017_development_records",
        sql: include_str!("migrations/0017_development_records.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0018_push_requests",
        sql: include_str!("migrations/0018_push_requests.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0019_learn_folders",
        sql: include_str!("migrations/0019_learn_folders.sql"),
        disable_foreign_keys: false,
    },
    Migration {
        name: "0020_push_request_failure",
        sql: include_str!("migrations/0020_push_request_failure.sql"),
        disable_foreign_keys: false,
    },
];

/// Schema version this build understands.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// What happened when an unreadable database was found at startup.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Recovery {
    /// Where the unreadable database was moved (it is kept, never deleted).
    pub quarantined_to: PathBuf,
    pub reason: String,
    /// The newest restorable backup, if any (offered for restoring).
    pub latest_backup: Option<String>,
}

/// Open (creating if needed) the database at `path`, configure it and apply pending migrations.
/// A copy of an existing database is written to `<dir>/backups/` before it is upgraded.
pub fn open(path: &Path) -> Result<Connection> {
    open_with(path, MIGRATIONS)
}

fn open_with(path: &Path, migrations: &[Migration]) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| CoreError::io("cannot create", dir, e))?;
    }
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    let app: i64 = conn.query_row("PRAGMA application_id", [], |r| r.get(0))?;
    if app != 0 && app != APPLICATION_ID {
        return Err(CoreError::Unsupported(format!("{} is not a NEXUS database (it belongs to another program); NEXUS leaves it unchanged", path.display())));
    }
    integrity_check(&conn)?;
    let backup_dir = path.parent().map(|d| d.join("backups"));
    migrate_with(&mut conn, migrations, backup_dir.as_deref())?;
    if app == 0 {
        conn.pragma_update(None, "application_id", APPLICATION_ID)?;
    }
    Ok(conn)
}

/// Like [`open`], but an unreadable or corrupt database is moved aside (`nexus.db.corrupt-<ms>`, together with its
/// `-wal`/`-shm` files) and a fresh database is created. Nothing is deleted. A database written by a *newer*
/// NEXUS is not corruption: that error is returned unchanged.
pub fn open_or_recover(path: &Path) -> Result<(Connection, Option<Recovery>)> {
    match open(path) {
        Ok(conn) => Ok((conn, None)),
        Err(e) if is_corruption(&e) => {
            let reason = e.to_string();
            let quarantined_to = quarantine(path)?;
            tracing::error!(reason = %reason, moved_to = %quarantined_to.display(), "database unreadable; started fresh");
            let conn = open(path)?;
            let latest_backup = path.parent().and_then(|d| backup::list(d).ok()).and_then(|l| l.into_iter().find(|b| b.restorable).map(|b| b.id));
            Ok((conn, Some(Recovery { quarantined_to, reason, latest_backup })))
        }
        Err(e) => Err(e),
    }
}

fn is_corruption(e: &CoreError) -> bool {
    match e {
        CoreError::Db(rusqlite::Error::SqliteFailure(f, _)) => {
            matches!(f.code, ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt)
        }
        CoreError::Corrupt(_) => true,
        _ => false,
    }
}

fn quarantine(path: &Path) -> Result<PathBuf> {
    let suffix = format!("corrupt-{}", crate::now_ms());
    let target = PathBuf::from(format!("{}.{suffix}", path.display()));
    std::fs::rename(path, &target).map_err(|e| CoreError::io("cannot move aside", path, e))?;
    for side in ["-wal", "-shm"] {
        let from = PathBuf::from(format!("{}{side}", path.display()));
        if from.exists() {
            let to = PathBuf::from(format!("{}{side}.{suffix}", path.display()));
            let _ = std::fs::rename(&from, &to);
        }
    }
    Ok(target)
}

/// Open an existing, current-schema database for a short-lived helper (lifecycle hooks): no integrity check, no
/// migrations, no backups. Returns `None` when the database is missing or on another schema version — the helper
/// then does nothing instead of upgrading or failing a Claude hook.
pub fn open_existing(path: &Path) -> Result<Option<Connection>> {
    if !path.is_file() {
        return Ok(None);
    }
    let conn = Connection::open(path)?;
    configure(&conn)?;
    if schema_version(&conn)? != SCHEMA_VERSION {
        return Ok(None);
    }
    Ok(Some(conn))
}

/// Read-only look at a database file: `(schema version, application id)`; nothing is changed or migrated.
pub fn peek(path: &Path) -> Result<(i64, i64)> {
    let c = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    Ok((c.query_row("PRAGMA user_version", [], |r| r.get(0))?, c.query_row("PRAGMA application_id", [], |r| r.get(0))?))
}

/// In-memory database with the full schema (tests).
pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate_with(&mut conn, MIGRATIONS, None)?;
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

/// `PRAGMA quick_check` — fast structural check (milliseconds for NEXUS-sized databases).
fn integrity_check(conn: &Connection) -> Result<()> {
    let result: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if result.eq_ignore_ascii_case("ok") {
        Ok(())
    } else {
        Err(CoreError::Corrupt(result.chars().take(300).collect()))
    }
}

pub fn schema_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

fn migrate_with(conn: &mut Connection, migrations: &[Migration], backup_dir: Option<&Path>) -> Result<()> {
    let target_version = migrations.len() as i64;
    let current = schema_version(conn)?;
    if current > target_version {
        return Err(CoreError::Unsupported(format!(
            "the NEXUS database uses schema version {current}, but this build only understands up to \
             {target_version}. Update NEXUS to open it."
        )));
    }
    if current == target_version {
        return Ok(());
    }
    if current > 0
        && let Some(dir) = backup_dir
    {
        backup(conn, dir, current)?;
    }
    for (idx, m) in migrations.iter().enumerate().skip(current as usize) {
        let target = idx as i64 + 1;
        if m.disable_foreign_keys {
            // Must be set outside a transaction to take effect.
            conn.pragma_update(None, "foreign_keys", "OFF")?;
        }
        let result = apply(conn, m, target);
        if m.disable_foreign_keys {
            conn.pragma_update(None, "foreign_keys", "ON")?;
        }
        result?;
        tracing::info!(migration = m.name, version = target, "applied database migration");
    }
    Ok(())
}

fn apply(conn: &mut Connection, m: &Migration, target: i64) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(m.sql).map_err(|e| migration_error(m, e))?;
    if m.disable_foreign_keys {
        let violations: i64 = tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
        if violations > 0 {
            // A failed migration is never treated as corruption: the database stays where it is, unchanged.
            return Err(CoreError::Unsupported(format!(
                "database migration {} would leave {violations} foreign-key violation(s); nothing was changed — \
                 the backup taken before the upgrade is in the backups folder",
                m.name
            )));
        }
    }
    tx.pragma_update(None, "user_version", target)?;
    tx.commit()?;
    Ok(())
}

fn migration_error(m: &Migration, e: rusqlite::Error) -> CoreError {
    CoreError::Unsupported(format!(
        "database migration {} failed (nothing was changed; the backup taken before the upgrade is in the backups folder): {e}",
        m.name
    ))
}

fn backup(conn: &Connection, dir: &Path, version: i64) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| CoreError::io("cannot create", dir, e))?;
    let file = dir.join(format!("nexus-v{version}-{}.db", crate::now_ms()));
    conn.execute("VACUUM INTO ?1", [file.to_string_lossy()])?;
    rotate_backups(dir, BACKUPS_KEPT);
    Ok(())
}

/// Keep the `keep` newest `nexus-v*.db` backups (file names embed a millisecond timestamp).
fn rotate_backups(dir: &Path, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut backups: Vec<(i64, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let ts = name.strip_prefix("nexus-v")?.strip_suffix(".db")?.rsplit('-').next()?.parse().ok()?;
            Some((ts, e.path()))
        })
        .collect();
    backups.sort_by_key(|(ts, _)| std::cmp::Reverse(*ts));
    for (_, path) in backups.into_iter().skip(keep) {
        let _ = std::fs::remove_file(path);
    }
}

/// Monotonic counter that changes when *another* connection commits (cheap change detection, ADR-0009).
pub fn data_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA data_version", [], |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M1: Migration = Migration {
        name: "t1",
        sql: "CREATE TABLE a(id INTEGER PRIMARY KEY, v TEXT);",
        disable_foreign_keys: false,
    };
    const M2: Migration = Migration {
        name: "t2",
        sql: "ALTER TABLE a ADD COLUMN w TEXT DEFAULT 'x'; CREATE TABLE b(id INTEGER);",
        disable_foreign_keys: false,
    };
    const M2_BROKEN: Migration = Migration {
        name: "t2-broken",
        sql: "CREATE TABLE b(id INTEGER); INSERT INTO does_not_exist VALUES (1);",
        disable_foreign_keys: false,
    };

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1", [name], |_| Ok(()))
            .is_ok()
    }

    #[test]
    fn fresh_database_has_current_schema() {
        let conn = open_in_memory().unwrap();
        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION);
        for t in ["app_settings", "claude_profiles", "projects", "sessions", "session_events", "session_hosts", "workspaces"] {
            assert!(table_exists(&conn, t), "missing table {t}");
        }
        let fk: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(fk, 1, "foreign keys are re-enabled after table-rebuild migrations");
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
    fn refuses_newer_schema_without_quarantining() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        {
            let conn = open(&path).unwrap();
            conn.pragma_update(None, "user_version", SCHEMA_VERSION + 5).unwrap();
        }
        assert_eq!(open(&path).unwrap_err().code(), "unsupported");
        assert_eq!(open_or_recover(&path).unwrap_err().code(), "unsupported");
        assert!(path.exists(), "a newer database is never moved aside");
    }

    #[test]
    fn incremental_upgrade_preserves_data_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        {
            let conn = open_with(&path, &[M1]).unwrap();
            conn.execute("INSERT INTO a(v) VALUES ('kept')", []).unwrap();
        }
        let conn = open_with(&path, &[M1, M2]).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 2);
        let (v, w): (String, String) = conn.query_row("SELECT v, w FROM a", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((v.as_str(), w.as_str()), ("kept", "x"));
        let backups: Vec<_> = std::fs::read_dir(dir.path().join("backups")).unwrap().flatten().collect();
        assert_eq!(backups.len(), 1);
        let old = Connection::open(backups[0].path()).unwrap();
        assert_eq!(schema_version(&old).unwrap(), 1, "backup is the pre-upgrade database");
    }

    #[test]
    fn failed_migration_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        drop(open_with(&path, &[M1]).unwrap());
        let err = open_with(&path, &[M1, M2_BROKEN]).unwrap_err();
        assert!(err.to_string().contains("t2-broken"), "{err}");
        let conn = Connection::open(&path).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 1);
        assert!(!table_exists(&conn, "b"), "partial migration must be rolled back");
    }

    #[test]
    fn table_rebuild_migration_rejects_foreign_key_violations() {
        let parent = Migration {
            name: "parent",
            sql: "CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(id INTEGER, p INTEGER REFERENCES p(id));
                  INSERT INTO p VALUES (1); INSERT INTO c VALUES (1, 1);",
            disable_foreign_keys: false,
        };
        let bad_rebuild = Migration {
            name: "bad-rebuild",
            sql: "DELETE FROM p;",
            disable_foreign_keys: true,
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        drop(open_with(&path, &[parent]).unwrap());
        let err = open_with(&path, &[parent, bad_rebuild]).unwrap_err();
        assert!(err.to_string().contains("foreign-key"), "{err}");
        let conn = Connection::open(&path).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM p", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "violating rebuild rolled back");
    }

    #[test]
    fn failed_migrations_and_foreign_databases_are_never_started_fresh() {
        // A migration failure is not corruption: no quarantine, no fresh database, the data stays.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        {
            let conn = open(&path).unwrap();
            crate::workspace::create(&conn, dir.path().join("ws").to_str().unwrap(), "Kept", "").unwrap();
        }
        let conn = Connection::open(&path).unwrap();
        let app: i64 = conn.query_row("PRAGMA application_id", [], |r| r.get(0)).unwrap();
        assert_eq!(app, APPLICATION_ID, "NEXUS databases are marked");
        drop(conn);
        // Another program's SQLite file is refused and left as it is.
        let foreign = dir.path().join("other.db");
        {
            let c = Connection::open(&foreign).unwrap();
            c.execute_batch("PRAGMA application_id = 7; CREATE TABLE t(a); INSERT INTO t VALUES (1);").unwrap();
        }
        let e = open_or_recover(&foreign).unwrap_err();
        assert!(e.to_string().contains("not a NEXUS database"), "{e}");
        assert!(foreign.is_file() && std::fs::read_dir(dir.path()).unwrap().flatten().all(|f| !f.file_name().to_string_lossy().contains("corrupt")));
        let c = Connection::open(&foreign).unwrap();
        assert_eq!(c.query_row("SELECT COUNT(*) FROM t", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    }

    #[test]
    fn v1_sessions_survive_the_v2_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        {
            let conn = open_with(&path, &MIGRATIONS[..1]).unwrap();
            conn.execute_batch(
                "INSERT INTO sessions(id, kind, title, cwd, program, state, started_at) VALUES
                    ('a', 'claude', 'first', '/x', 'claude', 'exited', 1);
                 INSERT INTO sessions(id, kind, title, cwd, program, state, resumed_from, started_at) VALUES
                    ('b', 'claude', 'resumed', '/x', 'claude', 'running', 'a', 2);",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        let (resumed_from, host_id): (Option<String>, Option<String>) = conn
            .query_row("SELECT resumed_from, host_id FROM sessions WHERE id = 'b'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(resumed_from.as_deref(), Some("a"), "self-references survive the table rebuild");
        assert_eq!(host_id, None);
        conn.execute("UPDATE sessions SET host = 'service' WHERE id = 'a'", []).unwrap();
        let violations: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0)).unwrap();
        assert_eq!(violations, 0);
    }

    #[test]
    fn open_existing_never_creates_or_migrates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        assert!(open_existing(&path).unwrap().is_none());
        assert!(!path.exists(), "a hook must not create the database");
        drop(open_with(&path, &MIGRATIONS[..1]).unwrap());
        assert!(open_existing(&path).unwrap().is_none(), "older schema: do nothing");
        drop(open(&path).unwrap());
        assert!(open_existing(&path).unwrap().is_some());
    }

    #[test]
    fn checkpoints_survive_the_v4_events_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        {
            let conn = open_with(&path, &MIGRATIONS[..3]).unwrap();
            conn.execute_batch(
                "INSERT INTO sessions(id, kind, title, cwd, program, state, started_at) VALUES ('s', 'claude', 't', '/x', 'c', 'exited', 1);
                 INSERT INTO session_events(id, session_id, kind, summary, source, created_at) VALUES ('e', 's', 'decision', 'kept', 'claude', 2);",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        let (summary, turns): (String, i64) = conn
            .query_row("SELECT e.summary, s.turns FROM session_events e JOIN sessions s ON s.id = e.session_id", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((summary.as_str(), turns), ("kept", 0));
        conn.execute("INSERT INTO session_events(id, session_id, kind, source, created_at) VALUES ('h', 's', 'file', 'hook', 3)", []).unwrap();
    }

    #[test]
    fn backups_are_rotated() {
        let dir = tempfile::tempdir().unwrap();
        for ts in [10, 20, 30, 40, 50, 60, 70] {
            std::fs::write(dir.path().join(format!("nexus-v1-{ts}.db")), "x").unwrap();
        }
        std::fs::write(dir.path().join("unrelated.txt"), "keep").unwrap();
        rotate_backups(dir.path(), 5);
        let mut names: Vec<String> =
            std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["nexus-v1-30.db", "nexus-v1-40.db", "nexus-v1-50.db", "nexus-v1-60.db", "nexus-v1-70.db", "unrelated.txt"]);
    }

    #[test]
    fn garbage_file_is_quarantined_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        std::fs::write(&path, "this is definitely not a SQLite database ".repeat(200)).unwrap();
        let (conn, recovery) = open_or_recover(&path).unwrap();
        let r = recovery.expect("recovery reported");
        assert!(r.quarantined_to.exists(), "unreadable file is kept");
        assert!(std::fs::read_to_string(&r.quarantined_to).unwrap().starts_with("this is definitely"));
        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION, "fresh database created");
    }

    #[test]
    fn damaged_pages_are_detected_and_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE);
        {
            let conn = open(&path).unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            for i in 0..400 {
                tx.execute(
                    "INSERT INTO app_settings(key, value_json) VALUES (?1, ?2)",
                    rusqlite::params![format!("key-{i}"), format!("\"{}\"", "v".repeat(200))],
                )
                .unwrap();
            }
            tx.commit().unwrap();
            conn.pragma_update(None, "journal_mode", "DELETE").unwrap(); // checkpoint everything into the main file
        }
        // Overwrite a span of b-tree pages (keep the 100-byte header so the file still looks like SQLite).
        let mut bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > 16 * 4096);
        for b in &mut bytes[4096 * 2..4096 * 8] {
            *b = 0xA5;
        }
        std::fs::write(&path, &bytes).unwrap();
        let (_conn, recovery) = open_or_recover(&path).unwrap();
        assert!(recovery.is_some(), "corruption must be detected by quick_check");
    }

    /// Drift guard (NX-3): every schema version is described in docs/ARCHITECTURE.md §7 (`-- vN …`).
    #[test]
    fn every_migration_is_described_in_the_architecture_schema() {
        let doc = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/ARCHITECTURE.md")).unwrap();
        let start = doc.find("## 7. SQLite schema").expect("§7 heading");
        let section = &doc[start..start + doc[start..].find("\n## 8.").unwrap()];
        let missing: Vec<String> = (2..=MIGRATIONS.len())
            .filter(|v| !section.match_indices(&format!("-- v{v}")).any(|(i, m)| !section[i + m.len()..].starts_with(|c: char| c.is_ascii_digit())))
            .map(|v| format!("v{v} ({})", MIGRATIONS[v - 1].name))
            .collect();
        assert!(missing.is_empty(), "describe these schema versions in docs/ARCHITECTURE.md §7: {missing:?}");
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

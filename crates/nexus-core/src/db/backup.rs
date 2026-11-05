//! Backups of NEXUS's machine-local state and a safe restore (ADR-0032).
//!
//! A backup is a folder `backups/<timestamp>-<kind>/` with a consistent copy of the database (`VACUUM INTO`), the
//! machine-local global knowledge folder (`global/`) and a `manifest.json` (schema version, NEXUS version, time, kind,
//! SHA-256 of the database copy). Kinds: `manual`, `automatic` (at most one a day, the newest 7 kept),
//! `pre-restore` (taken before every restore). Pre-migration copies (`nexus-v<schema>-<ms>.db`, ADR-0002) are listed
//! too. Workspace folders are not part of a backup — they live in their own folders and Git repositories.
//!
//! A restore never destroys anything: it verifies the backup (a NEXUS database, not corrupt, not newer than this
//! build), takes a `pre-restore` backup of the current state, moves the current database files aside (kept in
//! `backups/<ts>-replaced/`), copies the backup in, and opens it; if that fails, the previous files are put back.
//! The desktop cannot replace the database it has open, so it schedules the restore for its next start.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{APPLICATION_ID, DB_FILE, SCHEMA_VERSION};
use crate::{CoreError, Result, now_ms};

pub const BACKUP_DIR: &str = "backups";
const PENDING_FILE: &str = "restore-pending.json";
const AUTOMATIC_KEPT: usize = 7;
const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub kind: String,
    pub created_at: i64,
    pub schema_version: i64,
    pub app_version: String,
    pub db_sha256: String,
    pub db_bytes: u64,
    pub includes_global: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    /// Folder or file name under `backups/` — what `restore` takes.
    pub id: String,
    pub path: PathBuf,
    /// manual, automatic, pre-restore, pre-migration, replaced
    pub kind: String,
    pub created_at: i64,
    pub schema_version: Option<i64>,
    pub app_version: Option<String>,
    pub bytes: u64,
    /// Can this build restore it (a NEXUS database, intact, not newer)?
    pub restorable: bool,
    pub problem: Option<String>,
}

fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKUP_DIR)
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|e| CoreError::io("cannot read", path, e))?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

fn stamp(ms: i64) -> String {
    // UTC, sortable, file-name safe: 20261105-074512-123
    let (secs, millis) = (ms.div_euclid(1000), ms.rem_euclid(1000));
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}-{millis:03}", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).map_err(|e| CoreError::io("cannot create", to, e))?;
    for e in std::fs::read_dir(from).map_err(|e| CoreError::io("cannot read", from, e))?.flatten() {
        let Ok(t) = e.file_type() else { continue };
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if t.is_dir() {
            copy_dir(&src, &dst)?;
        } else if t.is_file() {
            std::fs::copy(&src, &dst).map_err(|err| CoreError::io("cannot copy", &src, err))?;
        }
    }
    Ok(())
}

/// Take a backup of the open database `conn` (and `global/`) — `kind`: manual, automatic, pre-restore.
pub fn create(conn: &Connection, data_dir: &Path, kind: &str) -> Result<BackupInfo> {
    let created_at = now_ms();
    let id = format!("{}-{kind}", stamp(created_at));
    let dir = backups_dir(data_dir).join(&id);
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::io("cannot create", &dir, e))?;
    let db = dir.join(DB_FILE);
    if let Err(e) = conn.execute("VACUUM INTO ?1", [db.to_string_lossy()]) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e.into());
    }
    let global = data_dir.join("global");
    let includes_global = global.is_dir();
    if includes_global {
        copy_dir(&global, &dir.join("global"))?;
    }
    let manifest = Manifest {
        kind: kind.into(),
        created_at,
        schema_version: super::schema_version(conn)?,
        app_version: env!("CARGO_PKG_VERSION").into(),
        db_sha256: sha256_file(&db)?,
        db_bytes: std::fs::metadata(&db).map(|m| m.len()).unwrap_or(0),
        includes_global,
    };
    crate::workspace::write_atomic(&dir.join("manifest.json"), serde_json::to_string_pretty(&manifest).unwrap_or_default().as_bytes())?;
    if kind == "automatic" {
        rotate(data_dir, "automatic", AUTOMATIC_KEPT);
    }
    Ok(inspect_folder(&id, &dir))
}

/// An automatic backup when the newest one is older than a day (called by the desktop at start).
pub fn automatic_if_due(conn: &Connection, data_dir: &Path) -> Result<Option<BackupInfo>> {
    let newest = list(data_dir)?.into_iter().filter(|b| b.kind == "automatic" || b.kind == "manual").map(|b| b.created_at).max();
    if newest.is_some_and(|t| now_ms() - t < DAY_MS) {
        return Ok(None);
    }
    Ok(Some(create(conn, data_dir, "automatic")?))
}

fn rotate(data_dir: &Path, kind: &str, keep: usize) {
    let Ok(mut all) = list(data_dir) else { return };
    all.retain(|b| b.kind == kind);
    all.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    for b in all.into_iter().skip(keep) {
        let _ = std::fs::remove_dir_all(&b.path);
    }
}

/// Is `db` a database this build can restore? (schema version or the problem)
fn check_db(db: &Path) -> std::result::Result<i64, String> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|e| format!("cannot be opened: {e}"))?;
    let app: i64 = conn.query_row("PRAGMA application_id", [], |r| r.get(0)).map_err(|e| format!("not readable: {e}"))?;
    if app != 0 && app != APPLICATION_ID {
        return Err("not a NEXUS database".into());
    }
    let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0)).map_err(|e| format!("not readable: {e}"))?;
    if !check.eq_ignore_ascii_case("ok") {
        return Err(format!("damaged ({})", check.chars().take(80).collect::<String>()));
    }
    let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(|e| format!("not readable: {e}"))?;
    if v > SCHEMA_VERSION {
        return Err(format!("made by a newer NEXUS (schema {v}; this version understands up to {SCHEMA_VERSION})"));
    }
    if v == 0 {
        return Err("empty — not a NEXUS database".into());
    }
    Ok(v)
}

fn inspect_folder(id: &str, dir: &Path) -> BackupInfo {
    let manifest: Option<Manifest> = std::fs::read_to_string(dir.join("manifest.json")).ok().and_then(|s| serde_json::from_str(&s).ok());
    let db = dir.join(DB_FILE);
    let kind = manifest.as_ref().map(|m| m.kind.clone()).unwrap_or_else(|| id.rsplit('-').next().unwrap_or("manual").to_string());
    let mut info = BackupInfo {
        id: id.into(),
        path: dir.to_path_buf(),
        created_at: manifest.as_ref().map(|m| m.created_at).unwrap_or_else(|| modified_ms(dir)),
        schema_version: manifest.as_ref().map(|m| m.schema_version),
        app_version: manifest.as_ref().map(|m| m.app_version.clone()),
        bytes: std::fs::metadata(&db).map(|m| m.len()).unwrap_or(0),
        restorable: false,
        problem: None,
        kind,
    };
    if info.kind == "replaced" {
        info.problem = Some("the files a restore replaced (kept for safety; not a backup)".into());
        return info;
    }
    if !db.is_file() {
        info.problem = Some("no database in the backup".into());
        return info;
    }
    if let Some(m) = &manifest
        && sha256_file(&db).ok().as_deref() != Some(m.db_sha256.as_str())
    {
        info.problem = Some("the database copy does not match its checksum".into());
        return info;
    }
    match check_db(&db) {
        Ok(v) => {
            info.schema_version = Some(v);
            info.restorable = true;
        }
        Err(p) => info.problem = Some(p),
    }
    info
}

fn modified_ms(p: &Path) -> i64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// All backups, newest first (folders and pre-migration copies).
pub fn list(data_dir: &Path) -> Result<Vec<BackupInfo>> {
    let dir = backups_dir(data_dir);
    let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            out.push(inspect_folder(&name, &e.path()));
        } else if t.is_file() && name.starts_with("nexus-v") && name.ends_with(".db") {
            let ts = name.trim_end_matches(".db").rsplit('-').next().and_then(|s| s.parse().ok()).unwrap_or_else(|| modified_ms(&e.path()));
            let mut info = BackupInfo { id: name.clone(), path: e.path(), kind: "pre-migration".into(), created_at: ts, schema_version: None, app_version: None, bytes: e.metadata().map(|m| m.len()).unwrap_or(0), restorable: false, problem: None };
            match check_db(&e.path()) {
                Ok(v) => {
                    info.schema_version = Some(v);
                    info.restorable = true;
                }
                Err(p) => info.problem = Some(p),
            }
            out.push(info);
        }
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(out)
}

fn resolve(data_dir: &Path, id: &str) -> Result<BackupInfo> {
    if id.is_empty() || id.contains(['/', '\\']) || id.contains("..") {
        return Err(CoreError::invalid("give the backup's id as `nexus backup list` shows it"));
    }
    list(data_dir)?.into_iter().find(|b| b.id == id).ok_or_else(|| CoreError::not_found(format!("no backup {id}")))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Restored {
    pub restored: String,
    /// The backup of the state before the restore.
    pub safety_backup: Option<String>,
    /// Where the replaced database files were kept.
    pub replaced_kept_in: PathBuf,
    pub schema_version: i64,
}

/// Restore backup `id`. **No connection to the database may be open** in this process (the files are replaced);
/// the caller makes sure no other NEXUS program runs (see [`live_hosts`]).
pub fn restore(data_dir: &Path, id: &str) -> Result<Restored> {
    let b = resolve(data_dir, id)?;
    if !b.restorable {
        return Err(CoreError::invalid(format!("backup {id} cannot be restored: {}", b.problem.unwrap_or_default())));
    }
    let source = if b.path.is_dir() { b.path.join(DB_FILE) } else { b.path.clone() };
    let current = data_dir.join(DB_FILE);
    // 1. A safety backup of the current state (if it is readable at all).
    let safety_backup = if current.is_file() {
        match Connection::open(&current).and_then(|c| {
            c.busy_timeout(std::time::Duration::from_secs(5))?;
            Ok(c)
        }) {
            Ok(c) => create(&c, data_dir, "pre-restore").ok().map(|i| i.id),
            Err(_) => None,
        }
    } else {
        None
    };
    // 2. Move the current files aside (kept).
    let replaced = backups_dir(data_dir).join(format!("{}-replaced", stamp(now_ms())));
    std::fs::create_dir_all(&replaced).map_err(|e| CoreError::io("cannot create", &replaced, e))?;
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for suffix in ["", "-wal", "-shm"] {
        let from = PathBuf::from(format!("{}{suffix}", current.display()));
        if from.exists() {
            let to = replaced.join(format!("{DB_FILE}{suffix}"));
            std::fs::rename(&from, &to).map_err(|e| CoreError::io("cannot move aside (is NEXUS still running?)", &from, e))?;
            moved.push((from, to));
        }
    }
    let global = data_dir.join("global");
    let backup_global = b.path.join("global");
    let swap_global = b.path.is_dir() && backup_global.is_dir();
    if swap_global && global.is_dir() {
        let to = replaced.join("global");
        std::fs::rename(&global, &to).map_err(|e| CoreError::io("cannot move aside", &global, e))?;
        moved.push((global.clone(), to));
    }
    let manifest = Manifest { kind: "replaced".into(), created_at: now_ms(), schema_version: 0, app_version: env!("CARGO_PKG_VERSION").into(), db_sha256: String::new(), db_bytes: 0, includes_global: swap_global };
    let _ = crate::workspace::write_atomic(&replaced.join("manifest.json"), serde_json::to_string_pretty(&manifest).unwrap_or_default().as_bytes());
    // 3. Copy the backup in and open it (migrating an older schema, with its own pre-migration copy).
    let result = (|| -> Result<i64> {
        let tmp = data_dir.join(format!("{DB_FILE}.restoring"));
        std::fs::copy(&source, &tmp).map_err(|e| CoreError::io("cannot copy", &source, e))?;
        std::fs::rename(&tmp, &current).map_err(|e| CoreError::io("cannot place", &current, e))?;
        if swap_global {
            copy_dir(&backup_global, &global)?;
        }
        let conn = super::open(&current)?;
        super::schema_version(&conn)
    })();
    match result {
        Ok(v) => Ok(Restored { restored: id.into(), safety_backup, replaced_kept_in: replaced, schema_version: v }),
        Err(e) => {
            // Put the previous state back exactly as it was.
            for suffix in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffix}", current.display()));
            }
            if swap_global {
                let _ = std::fs::remove_dir_all(&global);
            }
            for (from, to) in moved.iter().rev() {
                let _ = std::fs::rename(to, from);
            }
            Err(CoreError::External { program: "restore".into(), message: format!("the backup could not be opened ({e}); the previous database was put back") })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Pending {
    id: String,
    requested_at: i64,
}

/// The desktop's restore: remembered now, applied by [`apply_pending`] when NEXUS starts next.
pub fn schedule_restore(data_dir: &Path, id: &str) -> Result<BackupInfo> {
    let b = resolve(data_dir, id)?;
    if !b.restorable {
        return Err(CoreError::invalid(format!("backup {id} cannot be restored: {}", b.problem.clone().unwrap_or_default())));
    }
    let p = Pending { id: id.into(), requested_at: now_ms() };
    crate::workspace::write_atomic(&data_dir.join(PENDING_FILE), serde_json::to_string(&p).unwrap_or_default().as_bytes())?;
    Ok(b)
}

pub fn pending(data_dir: &Path) -> Option<String> {
    std::fs::read_to_string(data_dir.join(PENDING_FILE)).ok().and_then(|s| serde_json::from_str::<Pending>(&s).ok()).map(|p| p.id)
}

pub fn cancel_pending(data_dir: &Path) {
    let _ = std::fs::remove_file(data_dir.join(PENDING_FILE));
}

/// Apply a scheduled restore before the database is opened. The marker is removed first, so a restore that keeps
/// failing cannot block NEXUS from starting.
pub fn apply_pending(data_dir: &Path) -> Option<Result<Restored>> {
    let id = pending(data_dir)?;
    cancel_pending(data_dir);
    Some(restore(data_dir, &id))
}

pub fn delete(data_dir: &Path, id: &str) -> Result<()> {
    let b = resolve(data_dir, id)?;
    if b.path.is_dir() { std::fs::remove_dir_all(&b.path) } else { std::fs::remove_file(&b.path) }.map_err(|e| CoreError::io("cannot delete", &b.path, e))
}

/// Other NEXUS programs that have the database open right now (desktop, terminal launches), by kind.
pub fn live_hosts(conn: &Connection, except: Option<&str>) -> Result<Vec<String>> {
    let now = now_ms();
    let rows: Vec<(String, String)> = conn
        .prepare("SELECT id, kind FROM session_hosts WHERE stopped_at IS NULL AND heartbeat_at >= ?1")?
        .query_map([now - crate::host::LEASE_MS], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows.into_iter().filter(|(id, _)| Some(id.as_str()) != except).map(|(_, k)| k).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> (tempfile::TempDir, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("data");
        std::fs::create_dir_all(&d).unwrap();
        (t, d)
    }

    #[test]
    fn backups_restore_and_keep_what_they_replace() {
        let (_t, d) = data();
        let db = d.join(DB_FILE);
        {
            let conn = crate::db::open(&db).unwrap();
            crate::workspace::create(&conn, d.parent().unwrap().join("ws-a").to_str().unwrap(), "Before", "").unwrap();
            std::fs::create_dir_all(d.join("global").join("rules")).unwrap();
            std::fs::write(d.join("global").join("rules").join("mine.md"), "before").unwrap();
            let b = create(&conn, &d, "manual").unwrap();
            assert!(b.restorable && b.schema_version == Some(SCHEMA_VERSION), "{b:?}");
            // Changes after the backup.
            crate::workspace::create(&conn, d.parent().unwrap().join("ws-b").to_str().unwrap(), "After", "").unwrap();
            std::fs::write(d.join("global").join("rules").join("mine.md"), "after").unwrap();
        }
        let id = list(&d).unwrap().into_iter().find(|b| b.kind == "manual").unwrap().id;
        let r = restore(&d, &id).unwrap();
        assert!(r.safety_backup.is_some(), "the state before the restore is backed up");
        let conn = crate::db::open(&db).unwrap();
        let names: Vec<String> = crate::workspace::list(&conn).unwrap().into_iter().map(|w| w.name).collect();
        assert_eq!(names, vec!["Before".to_string()]);
        assert_eq!(std::fs::read_to_string(d.join("global").join("rules").join("mine.md")).unwrap(), "before");
        assert!(r.replaced_kept_in.join(DB_FILE).is_file(), "the replaced database is kept");
        drop(conn);
        // The safety backup brings the later state back.
        let safety = r.safety_backup.unwrap();
        restore(&d, &safety).unwrap();
        let conn = crate::db::open(&db).unwrap();
        assert_eq!(crate::workspace::list(&conn).unwrap().len(), 2);
    }

    #[test]
    fn unusable_backups_are_refused_and_failed_restores_put_everything_back() {
        let (_t, d) = data();
        let db = d.join(DB_FILE);
        let conn = crate::db::open(&db).unwrap();
        let b = create(&conn, &d, "manual").unwrap();
        drop(conn);
        // Damaged copy: checksum mismatch.
        std::fs::write(b.path.join(DB_FILE), b"not a database").unwrap();
        let listed = list(&d).unwrap();
        let bad = listed.iter().find(|x| x.id == b.id).unwrap();
        assert!(!bad.restorable && bad.problem.as_deref().unwrap().contains("checksum"));
        assert!(restore(&d, &b.id).unwrap_err().to_string().contains("cannot be restored"));
        // A newer schema (made by a future NEXUS) is refused.
        let future = d.join(BACKUP_DIR).join("20990101-000000-000-manual");
        std::fs::create_dir_all(&future).unwrap();
        {
            let c = Connection::open(future.join(DB_FILE)).unwrap();
            c.execute_batch(&format!("CREATE TABLE x(a); PRAGMA user_version = {};", SCHEMA_VERSION + 1)).unwrap();
        }
        let f = list(&d).unwrap().into_iter().find(|x| x.id.starts_with("20990101")).unwrap();
        assert!(f.problem.unwrap().contains("newer NEXUS"));
        // Not a NEXUS database at all.
        let foreign = d.join(BACKUP_DIR).join("nexus-v3-1.db");
        {
            let c = Connection::open(&foreign).unwrap();
            c.execute_batch("PRAGMA application_id = 42; CREATE TABLE y(a); PRAGMA user_version = 3;").unwrap();
        }
        assert!(list(&d).unwrap().iter().any(|x| x.id == "nexus-v3-1.db" && x.problem.as_deref() == Some("not a NEXUS database")));
        assert!(resolve(&d, "../secret").is_err());
        // Scheduling (the desktop's way) and applying at the next start.
        let conn = crate::db::open(&db).unwrap();
        let good = create(&conn, &d, "manual").unwrap();
        drop(conn);
        schedule_restore(&d, &good.id).unwrap();
        assert_eq!(pending(&d).as_deref(), Some(good.id.as_str()));
        assert!(apply_pending(&d).unwrap().is_ok());
        assert!(pending(&d).is_none() && apply_pending(&d).is_none());
    }

    #[test]
    fn automatic_backups_are_daily_and_rotated() {
        let (_t, d) = data();
        let conn = crate::db::open(&d.join(DB_FILE)).unwrap();
        assert!(automatic_if_due(&conn, &d).unwrap().is_some());
        assert!(automatic_if_due(&conn, &d).unwrap().is_none(), "at most one a day");
        for _ in 0..9 {
            create(&conn, &d, "automatic").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        assert_eq!(list(&d).unwrap().iter().filter(|b| b.kind == "automatic").count(), AUTOMATIC_KEPT);
        assert!(live_hosts(&conn, None).unwrap().is_empty());
    }
}

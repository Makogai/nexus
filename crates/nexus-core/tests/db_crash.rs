//! Crash consistency: a real child process writes to the database and aborts (no destructors, no checkpoint,
//! WAL left behind). Committed data must survive; uncommitted data must not; the database must stay healthy.

use std::path::Path;
use std::process::Command;

use nexus_core::db;

const CHILD_ENV: &str = "NEXUS_TEST_CRASH_DB";

/// Child half: does nothing in a normal test run.
#[test]
fn crash_child() {
    let Ok(path) = std::env::var(CHILD_ENV) else { return };
    let conn = db::open(Path::new(&path)).unwrap();
    conn.execute("INSERT INTO app_settings(key, value_json) VALUES ('committed', '1')", []).unwrap();
    conn.execute_batch("BEGIN; INSERT INTO app_settings(key, value_json) VALUES ('uncommitted', '1');").unwrap();
    std::process::abort();
}

#[test]
fn committed_writes_survive_a_crash_and_uncommitted_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(db::DB_FILE);
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["crash_child", "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, &path)
        .status()
        .unwrap();
    assert!(!status.success(), "child must have aborted");

    let (conn, recovery) = db::open_or_recover(&path).unwrap();
    assert!(recovery.is_none(), "an aborted writer is not corruption");
    let keys: Vec<String> = conn
        .prepare("SELECT key FROM app_settings ORDER BY key")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(keys, ["committed"]);
    let check: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    assert_eq!(check, "ok");
}

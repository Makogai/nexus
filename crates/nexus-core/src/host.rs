//! Session hosts (ADR-0014): which process owns which sessions, liveness by lease, reconciliation and safe
//! shutdown. Designed so an explicit background-service host can be added later without schema changes.

use std::time::Duration;

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::error::Result;
use crate::{new_id, now_ms};

/// How often a live host refreshes its lease.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
/// A host whose heartbeat is older than this is considered dead.
pub const LEASE_MS: i64 = 60_000;
/// Stopped host rows older than this are pruned (sessions keep working: `host_id` is set to NULL).
const PRUNE_AFTER_MS: i64 = 30 * 24 * 3600 * 1000;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HostKind {
    Desktop,
    Cli,
    /// Reserved for the future explicit background-service mode.
    Service,
}

impl HostKind {
    pub fn as_str(self) -> &'static str {
        match self {
            HostKind::Desktop => "desktop",
            HostKind::Cli => "cli",
            HostKind::Service => "service",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub id: String,
    pub kind: HostKind,
    pub pid: u32,
    pub started_at: i64,
}

/// Register this process as a host. Also prunes long-stopped host rows.
pub fn register(conn: &Connection, kind: HostKind) -> Result<Host> {
    let now = now_ms();
    let host = Host {
        id: new_id(),
        kind,
        pid: std::process::id(),
        started_at: now,
    };
    conn.execute(
        "INSERT INTO session_hosts(id, kind, pid, version, started_at, heartbeat_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![host.id, kind.as_str(), host.pid, env!("CARGO_PKG_VERSION"), now],
    )?;
    conn.execute(
        "DELETE FROM session_hosts WHERE stopped_at IS NOT NULL AND stopped_at < ?1",
        [now - PRUNE_AFTER_MS],
    )?;
    Ok(host)
}

/// Mark every other *running* host of `kind` as stopped. The desktop is single-instance per machine, so at startup
/// any previous desktop host is necessarily dead (crashed or killed).
pub fn stop_others(conn: &Connection, kind: HostKind, keep_id: &str) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE session_hosts SET stopped_at = ?3 WHERE kind = ?1 AND id <> ?2 AND stopped_at IS NULL",
        params![kind.as_str(), keep_id, now_ms()],
    )?)
}

/// Refresh the lease. Sessions this host still runs but that a reconciliation marked `interrupted` while the host
/// was paused (e.g. laptop sleep) are restored to `running`.
pub fn heartbeat(conn: &Connection, host_id: &str, running_sessions: &[String]) -> Result<()> {
    let now = now_ms();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE session_hosts SET heartbeat_at = ?2 WHERE id = ?1 AND stopped_at IS NULL",
        params![host_id, now],
    )?;
    for sid in running_sessions {
        tx.execute(
            "UPDATE sessions SET state = 'running'
             WHERE id = ?1 AND host_id = ?2 AND state = 'interrupted' AND ended_at IS NULL",
            params![sid, host_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Sessions in `starting`/`running` whose host is dead (stopped, lease expired, or unknown) become `interrupted`.
/// Returns how many sessions changed.
pub fn reconcile(conn: &Connection, now: i64) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE sessions SET state = 'interrupted'
         WHERE state IN ('starting', 'running')
           AND (host_id IS NULL
                OR host_id NOT IN (SELECT id FROM session_hosts WHERE stopped_at IS NULL AND heartbeat_at >= ?1))",
        [now - LEASE_MS],
    )?)
}

/// Orderly shutdown of a host: its still-live sessions are recorded as `interrupted` with an end time (final — a
/// late exit notification cannot overwrite it), then the host is marked stopped. Call *before* terminating the
/// children. Returns the number of sessions interrupted.
pub fn shutdown(conn: &Connection, host_id: &str) -> Result<usize> {
    let now = now_ms();
    let tx = conn.unchecked_transaction()?;
    let n = tx.execute(
        "UPDATE sessions SET state = 'interrupted', ended_at = ?2
         WHERE host_id = ?1 AND state IN ('starting', 'running')",
        params![host_id, now],
    )?;
    tx.execute("UPDATE session_hosts SET stopped_at = ?2 WHERE id = ?1", params![host_id, now])?;
    tx.commit()?;
    Ok(n)
}

pub fn is_alive(conn: &Connection, host_id: &str, now: i64) -> Result<bool> {
    let alive: Option<i64> = rusqlite::OptionalExtension::optional(conn.query_row(
        "SELECT 1 FROM session_hosts WHERE id = ?1 AND stopped_at IS NULL AND heartbeat_at >= ?2",
        params![host_id, now - LEASE_MS],
        |r| r.get(0),
    ))?;
    Ok(alive.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{self, NewSession, State};

    fn live_session(conn: &Connection, host: &Host) -> String {
        let s = session::insert_starting(
            conn,
            NewSession {
                host: Some(host.kind),
                host_id: Some(host.id.clone()),
                title: "t".into(),
                cwd: "/x".into(),
                program: "claude".into(),
                ..Default::default()
            },
        )
        .unwrap();
        session::mark_running(conn, &s.id, Some(1)).unwrap();
        s.id
    }

    fn state(conn: &Connection, id: &str) -> State {
        session::get(conn, id).unwrap().state
    }

    #[test]
    fn live_hosts_keep_their_sessions_dead_hosts_lose_them() {
        let conn = crate::db::open_in_memory().unwrap();
        let desktop = register(&conn, HostKind::Desktop).unwrap();
        let cli = register(&conn, HostKind::Cli).unwrap();
        let a = live_session(&conn, &desktop);
        let b = live_session(&conn, &cli);
        assert_eq!(reconcile(&conn, now_ms()).unwrap(), 0);

        // The CLI stops heartbeating: after the lease expires its session is interrupted, the desktop's is not.
        let later = now_ms() + LEASE_MS + 1;
        conn.execute("UPDATE session_hosts SET heartbeat_at = ?2 WHERE id = ?1", params![desktop.id, later]).unwrap();
        assert_eq!(reconcile(&conn, later).unwrap(), 1);
        assert_eq!(state(&conn, &a), State::Running);
        assert_eq!(state(&conn, &b), State::Interrupted);
        assert!(!is_alive(&conn, &cli.id, later).unwrap());
    }

    #[test]
    fn heartbeat_restores_sessions_interrupted_while_paused() {
        let conn = crate::db::open_in_memory().unwrap();
        let cli = register(&conn, HostKind::Cli).unwrap();
        let s = live_session(&conn, &cli);
        reconcile(&conn, now_ms() + LEASE_MS + 1).unwrap(); // e.g. machine slept
        assert_eq!(state(&conn, &s), State::Interrupted);
        heartbeat(&conn, &cli.id, std::slice::from_ref(&s)).unwrap();
        assert_eq!(state(&conn, &s), State::Running);
    }

    #[test]
    fn new_desktop_instance_closes_crashed_predecessor() {
        let conn = crate::db::open_in_memory().unwrap();
        let old = register(&conn, HostKind::Desktop).unwrap();
        let s = live_session(&conn, &old);
        let cli = register(&conn, HostKind::Cli).unwrap();
        let cli_s = live_session(&conn, &cli);
        let new = register(&conn, HostKind::Desktop).unwrap();
        assert_eq!(stop_others(&conn, HostKind::Desktop, &new.id).unwrap(), 1);
        reconcile(&conn, now_ms()).unwrap();
        assert_eq!(state(&conn, &s), State::Interrupted);
        assert_eq!(state(&conn, &cli_s), State::Running, "a live CLI host is not affected");
    }

    #[test]
    fn orderly_shutdown_is_final() {
        let conn = crate::db::open_in_memory().unwrap();
        let desktop = register(&conn, HostKind::Desktop).unwrap();
        let s = live_session(&conn, &desktop);
        assert_eq!(shutdown(&conn, &desktop.id).unwrap(), 1);
        let rec = session::get(&conn, &s).unwrap();
        assert_eq!(rec.state, State::Interrupted);
        assert!(rec.ended_at.is_some());
        // The killed child's exit notification arrives late: the shutdown record stands.
        session::mark_exited(&conn, &s, Some(1)).unwrap();
        assert_eq!(state(&conn, &s), State::Interrupted);
        // Heartbeats of a stopped host change nothing.
        heartbeat(&conn, &desktop.id, std::slice::from_ref(&s)).unwrap();
        assert_eq!(state(&conn, &s), State::Interrupted);
    }

    #[test]
    fn sessions_without_host_are_reconciled_and_hosts_are_pruned() {
        let conn = crate::db::open_in_memory().unwrap();
        let s = session::insert_starting(
            &conn,
            NewSession {
                title: "legacy".into(),
                cwd: "/x".into(),
                program: "claude".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(reconcile(&conn, now_ms()).unwrap(), 1);
        assert_eq!(state(&conn, &s.id), State::Interrupted);

        let old = register(&conn, HostKind::Cli).unwrap();
        conn.execute("UPDATE session_hosts SET stopped_at = 1 WHERE id = ?1", [&old.id]).unwrap();
        register(&conn, HostKind::Cli).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM session_hosts WHERE id = ?1", [&old.id], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "long-stopped hosts are pruned");
    }
}

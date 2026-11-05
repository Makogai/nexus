//! Synchronisation history and the one-operation-at-a-time rule (ADR-0031). Machine-local.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::{CoreError, Result, new_id, now_ms};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub workspace_id: String,
    pub op: String,
    pub status: String,
    pub branch: Option<String>,
    pub head_before: Option<String>,
    pub head_after: Option<String>,
    pub commits: i64,
    pub files: i64,
    pub error_kind: Option<String>,
    pub message: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

const SELECT: &str = "SELECT id, workspace_id, op, status, branch, head_before, head_after, commits, files, error_kind, message, started_at, ended_at FROM sync_history";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        op: r.get(2)?,
        status: r.get(3)?,
        branch: r.get(4)?,
        head_before: r.get(5)?,
        head_after: r.get(6)?,
        commits: r.get(7)?,
        files: r.get(8)?,
        error_kind: r.get(9)?,
        message: r.get(10)?,
        started_at: r.get(11)?,
        ended_at: r.get(12)?,
    })
}

/// Start an operation; refused while another one of this workspace runs in a live host.
pub fn begin(conn: &Connection, workspace_id: &str, op: &str, host_id: &str, branch: Option<&str>, head: Option<&str>) -> Result<String> {
    let now = now_ms();
    let running: Vec<(String, Option<String>, String)> = conn
        .prepare("SELECT id, host_id, op FROM sync_history WHERE workspace_id = ?1 AND status = 'running'")?
        .query_map([workspace_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?;
    for (id, host, other) in running {
        let alive = match host.as_deref() {
            Some(h) => crate::host::is_alive(conn, h, now)?,
            None => false,
        };
        if alive {
            return Err(CoreError::Conflict(format!("another sync operation ({other}) is running for this workspace — wait for it to finish")));
        }
        finish(conn, &id, "interrupted", None, 0, 0, Some("interrupted"), Some("the program running it stopped before it finished"))?;
    }
    let id = new_id();
    conn.execute(
        "INSERT INTO sync_history (id, workspace_id, op, status, host_id, branch, head_before, started_at) VALUES (?1, ?2, ?3, 'running', ?4, ?5, ?6, ?7)",
        params![id, workspace_id, op, host_id, branch, head, now],
    )?;
    Ok(id)
}

#[allow(clippy::too_many_arguments)]
pub fn finish(conn: &Connection, id: &str, status: &str, head_after: Option<&str>, commits: usize, files: usize, error_kind: Option<&str>, message: Option<&str>) -> Result<()> {
    let message: Option<String> = message.map(|m| m.chars().take(600).collect());
    conn.execute(
        "UPDATE sync_history SET status = ?2, head_after = ?3, commits = ?4, files = ?5, error_kind = ?6, message = ?7, ended_at = ?8 WHERE id = ?1",
        params![id, status, head_after, commits as i64, files as i64, error_kind, message, now_ms()],
    )?;
    Ok(())
}

/// A clone that became a workspace: attach its history row.
pub fn set_workspace(conn: &Connection, id: &str, workspace_id: &str) -> Result<()> {
    conn.execute("UPDATE sync_history SET workspace_id = ?2 WHERE id = ?1", params![id, workspace_id])?;
    Ok(())
}

pub fn list(conn: &Connection, workspace_id: &str, limit: usize) -> Result<Vec<Entry>> {
    Ok(conn
        .prepare(&format!("{SELECT} WHERE workspace_id = ?1 ORDER BY started_at DESC, rowid DESC LIMIT ?2"))?
        .query_map(params![workspace_id, limit.min(500) as i64], row)?
        .collect::<std::result::Result<_, _>>()?)
}

pub fn last_ok(conn: &Connection, workspace_id: &str, op: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row("SELECT MAX(ended_at) FROM sync_history WHERE workspace_id = ?1 AND op = ?2 AND status = 'ok'", params![workspace_id, op], |r| r.get(0))
        .optional()?
        .flatten())
}

/// Operations whose host died are interrupted (called when a host starts, like session reconciliation).
pub fn reconcile(conn: &Connection, now: i64) -> Result<usize> {
    let running: Vec<(String, Option<String>)> = conn
        .prepare("SELECT id, host_id FROM sync_history WHERE status = 'running'")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut n = 0;
    for (id, host) in running {
        let alive = match host.as_deref() {
            Some(h) => crate::host::is_alive(conn, h, now)?,
            None => false,
        };
        if !alive {
            finish(conn, &id, "interrupted", None, 0, 0, Some("interrupted"), Some("NEXUS stopped before the operation finished — the Sync Center shows the repository's state"))?;
            n += 1;
        }
    }
    Ok(n)
}

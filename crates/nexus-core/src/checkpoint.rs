//! Session checkpoints: short, bounded notes recorded during a session (by Claude through the NEXUS MCP server, or
//! by NEXUS itself) so work can be resumed later. Machine-local runtime state (ADR-0013).

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::{new_id, now_ms};

pub const MAX_SUMMARY_CHARS: usize = 2000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckpointKind {
    Progress,
    Decision,
    Blocker,
    Done,
    Note,
}

impl CheckpointKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "progress" => Self::Progress,
            "decision" => Self::Decision,
            "blocker" => Self::Blocker,
            "done" => Self::Done,
            "note" => Self::Note,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Decision => "decision",
            Self::Blocker => "blocker",
            Self::Done => "done",
            Self::Note => "note",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Claude,
    Human,
    Nexus,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::Claude => "claude",
            Source::Human => "human",
            Source::Nexus => "nexus",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub id: String,
    pub session_id: String,
    pub session_title: String,
    pub kind: String,
    pub summary: String,
    pub source: String,
    pub created_at: i64,
}

/// Record a checkpoint. The summary is trimmed, control characters other than newlines are removed and the length
/// is bounded — the text comes from a model and is stored as data.
/// A NEXUS checkpoint on the latest session that worked on a task (none when no session did — not an error).
pub fn add_for_task(conn: &Connection, task_id: &str, kind: CheckpointKind, summary: &str) -> Result<Option<Checkpoint>> {
    let sid: Option<String> = rusqlite::OptionalExtension::optional(conn.query_row(
        "SELECT id FROM sessions WHERE task_id = ?1 ORDER BY started_at DESC LIMIT 1",
        [task_id],
        |r| r.get(0),
    ))?;
    sid.map(|s| add(conn, &s, kind, summary, Source::Nexus)).transpose()
}

pub fn add(conn: &Connection, session_id: &str, kind: CheckpointKind, summary: &str, source: Source) -> Result<Checkpoint> {
    let clean: String = summary
        .chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .collect::<String>()
        .trim()
        .to_string();
    if clean.is_empty() {
        return Err(CoreError::invalid("summary must not be empty"));
    }
    if clean.chars().count() > MAX_SUMMARY_CHARS {
        return Err(CoreError::invalid(format!("summary must be at most {MAX_SUMMARY_CHARS} characters")));
    }
    let id = new_id();
    conn.execute(
        "INSERT INTO session_events(id, session_id, kind, summary, source, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, session_id, kind.as_str(), clean, source.as_str(), now_ms()],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            CoreError::not_found(format!("session {session_id} not found"))
        }
        other => other.into(),
    })?;
    list_where(conn, "e.id = ?1", &id, 1).map(|mut v| v.remove(0))
}

const SELECT: &str = "SELECT e.id, e.session_id, s.title, e.kind, e.summary, e.source, e.created_at
    FROM session_events e JOIN sessions s ON s.id = e.session_id";

/// Checkpoint kinds; `session_events` also holds lifecycle events recorded by hooks.
const CHECKPOINT_KINDS: &str = "('progress', 'decision', 'blocker', 'done', 'note')";

fn list_where(conn: &Connection, cond: &str, arg: &str, limit: u32) -> Result<Vec<Checkpoint>> {
    let mut stmt = conn.prepare(&format!("{SELECT} WHERE {cond} AND e.kind IN {CHECKPOINT_KINDS} ORDER BY e.created_at DESC, e.rowid DESC LIMIT {}", limit.min(200)))?;
    let rows = stmt
        .query_map([arg], |r| {
            Ok(Checkpoint {
                id: r.get(0)?,
                session_id: r.get(1)?,
                session_title: r.get(2)?,
                kind: r.get(3)?,
                summary: r.get(4)?,
                source: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn for_session(conn: &Connection, session_id: &str, limit: u32) -> Result<Vec<Checkpoint>> {
    list_where(conn, "e.session_id = ?1", session_id, limit)
}

/// Most recent checkpoints across all sessions of a project.
pub fn for_project(conn: &Connection, project_id: &str, limit: u32) -> Result<Vec<Checkpoint>> {
    list_where(conn, "s.project_id = ?1", project_id, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{self, NewSession};

    fn session(conn: &Connection, project: Option<&str>) -> String {
        session::insert_starting(
            conn,
            NewSession {
                title: "work".into(),
                cwd: "/x".into(),
                program: "claude".into(),
                project_id: project.map(String::from),
                ..Default::default()
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn add_and_list_with_sanitising_and_bounds() {
        let conn = crate::db::open_in_memory().unwrap();
        let s = session(&conn, None);
        let c = add(&conn, &s, CheckpointKind::Decision, "  Use SQLite\u{7}\nfor runtime state  ", Source::Claude).unwrap();
        assert_eq!(c.summary, "Use SQLite\nfor runtime state");
        assert_eq!(c.kind, "decision");
        assert_eq!(c.session_title, "work");
        assert!(add(&conn, &s, CheckpointKind::Note, "   ", Source::Claude).is_err());
        assert!(add(&conn, &s, CheckpointKind::Note, &"x".repeat(MAX_SUMMARY_CHARS + 1), Source::Claude).is_err());
        assert_eq!(add(&conn, "missing", CheckpointKind::Note, "x", Source::Claude).unwrap_err().code(), "not_found");
        add(&conn, &s, CheckpointKind::Progress, "second", Source::Claude).unwrap();
        let list = for_session(&conn, &s, 10).unwrap();
        assert_eq!(list.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(), ["second", "Use SQLite\nfor runtime state"]);
        assert_eq!(CheckpointKind::parse("blocker"), Some(CheckpointKind::Blocker));
        assert_eq!(CheckpointKind::parse("rm -rf"), None);
    }

    #[test]
    fn project_view_spans_sessions_and_cascades_on_delete() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute("INSERT INTO workspaces(id, name, root_path, created_at) VALUES ('w', 'W', '/w', 0)", []).unwrap();
        conn.execute("INSERT INTO projects(id, workspace_id, name, created_at) VALUES ('p', 'w', 'P', 0)", []).unwrap();
        let a = session(&conn, Some("p"));
        let b = session(&conn, Some("p"));
        add(&conn, &a, CheckpointKind::Progress, "one", Source::Claude).unwrap();
        add(&conn, &b, CheckpointKind::Done, "two", Source::Claude).unwrap();
        assert_eq!(for_project(&conn, "p", 10).unwrap().len(), 2);
        conn.execute("DELETE FROM sessions WHERE id = ?1", [&a]).unwrap();
        assert_eq!(for_project(&conn, "p", 10).unwrap().len(), 1);
    }
}

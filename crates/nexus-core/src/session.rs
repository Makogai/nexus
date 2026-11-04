//! Session records and lifecycle (ADR-0008).
//!
//! A *session* is a NEXUS-managed terminal process (Claude or a shell). A *conversation* is Claude's own
//! resumable transcript, identified by the `claude_session_id` NEXUS assigned with `--session-id`.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::{new_id, now_ms};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Claude,
    Shell,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Claude => "claude",
            Kind::Shell => "shell",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Starting,
    Running,
    Exited,
    Interrupted,
    Failed,
}

impl State {
    fn parse(s: &str) -> State {
        match s {
            "starting" => State::Starting,
            "running" => State::Running,
            "exited" => State::Exited,
            "interrupted" => State::Interrupted,
            _ => State::Failed,
        }
    }
    pub fn is_live(self) -> bool {
        matches!(self, State::Starting | State::Running)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub kind: Kind,
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    pub profile_id: Option<String>,
    pub claude_session_id: Option<String>,
    pub title: String,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub work_profile: Option<String>,
    pub effort: Option<String>,
    pub model: Option<String>,
    pub program: String,
    pub pid: Option<i64>,
    pub state: State,
    pub exit_code: Option<i64>,
    pub error: Option<String>,
    pub resumed_from: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct NewSession {
    pub id: Option<String>,
    pub kind: Option<Kind>,
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    pub profile_id: Option<String>,
    pub claude_session_id: Option<String>,
    pub title: String,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub work_profile: Option<String>,
    pub effort: Option<String>,
    pub model: Option<String>,
    pub program: String,
    pub resumed_from: Option<String>,
}

const SELECT: &str = "SELECT id, kind, workspace_id, project_id, profile_id, claude_session_id, title, cwd, branch,
    work_profile, effort, model, program, pid, state, exit_code, error, resumed_from, started_at, ended_at,
    archived_at FROM sessions";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Session> {
    let kind: String = r.get(1)?;
    let state: String = r.get(14)?;
    Ok(Session {
        id: r.get(0)?,
        kind: if kind == "claude" { Kind::Claude } else { Kind::Shell },
        workspace_id: r.get(2)?,
        project_id: r.get(3)?,
        profile_id: r.get(4)?,
        claude_session_id: r.get(5)?,
        title: r.get(6)?,
        cwd: PathBuf::from(r.get::<_, String>(7)?),
        branch: r.get(8)?,
        work_profile: r.get(9)?,
        effort: r.get(10)?,
        model: r.get(11)?,
        program: r.get(12)?,
        pid: r.get(13)?,
        state: State::parse(&state),
        exit_code: r.get(15)?,
        error: r.get(16)?,
        resumed_from: r.get(17)?,
        started_at: r.get(18)?,
        ended_at: r.get(19)?,
        archived_at: r.get(20)?,
    })
}

pub fn insert_starting(conn: &Connection, s: NewSession) -> Result<Session> {
    let id = s.id.unwrap_or_else(new_id);
    conn.execute(
        "INSERT INTO sessions(id, kind, workspace_id, project_id, profile_id, claude_session_id, title, cwd, branch,
            work_profile, effort, model, program, state, resumed_from, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'starting', ?14, ?15)",
        params![
            id,
            s.kind.unwrap_or(Kind::Claude).as_str(),
            s.workspace_id,
            s.project_id,
            s.profile_id,
            s.claude_session_id,
            s.title,
            s.cwd.to_string_lossy(),
            s.branch,
            s.work_profile,
            s.effort,
            s.model,
            s.program,
            s.resumed_from,
            now_ms()
        ],
    )?;
    get(conn, &id)
}

pub fn get(conn: &Connection, id: &str) -> Result<Session> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| CoreError::not_found(format!("session {id} not found")))
}

/// Transition helper: only applies when the session is currently in one of `from`.
fn transition(conn: &Connection, id: &str, from: &[&str], set_sql: &str, p: &[&dyn rusqlite::ToSql]) -> Result<bool> {
    let placeholders = from.iter().map(|s| format!("'{s}'")).collect::<Vec<_>>().join(",");
    let sql = format!("UPDATE sessions SET {set_sql} WHERE id = ?1 AND state IN ({placeholders})");
    let mut all: Vec<&dyn rusqlite::ToSql> = vec![&id];
    all.extend_from_slice(p);
    Ok(conn.execute(&sql, all.as_slice())? > 0)
}

pub fn mark_running(conn: &Connection, id: &str, pid: Option<u32>) -> Result<()> {
    let pid = pid.map(i64::from);
    transition(conn, id, &["starting"], "state = 'running', pid = ?2", &[&pid])?;
    Ok(())
}

pub fn mark_exited(conn: &Connection, id: &str, exit_code: Option<i64>) -> Result<()> {
    let now = now_ms();
    transition(
        conn,
        id,
        &["starting", "running", "interrupted"],
        "state = 'exited', exit_code = ?2, ended_at = ?3",
        &[&exit_code, &now],
    )?;
    Ok(())
}

pub fn mark_failed(conn: &Connection, id: &str, error: &str) -> Result<()> {
    let now = now_ms();
    let msg: String = error.chars().take(500).collect();
    transition(conn, id, &["starting", "running"], "state = 'failed', error = ?2, ended_at = ?3", &[&msg, &now])?;
    Ok(())
}

/// At desktop startup no PTY from a previous run can still be attached: every live desktop session
/// becomes `interrupted`. Returns how many were reconciled.
pub fn reconcile_on_startup(conn: &Connection) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE sessions SET state = 'interrupted' WHERE host = 'desktop' AND state IN ('starting', 'running')",
        [],
    )?)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Filter {
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    pub include_archived: bool,
    pub kind: Option<Kind>,
    pub limit: Option<u32>,
}

pub fn list(conn: &Connection, f: &Filter) -> Result<Vec<Session>> {
    let mut sql = format!("{SELECT} WHERE 1 = 1");
    let mut p: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(ws) = &f.workspace_id {
        p.push(Box::new(ws.clone()));
        sql.push_str(&format!(" AND workspace_id = ?{}", p.len()));
    }
    if let Some(pr) = &f.project_id {
        p.push(Box::new(pr.clone()));
        sql.push_str(&format!(" AND project_id = ?{}", p.len()));
    }
    if let Some(k) = f.kind {
        p.push(Box::new(k.as_str()));
        sql.push_str(&format!(" AND kind = ?{}", p.len()));
    }
    if !f.include_archived {
        sql.push_str(" AND archived_at IS NULL");
    }
    sql.push_str(&format!(" ORDER BY started_at DESC LIMIT {}", f.limit.unwrap_or(200).min(1000)));
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), from_row)?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn rename(conn: &Connection, id: &str, title: &str) -> Result<Session> {
    let title = crate::claude::launch::sanitize_title(title);
    if title.is_empty() {
        return Err(CoreError::invalid("title must not be empty"));
    }
    if conn.execute("UPDATE sessions SET title = ?2 WHERE id = ?1", params![id, title])? == 0 {
        return Err(CoreError::not_found(format!("session {id} not found")));
    }
    get(conn, id)
}

pub fn set_archived(conn: &Connection, id: &str, archived: bool) -> Result<Session> {
    let s = get(conn, id)?;
    if archived && s.state.is_live() {
        return Err(CoreError::invalid("stop the session before archiving it"));
    }
    conn.execute("UPDATE sessions SET archived_at = ?2 WHERE id = ?1", params![id, archived.then(now_ms)])?;
    get(conn, id)
}

/// Has Claude recorded a conversation for this id under `config_dir`? Looks for
/// `projects/*/<id>.jsonl` without depending on how the project folder name is encoded, and without
/// reading the transcript.
pub fn conversation_recorded(config_dir: &Path, claude_session_id: &str) -> bool {
    if uuid::Uuid::parse_str(claude_session_id).is_err() {
        return false;
    }
    let file = format!("{claude_session_id}.jsonl");
    let Ok(rd) = std::fs::read_dir(config_dir.join("projects")) else {
        return false;
    };
    rd.flatten().any(|e| e.path().join(&file).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(conn: &Connection) -> Session {
        insert_starting(
            conn,
            NewSession {
                claude_session_id: Some(new_id()),
                title: "t".into(),
                cwd: PathBuf::from("/x"),
                program: "claude".into(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn lifecycle_transitions() {
        let conn = crate::db::open_in_memory().unwrap();
        let s = new(&conn);
        assert_eq!(s.state, State::Starting);
        mark_running(&conn, &s.id, Some(42)).unwrap();
        let s2 = get(&conn, &s.id).unwrap();
        assert_eq!((s2.state, s2.pid), (State::Running, Some(42)));
        mark_exited(&conn, &s.id, Some(0)).unwrap();
        let s3 = get(&conn, &s.id).unwrap();
        assert_eq!((s3.state, s3.exit_code), (State::Exited, Some(0)));
        assert!(s3.ended_at.is_some());
        // Exited sessions cannot fail or run again.
        mark_failed(&conn, &s.id, "late").unwrap();
        mark_running(&conn, &s.id, Some(1)).unwrap();
        assert_eq!(get(&conn, &s.id).unwrap().state, State::Exited);
    }

    #[test]
    fn startup_reconcile_marks_interrupted() {
        let conn = crate::db::open_in_memory().unwrap();
        let a = new(&conn);
        let b = new(&conn);
        mark_running(&conn, &a.id, Some(1)).unwrap();
        let done = new(&conn);
        mark_exited(&conn, &done.id, Some(1)).unwrap();
        assert_eq!(reconcile_on_startup(&conn).unwrap(), 2);
        assert_eq!(get(&conn, &a.id).unwrap().state, State::Interrupted);
        assert_eq!(get(&conn, &b.id).unwrap().state, State::Interrupted);
        assert_eq!(get(&conn, &done.id).unwrap().state, State::Exited);
    }

    #[test]
    fn list_filters_and_archive_rules() {
        let conn = crate::db::open_in_memory().unwrap();
        let s = new(&conn);
        assert!(set_archived(&conn, &s.id, true).is_err(), "live sessions cannot be archived");
        mark_exited(&conn, &s.id, Some(0)).unwrap();
        set_archived(&conn, &s.id, true).unwrap();
        assert!(list(&conn, &Filter::default()).unwrap().is_empty());
        let all = list(&conn, &Filter { include_archived: true, ..Default::default() }).unwrap();
        assert_eq!(all.len(), 1);
        let renamed = rename(&conn, &s.id, "  New\tname ").unwrap();
        assert_eq!(renamed.title, "New name");
    }

    #[test]
    fn conversation_detection_by_glob() {
        let t = tempfile::tempdir().unwrap();
        let id = new_id();
        assert!(!conversation_recorded(t.path(), &id));
        let proj = t.path().join("projects").join("D--code-app");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join(format!("{id}.jsonl")), "{}").unwrap();
        assert!(conversation_recorded(t.path(), &id));
        assert!(!conversation_recorded(t.path(), "../../etc/passwd"));
    }
}

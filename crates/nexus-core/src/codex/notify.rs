//! Codex's `notify` program for NEXUS sessions (ADR-0052 §4): `nexus agent-notify --agent codex --session <id> <json>`.
//! Codex runs it after each turn with a JSON payload as the last argument. NEXUS reads only `type` and `thread-id` —
//! never `input-messages` or `last-assistant-message` (prompt and answer text) — and stores no text.

use rusqlite::{Connection, params};
use serde_json::Value;

use crate::agent::Kind;
use crate::error::Result;
use crate::session;

/// What a notification did (for tests and debug logs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A finished turn was recorded (and the conversation id learned, the first time).
    TurnFinished,
    /// Not a turn-complete event, or not parseable.
    Ignored,
    /// Not a live Codex session of NEXUS, or a different conversation than the one recorded.
    NotThisSession,
}

pub fn handle(conn: &Connection, nexus_session_id: &str, payload: &str) -> Result<Outcome> {
    let Ok(v) = serde_json::from_str::<Value>(payload) else { return Ok(Outcome::Ignored) };
    if v.get("type").and_then(Value::as_str) != Some("agent-turn-complete") {
        return Ok(Outcome::Ignored);
    }
    let thread = v.get("thread-id").and_then(Value::as_str).filter(|t| uuid::Uuid::parse_str(t).is_ok());
    let Ok(s) = session::get(conn, nexus_session_id) else { return Ok(Outcome::NotThisSession) };
    if s.agent != Some(Kind::Codex) || !s.state.is_live() {
        return Ok(Outcome::NotThisSession);
    }
    match (thread, s.claude_session_id.as_deref()) {
        (Some(t), None) => {
            conn.execute("UPDATE sessions SET claude_session_id = ?2 WHERE id = ?1 AND claude_session_id IS NULL", params![s.id, t])?;
        }
        (Some(t), Some(known)) if !t.eq_ignore_ascii_case(known) => return Ok(Outcome::NotThisSession),
        _ => {}
    }
    crate::activity::turn_finished(conn, &session::get(conn, &s.id)?)?;
    Ok(Outcome::TurnFinished)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Kind as SessionKind, NewSession};

    const THREAD: &str = "0199a213-81c0-7800-8aa1-bbab2a035a53";
    const OTHER: &str = "0199a213-81c0-7800-8aa1-000000000000";

    fn codex_session(conn: &Connection) -> String {
        let s = session::insert_starting(
            conn,
            NewSession { kind: Some(SessionKind::Claude), agent: Some(Kind::Codex), title: "shop".into(), cwd: std::env::temp_dir(), program: "codex".into(), ..Default::default() },
        )
        .unwrap();
        session::mark_running(conn, &s.id, Some(1)).unwrap();
        s.id
    }

    fn payload(thread: &str) -> String {
        serde_json::json!({
            "type": "agent-turn-complete",
            "thread-id": thread,
            "turn-id": "t1",
            "cwd": "/x",
            "input-messages": ["PROMPT-TEXT-NEVER-STORED"],
            "last-assistant-message": "ANSWER-TEXT-NEVER-STORED",
        })
        .to_string()
    }

    #[test]
    fn the_first_turn_names_the_conversation_and_every_turn_is_counted() {
        let conn = crate::db::open_in_memory().unwrap();
        let id = codex_session(&conn);
        assert_eq!(handle(&conn, &id, &payload(THREAD)).unwrap(), Outcome::TurnFinished);
        assert_eq!(handle(&conn, &id, &payload(THREAD)).unwrap(), Outcome::TurnFinished);
        let s = session::get(&conn, &id).unwrap();
        assert_eq!((s.claude_session_id.as_deref(), s.turns, s.attention_kind.as_deref()), (Some(THREAD), 2, Some(crate::attention::IDLE)));
        assert_eq!(handle(&conn, &id, &payload(OTHER)).unwrap(), Outcome::NotThisSession, "another conversation (a sub-agent) is not this one");
        assert_eq!(session::get(&conn, &id).unwrap().turns, 2);
        // No text from the payload is stored anywhere.
        let dump: String = conn
            .query_row("SELECT COALESCE(group_concat(COALESCE(summary,'') || COALESCE(data,''), '|'), '') FROM session_events", [], |r| r.get(0))
            .unwrap();
        let notes: String = conn.query_row("SELECT COALESCE(group_concat(title || COALESCE(body,''), '|'), '') FROM notifications", [], |r| r.get(0)).unwrap();
        assert!(!dump.contains("TEXT-NEVER-STORED") && !notes.contains("TEXT-NEVER-STORED"), "{dump} {notes}");
        assert!(notes.contains("Codex finished"), "the agent is named: {notes}");
    }

    #[test]
    fn anything_else_is_ignored() {
        let conn = crate::db::open_in_memory().unwrap();
        let id = codex_session(&conn);
        assert_eq!(handle(&conn, &id, "not json").unwrap(), Outcome::Ignored);
        assert_eq!(handle(&conn, &id, r#"{"type":"something-else","thread-id":"0199a213-81c0-7800-8aa1-bbab2a035a53"}"#).unwrap(), Outcome::Ignored);
        assert_eq!(handle(&conn, "no-such-session", &payload(THREAD)).unwrap(), Outcome::NotThisSession);
        // A bad thread id still counts the turn but names no conversation.
        assert_eq!(handle(&conn, &id, r#"{"type":"agent-turn-complete","thread-id":"--last"}"#).unwrap(), Outcome::TurnFinished);
        assert_eq!(session::get(&conn, &id).unwrap().claude_session_id, None);
        // Claude sessions do not take Codex notifications.
        let c = session::insert_starting(&conn, NewSession { kind: Some(SessionKind::Claude), title: "c".into(), cwd: std::env::temp_dir(), program: "claude".into(), ..Default::default() }).unwrap();
        session::mark_running(&conn, &c.id, None).unwrap();
        assert_eq!(handle(&conn, &c.id, &payload(THREAD)).unwrap(), Outcome::NotThisSession);
    }
}

//! Session attention (ADR-0025): what each Claude session needs from the user, derived **only** from signals Claude
//! Code reports through supported hooks, the NEXUS MCP tool `request_attention` and the process lifecycle.
//!
//! Principles:
//! * Observe, never act: NEXUS's hooks print nothing for `PermissionRequest`/`PreToolUse`, so they never approve,
//!   deny or answer anything; NEXUS never injects keystrokes.
//! * No inference from silence: a quiet session is "Running" (or "Unknown" without hooks), never "Permission
//!   required". Every status says whether it was *reported* or is an *assumption*.
//! * Identity comes from the NEXUS session row (workspace, project, profile), never from the hook payload. A hook
//!   event whose Claude conversation id does not belong to the session is ignored.

use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::Value;

use crate::error::Result;
use crate::notify::{self, Category, NewEvent, Priority};
use crate::now_ms;
use crate::session::{Kind, Session, State};

/// Structured attention kinds stored in `sessions.attention_kind`.
pub const PERMISSION: &str = "permission";
pub const INPUT: &str = "input";
pub const IDLE: &str = "idle";
pub const ERROR: &str = "error";

/// Notification categories that describe "Claude is waiting" and end when Claude continues.
pub const WAITING: [Category; 3] = [Category::InputRequired, Category::PermissionRequested, Category::AgentFinished];

/// Attention signal extracted from one hook event.
#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    /// Claude asks for a permission decision in its own dialog (`tool` is the tool name only; `message` is Claude's
    /// own notification text, shown escaped).
    Permission { tool: Option<String>, message: Option<String> },
    /// Claude (or an MCP server through Claude) waits for an answer.
    Input { title: String },
    /// The turn ended; Claude waits for the next prompt.
    TurnEnded,
    /// The turn ended on an API error.
    Failure { error: String },
    /// The user (or Claude) moved on: clears attention and resolves waiting notifications.
    Continued,
    /// Informational (e.g. sign-in succeeded): clears an error state only.
    Info,
}

/// The tool name only (never its input), restricted to identifier characters.
pub fn tool_name(payload: &Value) -> Option<String> {
    payload
        .get("tool_name")
        .and_then(Value::as_str)
        .map(|t| t.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')).take(80).collect::<String>())
        .filter(|t| !t.is_empty())
}

/// The `Notification` hook's `notification_type` → signal. Older Claude versions send no type; their message is
/// then used as the title (escaped) and the kind is "input" unless the message names a permission.
pub fn from_notification(payload: &Value) -> Option<Signal> {
    let message = payload.get("message").and_then(Value::as_str).unwrap_or("");
    match payload.get("notification_type").and_then(Value::as_str) {
        Some("permission_prompt") => Some(Signal::Permission { tool: tool_name(payload), message: Some(message.to_string()).filter(|m| !m.is_empty()) }),
        Some("idle_prompt") => Some(Signal::TurnEnded),
        Some("elicitation_dialog" | "elicitation_url_dialog") => Some(Signal::Input { title: "An MCP server asks for input".into() }),
        Some("agent_needs_input") => Some(Signal::Input { title: "Claude needs your input".into() }),
        Some("elicitation_complete" | "elicitation_response") => Some(Signal::Continued),
        Some("auth_success") => Some(Signal::Info),
        Some(_) => None,
        None if message.to_lowercase().contains("permission") => Some(Signal::Permission { tool: None, message: Some(message.into()) }),
        None => Some(Signal::Input { title: if message.is_empty() { "Claude is waiting for you".into() } else { message.into() } }),
    }
}

/// `StopFailure`'s `error` → user-facing title (allowlisted; anything else is "unknown").
pub fn failure_title(error: &str) -> &'static str {
    match error {
        "authentication_failed" | "oauth_org_not_allowed" => "Claude is not signed in for this profile",
        "rate_limit" => "Claude hit a usage limit",
        "overloaded" | "server_error" => "Claude's API is unavailable",
        "billing_error" | "account_on_hold" => "Claude stopped: account or billing problem",
        "model_not_found" | "invalid_request" => "Claude stopped: the request was rejected",
        "max_output_tokens" => "Claude stopped: output limit reached",
        "cloud_credential_error" => "Claude stopped: cloud provider credentials failed",
        _ => "Claude stopped on an error",
    }
}

fn known_error(error: &str) -> &str {
    const KNOWN: &[&str] = &[
        "authentication_failed",
        "oauth_org_not_allowed",
        "rate_limit",
        "overloaded",
        "server_error",
        "billing_error",
        "account_on_hold",
        "model_not_found",
        "invalid_request",
        "max_output_tokens",
        "cloud_credential_error",
    ];
    KNOWN.iter().find(|k| **k == error).copied().unwrap_or("unknown")
}

/// A hook payload belongs to this session unless it names a different Claude conversation.
pub fn owns(s: &Session, payload: &Value) -> bool {
    match (payload.get("session_id").and_then(Value::as_str), s.claude_session_id.as_deref()) {
        (Some(reported), Some(known)) => reported.eq_ignore_ascii_case(known),
        _ => true,
    }
}

/// The agent's short name for messages ("Codex finished in …"); sessions from before ADR-0052 are Claude's.
fn agent_name(s: &Session) -> &'static str {
    s.agent.unwrap_or(crate::agent::Kind::Claude).short()
}

fn set_kind(conn: &Connection, id: &str, kind: Option<&str>, text: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET attention_kind = ?2, attention_at = CASE WHEN ?2 IS NULL THEN NULL ELSE ?3 END,
            attention = ?4, last_activity_at = ?3 WHERE id = ?1",
        params![id, kind, now_ms(), text],
    )?;
    Ok(())
}

/// Apply a signal: update the session's attention and publish/resolve notifications.
pub fn apply(conn: &Connection, s: &Session, signal: Signal) -> Result<()> {
    if s.kind != Kind::Claude {
        return Ok(());
    }
    let label = &s.title;
    match signal {
        Signal::Permission { tool, message } => {
            // PermissionRequest (with the tool) usually precedes the Notification (with Claude's message): keep the
            // more specific title of the situation already reported.
            let title = match (&tool, s.attention_kind.as_deref(), message) {
                (Some(t), _, _) => format!("Permission requested: {t}"),
                (None, Some(PERMISSION), _) if s.attention.is_some() => s.attention.clone().unwrap_or_default(),
                (None, _, Some(m)) => crate::knowledge::tasks::escape_untrusted(&m, 200),
                (None, _, None) => "Claude asks for a permission".to_string(),
            };
            set_kind(conn, &s.id, Some(PERMISSION), Some(&title))?;
            notify::resolve_session(conn, &s.id, &[Category::AgentFinished])?;
            notify::publish(
                conn,
                NewEvent::for_session(Category::PermissionRequested, "hook", s, title)
                    .body(format!("Session “{label}” waits for your decision in its terminal."))
                    .dedupe(format!("permission:{}", s.id)),
            )?;
        }
        Signal::Input { title } => {
            set_kind(conn, &s.id, Some(INPUT), Some(&title))?;
            notify::resolve_session(conn, &s.id, &[Category::AgentFinished])?;
            notify::publish(
                conn,
                NewEvent::for_session(Category::InputRequired, "hook", s, title).body(format!("Session “{label}” waits for your answer.")).dedupe(format!("input:{}", s.id)),
            )?;
        }
        Signal::TurnEnded => {
            // A turn end answers any open permission/question of the turn.
            if s.attention_kind.as_deref() != Some(ERROR) {
                set_kind(conn, &s.id, Some(IDLE), None)?;
            }
            notify::resolve_session(conn, &s.id, &[Category::InputRequired, Category::PermissionRequested])?;
            if s.attention_kind.as_deref() != Some(ERROR) {
                notify::publish(
                    conn,
                    NewEvent::for_session(Category::AgentFinished, "hook", s, format!("{} finished in “{label}”", agent_name(s)))
                        .body("Waiting for your next prompt.")
                        .dedupe(format!("turn:{}", s.id)),
                )?;
            }
        }
        Signal::Failure { error } => {
            let error = known_error(&error);
            let title = failure_title(error);
            set_kind(conn, &s.id, Some(ERROR), Some(title))?;
            notify::resolve_session(conn, &s.id, &WAITING)?;
            let priority = if matches!(error, "authentication_failed" | "billing_error" | "account_on_hold" | "oauth_org_not_allowed") {
                Priority::Action
            } else {
                Priority::Warning
            };
            notify::publish(
                conn,
                NewEvent::for_session(Category::ProcessIssue, "hook", s, title)
                    .body(format!("Session “{label}” stopped ({error})."))
                    .priority(priority)
                    .dedupe(format!("failure:{}:{error}", s.id)),
            )?;
        }
        Signal::Continued => {
            set_kind(conn, &s.id, None, None)?;
            notify::resolve_session(conn, &s.id, &[Category::InputRequired, Category::PermissionRequested, Category::AgentFinished, Category::ProcessIssue])?;
        }
        Signal::Info => {
            if s.attention_kind.as_deref() == Some(ERROR) {
                set_kind(conn, &s.id, None, None)?;
                notify::resolve_session(conn, &s.id, &[Category::ProcessIssue])?;
            }
        }
    }
    Ok(())
}

/// Why Claude asks for attention through the NEXUS MCP tool `request_attention`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Claude needs an answer or a decision before it can continue.
    Question,
    /// Something is ready for the user's review.
    Review,
    /// Claude finished what it was asked to do.
    Done,
}

impl Request {
    pub fn parse(s: &str) -> Option<Request> {
        Some(match s {
            "question" => Request::Question,
            "review" => Request::Review,
            "done" => Request::Done,
            _ => return None,
        })
    }
}

/// `request_attention` from the session's own MCP server: bound to that session (the MCP server cannot name another
/// one), rate-limited ([`notify::MCP_RATE_PER_10_MIN`]), message escaped and bounded. NEXUS only *tells* the user.
pub fn request(conn: &Connection, s: &Session, reason: Request, message: &str) -> Result<notify::Published> {
    let message = crate::knowledge::tasks::escape_untrusted(message, 200);
    let (category, title, priority) = match reason {
        Request::Question => (Category::InputRequired, format!("{} needs your input in “{}”", agent_name(s), s.title), Priority::Action),
        Request::Review => (Category::ReviewRequired, format!("Ready for your review in “{}”", s.title), Priority::Action),
        Request::Done => (Category::AgentFinished, format!("{} reports done in “{}”", agent_name(s), s.title), Priority::Success),
    };
    let published = notify::publish(
        conn,
        NewEvent::for_session(category, "mcp", s, title).body(message.clone()).priority(priority).dedupe(format!("mcp:{}:{}", s.id, category.as_str())),
    )?;
    if reason == Request::Question && !matches!(published, notify::Published::Limited) {
        set_kind(conn, &s.id, Some(INPUT), Some(&message))?;
    }
    Ok(published)
}

/// The session's process ended: clear attention and resolve what was waiting.
pub fn clear_on_end(conn: &Connection, session_id: &str) -> Result<()> {
    conn.execute("UPDATE sessions SET attention_kind = NULL, attention_at = NULL, attention = NULL WHERE id = ?1", [session_id])?;
    notify::resolve_session(conn, session_id, &WAITING)?;
    Ok(())
}

// ---- Status shown in the UI ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Running,
    WaitingForInput,
    PermissionRequired,
    /// Claude finished its turn and waits for the next prompt.
    Idle,
    /// Claude stopped on an API error but the process still runs.
    Error,
    Completed,
    Failed,
    Interrupted,
    /// Live, but NEXUS receives no hook signals (hooks blocked by policy or not configured).
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub status: Status,
    /// `reported` by Claude (hook/MCP/process) or `assumed` (no signal — NEXUS does not guess prompts).
    pub certainty: &'static str,
    pub since: Option<i64>,
    pub detail: Option<String>,
}

fn hooks_enabled(s: &Session) -> bool {
    s.integrations.as_ref().and_then(|i| i["hooks"]["state"].as_str()) == Some("enabled")
}

/// Status of a session given whether its process is live (known only to the host / heartbeat).
pub fn status(s: &Session, live: bool) -> SessionStatus {
    let reported = |status, since, detail: Option<String>| SessionStatus { status, certainty: "reported", since, detail };
    if !live || !s.state.is_live() {
        return match s.state {
            State::Exited if s.exit_code.unwrap_or(0) == 0 => reported(Status::Completed, s.ended_at, s.end_reason.clone()),
            State::Exited => reported(Status::Failed, s.ended_at, s.exit_code.map(|c| format!("exit code {c}"))),
            State::Failed => reported(Status::Failed, s.ended_at, s.error.clone()),
            State::Interrupted => reported(Status::Interrupted, s.ended_at, None),
            // A live row whose host is gone: the host reconciliation will mark it interrupted.
            State::Starting | State::Running => SessionStatus { status: Status::Unknown, certainty: "assumed", since: s.last_activity_at, detail: Some("not running in this NEXUS window".into()) },
        };
    }
    if s.kind == Kind::Claude {
        match s.attention_kind.as_deref() {
            Some(PERMISSION) => return reported(Status::PermissionRequired, s.attention_at, s.attention.clone()),
            Some(INPUT) => return reported(Status::WaitingForInput, s.attention_at, s.attention.clone()),
            Some(IDLE) => return reported(Status::Idle, s.attention_at, None),
            Some(ERROR) => return reported(Status::Error, s.attention_at, s.attention.clone()),
            _ => {}
        }
        if s.agent == Some(crate::agent::Kind::Codex) {
            // No hooks for Codex (ADR-0052 §5): only finished turns are reported.
            let detail = "Codex reports finished turns to NEXUS, not permission prompts or questions";
            return SessionStatus { status: Status::Running, certainty: "assumed", since: s.last_activity_at, detail: Some(detail.into()) };
        }
        if !hooks_enabled(s) {
            return SessionStatus { status: Status::Unknown, certainty: "assumed", since: None, detail: Some("NEXUS hooks are off for this session; prompts cannot be detected".into()) };
        }
    }
    SessionStatus { status: Status::Running, certainty: if s.kind == Kind::Claude { "assumed" } else { "reported" }, since: s.last_activity_at, detail: None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{self, HookEvent};
    use crate::session::{self, NewSession};
    use serde_json::json;

    const CSID: &str = "0f8b1c2e-6a4d-4e2a-9d4b-2f1c3b5a7e90";

    fn setup() -> (Connection, String) {
        let conn = crate::db::open_in_memory().unwrap();
        let s = session::insert_starting(&conn, NewSession { claude_session_id: Some(CSID.into()), title: "Checkout".into(), cwd: std::env::temp_dir(), program: "claude".into(), ..Default::default() }).unwrap();
        session::set_integrations(&conn, &s.id, &json!({ "hooks": { "state": "enabled" } })).unwrap();
        session::mark_running(&conn, &s.id, Some(1)).unwrap();
        (conn, s.id)
    }

    fn st(conn: &Connection, id: &str) -> SessionStatus {
        status(&session::get(conn, id).unwrap(), true)
    }

    fn open(conn: &Connection, category: &str) -> usize {
        notify::list(conn, &notify::Filter::default()).unwrap().into_iter().filter(|n| n.category == category && n.resolved_at.is_none()).count()
    }

    #[test]
    fn permission_flow_is_reported_deduplicated_and_resolved_without_acting() {
        let (conn, id) = setup();
        assert_eq!((st(&conn, &id).status, st(&conn, &id).certainty), (Status::Running, "assumed"), "silence is never a prompt");
        let out = activity::handle(&conn, &id, HookEvent::PermissionRequest, &json!({ "session_id": CSID, "tool_name": "Bash", "tool_input": { "command": "rm -rf secret" } })).unwrap();
        assert_eq!(out, activity::Outcome::default(), "no decision output — Claude's own dialog decides");
        // The Notification hook follows ~6 s later with the same situation: one notification.
        activity::handle(&conn, &id, HookEvent::Notification, &json!({ "session_id": CSID, "notification_type": "permission_prompt", "message": "Claude needs your permission to use Bash" })).unwrap();
        let s = st(&conn, &id);
        assert_eq!((s.status, s.certainty, s.detail.as_deref()), (Status::PermissionRequired, "reported", Some("Permission requested: Bash")));
        let n = notify::list(&conn, &notify::Filter::default()).unwrap();
        assert_eq!(n.len(), 1, "{n:?}");
        assert_eq!((n[0].category.as_str(), n[0].occurrences), ("permission-requested", 2));
        let all = serde_json::to_string(&n).unwrap();
        assert!(!all.contains("rm -rf") && !all.contains("secret"), "tool input is never stored");
        // The tool ran (PostToolUse) → resolved.
        activity::handle(&conn, &id, HookEvent::PostToolUse, &json!({ "session_id": CSID, "tool_name": "Bash", "tool_input": { "command": "ls" } })).unwrap();
        assert_eq!(st(&conn, &id).status, Status::Running);
        assert_eq!(open(&conn, "permission-requested"), 0);
    }

    #[test]
    fn turn_end_idle_and_the_next_prompt() {
        let (conn, id) = setup();
        activity::handle(&conn, &id, HookEvent::Stop, &json!({ "session_id": CSID })).unwrap();
        assert_eq!(st(&conn, &id).status, Status::Idle);
        assert_eq!(open(&conn, "agent-finished"), 1);
        // idle_prompt 60 s later is the same situation: merged, not a second notification.
        activity::handle(&conn, &id, HookEvent::Notification, &json!({ "session_id": CSID, "notification_type": "idle_prompt", "message": "Claude is waiting for your input" })).unwrap();
        assert_eq!(notify::list(&conn, &notify::Filter::default()).unwrap().len(), 1);
        activity::handle(&conn, &id, HookEvent::UserPrompt, &json!({ "session_id": CSID, "prompt": "go on" })).unwrap();
        assert_eq!(st(&conn, &id).status, Status::Running);
        assert_eq!(open(&conn, "agent-finished"), 0);
        activity::handle(&conn, &id, HookEvent::Stop, &json!({ "session_id": CSID })).unwrap();
        assert_eq!(open(&conn, "agent-finished"), 1, "the next turn end notifies again");
    }

    #[test]
    fn questions_elicitations_and_api_errors() {
        let (conn, id) = setup();
        activity::handle(&conn, &id, HookEvent::AskUser, &json!({ "session_id": CSID, "tool_name": "AskUserQuestion", "tool_input": { "questions": [{ "question": "Which DB password?" }] } })).unwrap();
        let s = st(&conn, &id);
        assert_eq!((s.status, s.detail.as_deref()), (Status::WaitingForInput, Some("Claude asked a question")));
        assert!(!serde_json::to_string(&notify::list(&conn, &notify::Filter::default()).unwrap()).unwrap().contains("password"), "question text is not stored");
        activity::handle(&conn, &id, HookEvent::PostToolUse, &json!({ "session_id": CSID, "tool_name": "AskUserQuestion" })).unwrap();
        assert_eq!(open(&conn, "input-required"), 0, "answered");
        activity::handle(&conn, &id, HookEvent::Elicitation, &json!({ "session_id": CSID, "mcp_server_name": "jira<x>", "message": "Pick a project" })).unwrap();
        assert_eq!(st(&conn, &id).detail.as_deref(), Some("MCP server “jira‹x›” asks for input"));
        activity::handle(&conn, &id, HookEvent::StopFailure, &json!({ "session_id": CSID, "error": "authentication_failed", "last_assistant_message": "secret answer" })).unwrap();
        let s = st(&conn, &id);
        assert_eq!((s.status, s.detail.as_deref()), (Status::Error, Some("Claude is not signed in for this profile")));
        assert_eq!(open(&conn, "input-required"), 0, "a failure ends the wait");
        let n = notify::list(&conn, &notify::Filter { priority: Some("action".into()), ..Default::default() }).unwrap();
        assert_eq!(n[0].category, "process-issue");
        assert!(!serde_json::to_string(&n).unwrap().contains("secret answer"));
        activity::handle(&conn, &id, HookEvent::StopFailure, &json!({ "session_id": CSID, "error": "<injected>" })).unwrap();
        assert!(notify::list(&conn, &notify::Filter::default()).unwrap().iter().any(|n| n.body.contains("(unknown)")), "unknown error codes are normalised");
        // A Stop after an error keeps the error visible (no "finished" toast for a failed turn).
        activity::handle(&conn, &id, HookEvent::Stop, &json!({ "session_id": CSID })).unwrap();
        assert_eq!(st(&conn, &id).status, Status::Error);
        activity::handle(&conn, &id, HookEvent::UserPrompt, &json!({ "session_id": CSID })).unwrap();
        assert_eq!(st(&conn, &id).status, Status::Running);
    }

    #[test]
    fn foreign_conversations_and_sessions_without_hooks() {
        let (conn, id) = setup();
        activity::handle(&conn, &id, HookEvent::PermissionRequest, &json!({ "session_id": "11111111-2222-4333-8444-555555555555", "tool_name": "Bash" })).unwrap();
        assert_eq!(st(&conn, &id).status, Status::Running, "another conversation cannot raise attention for this session");
        assert!(notify::list(&conn, &notify::Filter::default()).unwrap().is_empty());

        let s = session::insert_starting(&conn, NewSession { title: "No hooks".into(), cwd: std::env::temp_dir(), program: "claude".into(), ..Default::default() }).unwrap();
        session::mark_running(&conn, &s.id, None).unwrap();
        let u = status(&session::get(&conn, &s.id).unwrap(), true);
        assert_eq!((u.status, u.certainty), (Status::Unknown, "assumed"));
        session::mark_exited(&conn, &s.id, Some(0)).unwrap();
        assert_eq!(status(&session::get(&conn, &s.id).unwrap(), false).status, Status::Completed);
    }

    #[test]
    fn notification_types_table() {
        for (payload, want) in [
            (json!({ "notification_type": "permission_prompt", "tool_name": "Write" }), Some(Signal::Permission { tool: Some("Write".into()), message: None })),
            (json!({ "notification_type": "idle_prompt" }), Some(Signal::TurnEnded)),
            (json!({ "notification_type": "elicitation_dialog" }), Some(Signal::Input { title: "An MCP server asks for input".into() })),
            (json!({ "notification_type": "elicitation_complete" }), Some(Signal::Continued)),
            (json!({ "notification_type": "auth_success" }), Some(Signal::Info)),
            (json!({ "notification_type": "plugin_notification" }), None),
            (json!({ "message": "Claude needs your permission to use Edit" }), Some(Signal::Permission { tool: None, message: Some("Claude needs your permission to use Edit".into()) })),
            (json!({ "message": "Claude is waiting for your input" }), Some(Signal::Input { title: "Claude is waiting for your input".into() })),
        ] {
            assert_eq!(from_notification(&payload), want, "{payload}");
        }
    }
}

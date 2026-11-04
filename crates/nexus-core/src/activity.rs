//! Lifecycle hook events (ADR-0016): deterministic, cheap updates from Claude Code hooks — no model calls, no
//! prompt text, no raw shell commands. Structured facts are stored so they can be consolidated into durable memory
//! later (by a human, or by Claude through the MCP tools when asked).

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

use crate::attention::{self, Signal};
use crate::checkpoint;
use crate::error::Result;
use crate::session::{self, Session};
use crate::{new_id, now_ms};

/// Upper bound of hook-recorded events per session (counters keep counting beyond it).
pub const MAX_EVENTS_PER_SESSION: i64 = 1000;
/// Upper bound of text a SessionStart hook injects into Claude's context.
/// Upper bound for SessionStart context (task data block ≤ 900 plus pointers/checkpoints).
pub const MAX_CONTEXT_CHARS: usize = 1800;
const MAX_ITEM_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    SessionStart,
    SessionEnd,
    PreCompact,
    Stop,
    UserPrompt,
    PostToolUse,
    Notification,
    SubagentStop,
    /// Attention events (ADR-0025) — observed only; NEXUS never answers them.
    PermissionRequest,
    StopFailure,
    /// `PreToolUse` with matcher `AskUserQuestion`: Claude is about to ask the user a question.
    AskUser,
    Elicitation,
}

impl HookEvent {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "session-start" => Self::SessionStart,
            "session-end" => Self::SessionEnd,
            "pre-compact" => Self::PreCompact,
            "stop" => Self::Stop,
            "user-prompt" => Self::UserPrompt,
            "post-tool-use" => Self::PostToolUse,
            "notification" => Self::Notification,
            "subagent-stop" => Self::SubagentStop,
            "permission-request" => Self::PermissionRequest,
            "stop-failure" => Self::StopFailure,
            "ask-user" => Self::AskUser,
            "elicitation" => Self::Elicitation,
            _ => return None,
        })
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    /// Text for `hookSpecificOutput.additionalContext` (SessionStart only).
    pub additional_context: Option<String>,
}

fn clip(s: &str, max: usize) -> String {
    let clean: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= max {
        clean
    } else {
        format!("{}…", clean.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

fn one_of<'a>(value: Option<&'a str>, allowed: &[&'a str], fallback: &'a str) -> &'a str {
    value.filter(|v| allowed.contains(v)).unwrap_or(fallback)
}

fn hook_events(conn: &Connection, session_id: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM session_events WHERE session_id = ?1 AND source = 'hook'",
        [session_id],
        |r| r.get(0),
    )?)
}

fn record(conn: &Connection, session_id: &str, kind: &str, summary: &str, data: Option<Value>) -> Result<()> {
    if hook_events(conn, session_id)? >= MAX_EVENTS_PER_SESSION {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO session_events(id, session_id, kind, summary, data, source, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'hook', ?6)",
        params![new_id(), session_id, kind, summary, data.map(|d| d.to_string()), now_ms()],
    )?;
    Ok(())
}

fn touch(conn: &Connection, session_id: &str, set: &str) -> Result<()> {
    conn.execute(
        &format!("UPDATE sessions SET last_activity_at = ?2{set} WHERE id = ?1"),
        params![session_id, now_ms()],
    )?;
    Ok(())
}

/// Handle one hook invocation for the NEXUS session `nexus_session_id`. Unknown sessions are ignored.
pub fn handle(conn: &Connection, nexus_session_id: &str, event: HookEvent, payload: &Value) -> Result<Outcome> {
    let Ok(s) = session::get(conn, nexus_session_id) else {
        return Ok(Outcome::default());
    };
    let str_field = |k: &str| payload.get(k).and_then(Value::as_str);
    if !matches!(event, HookEvent::SessionStart) && !attention::owns(&s, payload) {
        // A different Claude conversation (e.g. this session's settings file reused elsewhere): not this session's.
        tracing::debug!(session = %s.id, "hook event for a foreign conversation ignored");
        return Ok(Outcome::default());
    }
    match event {
        HookEvent::SessionStart => {
            let source = one_of(str_field("source"), &["startup", "resume", "clear", "compact", "fork"], "other");
            let mut data = json!({ "source": source });
            // After /clear (or a fork) Claude continues in a *new* conversation: follow it so Resume picks it up.
            if let Some(sid) = str_field("session_id").filter(|v| uuid::Uuid::parse_str(v).is_ok())
                && s.claude_session_id.as_deref() != Some(sid)
            {
                data["previousClaudeSessionId"] = json!(s.claude_session_id);
                conn.execute("UPDATE sessions SET claude_session_id = ?2 WHERE id = ?1", params![s.id, sid])?;
            }
            if let Some(tp) = str_field("transcript_path").filter(|p| p.len() < 2048 && Path::new(p).is_absolute()) {
                conn.execute("UPDATE sessions SET transcript_path = ?2 WHERE id = ?1", params![s.id, tp])?;
            }
            record(conn, &s.id, "session-start", source, Some(data))?;
            touch(conn, &s.id, ", attention = NULL, attention_kind = NULL, attention_at = NULL")?;
            Ok(Outcome {
                additional_context: recovery_context(conn, &s, source)?,
            })
        }
        HookEvent::SessionEnd => {
            let reason = one_of(
                str_field("reason"),
                &["clear", "resume", "logout", "prompt_input_exit", "bypass_permissions_disabled", "other"],
                "other",
            );
            record(conn, &s.id, "session-end", reason, None)?;
            conn.execute("UPDATE sessions SET end_reason = ?2, last_activity_at = ?3 WHERE id = ?1", params![s.id, reason, now_ms()])?;
            attention::clear_on_end(conn, &s.id)?;
            // Deterministic, SQL-only: candidate memories for human review (ADR-0022). Never fails the hook.
            if let Err(e) = crate::knowledge::proposals::extract_for_session(conn, &s.id) {
                tracing::debug!(error = %e, "memory proposal extraction skipped");
            }
            Ok(Outcome::default())
        }
        HookEvent::PreCompact => {
            let trigger = one_of(str_field("trigger"), &["manual", "auto"], "auto");
            record(conn, &s.id, "compact", trigger, None)?;
            touch(conn, &s.id, "")?;
            Ok(Outcome::default())
        }
        HookEvent::Stop => {
            touch(conn, &s.id, ", turns = turns + 1")?;
            attention::apply(conn, &s, Signal::TurnEnded)?;
            Ok(Outcome::default())
        }
        HookEvent::UserPrompt => {
            touch(conn, &s.id, ", prompts = prompts + 1")?;
            attention::apply(conn, &s, Signal::Continued)?;
            Ok(Outcome::default())
        }
        HookEvent::PermissionRequest => {
            let tool = attention::tool_name(payload);
            if s.attention_kind.as_deref() != Some(attention::PERMISSION) {
                record(conn, &s.id, "attention", "permission", tool.as_ref().map(|t| json!({ "tool": t })))?;
            }
            attention::apply(conn, &s, Signal::Permission { tool, message: None })?;
            Ok(Outcome::default())
        }
        HookEvent::AskUser => {
            record(conn, &s.id, "attention", "question", None)?;
            attention::apply(conn, &s, Signal::Input { title: "Claude asked a question".into() })?;
            Ok(Outcome::default())
        }
        HookEvent::Elicitation => {
            let server = crate::knowledge::tasks::escape_untrusted(str_field("mcp_server_name").unwrap_or("MCP"), 60);
            record(conn, &s.id, "attention", "elicitation", None)?;
            attention::apply(conn, &s, Signal::Input { title: format!("MCP server “{server}” asks for input") })?;
            Ok(Outcome::default())
        }
        HookEvent::StopFailure => {
            let error = str_field("error").unwrap_or("unknown").to_string();
            record(conn, &s.id, "stop-failure", attention::failure_title(&error), None)?;
            touch(conn, &s.id, ", turns = turns + 1")?;
            attention::apply(conn, &s, Signal::Failure { error })?;
            Ok(Outcome::default())
        }
        HookEvent::SubagentStop => {
            touch(conn, &s.id, ", subagents = subagents + 1")?;
            Ok(Outcome::default())
        }
        HookEvent::Notification => {
            let message = clip(str_field("message").unwrap_or("Claude is waiting for you"), MAX_ITEM_CHARS);
            if s.attention.as_deref() != Some(message.as_str()) {
                record(conn, &s.id, "attention", &message, None)?;
            }
            match attention::from_notification(payload) {
                Some(Signal::Input { title }) => attention::apply(conn, &s, Signal::Input { title: clip(&title, MAX_ITEM_CHARS) })?,
                Some(signal) => attention::apply(conn, &s, signal)?,
                None => touch(conn, &s.id, "")?,
            }
            Ok(Outcome::default())
        }
        HookEvent::PostToolUse => {
            let tool = str_field("tool_name").unwrap_or_default();
            let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
            match tool {
                "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => {
                    let file = input.get("file_path").or_else(|| input.get("notebook_path")).and_then(Value::as_str);
                    if let Some(file) = file {
                        let rel = project_relative(&s.cwd, file);
                        let seen: Option<i64> = conn
                            .query_row(
                                "SELECT 1 FROM session_events WHERE session_id = ?1 AND kind = 'file' AND summary = ?2",
                                params![s.id, rel],
                                |r| r.get(0),
                            )
                            .optional()?;
                        if seen.is_none() {
                            record(conn, &s.id, "file", &rel, Some(json!({ "tool": tool })))?;
                        }
                    }
                }
                "Bash" => {
                    if let Some(category) = input.get("command").and_then(Value::as_str).and_then(classify_command) {
                        record(conn, &s.id, "command", category, None)?;
                    }
                }
                _ => {}
            }
            touch(conn, &s.id, "")?;
            attention::apply(conn, &s, Signal::Continued)?;
            Ok(Outcome::default())
        }
    }
}

/// A file path relative to the session's directory, with forward slashes. Paths outside the project keep only
/// their file name — the event log never records unrelated parts of the user's file system.
pub fn project_relative(cwd: &Path, file: &str) -> String {
    let p = Path::new(file);
    let joined = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
    let rel = if cfg!(windows) {
        let (f, c) = (joined.to_string_lossy().replace('/', "\\"), cwd.to_string_lossy().replace('/', "\\"));
        f.to_lowercase()
            .strip_prefix(&format!("{}\\", c.trim_end_matches('\\').to_lowercase()))
            .map(|_| f[c.trim_end_matches('\\').len() + 1..].to_string())
    } else {
        joined.strip_prefix(cwd).ok().map(|r| r.to_string_lossy().into_owned())
    };
    match rel {
        Some(r) if !r.is_empty() && !r.split(['/', '\\']).any(|part| part == "..") => clip(&r.replace('\\', "/"), 300),
        _ => format!(
            "(outside project) {}",
            clip(&p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), 120)
        ),
    }
}

/// Deterministic category of a shell command, or `None` when it is not notable. The command text itself is never
/// stored (it may contain secrets).
pub fn classify_command(cmd: &str) -> Option<&'static str> {
    const RANK: &[&str] = &["git-commit", "git-push", "test", "build", "dependencies"];
    let mut best: Option<&'static str> = None;
    for part in cmd.split([';', '|', '&', '\n']) {
        let words: Vec<&str> = part
            .split_whitespace()
            .skip_while(|w| w.contains('=') && !w.starts_with('-')) // leading FOO=bar assignments
            .collect();
        let Some(&first) = words.first() else { continue };
        let tool = first.rsplit(['/', '\\']).next().unwrap_or(first).trim_end_matches(".exe");
        let rest: Vec<&str> = words[1..].to_vec();
        let found = match tool {
            "git" => git_subcommand(&rest).and_then(|sub| match sub {
                "commit" => Some("git-commit"),
                "push" => Some("git-push"),
                _ => None,
            }),
            "npm" | "pnpm" | "yarn" | "bun" => match rest.as_slice() {
                ["test", ..] | ["t", ..] | ["run", "test", ..] => Some("test"),
                ["run", "build", ..] | ["build", ..] => Some("build"),
                ["install", ..] | ["i", ..] | ["ci", ..] | ["add", ..] => Some("dependencies"),
                _ => None,
            },
            "npx" => match rest.first() {
                Some(&"vitest" | &"jest" | &"playwright" | &"mocha") => Some("test"),
                Some(&"vite" | &"tsc") if rest.get(1) != Some(&"--version") => Some("build"),
                _ => None,
            },
            "cargo" => match rest.first() {
                Some(&"test" | &"nextest") => Some("test"),
                Some(&"build") => Some("build"),
                Some(&"add") => Some("dependencies"),
                _ => None,
            },
            "go" | "dotnet" | "flutter" => match rest.first() {
                Some(&"test") => Some("test"),
                Some(&"build") => Some("build"),
                _ => None,
            },
            "pytest" | "vitest" | "jest" | "phpunit" => Some("test"),
            "python" | "python3" | "py" if rest.starts_with(&["-m", "pytest"]) => Some("test"),
            "php" if rest.starts_with(&["artisan", "test"]) => Some("test"),
            "tsc" => Some("build"),
            "pip" | "pip3" if rest.first() == Some(&"install") => Some("dependencies"),
            "uv" if matches!(rest.first(), Some(&"add" | &"sync")) => Some("dependencies"),
            _ => None,
        };
        if let Some(f) = found {
            let better = best.is_none_or(|b| RANK.iter().position(|r| *r == f) < RANK.iter().position(|r| *r == b));
            if better {
                best = Some(f);
            }
        }
    }
    best
}

fn git_subcommand<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        if a == "-C" || a == "-c" {
            i += 2;
        } else if a.starts_with('-') {
            i += 1;
        } else {
            return Some(a);
        }
    }
    None
}

fn mcp_enabled(s: &Session) -> bool {
    s.integrations.as_ref().and_then(|i| i["mcp"]["state"].as_str()) == Some("enabled")
}

/// Bounded recovery text for SessionStart. Lazy by design: a fresh start or `/clear` gets at most a one-line
/// pointer (plus the task data block for a task session); a resumed conversation gets the task's current state and
/// the latest project checkpoints; after a compaction Claude gets back the checkpoints and files of *this* session.
/// Everything that originates from workspace files or from Claude is escaped data (threat T22).
fn recovery_context(conn: &Connection, s: &Session, source: &str) -> Result<Option<String>> {
    use crate::knowledge::tasks::escape_untrusted as esc;
    let tools = if mcp_enabled(s) {
        " Details: NEXUS MCP tools get_resume_summary, get_session_context, get_checkpoints, get_session_activity."
    } else {
        ""
    };
    let project_cps = |limit| match &s.project_id {
        Some(p) => checkpoint::for_project(conn, p, limit),
        None => checkpoint::for_session(conn, &s.id, limit),
    };
    let fresh = !matches!(source, "compact" | "resume");
    let task = task_data(conn, s, fresh);
    let text = match source {
        "compact" => {
            let cps = checkpoint::for_session(conn, &s.id, 5)?;
            let files: Vec<String> = conn
                .prepare("SELECT summary FROM session_events WHERE session_id = ?1 AND kind = 'file' ORDER BY created_at DESC LIMIT 10")?
                .query_map([&s.id], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            if cps.is_empty() && files.is_empty() && task.is_none() {
                return Ok(None);
            }
            let mut t = String::new();
            if let Some(block) = &task {
                t.push_str(block);
                t.push('\n');
            }
            t.push_str("NEXUS recovery after compaction (data recorded earlier in this session):");
            for c in &cps {
                t.push_str(&format!("\n- [{}] {}", c.kind, esc(&c.summary, MAX_ITEM_CHARS)));
            }
            if !files.is_empty() {
                t.push_str(&format!("\n- files changed: {}", esc(&files.join(", "), 400)));
            }
            t.push_str(tools);
            t
        }
        "resume" => {
            let cps = project_cps(3)?;
            if cps.is_empty() && task.is_none() {
                return Ok(None);
            }
            let mut t = task.clone().unwrap_or_default();
            if !cps.is_empty() {
                if !t.is_empty() {
                    t.push('\n');
                }
                t.push_str("NEXUS: latest checkpoints for this project (data, newest first):");
                for c in &cps {
                    t.push_str(&format!("\n- [{}] {} ({})", c.kind, esc(&c.summary, MAX_ITEM_CHARS), esc(&c.session_title, 60)));
                }
            }
            t.push_str(tools);
            t
        }
        _ => {
            let mut parts = Vec::new();
            if !tools.is_empty() {
                let n = project_cps(50)?.len();
                if n > 0 {
                    parts.push(format!("NEXUS: {n} checkpoint(s) from earlier sessions of this project are available."));
                }
                if let Some(line) = knowledge_line(conn, s)? {
                    parts.push(line);
                }
            }
            let pointer = (!parts.is_empty()).then(|| format!("{}{tools} Workspace knowledge: get_context_index, search_knowledge.", parts.join(" ")));
            match (task, pointer) {
                (None, None) => return Ok(None),
                (Some(block), None) => block,
                (None, Some(p)) => p,
                (Some(block), Some(p)) => format!("{block}\n{p}"),
            }
        }
    };
    Ok(Some(clip_block(&text, MAX_CONTEXT_CHARS)))
}

/// "NEXUS knowledge: 3 rules, 2 open tasks." from the index (no file access — hooks must stay fast).
fn knowledge_line(conn: &Connection, s: &Session) -> Result<Option<String>> {
    use crate::knowledge::{Kind, index};
    let Some(ws) = &s.workspace_id else { return Ok(None) };
    let project = s.project_id.as_deref().and_then(|id| crate::project::get(conn, id).ok());
    let stacks = project.as_ref().map(|p| p.stack.stacks.clone()).unwrap_or_default();
    let summary = crate::knowledge::context::summary(conn, ws, project.as_ref().map(|p| (p.id.as_str(), stacks.as_slice())))?;
    let mut line = crate::knowledge::context::summary_line(&summary);
    // Branch-aware memory (ADR-0018): counted by name from the index — no Git call in a hook.
    if let Some(b) = &s.branch {
        let filter = index::Filter { kinds: vec![Kind::Note, Kind::Decision], project_id: s.project_id.clone(), ..Default::default() };
        let n = index::list(conn, ws, &filter)?.iter().filter(|i| i.branch.as_deref() == Some(b.as_str())).count();
        if n > 0 {
            let base = line.unwrap_or_else(|| "NEXUS knowledge:".into());
            line = Some(format!("{base} {n} {} recorded for branch {b}.", if n == 1 { "note is" } else { "notes are" }));
        }
    }
    Ok(line)
}

/// The session's task as a data block from its file: the full (bounded) block for a fresh session, the current state
/// after resume/compaction (a resumed conversation keeps its original context).
fn task_data(conn: &Connection, s: &Session, fresh: bool) -> Option<String> {
    let ws = crate::workspace::get(conn, s.workspace_id.as_deref()?).ok().filter(|w| w.available)?;
    let v = crate::knowledge::tasks::view(conn, &ws, s.task_id.as_deref()?, 2000).ok()?;
    Some(if fresh { crate::knowledge::tasks::data_block(&v) } else { crate::knowledge::tasks::status_data(&v) })
}

fn clip_block(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max - 1).collect::<String>())
}

/// Structured activity of one session (for the MCP server and the desktop).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub session_id: String,
    pub title: String,
    pub turns: i64,
    pub prompts: i64,
    pub subagents: i64,
    pub files: Vec<String>,
    /// Notable command categories with counts, e.g. `[("test", 3), ("git-commit", 1)]`.
    pub commands: Vec<(String, i64)>,
    pub compactions: i64,
    pub end_reason: Option<String>,
    pub last_activity_at: Option<i64>,
}

pub fn activity(conn: &Connection, session_id: &str) -> Result<Activity> {
    let s = session::get(conn, session_id)?;
    let files: Vec<String> = conn
        .prepare("SELECT summary FROM session_events WHERE session_id = ?1 AND kind = 'file' ORDER BY created_at LIMIT 200")?
        .query_map([session_id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let commands: Vec<(String, i64)> = conn
        .prepare("SELECT summary, COUNT(*) FROM session_events WHERE session_id = ?1 AND kind = 'command' GROUP BY summary ORDER BY COUNT(*) DESC")?
        .query_map([session_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let compactions: i64 = conn.query_row(
        "SELECT COUNT(*) FROM session_events WHERE session_id = ?1 AND kind = 'compact'",
        [session_id],
        |r| r.get(0),
    )?;
    Ok(Activity {
        session_id: s.id,
        title: s.title,
        turns: s.turns,
        prompts: s.prompts,
        subagents: s.subagents,
        files,
        commands,
        compactions,
        end_reason: s.end_reason,
        last_activity_at: s.last_activity_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::{CheckpointKind, Source};
    use crate::session::NewSession;

    const CSID: &str = "0f8b1c2e-6a4d-4e2a-9d4b-2f1c3b5a7e90";
    const NEW_CSID: &str = "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d";

    fn setup(mcp: bool) -> (Connection, String, std::path::PathBuf) {
        let conn = crate::db::open_in_memory().unwrap();
        let cwd = std::env::temp_dir().join("nexus-activity-test");
        let s = session::insert_starting(
            &conn,
            NewSession {
                claude_session_id: Some(CSID.into()),
                title: "Boss fight".into(),
                cwd: cwd.clone(),
                program: "claude".into(),
                ..Default::default()
            },
        )
        .unwrap();
        if mcp {
            session::set_integrations(&conn, &s.id, &json!({ "mcp": { "state": "enabled" }, "hooks": { "state": "enabled" } })).unwrap();
        }
        (conn, s.id, cwd)
    }

    fn get(conn: &Connection, id: &str) -> Session {
        session::get(conn, id).unwrap()
    }

    #[test]
    fn clear_follows_the_new_conversation_and_counters_advance() {
        let (conn, id, _) = setup(true);
        handle(&conn, &id, HookEvent::SessionStart, &json!({ "source": "startup", "session_id": CSID, "transcript_path": std::env::temp_dir().join("t.jsonl") })).unwrap();
        for _ in 0..3 {
            handle(&conn, &id, HookEvent::Stop, &json!({})).unwrap();
        }
        handle(&conn, &id, HookEvent::UserPrompt, &json!({ "prompt": "secret prompt text" })).unwrap();
        handle(&conn, &id, HookEvent::SessionStart, &json!({ "source": "clear", "session_id": NEW_CSID })).unwrap();
        let s = get(&conn, &id);
        assert_eq!(s.claude_session_id.as_deref(), Some(NEW_CSID), "Resume must continue the post-/clear conversation");
        assert_eq!((s.turns, s.prompts), (3, 1));
        assert!(s.transcript_path.is_some());
        let stored: Vec<String> = conn
            .prepare("SELECT summary || coalesce(data, '') FROM session_events").unwrap()
            .query_map([], |r| r.get(0)).unwrap().collect::<std::result::Result<_, _>>().unwrap();
        assert!(!stored.join(" ").contains("secret prompt text"), "prompt text is never stored");
    }

    #[test]
    fn compaction_recovery_returns_this_sessions_checkpoints_and_files() {
        let (conn, id, cwd) = setup(true);
        checkpoint::add(&conn, &id, CheckpointKind::Decision, "Use a state machine for the boss", Source::Claude).unwrap();
        handle(&conn, &id, HookEvent::PostToolUse, &json!({ "tool_name": "Edit", "tool_input": { "file_path": cwd.join("src").join("boss.rs") } })).unwrap();
        handle(&conn, &id, HookEvent::PreCompact, &json!({ "trigger": "auto" })).unwrap();
        let out = handle(&conn, &id, HookEvent::SessionStart, &json!({ "source": "compact", "session_id": CSID })).unwrap();
        let ctx = out.additional_context.unwrap();
        assert!(ctx.contains("[decision] Use a state machine for the boss"), "{ctx}");
        assert!(ctx.contains("files changed: src/boss.rs"), "{ctx}");
        assert!(ctx.contains("get_checkpoints"));
        assert!(ctx.chars().count() <= MAX_CONTEXT_CHARS);
    }

    #[test]
    fn startup_injects_at_most_a_pointer_and_only_with_tools() {
        let (conn, id, _) = setup(true);
        let none = handle(&conn, &id, HookEvent::SessionStart, &json!({ "source": "startup" })).unwrap();
        assert_eq!(none.additional_context, None, "nothing to point at → nothing injected");
        checkpoint::add(&conn, &id, CheckpointKind::Progress, &"long ".repeat(300), Source::Claude).unwrap();
        let ptr = handle(&conn, &id, HookEvent::SessionStart, &json!({ "source": "startup" })).unwrap().additional_context.unwrap();
        assert!(ptr.starts_with("NEXUS: 1 checkpoint(s)") && ptr.len() < 300, "lazy: a pointer: {ptr}");
        assert!(!ptr.contains("long long"), "never checkpoint content at startup");

        let (conn2, id2, _) = setup(false);
        checkpoint::add(&conn2, &id2, CheckpointKind::Progress, "x", Source::Claude).unwrap();
        let out = handle(&conn2, &id2, HookEvent::SessionStart, &json!({ "source": "startup" })).unwrap();
        assert_eq!(out.additional_context, None, "no pointer to tools that this session does not have");
    }

    #[test]
    fn files_are_deduplicated_and_kept_inside_the_project() {
        let (conn, id, cwd) = setup(false);
        for f in [cwd.join("a.rs"), cwd.join("a.rs"), std::path::PathBuf::from(if cfg!(windows) { r"C:\Users\x\secrets\notes.txt" } else { "/home/x/secrets/notes.txt" })] {
            handle(&conn, &id, HookEvent::PostToolUse, &json!({ "tool_name": "Write", "tool_input": { "file_path": f } })).unwrap();
        }
        let a = activity(&conn, &id).unwrap();
        assert_eq!(a.files, ["a.rs", "(outside project) notes.txt"]);
    }

    #[test]
    fn bash_commands_are_categorised_never_stored() {
        let (conn, id, _) = setup(false);
        for cmd in ["cargo test -p core", "export TOKEN=s3cr3t && git commit -m 'wip'", "ls -la", "npm run build"] {
            handle(&conn, &id, HookEvent::PostToolUse, &json!({ "tool_name": "Bash", "tool_input": { "command": cmd } })).unwrap();
        }
        let a = activity(&conn, &id).unwrap();
        let mut cats = a.commands.clone();
        cats.sort();
        assert_eq!(cats, [("build".to_string(), 1), ("git-commit".to_string(), 1), ("test".to_string(), 1)]);
        let all: String = conn
            .prepare("SELECT group_concat(summary || coalesce(data, ''), ' ') FROM session_events").unwrap()
            .query_row([], |r| r.get(0)).unwrap();
        assert!(!all.contains("s3cr3t") && !all.contains("wip"));
    }

    #[test]
    fn command_classification_table() {
        for (cmd, want) in [
            ("git commit -am x", Some("git-commit")),
            ("git -C repo push origin main", Some("git-push")),
            ("git log --grep commit", None),
            ("FOO=1 BAR=2 cargo test", Some("test")),
            ("npx vitest run", Some("test")),
            ("python -m pytest -q", Some("test")),
            ("php artisan test", Some("test")),
            ("pnpm install && pnpm build", Some("build")),
            ("cargo build && git commit -m x", Some("git-commit")),
            ("/usr/bin/git.exe commit", Some("git-commit")),
            ("echo test", None),
            ("", None),
        ] {
            assert_eq!(classify_command(cmd), want, "{cmd}");
        }
    }

    #[test]
    fn attention_lifecycle_end_reason_and_unknown_sessions() {
        let (conn, id, _) = setup(false);
        handle(&conn, &id, HookEvent::Notification, &json!({ "message": "Claude needs your permission to use Bash" })).unwrap();
        handle(&conn, &id, HookEvent::Notification, &json!({ "message": "Claude needs your permission to use Bash" })).unwrap();
        assert_eq!(get(&conn, &id).attention.as_deref(), Some("Claude needs your permission to use Bash"));
        handle(&conn, &id, HookEvent::PostToolUse, &json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } })).unwrap();
        assert_eq!(get(&conn, &id).attention, None, "activity after the prompt clears it");
        handle(&conn, &id, HookEvent::SubagentStop, &json!({})).unwrap();
        handle(&conn, &id, HookEvent::SessionEnd, &json!({ "reason": "prompt_input_exit" })).unwrap();
        let s = get(&conn, &id);
        assert_eq!((s.subagents, s.end_reason.as_deref()), (1, Some("prompt_input_exit")));
        let attention_events: i64 = conn.query_row("SELECT COUNT(*) FROM session_events WHERE kind = 'attention'", [], |r| r.get(0)).unwrap();
        assert_eq!(attention_events, 1, "repeated notifications are recorded once");
        assert_eq!(handle(&conn, "no-such-session", HookEvent::Stop, &json!({})).unwrap(), Outcome::default());
        handle(&conn, &id, HookEvent::SessionEnd, &json!({ "reason": "'; DROP TABLE sessions; --" })).unwrap();
        assert_eq!(get(&conn, &id).end_reason.as_deref(), Some("other"), "unknown values are normalised");
    }

    #[test]
    fn event_volume_is_bounded() {
        let (conn, id, cwd) = setup(false);
        for i in 0..(MAX_EVENTS_PER_SESSION + 20) {
            handle(&conn, &id, HookEvent::PostToolUse, &json!({ "tool_name": "Write", "tool_input": { "file_path": cwd.join(format!("f{i}.rs")) } })).unwrap();
        }
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM session_events WHERE source = 'hook'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, MAX_EVENTS_PER_SESSION);
    }
}

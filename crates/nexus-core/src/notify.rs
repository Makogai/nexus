//! Unified events → notifications (ADR-0025). Every producer — Claude hooks (separate processes), the NEXUS MCP
//! server, session lifecycle, task updates, review queues — calls [`publish`] with a typed [`NewEvent`]. Publishing
//! is deterministic and cheap (SQL only): it sanitises text, deduplicates repeats, rate-limits noisy sources and
//! stores a durable notification. The desktop process delivers pending ones as native notifications (it is the only
//! process with a UI); the in-app notification center reads the same rows.
//!
//! No prompt text, terminal output or secrets are stored: titles and bodies are short, escaped summaries.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::knowledge::tasks::escape_untrusted;
use crate::session::Session;
use crate::{new_id, now_ms, settings};

/// Repeats with the same dedupe key inside this window merge into one notification.
pub const DEDUPE_WINDOW_MS: i64 = 10 * 60 * 1000;
/// At most this many new notifications per session per minute (beyond that they are dropped).
pub const SESSION_RATE_PER_MINUTE: i64 = 8;
/// MCP `request_attention`: at most this many per session in ten minutes.
pub const MCP_RATE_PER_10_MIN: i64 = 3;
pub const RETENTION_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    /// Claude waits for an answer (idle prompt, question, MCP elicitation, explicit request).
    InputRequired,
    /// Claude waits for a permission decision in its own dialog.
    PermissionRequested,
    /// Claude finished responding (turn ended) — the session is idle.
    AgentFinished,
    TaskCompleted,
    TaskBlocked,
    /// The Claude process ended normally.
    SessionFinished,
    /// The Claude process ended with an error, or its host died.
    SessionInterrupted,
    /// Reserved for the workflow engine (no producer yet).
    WorkflowFailed,
    /// Claude stopped on an API error (sign-in, usage limit, overload …) or another recoverable problem.
    ProcessIssue,
    /// Something waits for a person's review: memory proposals, rule approvals, changed kits.
    ReviewRequired,
    Info,
}

pub const ALL_CATEGORIES: [Category; 11] = [
    Category::InputRequired,
    Category::PermissionRequested,
    Category::AgentFinished,
    Category::TaskCompleted,
    Category::TaskBlocked,
    Category::SessionFinished,
    Category::SessionInterrupted,
    Category::WorkflowFailed,
    Category::ProcessIssue,
    Category::ReviewRequired,
    Category::Info,
];

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::InputRequired => "input-required",
            Category::PermissionRequested => "permission-requested",
            Category::AgentFinished => "agent-finished",
            Category::TaskCompleted => "task-completed",
            Category::TaskBlocked => "task-blocked",
            Category::SessionFinished => "session-finished",
            Category::SessionInterrupted => "session-interrupted",
            Category::WorkflowFailed => "workflow-failed",
            Category::ProcessIssue => "process-issue",
            Category::ReviewRequired => "review-required",
            Category::Info => "info",
        }
    }
    pub fn parse(s: &str) -> Option<Category> {
        ALL_CATEGORIES.into_iter().find(|c| c.as_str() == s)
    }
    pub fn default_priority(self) -> Priority {
        match self {
            Category::InputRequired | Category::PermissionRequested => Priority::Action,
            Category::AgentFinished | Category::TaskCompleted | Category::SessionFinished => Priority::Success,
            Category::TaskBlocked | Category::SessionInterrupted | Category::ProcessIssue => Priority::Warning,
            Category::WorkflowFailed => Priority::Critical,
            Category::ReviewRequired | Category::Info => Priority::Info,
        }
    }
    /// Desktop notifications on by default only for what needs the user or finished work.
    fn desktop_default(self) -> bool {
        matches!(
            self,
            Category::InputRequired
                | Category::PermissionRequested
                | Category::AgentFinished
                | Category::TaskBlocked
                | Category::SessionInterrupted
                | Category::WorkflowFailed
                | Category::ProcessIssue
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Priority {
    Critical,
    Action,
    Success,
    Warning,
    Info,
}

impl Priority {
    pub fn as_str(self) -> &'static str {
        match self {
            Priority::Critical => "critical",
            Priority::Action => "action",
            Priority::Success => "success",
            Priority::Warning => "warning",
            Priority::Info => "info",
        }
    }
}

/// A typed event to publish.
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub category: Category,
    pub priority: Option<Priority>,
    /// `hook`, `mcp`, `session`, `task`, `knowledge`, `kit`, `nexus`.
    pub source: &'static str,
    pub title: String,
    pub body: String,
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    pub profile_id: Option<String>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    /// What "Open" navigates to: (`session`|`task`|`proposals`|`rules`|`kit`|`project`, id).
    pub target: Option<(&'static str, String)>,
    /// Repeats with the same key inside [`DEDUPE_WINDOW_MS`] merge (counted, not re-notified).
    pub dedupe_key: Option<String>,
}

impl NewEvent {
    pub fn new(category: Category, source: &'static str, title: impl Into<String>) -> NewEvent {
        NewEvent {
            category,
            priority: None,
            source,
            title: title.into(),
            body: String::new(),
            workspace_id: None,
            project_id: None,
            profile_id: None,
            session_id: None,
            task_id: None,
            target: None,
            dedupe_key: None,
        }
    }

    /// Identity of a session: workspace, project, profile, task, and "Open" goes to the session.
    pub fn for_session(category: Category, source: &'static str, s: &Session, title: impl Into<String>) -> NewEvent {
        let mut e = NewEvent::new(category, source, title);
        e.workspace_id = s.workspace_id.clone();
        e.project_id = s.project_id.clone();
        e.profile_id = s.profile_id.clone();
        e.session_id = Some(s.id.clone());
        e.task_id = s.task_id.clone();
        e.target = Some(("session", s.id.clone()));
        e
    }

    pub fn body(mut self, b: impl Into<String>) -> Self {
        self.body = b.into();
        self
    }
    pub fn dedupe(mut self, key: impl Into<String>) -> Self {
        self.dedupe_key = Some(key.into());
        self
    }
    pub fn target(mut self, kind: &'static str, id: impl Into<String>) -> Self {
        self.target = Some((kind, id.into()));
        self
    }
    pub fn priority(mut self, p: Priority) -> Self {
        self.priority = Some(p);
        self
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    pub category: String,
    pub priority: String,
    pub source: String,
    pub title: String,
    pub body: String,
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    pub profile_id: Option<String>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub workflow_id: Option<String>,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub dedupe_key: Option<String>,
    pub occurrences: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub read_at: Option<i64>,
    pub dismissed_at: Option<i64>,
    pub snoozed_until: Option<i64>,
    pub resolved_at: Option<i64>,
    pub desktop_state: String,
    pub desktop_reason: Option<String>,
}

/// Why [`publish`] did not create a new notification.
#[derive(Debug, Clone, PartialEq)]
pub enum Published {
    Created(Box<Notification>),
    /// Merged into an existing, still-open notification with the same dedupe key.
    Merged(String),
    /// Dropped by the rate limit.
    Limited,
}

const SELECT: &str = "SELECT id, category, priority, source, title, body, workspace_id, project_id, profile_id, session_id, task_id,
    workflow_id, target_kind, target_id, dedupe_key, occurrences, created_at, updated_at, read_at, dismissed_at,
    snoozed_until, resolved_at, desktop_state, desktop_reason FROM notifications";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Notification> {
    Ok(Notification {
        id: r.get(0)?,
        category: r.get(1)?,
        priority: r.get(2)?,
        source: r.get(3)?,
        title: r.get(4)?,
        body: r.get(5)?,
        workspace_id: r.get(6)?,
        project_id: r.get(7)?,
        profile_id: r.get(8)?,
        session_id: r.get(9)?,
        task_id: r.get(10)?,
        workflow_id: r.get(11)?,
        target_kind: r.get(12)?,
        target_id: r.get(13)?,
        dedupe_key: r.get(14)?,
        occurrences: r.get(15)?,
        created_at: r.get(16)?,
        updated_at: r.get(17)?,
        read_at: r.get(18)?,
        dismissed_at: r.get(19)?,
        snoozed_until: r.get(20)?,
        resolved_at: r.get(21)?,
        desktop_state: r.get(22)?,
        desktop_reason: r.get(23)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<Notification> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row).optional()?.ok_or_else(|| CoreError::not_found(format!("no notification {id}")))
}

/// Publish an event. Deterministic SQL only — safe to call from hooks.
pub fn publish(conn: &Connection, e: NewEvent) -> Result<Published> {
    let now = now_ms();
    let title = escape_untrusted(&e.title, 120);
    let body = escape_untrusted(&e.body, 300);
    if let Some(key) = &e.dedupe_key {
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM notifications WHERE dedupe_key = ?1 AND dismissed_at IS NULL AND resolved_at IS NULL AND updated_at >= ?2
                 ORDER BY updated_at DESC LIMIT 1",
                params![key, now - DEDUPE_WINDOW_MS],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            conn.execute(
                "UPDATE notifications SET occurrences = occurrences + 1, updated_at = ?2, read_at = NULL, title = ?3, body = ?4 WHERE id = ?1",
                params![id, now, title, body],
            )?;
            return Ok(Published::Merged(id));
        }
    }
    if let Some(sid) = &e.session_id {
        let recent: i64 = conn.query_row("SELECT COUNT(*) FROM notifications WHERE session_id = ?1 AND created_at >= ?2", params![sid, now - 60_000], |r| r.get(0))?;
        if recent >= SESSION_RATE_PER_MINUTE {
            return Ok(Published::Limited);
        }
        if e.source == "mcp" {
            let mcp: i64 = conn.query_row(
                "SELECT COUNT(*) FROM notifications WHERE session_id = ?1 AND source = 'mcp' AND created_at >= ?2",
                params![sid, now - 600_000],
                |r| r.get(0),
            )?;
            if mcp >= MCP_RATE_PER_10_MIN {
                return Ok(Published::Limited);
            }
        }
    }
    let id = new_id();
    let priority = e.priority.unwrap_or_else(|| e.category.default_priority());
    conn.execute(
        "INSERT INTO notifications(id, category, priority, source, title, body, workspace_id, project_id, profile_id, session_id, task_id,
            target_kind, target_id, dedupe_key, occurrences, created_at, updated_at, desktop_state)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 1, ?15, ?15, 'pending')",
        params![
            id,
            e.category.as_str(),
            priority.as_str(),
            e.source,
            title,
            body,
            e.workspace_id,
            e.project_id,
            e.profile_id,
            e.session_id,
            e.task_id,
            e.target.as_ref().map(|t| t.0),
            e.target.as_ref().map(|t| t.1.clone()),
            e.dedupe_key,
            now
        ],
    )?;
    Ok(Published::Created(Box::new(get(conn, &id)?)))
}

/// The situation behind a session's open action notifications ended (Claude continued, the session ended).
pub fn resolve_session(conn: &Connection, session_id: &str, categories: &[Category]) -> Result<usize> {
    let list = categories.iter().map(|c| format!("'{}'", c.as_str())).collect::<Vec<_>>().join(",");
    Ok(conn.execute(
        &format!("UPDATE notifications SET resolved_at = ?2 WHERE session_id = ?1 AND resolved_at IS NULL AND category IN ({list})"),
        params![session_id, now_ms()],
    )?)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Filter {
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    pub profile_id: Option<String>,
    pub session_id: Option<String>,
    pub priority: Option<String>,
    pub unread_only: bool,
    /// Include dismissed ones (history).
    pub include_dismissed: bool,
    pub limit: Option<u32>,
}

pub fn list(conn: &Connection, f: &Filter) -> Result<Vec<Notification>> {
    let mut sql = format!("{SELECT} WHERE 1 = 1");
    if !f.include_dismissed {
        sql.push_str(" AND dismissed_at IS NULL AND (snoozed_until IS NULL OR snoozed_until <= ?7)");
    } else {
        sql.push_str(" AND ?7 IS NOT NULL");
    }
    if f.unread_only {
        sql.push_str(" AND read_at IS NULL");
    }
    sql.push_str(
        " AND (?1 IS NULL OR workspace_id = ?1) AND (?2 IS NULL OR project_id = ?2) AND (?3 IS NULL OR profile_id = ?3)
          AND (?4 IS NULL OR session_id = ?4) AND (?5 IS NULL OR priority = ?5) ORDER BY updated_at DESC LIMIT ?6",
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(
            params![f.workspace_id, f.project_id, f.profile_id, f.session_id, f.priority, f.limit.unwrap_or(200).min(1000), now_ms()],
            from_row,
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub unread: usize,
    /// Unread or unresolved notifications that need the user (input, permission, critical).
    pub action_required: usize,
}

pub fn counts(conn: &Connection) -> Result<Counts> {
    let visible = "dismissed_at IS NULL AND (snoozed_until IS NULL OR snoozed_until <= ?1)";
    let unread: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM notifications WHERE {visible} AND read_at IS NULL"), [now_ms()], |r| r.get(0))?;
    let action: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM notifications WHERE {visible} AND resolved_at IS NULL AND priority IN ('action', 'critical') AND read_at IS NULL"),
        [now_ms()],
        |r| r.get(0),
    )?;
    Ok(Counts { unread: unread as usize, action_required: action as usize })
}

pub fn mark_read(conn: &Connection, ids: &[String]) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute("UPDATE notifications SET read_at = ?2 WHERE id = ?1 AND read_at IS NULL", params![id, now_ms()])?;
    }
    Ok(n)
}

pub fn mark_all_read(conn: &Connection) -> Result<usize> {
    Ok(conn.execute("UPDATE notifications SET read_at = ?1 WHERE read_at IS NULL", [now_ms()])?)
}

pub fn dismiss(conn: &Connection, ids: &[String]) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute("UPDATE notifications SET dismissed_at = ?2, read_at = COALESCE(read_at, ?2) WHERE id = ?1", params![id, now_ms()])?;
    }
    Ok(n)
}

pub fn snooze(conn: &Connection, id: &str, minutes: i64) -> Result<()> {
    if !(1..=7 * 24 * 60).contains(&minutes) {
        return Err(CoreError::invalid("snooze between 1 minute and 7 days"));
    }
    // Hidden until then; it is delivered to the desktop again when it comes back.
    conn.execute(
        "UPDATE notifications SET snoozed_until = ?2, read_at = NULL, desktop_state = 'pending', desktop_reason = NULL WHERE id = ?1",
        params![id, now_ms() + minutes * 60_000],
    )?;
    Ok(())
}

/// Delete notifications older than the retention period, and dismissed ones when `all_dismissed`.
pub fn cleanup(conn: &Connection, retention_days: i64, all_dismissed: bool) -> Result<usize> {
    let cutoff = now_ms() - retention_days.max(1) * 24 * 3600 * 1000;
    let mut n = conn.execute("DELETE FROM notifications WHERE created_at < ?1", [cutoff])?;
    if all_dismissed {
        n += conn.execute("DELETE FROM notifications WHERE dismissed_at IS NOT NULL", [])?;
    }
    Ok(n)
}

// ---- Preferences -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CategoryPref {
    pub in_app: bool,
    pub desktop: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    pub desktop_enabled: bool,
    pub sound: bool,
    /// Also notify on the desktop for the session that is visible in a focused NEXUS window.
    pub notify_focused_session: bool,
    /// Local time "HH:MM"–"HH:MM"; desktop notifications are held back (in-app still recorded). Critical ones pass.
    pub quiet_hours: Option<(String, String)>,
    pub categories: std::collections::BTreeMap<String, CategoryPref>,
    pub muted_workspaces: Vec<String>,
    pub muted_profiles: Vec<String>,
}

impl Default for CategoryPref {
    fn default() -> Self {
        CategoryPref { in_app: true, desktop: false }
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            desktop_enabled: true,
            sound: true,
            notify_focused_session: false,
            quiet_hours: None,
            categories: ALL_CATEGORIES
                .into_iter()
                .map(|c| (c.as_str().to_string(), CategoryPref { in_app: true, desktop: c.desktop_default() }))
                .collect(),
            muted_workspaces: Vec::new(),
            muted_profiles: Vec::new(),
        }
    }
}

const PREFS_KEY: &str = "notification_prefs";

pub fn prefs(conn: &Connection) -> Result<Prefs> {
    let mut p: Prefs = settings::get(conn, PREFS_KEY)?.unwrap_or_default();
    for c in ALL_CATEGORIES {
        p.categories.entry(c.as_str().to_string()).or_insert(CategoryPref { in_app: true, desktop: c.desktop_default() });
    }
    Ok(p)
}

pub fn set_prefs(conn: &Connection, p: &Prefs) -> Result<()> {
    if let Some((a, b)) = &p.quiet_hours
        && (minutes_of(a).is_none() || minutes_of(b).is_none())
    {
        return Err(CoreError::invalid("quiet hours use HH:MM"));
    }
    settings::set(conn, PREFS_KEY, p)
}

fn minutes_of(hhmm: &str) -> Option<i64> {
    let (h, m) = hhmm.split_once(':')?;
    let (h, m): (i64, i64) = (h.parse().ok()?, m.parse().ok()?);
    ((0..24).contains(&h) && (0..60).contains(&m)).then_some(h * 60 + m)
}

/// What the desktop knows about where the user is looking.
#[derive(Debug, Clone, Default)]
pub struct Focus {
    pub window_focused: bool,
    pub visible_session: Option<String>,
    /// Minutes since local midnight.
    pub local_minutes: i64,
}

/// Should this notification be shown as a native desktop notification? `Err(reason)` explains a suppression.
pub fn desktop_decision(p: &Prefs, n: &Notification, focus: &Focus) -> std::result::Result<(), String> {
    if !p.desktop_enabled {
        return Err("desktop notifications are off".into());
    }
    let cat = p.categories.get(&n.category).cloned().unwrap_or_default();
    if !cat.desktop || !cat.in_app {
        return Err(format!("desktop notifications are off for {}", n.category));
    }
    if n.workspace_id.as_ref().is_some_and(|w| p.muted_workspaces.contains(w)) || n.profile_id.as_ref().is_some_and(|pr| p.muted_profiles.contains(pr)) {
        return Err("muted workspace or profile".into());
    }
    if n.resolved_at.is_some() || n.dismissed_at.is_some() {
        return Err("already handled".into());
    }
    if focus.window_focused && !p.notify_focused_session && (n.session_id.is_none() || n.session_id == focus.visible_session) {
        return Err("the user is looking at it".into());
    }
    if n.priority != Priority::Critical.as_str()
        && let Some((a, b)) = &p.quiet_hours
        && let (Some(a), Some(b)) = (minutes_of(a), minutes_of(b))
    {
        let t = focus.local_minutes;
        let quiet = if a <= b { t >= a && t < b } else { t >= a || t < b };
        if quiet {
            return Err("quiet hours".into());
        }
    }
    Ok(())
}

/// Not yet decided for the desktop (snoozed ones wait until they come back).
pub fn pending_desktop(conn: &Connection, limit: u32) -> Result<Vec<Notification>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE desktop_state = 'pending' AND (snoozed_until IS NULL OR snoozed_until <= ?2) ORDER BY created_at LIMIT ?1"
    ))?;
    let rows = stmt.query_map(params![limit.min(50), now_ms()], from_row)?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Changes whenever a notification is added, merged, read, dismissed, snoozed or resolved (cheap; for UI refresh).
pub fn fingerprint(conn: &Connection) -> Result<String> {
    Ok(conn.query_row(
        "SELECT COUNT(*) || ':' || COALESCE(MAX(updated_at), 0) || ':' || COALESCE(MAX(read_at), 0) || ':' || COALESCE(MAX(dismissed_at), 0)
            || ':' || COALESCE(MAX(resolved_at), 0) || ':' || COALESCE(MAX(snoozed_until), 0) || ':' || SUM(read_at IS NULL)
         FROM notifications",
        [],
        |r| r.get(0),
    )?)
}

pub fn mark_desktop(conn: &Connection, id: &str, sent: bool, reason: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE notifications SET desktop_state = ?2, desktop_reason = ?3 WHERE id = ?1",
        params![id, if sent { "sent" } else { "suppressed" }, reason],
    )?;
    Ok(())
}

// ---- Producers shared by several hosts ----------------------------------------------------------------------------

/// Record a session's process exit and publish what it means. `stopped_by_user`: the user ended it from NEXUS (no
/// notification — they know). `code` = exit code; `None` = killed/unknown.
pub fn session_exited(conn: &Connection, session_id: &str, code: Option<i64>, stopped_by_user: bool) -> Result<()> {
    crate::session::mark_exited(conn, session_id, code)?;
    let s = crate::session::get(conn, session_id)?;
    crate::attention::clear_on_end(conn, &s.id)?;
    if stopped_by_user || s.state != crate::session::State::Exited {
        return Ok(());
    }
    session_ended(conn, &s, code)
}

/// Claude set a task to `done` or `blocked` (through the NEXUS MCP server of session `s`).
pub fn task_status(conn: &Connection, s: &Session, task_id: &str, task_title: &str, status: &str, log: Option<&str>) -> Result<()> {
    let (category, title) = match status {
        "done" => (Category::TaskCompleted, format!("Task done: {task_title}")),
        "blocked" => (Category::TaskBlocked, format!("Task blocked: {task_title}")),
        _ => return Ok(()),
    };
    let mut e = NewEvent::for_session(category, "task", s, title)
        .body(log.unwrap_or_default())
        .target("task", task_id)
        .dedupe(format!("task:{task_id}:{status}"));
    e.task_id = Some(task_id.to_string());
    publish(conn, e)?;
    Ok(())
}

/// Publish the end of a Claude session (`None` = the process or its host vanished).
pub fn session_ended(conn: &Connection, s: &Session, code: Option<i64>) -> Result<()> {
    if s.kind != crate::session::Kind::Claude {
        return Ok(());
    }
    let (cat, title) = match code {
        Some(0) => (Category::SessionFinished, format!("Session “{}” finished", s.title)),
        Some(c) => (Category::SessionInterrupted, format!("Session “{}” ended with exit code {c}", s.title)),
        None => (Category::SessionInterrupted, format!("Session “{}” ended unexpectedly", s.title)),
    };
    publish(conn, NewEvent::for_session(cat, "session", s, title).dedupe(format!("ended:{}", s.id)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{self, NewSession};

    /// A workspace + profile (ids are real rows: sessions reference them) and a session factory.
    fn fixture(conn: &Connection, t: &tempfile::TempDir) -> (String, String) {
        let ws = crate::workspace::create(conn, t.path().join("ws").to_str().unwrap(), "W", "").unwrap();
        let input = crate::claude::profile::ProfileInput { name: "P".into(), config_dir: Some(t.path().join("cfg").to_string_lossy().into_owned()), ..Default::default() };
        let prof = crate::claude::profile::create_managed(conn, &input, &t.path().join("home")).unwrap();
        (ws.id, prof.id)
    }

    fn session_in(conn: &Connection, ids: &(String, String)) -> Session {
        session::insert_starting(conn, NewSession { title: "Feature A".into(), cwd: std::env::temp_dir(), program: "claude".into(), workspace_id: Some(ids.0.clone()), profile_id: Some(ids.1.clone()), ..Default::default() }).unwrap()
    }

    #[test]
    fn publish_dedupes_rate_limits_and_resolves() {
        let conn = crate::db::open_in_memory().unwrap();
        let t = tempfile::tempdir().unwrap();
        let ids = fixture(&conn, &t);
        let s = session_in(&conn, &ids);
        let e = || NewEvent::for_session(Category::PermissionRequested, "hook", &s, "Permission requested: Bash").dedupe(format!("perm:{}", s.id));
        let first = match publish(&conn, e()).unwrap() {
            Published::Created(n) => n,
            other => panic!("{other:?}"),
        };
        assert_eq!((first.priority.as_str(), first.target_kind.as_deref(), first.workspace_id.as_deref()), ("action", Some("session"), Some(ids.0.as_str())));
        assert_eq!(publish(&conn, e()).unwrap(), Published::Merged(first.id.clone()), "duplicates merge");
        assert_eq!(get(&conn, &first.id).unwrap().occurrences, 2);
        assert_eq!(counts(&conn).unwrap(), Counts { unread: 1, action_required: 1 });

        assert_eq!(resolve_session(&conn, &s.id, &[Category::PermissionRequested]).unwrap(), 1);
        assert_eq!(counts(&conn).unwrap().action_required, 0, "resolved when Claude continued");
        assert!(matches!(publish(&conn, e()).unwrap(), Published::Created(_)), "a new request after resolution is new");

        for i in 0..20 {
            publish(&conn, NewEvent::for_session(Category::Info, "hook", &s, format!("n{i}"))).unwrap();
        }
        assert!(list(&conn, &Filter { session_id: Some(s.id.clone()), ..Default::default() }).unwrap().len() <= SESSION_RATE_PER_MINUTE as usize);
        let s2 = session_in(&conn, &ids);
        for _ in 0..5 {
            publish(&conn, NewEvent::for_session(Category::InputRequired, "mcp", &s2, "x")).unwrap();
        }
        assert_eq!(list(&conn, &Filter { session_id: Some(s2.id.clone()), ..Default::default() }).unwrap().len(), MCP_RATE_PER_10_MIN as usize);
    }

    #[test]
    fn text_is_sanitised_and_preferences_decide_desktop_delivery() {
        let conn = crate::db::open_in_memory().unwrap();
        let t = tempfile::tempdir().unwrap();
        let ids = fixture(&conn, &t);
        let s = session_in(&conn, &ids);
        let Published::Created(n) = publish(&conn, NewEvent::for_session(Category::InputRequired, "hook", &s, "Line1\nLine2 <script>").body("x".repeat(1000))).unwrap() else { panic!() };
        assert_eq!(n.title, "Line1 Line2 ‹script›");
        assert!(n.body.chars().count() <= 300);
        let p = prefs(&conn).unwrap();
        let away = Focus { window_focused: false, visible_session: None, local_minutes: 12 * 60 };
        assert!(desktop_decision(&p, &n, &away).is_ok());
        let looking = Focus { window_focused: true, visible_session: Some(s.id.clone()), local_minutes: 12 * 60 };
        assert!(desktop_decision(&p, &n, &looking).is_err(), "no toast for the session the user is looking at");
        let elsewhere = Focus { window_focused: true, visible_session: Some("other".into()), ..looking.clone() };
        assert!(desktop_decision(&p, &n, &elsewhere).is_ok(), "background sessions do notify");
        let quiet = Prefs { quiet_hours: Some(("22:00".into(), "07:00".into())), ..p.clone() };
        assert!(desktop_decision(&quiet, &n, &Focus { local_minutes: 23 * 60, ..away.clone() }).is_err());
        assert!(desktop_decision(&quiet, &n, &Focus { local_minutes: 8 * 60, ..away.clone() }).is_ok());
        let muted = Prefs { muted_profiles: vec![ids.1.clone()], ..p.clone() };
        assert!(desktop_decision(&muted, &n, &away).is_err());
        let Published::Created(info) = publish(&conn, NewEvent::new(Category::Info, "nexus", "Indexed")).unwrap() else { panic!() };
        assert!(desktop_decision(&p, &info, &away).is_err(), "informational events stay in-app by default");
        assert!(set_prefs(&conn, &Prefs { quiet_hours: Some(("25:00".into(), "x".into())), ..p }).is_err());
    }
}

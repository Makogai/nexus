//! Notification center and attention commands (ADR-0025).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use nexus_core::notify::{self, Counts, Filter, Notification, Prefs};
use nexus_core::session;

use crate::error::{IpcError, IpcResult, blocking};
use crate::notifier::{self, Availability};
use crate::state::{Ctx, ViewFocus};

#[tauri::command]
pub async fn notifications_list(ctx: State<'_, Ctx>, filter: Option<Filter>) -> IpcResult<Vec<Notification>> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(notify::list(&ctx.db(), &filter.unwrap_or_default())?)).await
}

#[tauri::command]
pub fn notifications_counts(ctx: State<'_, Ctx>) -> IpcResult<Counts> {
    Ok(notify::counts(&ctx.db())?)
}

#[tauri::command]
pub fn notifications_mark_read(ctx: State<'_, Ctx>, ids: Vec<String>, all: Option<bool>) -> IpcResult<usize> {
    let db = ctx.db();
    Ok(if all.unwrap_or(false) { notify::mark_all_read(&db)? } else { notify::mark_read(&db, &ids)? })
}

#[tauri::command]
pub fn notifications_dismiss(ctx: State<'_, Ctx>, ids: Vec<String>) -> IpcResult<usize> {
    Ok(notify::dismiss(&ctx.db(), &ids)?)
}

#[tauri::command]
pub fn notifications_snooze(ctx: State<'_, Ctx>, id: String, minutes: i64) -> IpcResult<()> {
    Ok(notify::snooze(&ctx.db(), &id, minutes)?)
}

#[tauri::command]
pub fn notifications_prefs(ctx: State<'_, Ctx>) -> IpcResult<Prefs> {
    Ok(notify::prefs(&ctx.db())?)
}

#[tauri::command]
pub fn notifications_set_prefs(ctx: State<'_, Ctx>, prefs: Prefs) -> IpcResult<Prefs> {
    let db = ctx.db();
    notify::set_prefs(&db, &prefs)?;
    Ok(notify::prefs(&db)?)
}

/// The UI reports which session terminal is visible (and the local time zone) for focus suppression/quiet hours.
#[tauri::command]
pub fn notifications_set_view(ctx: State<'_, Ctx>, visible_session: Option<String>, utc_offset_minutes: i64) {
    ctx.attention.set_view(ViewFocus { visible_session, utc_offset_minutes: utc_offset_minutes.clamp(-24 * 60, 24 * 60) });
}

#[tauri::command]
pub fn notifications_availability(app: AppHandle, ctx: State<'_, Ctx>) -> Availability {
    ctx.notifier.availability(&app)
}

/// Show a sample desktop notification now (not stored), so the user can check the OS settings.
#[tauri::command]
pub fn notifications_test(app: AppHandle, ctx: State<'_, Ctx>) -> IpcResult<()> {
    let sound = notify::prefs(&ctx.db())?.sound;
    let sample = Notification {
        id: nexus_core::new_id(),
        category: "info".into(),
        priority: "info".into(),
        source: "nexus".into(),
        title: "NEXUS notifications work".into(),
        body: "This is how NEXUS tells you that a session needs you.".into(),
        workspace_id: None,
        project_id: None,
        profile_id: None,
        session_id: None,
        task_id: None,
        workflow_id: None,
        target_kind: None,
        target_id: None,
        dedupe_key: None,
        occurrences: 1,
        created_at: nexus_core::now_ms(),
        updated_at: nexus_core::now_ms(),
        read_at: None,
        dismissed_at: None,
        snoozed_until: None,
        resolved_at: None,
        desktop_state: "sent".into(),
        desktop_reason: None,
    };
    ctx.notifier.show(&app, &sample, &sample.title, &sample.body, sound).map_err(|e| IpcError::new("notification", e))
}

/// A click on a notification that started NEXUS (`nexus://notification/<id>`): the UI asks once after it loaded.
#[tauri::command]
pub fn notifications_take_activation(ctx: State<'_, Ctx>) -> Option<String> {
    ctx.attention.take_activation()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub kind: Option<String>,
    pub id: Option<String>,
    /// The target still exists (a session row, …). The UI explains when it does not.
    pub exists: bool,
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    /// Session targets: its terminal can be shown in this window.
    pub attached: bool,
    pub live_elsewhere: bool,
    pub resumable: bool,
}

/// Resolve where a notification leads (click in the center or on a desktop notification). Marks it read.
#[tauri::command]
pub async fn notification_target(ctx: State<'_, Ctx>, id: String) -> IpcResult<Target> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let n = notify::get(&ctx.db(), &id)?;
        notify::mark_read(&ctx.db(), std::slice::from_ref(&n.id))?;
        let mut t = Target {
            kind: n.target_kind.clone(),
            id: n.target_id.clone(),
            exists: true,
            workspace_id: n.workspace_id.clone(),
            project_id: n.project_id.clone(),
            attached: false,
            live_elsewhere: false,
            resumable: false,
        };
        if n.target_kind.as_deref() == Some("session")
            && let Some(sid) = &n.target_id
        {
            match session::get(&ctx.db(), sid) {
                Ok(s) => {
                    let running = ctx.pty.is_running(&s.id);
                    t.attached = running || ctx.pty.is_attached(&s.id);
                    t.live_elsewhere = !running && s.state.is_live() && s.host_id.as_deref() != Some(ctx.host.id.as_str());
                    t.resumable = !running && s.claude_session_id.is_some() && s.cwd.is_dir();
                    t.workspace_id = s.workspace_id.or(t.workspace_id);
                }
                Err(_) => t.exists = false,
            }
        }
        Ok(t)
    })
    .await
}

/// E2E builds with the recording notifier only: what would have been shown on the desktop.
#[tauri::command]
pub fn e2e_notifications_recorded(ctx: State<'_, Ctx>) -> IpcResult<Vec<Notification>> {
    ctx.notifier.recorded().map(|r| r.0).ok_or_else(|| IpcError::invalid("not available in this build"))
}

/// E2E builds with the recording notifier only: notifications withdrawn from the OS notification center.
#[tauri::command]
pub fn e2e_notifications_withdrawn(ctx: State<'_, Ctx>) -> IpcResult<Vec<String>> {
    ctx.notifier.recorded().map(|r| r.1).ok_or_else(|| IpcError::invalid("not available in this build"))
}

/// E2E builds with the recording notifier only: the focus the delivery decision sees (OS window focus).
#[tauri::command]
pub fn e2e_focus_state(app: AppHandle, ctx: State<'_, Ctx>) -> IpcResult<serde_json::Value> {
    if ctx.notifier.recorded().is_none() {
        return Err(IpcError::invalid("not available in this build"));
    }
    let f = notifier::focus(&app, ctx.inner());
    Ok(serde_json::json!({ "windowFocused": f.window_focused, "visibleSession": f.visible_session }))
}

/// E2E builds with the recording notifier only: simulate a click on a desktop notification.
#[tauri::command]
pub fn e2e_notification_activate(app: AppHandle, ctx: State<'_, Ctx>, id: String) -> IpcResult<()> {
    if ctx.notifier.recorded().is_none() {
        return Err(IpcError::invalid("not available in this build"));
    }
    notifier::activate(&app, &id);
    Ok(())
}

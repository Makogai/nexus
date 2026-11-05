//! Sync Center (ADR-0031, M29): Git-based workspace synchronisation. Network operations (fetch, push, clone) can take
//! long, so every command opens its own database connection on a blocking thread instead of holding the shared one.
//! Commits and pushes need the token of the review the person confirmed; nothing is published automatically.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use nexus_core::db::{self, Connection};
use nexus_core::workspace::{self, Workspace};
use nexus_core::workspace_sync::{self as sync, ops};

use crate::error::{IpcError, IpcResult, blocking};
use crate::state::Ctx;

pub const SYNC_EVENT: &str = "nexus://sync";

fn conn(ctx: &Ctx) -> IpcResult<Connection> {
    Ok(db::open(&ctx.data_dir.join(db::DB_FILE))?)
}

fn ws(conn: &Connection, id: &str) -> IpcResult<Workspace> {
    let ws = workspace::get(conn, id)?;
    if !ws.available {
        return Err(IpcError::invalid("the workspace folder is not available on this machine"));
    }
    Ok(ws)
}

/// Run a sync operation with its own connection; tell the views afterwards (also after a refusal or failure, since
/// the history changed).
async fn op<T: Send + 'static>(app: AppHandle, ctx: &Ctx, workspace_id: String, f: impl FnOnce(&ops::Sync<'_>) -> nexus_core::Result<T> + Send + 'static) -> IpcResult<T> {
    let ctx = ctx.clone();
    let r = blocking(move || {
        let c = conn(&ctx)?;
        let w = ws(&c, &workspace_id)?;
        let s = ops::Sync { conn: &c, ws: &w, data_dir: &ctx.data_dir, home: &ctx.home, host_id: &ctx.host.id };
        Ok(f(&s)?)
    })
    .await;
    let _ = app.emit(SYNC_EVENT, ());
    r
}

#[tauri::command]
pub async fn sync_status(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<sync::SyncStatus> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = conn(&ctx)?;
        let w = ws(&c, &workspace_id)?;
        Ok(sync::status(&c, &w, &ctx.data_dir)?)
    })
    .await
}

#[tauri::command]
pub async fn sync_fetch(app: AppHandle, ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<ops::Fetched> {
    op(app, ctx.inner(), workspace_id, ops::fetch).await
}

/// Fast-forward to what the last fetch brought (refused when diverged, conflicted or overlapping local changes).
#[tauri::command]
pub async fn sync_pull(app: AppHandle, ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<ops::Pulled> {
    op(app, ctx.inner(), workspace_id, ops::pull).await
}

#[tauri::command]
pub async fn sync_diff(ctx: State<'_, Ctx>, workspace_id: String, side: ops::Side, path: String) -> IpcResult<ops::FileDiff> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = conn(&ctx)?;
        Ok(ops::file_diff(&ws(&c, &workspace_id)?, side, &path)?)
    })
    .await
}

#[tauri::command]
pub async fn sync_review_commit(ctx: State<'_, Ctx>, workspace_id: String, paths: Option<Vec<String>>) -> IpcResult<ops::CommitReview> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = conn(&ctx)?;
        let w = ws(&c, &workspace_id)?;
        let s = ops::Sync { conn: &c, ws: &w, data_dir: &ctx.data_dir, home: &ctx.home, host_id: &ctx.host.id };
        Ok(ops::review_commit(&s, paths.as_deref())?)
    })
    .await
}

/// Commit exactly the reviewed changes (`token` from the review the person confirmed). Never pushes.
#[tauri::command]
pub async fn sync_commit(app: AppHandle, ctx: State<'_, Ctx>, workspace_id: String, message: String, paths: Option<Vec<String>>, token: String) -> IpcResult<ops::Committed> {
    op(app, ctx.inner(), workspace_id, move |s| ops::commit(s, &message, paths.as_deref(), &token)).await
}

#[tauri::command]
pub async fn sync_review_push(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<ops::PushReview> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = conn(&ctx)?;
        let w = ws(&c, &workspace_id)?;
        let s = ops::Sync { conn: &c, ws: &w, data_dir: &ctx.data_dir, home: &ctx.home, host_id: &ctx.host.id };
        Ok(ops::review_push(&s)?)
    })
    .await
}

/// Push exactly the reviewed commits (`token` from the push review the person confirmed). Never forced.
#[tauri::command]
pub async fn sync_push(app: AppHandle, ctx: State<'_, Ctx>, workspace_id: String, token: String) -> IpcResult<ops::Pushed> {
    op(app, ctx.inner(), workspace_id, move |s| ops::push(s, &token)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cloned {
    pub workspace: Workspace,
    pub restore: ops::Restore,
}

/// Onboard a workspace from an existing Git repository (new or empty folder) and make it active.
#[tauri::command]
pub async fn sync_clone(app: AppHandle, ctx: State<'_, Ctx>, url: String, folder: String) -> IpcResult<Cloned> {
    let ctx = ctx.inner().clone();
    let r = blocking(move || {
        let c = conn(&ctx)?;
        let (workspace, restore) = ops::clone_workspace(&c, &ctx.data_dir, &ctx.host.id, &url, &folder)?;
        Ok(Cloned { workspace, restore })
    })
    .await;
    let _ = app.emit(SYNC_EVENT, ());
    r
}

#[tauri::command]
pub async fn sync_restore(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<ops::Restore> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = conn(&ctx)?;
        Ok(ops::restore(&c, &ctx.data_dir, &ws(&c, &workspace_id)?)?)
    })
    .await
}

/// The workspace's sync policy on this machine (ADR-0039).
#[tauri::command]
pub async fn sync_policy_get(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<sync::policy::PolicyView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = conn(&ctx)?;
        let w = workspace::get(&c, &workspace_id)?;
        Ok(sync::policy::view(&c, &w, &sync::policy::default_managed_path())?)
    })
    .await
}

/// Change the policy: trusted needs `remote_is_private` (the person's confirmation) and the managed policy's consent.
#[tauri::command]
pub async fn sync_policy_set(app: AppHandle, ctx: State<'_, Ctx>, workspace_id: String, mode: sync::policy::Mode, remote_is_private: bool) -> IpcResult<sync::policy::PolicyView> {
    let ctx = ctx.inner().clone();
    let r = blocking(move || {
        let c = conn(&ctx)?;
        let w = ws(&c, &workspace_id)?;
        let path = sync::policy::default_managed_path();
        sync::policy::set(&c, &w, mode, remote_is_private, &sync::policy::managed(&path))?;
        Ok(sync::policy::view(&c, &w, &path)?)
    })
    .await;
    let _ = app.emit(SYNC_EVENT, ());
    r
}

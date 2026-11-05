use serde::Serialize;
use tauri::State;

use nexus_core::db::Recovery;
use nexus_core::{db, git, settings};

use crate::error::{IpcError, IpcResult, blocking};
use crate::state::Ctx;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub data_dir: String,
    pub db_path: String,
    pub schema_version: i64,
    pub git_version: Option<String>,
    pub home_dir: String,
    /// The database was unreadable at startup and was moved aside (shown once to the user).
    pub recovery: Option<Recovery>,
    /// Schema version this build understands (a newer database is refused, never changed).
    pub supported_schema: i64,
    /// The `nexus` command-line tool next to this program (also the MCP server and hook handler).
    pub cli_path: Option<String>,
    /// A restore scheduled for the next start.
    pub pending_restore: Option<String>,
    /// The outcome of a restore applied at this start: `ok` + backup id, or the reason it failed.
    pub restored: Option<String>,
    pub restore_error: Option<String>,
}

#[tauri::command]
pub async fn app_info(ctx: State<'_, Ctx>) -> IpcResult<AppInfo> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let schema_version = db::schema_version(&ctx.db())?;
        Ok(AppInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
            data_dir: ctx.data_dir.display().to_string(),
            db_path: ctx.db_path.display().to_string(),
            schema_version,
            git_version: git::version(),
            home_dir: ctx.home.display().to_string(),
            recovery: ctx.recovery.clone(),
            supported_schema: db::SCHEMA_VERSION,
            cli_path: nexus_core::claude::mcp::nexus_next_to_current_exe().map(|p| p.display().to_string()),
            pending_restore: db::backup::pending(&ctx.data_dir),
            restored: ctx.restore.as_ref().and_then(|r| r.as_ref().ok().cloned()),
            restore_error: ctx.restore.as_ref().and_then(|r| r.as_ref().err().cloned()),
        })
    })
    .await
}

const MAX_PREFS_BYTES: usize = 64 * 1024;

#[tauri::command]
pub fn ui_preferences_get(ctx: State<'_, Ctx>) -> IpcResult<Option<serde_json::Value>> {
    Ok(settings::get(&ctx.db(), settings::UI_PREFERENCES)?)
}

#[tauri::command]
pub fn ui_preferences_set(ctx: State<'_, Ctx>, value: serde_json::Value) -> IpcResult<()> {
    if !value.is_object() || value.to_string().len() > MAX_PREFS_BYTES {
        return Err(IpcError::invalid("preferences must be a small JSON object"));
    }
    Ok(settings::set(&ctx.db(), settings::UI_PREFERENCES, &value)?)
}

/// Cheap change counter so the UI can refresh when another process (CLI, MCP) wrote to the database.
#[tauri::command]
pub fn db_data_version(ctx: State<'_, Ctx>) -> IpcResult<i64> {
    Ok(db::data_version(&ctx.db())?)
}

// ---- Backups (ADR-0032) -------------------------------------------------------------------------------------------

#[tauri::command]
pub async fn backup_list(ctx: State<'_, Ctx>) -> IpcResult<Vec<db::backup::BackupInfo>> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(db::backup::list(&ctx.data_dir)?)).await
}

#[tauri::command]
pub async fn backup_create(ctx: State<'_, Ctx>) -> IpcResult<db::backup::BackupInfo> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(db::backup::create(&ctx.db(), &ctx.data_dir, "manual")?)).await
}

/// The open database cannot be replaced: the restore is applied when NEXUS starts next (see `app_restart`).
#[tauri::command]
pub async fn backup_schedule_restore(ctx: State<'_, Ctx>, id: String) -> IpcResult<db::backup::BackupInfo> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(db::backup::schedule_restore(&ctx.data_dir, &id)?)).await
}

#[tauri::command]
pub fn backup_cancel_restore(ctx: State<'_, Ctx>) {
    db::backup::cancel_pending(&ctx.data_dir);
}

#[tauri::command]
pub async fn backup_delete(ctx: State<'_, Ctx>, id: String) -> IpcResult<()> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(db::backup::delete(&ctx.data_dir, &id)?)).await
}

/// Restart NEXUS (to apply a scheduled restore). Live sessions are stopped like on quit.
#[tauri::command]
pub fn app_restart(app: tauri::AppHandle) {
    app.restart();
}

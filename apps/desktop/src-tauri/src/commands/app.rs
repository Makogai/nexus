use serde::Serialize;
use tauri::State;

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

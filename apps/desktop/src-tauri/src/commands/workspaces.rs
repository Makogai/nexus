use tauri::State;

use nexus_core::workspace::{self, Workspace};

use crate::error::IpcResult;
use crate::state::Ctx;

#[tauri::command]
pub fn workspace_list(ctx: State<'_, Ctx>) -> IpcResult<Vec<Workspace>> {
    Ok(workspace::list(&ctx.db())?)
}

#[tauri::command]
pub fn workspace_active(ctx: State<'_, Ctx>) -> IpcResult<Option<Workspace>> {
    Ok(workspace::active(&ctx.db())?)
}

#[tauri::command]
pub fn workspace_create(ctx: State<'_, Ctx>, path: String, name: String, description: Option<String>) -> IpcResult<Workspace> {
    let db = ctx.db();
    let ws = workspace::create(&db, &path, &name, description.as_deref().unwrap_or(""))?;
    Ok(workspace::activate(&db, &ws.id)?)
}

#[tauri::command]
pub fn workspace_import(ctx: State<'_, Ctx>, path: String) -> IpcResult<Workspace> {
    let db = ctx.db();
    let ws = workspace::import(&db, &path)?;
    Ok(workspace::activate(&db, &ws.id)?)
}

#[tauri::command]
pub fn workspace_activate(ctx: State<'_, Ctx>, id: String) -> IpcResult<Workspace> {
    Ok(workspace::activate(&ctx.db(), &id)?)
}

#[tauri::command]
pub fn workspace_forget(ctx: State<'_, Ctx>, id: String) -> IpcResult<()> {
    Ok(workspace::forget(&ctx.db(), &id)?)
}

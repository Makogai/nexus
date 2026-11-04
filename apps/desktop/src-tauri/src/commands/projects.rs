use tauri::State;

use nexus_core::git::{self, GitStatus};
use nexus_core::project::{self, PathInspection, Project};

use crate::error::{IpcError, IpcResult, blocking};
use crate::state::Ctx;

#[tauri::command]
pub fn project_list(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<Vec<Project>> {
    Ok(project::list(&ctx.db(), &workspace_id)?)
}

#[tauri::command]
pub fn project_get(ctx: State<'_, Ctx>, id: String) -> IpcResult<Project> {
    Ok(project::get(&ctx.db(), &id)?)
}

#[tauri::command]
pub async fn project_inspect_path(ctx: State<'_, Ctx>, workspace_id: String, path: String) -> IpcResult<PathInspection> {
    let ctx = ctx.inner().clone();
    // Git calls run while the DB lock is held only briefly inside core; acceptable for an inspection.
    blocking(move || Ok(project::inspect_path(&ctx.db(), &workspace_id, &path)?)).await
}

#[tauri::command]
pub async fn project_register(
    ctx: State<'_, Ctx>,
    workspace_id: String,
    path: String,
    name: Option<String>,
) -> IpcResult<Project> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(project::register(&ctx.db(), &workspace_id, &path, name.as_deref())?)).await
}

#[tauri::command]
pub fn project_unregister(ctx: State<'_, Ctx>, id: String) -> IpcResult<()> {
    Ok(project::unregister(&ctx.db(), &id)?)
}

#[tauri::command]
pub fn project_locate(ctx: State<'_, Ctx>, id: String, path: String) -> IpcResult<Project> {
    Ok(project::locate(&ctx.db(), &id, &path)?)
}

#[tauri::command]
pub async fn project_refresh(ctx: State<'_, Ctx>, id: String) -> IpcResult<Project> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(project::refresh(&ctx.db(), &id)?)).await
}

#[tauri::command]
pub async fn project_git_status(ctx: State<'_, Ctx>, id: String) -> IpcResult<GitStatus> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let path = project::get(&ctx.db(), &id)?
            .local_path
            .filter(|p| p.is_dir())
            .ok_or_else(|| IpcError::invalid("project folder is not available on this machine"))?;
        Ok(git::status(&path)?)
    })
    .await
}

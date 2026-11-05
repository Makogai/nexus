use tauri::State;

use nexus_core::checkpoint::{self, Checkpoint};
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
    blocking(move || Ok(project::register_with_context(&ctx.db(), &ctx.data_dir, &workspace_id, &path, name.as_deref())?)).await
}

fn own_conn(ctx: &Ctx) -> IpcResult<nexus_core::db::Connection> {
    Ok(nexus_core::db::open(&ctx.data_dir.join(nexus_core::db::DB_FILE))?)
}

/// Repositories in the workspace's clone folder and projects missing on this computer (Portable Workspace P4).
#[tauri::command]
pub async fn project_scan(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<project::discovery::Scan> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = own_conn(&ctx)?;
        let ws = nexus_core::workspace::get(&c, &workspace_id)?;
        Ok(project::discovery::scan(&c, &ws)?)
    })
    .await
}

/// Clone a missing project from its recorded remote (network: own connection, the shared one stays free).
#[tauri::command]
pub async fn project_clone(ctx: State<'_, Ctx>, workspace_id: String, id: String, dest: Option<String>) -> IpcResult<Project> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = own_conn(&ctx)?;
        let ws = nexus_core::workspace::get(&c, &workspace_id)?;
        Ok(project::discovery::clone_missing(&c, &ctx.data_dir, &ws, &id, dest.as_deref())?)
    })
    .await
}

/// Create or refresh a project's portable context folder.
#[tauri::command]
pub async fn project_context_init(ctx: State<'_, Ctx>, workspace_id: String, id: String) -> IpcResult<project::context::Initialized> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = own_conn(&ctx)?;
        let ws = nexus_core::workspace::get(&c, &workspace_id)?;
        Ok(project::context::init(&c, &ctx.data_dir, &ws, &id)?)
    })
    .await
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

/// Recent session checkpoints of a project (recorded by Claude through the NEXUS MCP server).
#[tauri::command]
pub fn project_checkpoints(ctx: State<'_, Ctx>, id: String, limit: Option<u32>) -> IpcResult<Vec<Checkpoint>> {
    Ok(checkpoint::for_project(&ctx.db(), &id, limit.unwrap_or(30).min(100))?)
}

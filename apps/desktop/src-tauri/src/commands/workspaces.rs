use tauri::State;

use nexus_core::{templates, workspace_doctor};
use nexus_core::workspace::{self, Workspace, migrate};

use crate::error::{IpcResult, blocking};
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
pub fn workspace_create(ctx: State<'_, Ctx>, path: String, name: String, description: Option<String>, template: Option<String>) -> IpcResult<Workspace> {
    let db = ctx.db();
    if let Some(t) = template.as_deref() {
        templates::info(t)?; // an unknown template fails before anything is created
    }
    let ws = workspace::create(&db, &path, &name, description.as_deref().unwrap_or(""))?;
    if let Some(t) = template.as_deref() {
        templates::apply(&db, &ctx.data_dir, &ws, t)?;
    }
    Ok(workspace::activate(&db, &ws.id)?)
}

fn own_conn(ctx: &Ctx) -> IpcResult<nexus_core::db::Connection> {
    Ok(nexus_core::db::open(&ctx.data_dir.join(nexus_core::db::DB_FILE))?)
}

/// Workspace Doctor: health of the workspace with recommendations and repairs (Portable Workspace P3). Probing
/// tools and scanning take a moment, so it runs on its own connection.
#[tauri::command]
pub async fn workspace_doctor(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<workspace_doctor::Report> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = own_conn(&ctx)?;
        let ws = workspace::get(&c, &workspace_id)?;
        Ok(workspace_doctor::run(&c, &ctx.data_dir, &ws, &workspace_doctor::Options { home: &ctx.home, probe_tools: true })?)
    })
    .await
}

/// What a repair would change. Read-only.
#[tauri::command]
pub async fn workspace_repair_plan(ctx: State<'_, Ctx>, workspace_id: String, repair: String) -> IpcResult<workspace_doctor::RepairPlan> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = own_conn(&ctx)?;
        let ws = workspace::get(&c, &workspace_id)?;
        Ok(workspace_doctor::repair_plan(&c, &ctx.data_dir, &ws, &repair)?)
    })
    .await
}

/// Apply a repair the person reviewed (cloning uses the network: own connection).
#[tauri::command]
pub async fn workspace_repair(ctx: State<'_, Ctx>, workspace_id: String, repair: String) -> IpcResult<workspace_doctor::Repaired> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = own_conn(&ctx)?;
        let ws = workspace::get(&c, &workspace_id)?;
        Ok(workspace_doctor::repair(&c, &ctx.data_dir, &ws, &repair)?)
    })
    .await
}

/// What restoring the workspace on this computer still needs, step by step, and what came with it (Portable
/// Workspace P8). Probes Claude Code and the projects' tools, so it runs on its own connection.
#[tauri::command]
pub async fn workspace_restoration(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<nexus_core::restoration::Restoration> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = own_conn(&ctx)?;
        let ws = workspace::get(&c, &workspace_id)?;
        Ok(nexus_core::restoration::plan(&c, &ctx.data_dir, &ctx.home, &ws, true)?)
    })
    .await
}

/// The built-in workspace templates (ADR-0037).
#[tauri::command]
pub fn template_list() -> IpcResult<Vec<templates::Summary>> {
    Ok(templates::list()?)
}

/// What installing or updating a template in the workspace would do. Read-only.
#[tauri::command]
pub fn template_plan(ctx: State<'_, Ctx>, workspace_id: String, template: String) -> IpcResult<templates::Plan> {
    let ws = workspace::get(&ctx.db(), &workspace_id)?;
    Ok(templates::plan(&ws.root_path, &template)?)
}

/// Install or update the template after the person reviewed the plan; never overwrites changed or own files.
#[tauri::command]
pub fn template_apply(ctx: State<'_, Ctx>, workspace_id: String, template: String) -> IpcResult<templates::Applied> {
    let db = ctx.db();
    let ws = workspace::get(&db, &workspace_id)?;
    Ok(templates::apply(&db, &ctx.data_dir, &ws, &template)?)
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

/// What migrating the workspace to the current schema would change (ADR-0036). Read-only.
#[tauri::command]
pub fn workspace_migration_plan(ctx: State<'_, Ctx>, id: String) -> IpcResult<migrate::Plan> {
    let ws = workspace::get(&ctx.db(), &id)?;
    Ok(migrate::plan(&ws.root_path)?)
}

/// Apply the migration after the person reviewed the plan: changed files are backed up first and restored on failure.
#[tauri::command]
pub fn workspace_migrate(ctx: State<'_, Ctx>, id: String) -> IpcResult<migrate::Migrated> {
    let db = ctx.db();
    let ws = workspace::get(&db, &id)?;
    Ok(migrate::apply(&db, &ctx.data_dir, &ws)?)
}

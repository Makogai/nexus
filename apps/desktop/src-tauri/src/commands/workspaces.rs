use tauri::State;

use nexus_core::{templates, workspace_doctor};
use nexus_core::workspace::{self, Workspace, migrate};

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

/// Workspace Doctor: health of the workspace with recommendations and repairs (Portable Workspace P3).
#[tauri::command]
pub fn workspace_doctor(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<workspace_doctor::Report> {
    let db = ctx.db();
    let ws = workspace::get(&db, &workspace_id)?;
    Ok(workspace_doctor::run(&db, &ctx.data_dir, &ws, &workspace_doctor::Options { home: &ctx.home, probe_tools: true })?)
}

/// What a repair would change. Read-only.
#[tauri::command]
pub fn workspace_repair_plan(ctx: State<'_, Ctx>, workspace_id: String, repair: String) -> IpcResult<workspace_doctor::RepairPlan> {
    let db = ctx.db();
    let ws = workspace::get(&db, &workspace_id)?;
    Ok(workspace_doctor::repair_plan(&db, &ctx.data_dir, &ws, &repair)?)
}

/// Apply a repair the person reviewed.
#[tauri::command]
pub fn workspace_repair(ctx: State<'_, Ctx>, workspace_id: String, repair: String) -> IpcResult<workspace_doctor::Repaired> {
    let db = ctx.db();
    let ws = workspace::get(&db, &workspace_id)?;
    Ok(workspace_doctor::repair(&db, &ctx.data_dir, &ws, &repair)?)
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

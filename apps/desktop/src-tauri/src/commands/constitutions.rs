//! Constitution Manager (ADR-0027). The desktop is a human author: saving a constitution here approves that version
//! on this machine; versions that arrive through the synced workspace folder wait for approval (`knowledge_approve`).

use tauri::State;

use nexus_core::knowledge::constitution::{self, Compliance, Effective, History, NewConstitution, RuleHit, Structure, Summary, Target, View};
use nexus_core::{project, workspace};

use crate::error::{IpcError, IpcResult, blocking};
use crate::state::Ctx;

fn available(ctx: &Ctx, workspace_id: &str) -> IpcResult<workspace::Workspace> {
    let ws = workspace::get(&ctx.db(), workspace_id)?;
    if !ws.available {
        return Err(IpcError::invalid("the workspace folder is not available on this machine"));
    }
    Ok(ws)
}

#[tauri::command]
pub async fn constitution_list(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<Vec<Summary>> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(constitution::list(&ctx.db(), &ctx.data_dir, &ws)?)
    })
    .await
}

#[tauri::command]
pub async fn constitution_get(ctx: State<'_, Ctx>, workspace_id: String, id: String) -> IpcResult<View> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(constitution::get(&ctx.db(), &ctx.data_dir, &ws, &id)?)
    })
    .await
}

#[tauri::command]
pub async fn constitution_create(ctx: State<'_, Ctx>, workspace_id: String, input: NewConstitution) -> IpcResult<View> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(constitution::create(&ctx.db(), &ctx.data_dir, &ws, &input)?)
    })
    .await
}

/// Save the structured editor's content (ids assigned, validated, optimistic on `expected_hash`).
#[tauri::command]
pub async fn constitution_save(ctx: State<'_, Ctx>, workspace_id: String, id: String, structure: Structure, expected_hash: Option<String>) -> IpcResult<View> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(constitution::save(&ctx.db(), &ctx.data_dir, &ws, &id, structure, expected_hash.as_deref())?)
    })
    .await
}

/// Save raw Markdown (the editor's text mode). At most 512 KiB.
#[tauri::command]
pub async fn constitution_save_raw(ctx: State<'_, Ctx>, workspace_id: String, id: String, body: String, expected_hash: Option<String>) -> IpcResult<View> {
    if body.len() > 512 * 1024 {
        return Err(IpcError::invalid("a constitution is at most 512 KiB"));
    }
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(constitution::save_raw(&ctx.db(), &ctx.data_dir, &ws, &id, &body, expected_hash.as_deref())?)
    })
    .await
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveView {
    #[serde(flatten)]
    pub effective: Effective,
    /// Per rule: instructed only, or the linked gate's latest result for the project (ADR-0027/0029).
    pub compliance: Vec<Compliance>,
}

/// Effective rules for a project (and task); `include_pending` previews unapproved versions.
#[tauri::command]
pub async fn constitution_effective(
    ctx: State<'_, Ctx>,
    workspace_id: String,
    project_id: Option<String>,
    task_id: Option<String>,
    include_pending: Option<bool>,
) -> IpcResult<EffectiveView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        let db = ctx.db();
        let p = match &project_id {
            Some(id) => Some(project::get(&db, id)?).filter(|p| p.workspace_id == ws.id),
            None => None,
        };
        let stacks = p.as_ref().map(|p| p.stack.stacks.clone()).unwrap_or_default();
        let t = Target { project: p.as_ref().map(|p| (p.id.as_str(), stacks.as_slice())), task_id: task_id.as_deref(), include_pending: include_pending.unwrap_or(false) };
        let effective = constitution::effective(&db, &ctx.data_dir, &ws, &t)?;
        let compliance = constitution::compliance(&db, p.as_ref().map(|p| p.id.as_str()), &effective)?;
        Ok(EffectiveView { effective, compliance })
    })
    .await
}

#[tauri::command]
pub async fn constitution_search(ctx: State<'_, Ctx>, workspace_id: String, query: String) -> IpcResult<Vec<RuleHit>> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(constitution::search(&ctx.db(), &ctx.data_dir, &ws, &query)?)
    })
    .await
}

/// Git history of the file (runs `git` without holding the database).
#[tauri::command]
pub async fn constitution_history(ctx: State<'_, Ctx>, workspace_id: String, id: String) -> IpcResult<History> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        let path = constitution::path_of(&ctx.db(), &ctx.data_dir, &ws, &id)?;
        Ok(constitution::history_at(&path))
    })
    .await
}

#[tauri::command]
pub async fn constitution_version(ctx: State<'_, Ctx>, workspace_id: String, id: String, commit: String) -> IpcResult<String> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        let path = constitution::path_of(&ctx.db(), &ctx.data_dir, &ws, &id)?;
        Ok(constitution::version_at(&path, &commit)?)
    })
    .await
}

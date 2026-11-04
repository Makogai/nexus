//! Portable workspace knowledge and tasks (ADR-0013/0017). The desktop is a human author: it may create every kind
//! and approve always-on rules; Claude goes through the MCP server with narrower rights.

use tauri::State;

use nexus_core::knowledge::Kind;
use nexus_core::knowledge::index::{self as kindex, Hit, Item};
use nexus_core::knowledge::service::{self as knowledge, Author, CreateInput, DocView, Overview};
use nexus_core::workspace::{self, Workspace};

use crate::error::{IpcError, IpcResult, blocking};
use crate::state::Ctx;

fn available(ctx: &Ctx, workspace_id: &str) -> IpcResult<Workspace> {
    let ws = workspace::get(&ctx.db(), workspace_id)?;
    if !ws.available {
        return Err(IpcError::invalid("the workspace folder is not available on this machine"));
    }
    Ok(ws)
}

/// Refresh the index from the files (or rebuild it from scratch) and return every document, including invalid ones.
#[tauri::command]
pub async fn knowledge_overview(ctx: State<'_, Ctx>, workspace_id: String, rebuild: Option<bool>) -> IpcResult<Overview> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(knowledge::overview(&ctx.db(), &ctx.data_dir, &ws, rebuild.unwrap_or(false))?)
    })
    .await
}

#[tauri::command]
pub async fn knowledge_read(ctx: State<'_, Ctx>, workspace_id: String, id: String) -> IpcResult<DocView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(knowledge::read(&ctx.db(), &ws, &id, 200_000)?)
    })
    .await
}

#[tauri::command]
pub async fn knowledge_search(ctx: State<'_, Ctx>, workspace_id: String, query: String, kinds: Option<Vec<String>>) -> IpcResult<Vec<Hit>> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        let kinds = kinds
            .unwrap_or_default()
            .iter()
            .map(|k| Kind::parse(k).ok_or_else(|| IpcError::invalid(format!("unknown kind {k}"))))
            .collect::<IpcResult<Vec<_>>>()?;
        let db = ctx.db();
        knowledge::refresh(&db, &ctx.data_dir, &ws)?;
        Ok(kindex::search(&db, &ws.id, &query, &kinds, 30)?)
    })
    .await
}

#[tauri::command]
pub async fn knowledge_create(ctx: State<'_, Ctx>, workspace_id: String, input: CreateInput) -> IpcResult<Item> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(knowledge::create(&ctx.db(), &ctx.data_dir, &ws, &input, Author::Human)?)
    })
    .await
}

#[tauri::command]
pub async fn knowledge_update_task(
    ctx: State<'_, Ctx>,
    workspace_id: String,
    id: String,
    status: Option<String>,
    log: Option<String>,
    expected_hash: Option<String>,
) -> IpcResult<Item> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(knowledge::update_task(&ctx.db(), &ctx.data_dir, &ws, &id, status.as_deref(), log.as_deref(), expected_hash.as_deref())?)
    })
    .await
}

/// Approve the current version of an always-on rule on this machine (threat T19).
#[tauri::command]
pub async fn knowledge_approve(ctx: State<'_, Ctx>, workspace_id: String, id: String) -> IpcResult<Item> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(knowledge::approve(&ctx.db(), &ctx.data_dir, &ws, &id)?)
    })
    .await
}

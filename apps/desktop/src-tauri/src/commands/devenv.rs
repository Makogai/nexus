//! Dev Environment (ADR-0051): this computer's development tools and what the active workspace's projects need.
//! Probing runs only the tools' version commands, on the blocking pool; the last inventory is kept in memory.

use tauri::State;

use nexus_core::{devenv, now_ms, toolchain};

use crate::error::{IpcError, IpcResult, blocking};
use crate::state::Ctx;

/// The Dev Environment view. Probes every tool the first time and when `refresh` is set; otherwise reuses the last
/// inventory (project needs are always read again).
#[tauri::command]
pub async fn dev_environment(ctx: State<'_, Ctx>, refresh: Option<bool>) -> IpcResult<devenv::View> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let (at, inventory) = match ctx.inventory() {
            Some(cached) if refresh != Some(true) => cached,
            _ => ctx.set_inventory(now_ms(), toolchain::inventory()),
        };
        Ok(devenv::view(&ctx.db(), &inventory, at)?)
    })
    .await
}

/// Look one tool up again (after the person ran an install command) and return the whole view.
#[tauri::command]
pub async fn dev_environment_check(ctx: State<'_, Ctx>, tool: String) -> IpcResult<devenv::View> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let t = toolchain::tool(&tool).filter(|t| t.on_this_platform()).ok_or_else(|| IpcError::invalid(format!("unknown tool '{tool}'")))?;
        let (_, mut inventory) = ctx.inventory().unwrap_or_else(|| (0, toolchain::inventory()));
        let fresh = toolchain::installed(t);
        match inventory.iter_mut().find(|i| i.tool.id == t.id) {
            Some(slot) => *slot = fresh,
            None => inventory.push(fresh),
        }
        let (at, inventory) = ctx.set_inventory(now_ms(), inventory);
        Ok(devenv::view(&ctx.db(), &inventory, at)?)
    })
    .await
}

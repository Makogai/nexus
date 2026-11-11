//! AI agents beyond Claude Code (ADR-0052): what NEXUS gives their sessions, and their profiles. Registering a profile
//! only records a home folder; nothing is written into it.

use serde::Serialize;
use tauri::State;

use nexus_core::agent::profile::{self, AgentProfile, AgentProfileInput, Detected};
use nexus_core::agent::{self, Kind};

use crate::error::{IpcResult, blocking};
use crate::state::Ctx;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsOverview {
    /// Every agent NEXUS can start (Claude Code first), installed or not, with its capabilities.
    pub agents: Vec<agent::Info>,
    pub profiles: Vec<AgentProfile>,
    pub detected: Vec<Detected>,
}

#[tauri::command]
pub async fn agents_overview(ctx: State<'_, Ctx>) -> IpcResult<AgentsOverview> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let agents = Kind::ALL.iter().map(|k| agent::info(*k, true)).collect();
        let db = ctx.db();
        Ok(AgentsOverview { agents, profiles: profile::list(&db, true)?, detected: profile::detect(&db, &ctx.home)? })
    })
    .await
}

#[tauri::command]
pub async fn agent_profile_register(ctx: State<'_, Ctx>, input: AgentProfileInput) -> IpcResult<AgentProfile> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(profile::register(&ctx.db(), &input, &ctx.home)?)).await
}

#[tauri::command]
pub async fn agent_profile_update(ctx: State<'_, Ctx>, id: String, input: AgentProfileInput) -> IpcResult<AgentProfile> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(profile::update(&ctx.db(), &id, &input, &ctx.home)?)).await
}

#[tauri::command]
pub async fn agent_profile_set_archived(ctx: State<'_, Ctx>, id: String, archived: bool) -> IpcResult<AgentProfile> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(profile::set_archived(&ctx.db(), &id, archived)?)).await
}

#[tauri::command]
pub async fn agent_profile_forget(ctx: State<'_, Ctx>, id: String) -> IpcResult<()> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(profile::forget(&ctx.db(), &id)?)).await
}

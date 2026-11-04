//! Profile diagnostics (ADR-0023) and AI Kits (ADR-0024). Thin adapters over `nexus_core::claude::diagnostics` and
//! `nexus_core::kits`; nothing here writes into a Claude config dir.

use tauri::State;

use nexus_core::claude::diagnostics::{self, Diagnostics};
use nexus_core::claude::{install, profile};
use nexus_core::env;
use nexus_core::kits::{self, KitProfile, KitView, Selection, Validation};

use crate::error::{IpcError, IpcResult, blocking};
use crate::state::Ctx;

#[tauri::command]
pub async fn profile_diagnostics(ctx: State<'_, Ctx>, profile_id: String) -> IpcResult<Diagnostics> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        // Read what is needed under the DB lock, then probe the executable without holding it.
        let (p, kit_uses) = {
            let db = ctx.db();
            let p = profile::get(&db, &profile_id)?;
            let pol = nexus_core::claude::policy::evaluate(&p.effective_config_dir(&ctx.home), &ctx.machine_policy);
            let uses = kits::uses_for_profile(&db, &ctx.data_dir, &p.id, &pol)?;
            (p, uses)
        };
        let (executable, executable_error) = match install::resolve(p.executable.as_deref(), &ctx.home) {
            Ok(e) => (Some(e), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let version = executable.as_deref().and_then(|exe| {
            let (child_env, _) = nexus_core::claude::launch::profile_env(&p, &env::current(), &[]);
            install::version_with_env(exe, &child_env)
        });
        Ok(diagnostics::diagnose(&diagnostics::Input {
            profile: &p,
            home: &ctx.home,
            machine: &ctx.machine_policy,
            executable,
            version,
            executable_error,
            kits: kit_uses,
        }))
    })
    .await
}

#[tauri::command]
pub async fn kit_list(ctx: State<'_, Ctx>) -> IpcResult<Vec<KitView>> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(kits::list(&ctx.db(), &ctx.data_dir)?)).await
}

#[tauri::command]
pub async fn kit_register(
    ctx: State<'_, Ctx>,
    source: String,
    git: bool,
    git_ref: Option<String>,
    subdir: Option<String>,
    plugin: Option<String>,
) -> IpcResult<KitView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let db = ctx.db();
        let k = if git {
            kits::register_git(&db, &ctx.data_dir, &source, git_ref.as_deref(), subdir.as_deref(), plugin.as_deref())?
        } else {
            kits::register_local(&db, &ctx.data_dir, &source, subdir.as_deref(), plugin.as_deref())?
        };
        Ok(kits::view(&db, &ctx.data_dir, &k)?)
    })
    .await
}

#[tauri::command]
pub async fn kit_approve(ctx: State<'_, Ctx>, id: String) -> IpcResult<KitView> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(kits::approve(&ctx.db(), &ctx.data_dir, &id)?)).await
}

#[tauri::command]
pub async fn kit_update(ctx: State<'_, Ctx>, id: String) -> IpcResult<KitView> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(kits::update(&ctx.db(), &ctx.data_dir, &id)?)).await
}

#[tauri::command]
pub async fn kit_remove(ctx: State<'_, Ctx>, id: String) -> IpcResult<()> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(kits::remove(&ctx.db(), &ctx.data_dir, &id)?)).await
}

#[tauri::command]
pub async fn kit_enable(ctx: State<'_, Ctx>, kit_id: String, profile_id: String, selection: Option<Selection>) -> IpcResult<KitProfile> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(kits::enable(&ctx.db(), &ctx.data_dir, &kit_id, &profile_id, selection)?)).await
}

#[tauri::command]
pub fn kit_disable(ctx: State<'_, Ctx>, kit_id: String, profile_id: String) -> IpcResult<()> {
    Ok(kits::disable(&ctx.db(), &kit_id, &profile_id)?)
}

/// Claude Code's own validator, run with an empty NEXUS-owned config dir.
#[tauri::command]
pub async fn kit_validate(ctx: State<'_, Ctx>, id: String, profile_id: Option<String>) -> IpcResult<Validation> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let db = ctx.db();
        let k = kits::get(&db, &id)?;
        let p = match profile_id {
            Some(pid) => profile::get(&db, &pid)?,
            None => profile::list(&db, false)?.into_iter().next().ok_or_else(|| IpcError::invalid("register a Claude profile first"))?,
        };
        drop(db);
        let exe = install::resolve(p.executable.as_deref(), &ctx.home)?;
        Ok(kits::validate(&ctx.data_dir, &k, &exe, &env::current())?)
    })
    .await
}

//! Context Manager (ADR-0028): per-project strategy, onboarding analysis, the project-docs index and the Context
//! Inspector. Read-mostly; `context_set` changes only NEXUS's own machine-local preference.

use tauri::State;

use nexus_core::claude::prepare::{self, PreviewRequest};
use nexus_core::claude::profile;
use nexus_core::context::{self, Config, Settings, Strategy, analysis::Analysis, docs, inspector::Report};
use nexus_core::{env, project};

use crate::error::{IpcError, IpcResult, blocking};
use crate::state::Ctx;

#[tauri::command]
pub fn context_get(ctx: State<'_, Ctx>, project_id: String) -> IpcResult<Config> {
    Ok(context::get(&ctx.db(), &project_id)?)
}

#[tauri::command]
pub fn context_set(ctx: State<'_, Ctx>, project_id: String, strategy: String, settings: Option<Settings>) -> IpcResult<Config> {
    let s = Strategy::parse(&strategy).ok_or_else(|| IpcError::invalid("strategy must be minimal, balanced, deep or custom"))?;
    Ok(context::set(&ctx.db(), &project_id, s, settings)?)
}

/// Presets for the wizard (what each strategy resolves to).
#[tauri::command]
pub fn context_presets() -> Vec<(Strategy, Settings)> {
    [Strategy::Minimal, Strategy::Balanced, Strategy::Deep].into_iter().map(|s| (s, Settings::preset(s))).collect()
}

#[tauri::command]
pub async fn context_analyze(ctx: State<'_, Ctx>, project_id: String) -> IpcResult<Analysis> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(context::analysis::analyze(&ctx.db(), &ctx.data_dir, &project_id)?)).await
}

#[tauri::command]
pub async fn context_docs(ctx: State<'_, Ctx>, project_id: String) -> IpcResult<Vec<docs::DocEntry>> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let db = ctx.db();
        let p = project::get(&db, &project_id)?;
        if let Some(root) = p.local_path.as_ref().filter(|d| d.is_dir()) {
            docs::refresh(&db, &p.id, root)?;
        }
        Ok(docs::list(&db, &p.id)?)
    })
    .await
}

/// Configured / predicted (for `profile_id`) / observed / on demand / retrieved, plus metrics.
#[tauri::command]
pub async fn context_inspect(ctx: State<'_, Ctx>, project_id: String, profile_id: Option<String>) -> IpcResult<Report> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let db = ctx.db();
        let p = project::get(&db, &project_id)?;
        let predicted = match (&profile_id, p.local_path.as_ref().filter(|d| d.is_dir())) {
            (Some(pid), Some(cwd)) => {
                let prof = profile::get(&db, pid)?;
                let base_env = env::current();
                Some(prepare::preview(&PreviewRequest {
                    conn: &db,
                    data_dir: &ctx.data_dir,
                    home: &ctx.home,
                    profile: &prof,
                    machine: &ctx.machine_policy,
                    workspace_id: Some(&p.workspace_id),
                    project_id: Some(&p.id),
                    task_id: None,
                    cwd,
                    base_env: &base_env,
                    work_profile: None,
                })?)
            }
            _ => None,
        };
        Ok(context::inspector::inspect(context::inspector::Request {
            conn: &db,
            workspace_id: &p.workspace_id,
            project_id: &p.id,
            stacks: &p.stack.stacks,
            profile_id: profile_id.as_deref(),
            predicted,
        })?)
    })
    .await
}

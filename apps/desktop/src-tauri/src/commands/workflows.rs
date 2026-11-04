//! Workflow Manager (ADR-0029): definitions, templates, runs, reviews and the trusted quality gates. The desktop is a
//! host: its runner thread ([`spawn_runner`]) executes pending gates outside the database lock.

use tauri::{AppHandle, Emitter, State};

use nexus_core::workflow::gates::{self, Gate, GateInput, GateResult};
use nexus_core::workflow::run::{self, Run, RunFilter, RunView, StartRun};
use nexus_core::workflow::{self, Definition, NewWorkflow, Summary, View, templates};
use nexus_core::{env, project, workspace};

use crate::error::{IpcError, IpcResult, blocking};
use crate::state::Ctx;

pub const WORKFLOW_EVENT: &str = "nexus://workflows";

fn ws_of(ctx: &Ctx, id: &str) -> IpcResult<workspace::Workspace> {
    let ws = workspace::get(&ctx.db(), id)?;
    if !ws.available {
        return Err(IpcError::invalid("the workspace folder is not available on this machine"));
    }
    Ok(ws)
}

#[tauri::command]
pub fn workflow_templates() -> Vec<templates::Template> {
    templates::TEMPLATES.to_vec()
}

#[tauri::command]
pub async fn workflow_list(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<Vec<Summary>> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = ws_of(&ctx, &workspace_id)?;
        Ok(workflow::list(&ctx.db(), &ctx.data_dir, &ws)?)
    })
    .await
}

#[tauri::command]
pub async fn workflow_get(ctx: State<'_, Ctx>, workspace_id: String, id: String) -> IpcResult<View> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = ws_of(&ctx, &workspace_id)?;
        Ok(workflow::get(&ctx.db(), &ctx.data_dir, &ws, &id)?)
    })
    .await
}

#[tauri::command]
pub async fn workflow_create(ctx: State<'_, Ctx>, workspace_id: String, input: NewWorkflow) -> IpcResult<View> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = ws_of(&ctx, &workspace_id)?;
        Ok(workflow::create(&ctx.db(), &ctx.data_dir, &ws, &input)?)
    })
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn workflow_save(
    ctx: State<'_, Ctx>,
    workspace_id: String,
    id: String,
    definition: Definition,
    rigor: String,
    skills: Vec<String>,
    expected_hash: Option<String>,
) -> IpcResult<View> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = ws_of(&ctx, &workspace_id)?;
        Ok(workflow::save(&ctx.db(), &ctx.data_dir, &ws, &id, &definition, Some((&rigor, &skills)), expected_hash.as_deref())?)
    })
    .await
}

#[tauri::command]
pub async fn workflow_save_raw(ctx: State<'_, Ctx>, workspace_id: String, id: String, body: String, expected_hash: Option<String>) -> IpcResult<View> {
    if body.len() > 512 * 1024 {
        return Err(IpcError::invalid("a workflow is at most 512 KiB"));
    }
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = ws_of(&ctx, &workspace_id)?;
        Ok(workflow::save_raw(&ctx.db(), &ctx.data_dir, &ws, &id, &body, expected_hash.as_deref())?)
    })
    .await
}

/// Start a run (its task is created unless `input.taskId` is given). The UI then launches Claude for the run's task.
#[tauri::command]
pub async fn workflow_start(app: AppHandle, ctx: State<'_, Ctx>, workspace_id: String, input: StartRun) -> IpcResult<RunView> {
    let ctx = ctx.inner().clone();
    let v = blocking(move || {
        let ws = ws_of(&ctx, &workspace_id)?;
        Ok(run::start(&ctx.db(), &ctx.data_dir, &ws, &input)?)
    })
    .await?;
    let _ = app.emit(WORKFLOW_EVENT, &v.run.id);
    Ok(v)
}

#[tauri::command]
pub fn workflow_runs(ctx: State<'_, Ctx>, workspace_id: String, filter: Option<RunFilter>) -> IpcResult<Vec<Run>> {
    Ok(run::list(&ctx.db(), &workspace_id, &filter.unwrap_or_default())?)
}

#[tauri::command]
pub fn workflow_run(ctx: State<'_, Ctx>, id: String) -> IpcResult<RunView> {
    Ok(run::view(&ctx.db(), &id)?)
}

fn with_run<T: Send + 'static>(
    app: AppHandle,
    ctx: Ctx,
    id: String,
    f: impl FnOnce(&nexus_core::db::Connection, &std::path::Path, &workspace::Workspace) -> nexus_core::Result<T> + Send + 'static,
) -> impl std::future::Future<Output = IpcResult<T>> {
    blocking(move || {
        let r = run::get_run(&ctx.db(), &id)?;
        let ws = ws_of(&ctx, &r.workspace_id)?;
        let out = f(&ctx.db(), &ctx.data_dir, &ws)?;
        let _ = app.emit(WORKFLOW_EVENT, &id);
        Ok(out)
    })
}

#[tauri::command]
pub async fn workflow_review(app: AppHandle, ctx: State<'_, Ctx>, id: String, step: String, approve: bool, note: Option<String>) -> IpcResult<RunView> {
    let run_id = id.clone();
    with_run(app, ctx.inner().clone(), id, move |db, data, ws| run::review(db, data, ws, &run_id, &step, approve, note.as_deref().unwrap_or(""))).await
}

#[tauri::command]
pub async fn workflow_cancel(app: AppHandle, ctx: State<'_, Ctx>, id: String) -> IpcResult<RunView> {
    let run_id = id.clone();
    with_run(app, ctx.inner().clone(), id, move |db, data, ws| run::cancel(db, data, ws, &run_id)).await
}

#[tauri::command]
pub async fn workflow_retry(app: AppHandle, ctx: State<'_, Ctx>, id: String, step: String) -> IpcResult<RunView> {
    let run_id = id.clone();
    with_run(app, ctx.inner().clone(), id, move |db, data, ws| run::retry(db, data, ws, &run_id, &step)).await
}

#[tauri::command]
pub fn gates_list(ctx: State<'_, Ctx>, project_id: String) -> IpcResult<Vec<(Gate, Option<GateResult>)>> {
    let db = ctx.db();
    Ok(gates::list(&db, &project_id)?.into_iter().map(|g| {
        let last = gates::latest(&db, &project_id, &g.name).ok().flatten();
        (g, last)
    }).collect())
}

#[tauri::command]
pub fn gate_upsert(ctx: State<'_, Ctx>, project_id: String, input: GateInput) -> IpcResult<Gate> {
    Ok(gates::upsert(&ctx.db(), &project_id, &input)?)
}

/// Authorise a gate's command as reviewed: `expected_hash` is the command hash the person saw; a command
/// changed since (a new version) is refused. Separate from approving a workflow definition.
#[tauri::command]
pub fn gate_approve(app: AppHandle, ctx: State<'_, Ctx>, project_id: String, name: String, expected_hash: String) -> IpcResult<Gate> {
    let db = ctx.db();
    let g = gates::approve(&db, &project_id, &name, &expected_hash)?;
    // Steps that waited for this approval verify again (the runner picks them up).
    if run::resume_gate_waits(&db, &project_id)? > 0 {
        let _ = app.emit(WORKFLOW_EVENT, ());
    }
    Ok(g)
}

#[tauri::command]
pub fn gate_revoke(ctx: State<'_, Ctx>, project_id: String, name: String) -> IpcResult<()> {
    Ok(gates::revoke(&ctx.db(), &project_id, &name)?)
}

#[tauri::command]
pub fn gate_remove(ctx: State<'_, Ctx>, project_id: String, name: String) -> IpcResult<bool> {
    Ok(gates::remove(&ctx.db(), &project_id, &name)?)
}

#[tauri::command]
pub fn gate_suggest(ctx: State<'_, Ctx>, project_id: String) -> IpcResult<Vec<GateInput>> {
    Ok(gates::suggest(&project::get(&ctx.db(), &project_id)?))
}

/// Run one gate now (not part of a run) and record the result — e.g. to check a constitution rule's compliance.
#[tauri::command]
pub async fn gate_run(ctx: State<'_, Ctx>, project_id: String, name: String) -> IpcResult<GateResult> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let (g, dir, id) = {
            let db = ctx.db();
            let p = project::get(&db, &project_id)?;
            let g = gates::find(&db, &p.id, &name)?.ok_or_else(|| IpcError::invalid(format!("no gate {name} for {}", p.name)))?;
            let dir = p.local_path.clone().filter(|d| d.is_dir()).ok_or_else(|| IpcError::invalid("the project folder is not available"))?;
            let id = gates::begin(&db, &p.id, None, &g, &ctx.host.id)?;
            (g, dir, id)
        };
        let o = gates::execute(&g, &dir, &env::current(), &id);
        Ok(gates::finish(&ctx.db(), &id, &o)?)
    })
    .await
}

/// Execute pending gate verifications (runs, check steps) — the desktop's host role for workflows.
pub fn spawn_runner(app: AppHandle, ctx: Ctx) {
    let _ = std::thread::Builder::new().name("workflow-runner".into()).spawn(move || {
        let base = env::current();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1200));
            let r = run::verify_pending(|f: &mut dyn FnMut(&nexus_core::db::Connection) -> nexus_core::Result<()>| f(&ctx.db()), &ctx.data_dir, &ctx.host.id, &base);
            match r {
                Ok(n) if n > 0 => {
                    let _ = app.emit(WORKFLOW_EVENT, ());
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "workflow runner"),
            }
        }
    });
}

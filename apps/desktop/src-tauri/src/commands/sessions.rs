//! Session commands: launching Claude and shells in PTYs, attaching terminals, lifecycle (ADR-0008).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, State};

use nexus_core::claude::install;
use nexus_core::claude::mcp::{self, IntegrationStatus};
use nexus_core::claude::prepare::{self, Prepared};
use nexus_core::claude::launch::{self, LaunchRequest, Mode};
use nexus_core::knowledge::{index as kindex, service as knowledge};
use nexus_core::claude::profile::{self, ClaudeProfile};
use nexus_core::env;
use nexus_core::paths::{self, Expect};
use nexus_core::session::{self, Kind, NewSession, Session};
use nexus_core::host::HostKind;
use nexus_core::{attention, git, new_id, notify, project, workprofile, workspace};

use crate::error::{IpcError, IpcResult, blocking};
use crate::pty::SpawnRequest;
use crate::shells::{self, ShellInfo};
use crate::state::Ctx;

pub const SESSION_EVENT: &str = "nexus://session";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    #[serde(flatten)]
    pub session: Session,
    /// A PTY for this session exists in this NEXUS process (its output can be attached).
    pub attached: bool,
    pub running: bool,
    /// Live, but owned by another session host (e.g. `nexus launch` in a terminal): visible, not attachable here.
    pub live_elsewhere: bool,
    /// Claude sessions: whether Claude recorded a conversation that `--resume` can continue.
    pub conversation_recorded: Option<bool>,
    pub resumable: bool,
    /// What the session needs from the user, from reported signals only (ADR-0025).
    pub status: attention::SessionStatus,
}

fn view(ctx: &Ctx, s: Session, profiles: &HashMap<String, ClaudeProfile>) -> SessionView {
    let running = ctx.pty.is_running(&s.id);
    let conversation_recorded = match (s.kind, &s.claude_session_id, &s.profile_id) {
        (Kind::Claude, Some(csid), Some(pid)) => profiles
            .get(pid)
            .map(|p| session::conversation_recorded(&p.effective_config_dir(&ctx.home), csid)),
        _ => None,
    };
    let live_elsewhere = !running && s.state.is_live() && s.host_id.as_deref() != Some(ctx.host.id.as_str());
    let resumable = !running && !live_elsewhere && conversation_recorded == Some(true) && s.cwd.is_dir();
    let status = attention::status(&s, running || live_elsewhere);
    SessionView {
        status,
        attached: running || ctx.pty.is_attached(&s.id),
        running,
        live_elsewhere,
        conversation_recorded,
        resumable,
        session: s,
    }
}

fn profiles_by_id(ctx: &Ctx) -> IpcResult<HashMap<String, ClaudeProfile>> {
    Ok(profile::list(&ctx.db(), true)?.into_iter().map(|p| (p.id.clone(), p)).collect())
}

fn view_of(ctx: &Ctx, id: &str) -> IpcResult<SessionView> {
    let profiles = profiles_by_id(ctx)?;
    let s = session::get(&ctx.db(), id)?;
    Ok(view(ctx, s, &profiles))
}

fn emit(app: &AppHandle, ctx: &Ctx, id: &str) {
    match view_of(ctx, id) {
        Ok(v) => {
            let _ = app.emit(SESSION_EVENT, v);
        }
        Err(e) => tracing::warn!(session = id, error = %e.message, "cannot emit session update"),
    }
}

#[tauri::command]
pub async fn session_list(ctx: State<'_, Ctx>, filter: Option<session::Filter>) -> IpcResult<Vec<SessionView>> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let profiles = profiles_by_id(&ctx)?;
        let rows = session::list(&ctx.db(), &filter.unwrap_or_default())?;
        Ok(rows.into_iter().map(|s| view(&ctx, s, &profiles)).collect())
    })
    .await
}

#[tauri::command]
pub async fn session_get(ctx: State<'_, Ctx>, id: String) -> IpcResult<SessionView> {
    let ctx = ctx.inner().clone();
    blocking(move || view_of(&ctx, &id)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartClaude {
    pub profile_id: String,
    pub project_id: Option<String>,
    /// Used when no project is given (e.g. ad-hoc directory).
    pub cwd: Option<String>,
    pub work_profile: Option<String>,
    pub effort: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    /// Work on this knowledge task (ADR-0018): its project is used when none is given.
    pub task_id: Option<String>,
    /// Move a backlog/ready task to in-progress (default: yes).
    pub start_task: Option<bool>,
    /// Ask Claude to start on the task right away (NEXUS sends `tasks::KICKOFF` as the first message; default: no).
    pub kickoff: Option<bool>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

struct Target {
    cwd: PathBuf,
    workspace_id: Option<String>,
    project_id: Option<String>,
    default_title: String,
}

fn resolve_target(ctx: &Ctx, project_id: Option<&str>, cwd: Option<&str>) -> IpcResult<Target> {
    let db = ctx.db();
    if let Some(pid) = project_id {
        let p = project::get(&db, pid)?;
        let cwd = p
            .local_path
            .clone()
            .filter(|lp| lp.is_dir())
            .ok_or_else(|| IpcError::invalid(format!("project '{}' is not available on this machine", p.name)))?;
        return Ok(Target {
            cwd,
            workspace_id: Some(p.workspace_id),
            project_id: Some(p.id),
            default_title: p.name,
        });
    }
    let raw = cwd.ok_or_else(|| IpcError::invalid("choose a project or a directory"))?;
    let cwd = paths::validate_user_path(raw, Expect::ExistingDir)?;
    let default_title = cwd.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "session".into());
    Ok(Target {
        cwd,
        workspace_id: workspace::active(&db)?.map(|w| w.id),
        project_id: None,
        default_title,
    })
}

/// Session-scoped NEXUS integrations and appended context (shared with `nexus launch`: `claude::prepare`).
#[allow(clippy::too_many_arguments)]
fn prepare_session(
    ctx: &Ctx,
    profile: &ClaudeProfile,
    nexus_id: &str,
    workspace_id: Option<&str>,
    project_id: Option<&str>,
    task_id: Option<&str>,
    cwd: &std::path::Path,
    start_task: bool,
) -> IpcResult<Prepared> {
    let exe = mcp::nexus_next_to_current_exe();
    let base_env = env::current();
    let db = ctx.db();
    let prepared = prepare::prepare(&prepare::Request {
        conn: &db,
        data_dir: &ctx.data_dir,
        home: &ctx.home,
        profile,
        machine: &ctx.machine_policy,
        nexus_exe: exe.as_deref(),
        nexus_session_id: nexus_id,
        workspace_id,
        project_id,
        task_id,
        cwd,
        base_env: &base_env,
        start_task,
    })?;
    for (name, status) in [("MCP tools", &prepared.integrations.mcp), ("lifecycle hooks", &prepared.integrations.hooks)] {
        if let IntegrationStatus::Blocked { reason } | IntegrationStatus::Unavailable { reason } = status {
            tracing::info!(profile = %profile.name, integration = name, %reason, "NEXUS integration not enabled for this session");
        }
    }
    Ok(prepared)
}

/// The project of a task in the active workspace (a task launch without an explicit project).
fn task_project(ctx: &Ctx, task_id: &str) -> IpcResult<Option<String>> {
    let db = ctx.db();
    let ws = workspace::active(&db)?.ok_or_else(|| IpcError::invalid("no active workspace"))?;
    knowledge::refresh(&db, &ctx.data_dir, &ws)?;
    Ok(kindex::get(&db, &ws.id, task_id)?.project_id)
}

fn pty_size(cols: Option<u16>, rows: Option<u16>) -> (u16, u16) {
    (cols.unwrap_or(120), rows.unwrap_or(32))
}

#[tauri::command]
pub async fn session_start_claude(app: AppHandle, ctx: State<'_, Ctx>, args: StartClaude) -> IpcResult<SessionView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let profile = profile::get(&ctx.db(), &args.profile_id)?;
        if profile.archived_at.is_some() {
            return Err(IpcError::invalid("this profile is archived"));
        }
        let project_id = match (&args.project_id, &args.task_id) {
            (None, Some(t)) => task_project(&ctx, t)?,
            (p, _) => p.clone(),
        };
        let target = resolve_target(&ctx, project_id.as_deref(), args.cwd.as_deref())?;
        let exe = install::resolve(profile.executable.as_deref(), &ctx.home)?;

        let wp_id = args.work_profile.or_else(|| profile.default_work_profile.clone());
        let wp = match wp_id.as_deref() {
            Some(id) => Some(workprofile::get(id).ok_or_else(|| IpcError::invalid(format!("unknown work profile '{id}'")))?),
            None => None,
        };
        let effort = args.effort.filter(|e| !e.is_empty()).or_else(|| profile.default_effort.clone());
        let model = args.model.filter(|m| !m.is_empty()).or_else(|| profile.default_model.clone());
        let title = args
            .title
            .as_deref()
            .map(launch::sanitize_title)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| target.default_title.clone());

        let nexus_id = new_id();
        let claude_session_id = uuid::Uuid::new_v4().to_string();
        let prepared = prepare_session(
            &ctx,
            &profile,
            &nexus_id,
            target.workspace_id.as_deref(),
            target.project_id.as_deref(),
            args.task_id.as_deref(),
            &target.cwd,
            args.start_task.unwrap_or(true),
        )?;
        let title = match (&args.title, &prepared.integrations.task) {
            (None, Some(t)) => launch::sanitize_title(&t.title),
            _ => title,
        };
        let settings_path = prepared.settings.clone();
        let plugin_dirs = prepared.plugin_dirs.clone();
        let claude_name = prepared.claude_name.clone();
        let integrations = prepared.integrations;
        let spec = launch::build(
            &LaunchRequest {
                profile: &profile,
                executable: &exe,
                cwd: &target.cwd,
                mode: Mode::New,
                claude_session_id: &claude_session_id,
                title: Some(claude_name.as_deref().unwrap_or(&title)),
                work_profile: wp,
                effort: effort.as_deref(),
                model: model.as_deref(),
                nexus_session_id: &nexus_id,
                workspace_id: target.workspace_id.as_deref(),
                project_id: target.project_id.as_deref(),
                mcp_config: integrations.mcp.config_path(),
                settings: settings_path.as_deref(),
                context_prompt: prepared.prompt.as_deref(),
                plugin_dirs: &plugin_dirs,
                // Only for a task whose text reaches Claude (session start or tools) — otherwise nothing to start on.
                first_prompt: (args.kickoff == Some(true) && integrations.task.as_ref().is_some_and(|t| t.delivery != "none"))
                    .then_some(nexus_core::knowledge::tasks::KICKOFF),
            },
            &env::current(),
        )?;
        let branch = git::status(&target.cwd).ok().and_then(|s| s.branch);

        let session = {
            let db = ctx.db();
            let s = session::insert_starting(
                &db,
                NewSession {
                    id: Some(nexus_id),
                    kind: Some(Kind::Claude),
                    host: Some(HostKind::Desktop),
                    host_id: Some(ctx.host.id.clone()),
                    workspace_id: target.workspace_id,
                    project_id: target.project_id.clone(),
                    profile_id: Some(profile.id.clone()),
                    claude_session_id: Some(claude_session_id),
                    title,
                    cwd: target.cwd,
                    branch,
                    work_profile: wp.map(|w| w.id.to_string()),
                    effort,
                    model,
                    program: exe.display().to_string(),
                    resumed_from: None,
                    task_id: integrations.task.as_ref().map(|t| t.id.clone()),
                },
            )?;
            session::set_integrations(&db, &s.id, &integrations)?;
            profile::touch_used(&db, &profile.id)?;
            if let Some(pid) = &target.project_id {
                project::touch_opened(&db, pid)?;
            }
            s
        };
        let (cols, rows) = pty_size(args.cols, args.rows);
        spawn(&app, &ctx, &session, SpawnRequest {
            program: spec.program,
            args: spec.args,
            cwd: spec.cwd,
            env: spec.env,
            cols,
            rows,
        })
    })
    .await
}

#[tauri::command]
pub async fn session_resume(
    app: AppHandle,
    ctx: State<'_, Ctx>,
    id: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> IpcResult<SessionView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let prev = session::get(&ctx.db(), &id)?;
        if prev.kind != Kind::Claude {
            return Err(IpcError::invalid("only Claude sessions can be resumed"));
        }
        if ctx.pty.is_running(&prev.id) {
            return Err(IpcError::invalid("this session is still running"));
        }
        let csid = prev.claude_session_id.clone().ok_or_else(|| IpcError::invalid("session has no conversation id"))?;
        let profile_id = prev.profile_id.clone().ok_or_else(|| IpcError::invalid("the session's profile was removed"))?;
        let profile = profile::get(&ctx.db(), &profile_id)?;
        if !session::conversation_recorded(&profile.effective_config_dir(&ctx.home), &csid) {
            return Err(IpcError::invalid("Claude has no recorded conversation for this session"));
        }
        if !prev.cwd.is_dir() {
            return Err(IpcError::invalid(format!("{} no longer exists", prev.cwd.display())));
        }
        let exe = install::resolve(profile.executable.as_deref(), &ctx.home)?;
        let wp = prev.work_profile.as_deref().and_then(workprofile::get);
        let nexus_id = new_id();
        let prepared = prepare_session(
            &ctx,
            &profile,
            &nexus_id,
            prev.workspace_id.as_deref(),
            prev.project_id.as_deref(),
            prev.task_id.as_deref(),
            &prev.cwd,
            false,
        )?;
        let settings_path = prepared.settings.clone();
        let plugin_dirs = prepared.plugin_dirs.clone();
        let integrations = prepared.integrations;
        let spec = launch::build(
            &LaunchRequest {
                profile: &profile,
                executable: &exe,
                cwd: &prev.cwd,
                mode: Mode::Resume,
                claude_session_id: &csid,
                title: None,
                work_profile: wp,
                effort: prev.effort.as_deref(),
                model: prev.model.as_deref(),
                nexus_session_id: &nexus_id,
                workspace_id: prev.workspace_id.as_deref(),
                project_id: prev.project_id.as_deref(),
                mcp_config: integrations.mcp.config_path(),
                settings: settings_path.as_deref(),
                context_prompt: prepared.prompt.as_deref(),
                plugin_dirs: &plugin_dirs,
                first_prompt: None, // a resumed conversation continues where it was
            },
            &env::current(),
        )?;
        let branch = git::status(&prev.cwd).ok().and_then(|s| s.branch);
        let session = {
            let db = ctx.db();
            let s = session::insert_starting(
                &db,
                NewSession {
                    id: Some(nexus_id),
                    kind: Some(Kind::Claude),
                    host: Some(HostKind::Desktop),
                    host_id: Some(ctx.host.id.clone()),
                    workspace_id: prev.workspace_id.clone(),
                    project_id: prev.project_id.clone(),
                    profile_id: Some(profile.id.clone()),
                    claude_session_id: Some(csid),
                    title: prev.title.clone(),
                    cwd: prev.cwd.clone(),
                    branch,
                    work_profile: prev.work_profile.clone(),
                    effort: prev.effort.clone(),
                    model: prev.model.clone(),
                    program: exe.display().to_string(),
                    resumed_from: Some(prev.id.clone()),
                    task_id: prev.task_id.clone(),
                },
            )?;
            session::set_integrations(&db, &s.id, &integrations)?;
            profile::touch_used(&db, &profile.id)?;
            s
        };
        let (cols, rows) = pty_size(cols, rows);
        spawn(&app, &ctx, &session, SpawnRequest {
            program: spec.program,
            args: spec.args,
            cwd: spec.cwd,
            env: spec.env,
            cols,
            rows,
        })
    })
    .await
}

#[tauri::command]
pub fn shell_list() -> Vec<ShellInfo> {
    shells::available()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenShell {
    pub shell_id: Option<String>,
    pub project_id: Option<String>,
    pub cwd: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

#[tauri::command]
pub async fn session_open_shell(app: AppHandle, ctx: State<'_, Ctx>, args: OpenShell) -> IpcResult<SessionView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let shell = match args.shell_id.as_deref() {
            Some(id) => shells::find(id).ok_or_else(|| IpcError::invalid(format!("shell '{id}' is not available")))?,
            None => shells::default_shell().ok_or_else(|| IpcError::invalid("no shell found on this system"))?,
        };
        let target = match (args.project_id.as_deref(), args.cwd.as_deref()) {
            (None, None) => Target {
                cwd: ctx.home.clone(),
                workspace_id: workspace::active(&ctx.db())?.map(|w| w.id),
                project_id: None,
                default_title: "home".into(),
            },
            (p, c) => resolve_target(&ctx, p, c)?,
        };
        let child_env = env::shell_env(&env::current());
        let session = session::insert_starting(
            &ctx.db(),
            NewSession {
                kind: Some(Kind::Shell),
                host: Some(HostKind::Desktop),
                host_id: Some(ctx.host.id.clone()),
                workspace_id: target.workspace_id,
                project_id: target.project_id,
                title: format!("{} · {}", shell.name, target.default_title),
                cwd: target.cwd.clone(),
                program: shell.program.display().to_string(),
                ..Default::default()
            },
        )?;
        let (cols, rows) = pty_size(args.cols, args.rows);
        spawn(&app, &ctx, &session, SpawnRequest {
            program: shell.program,
            args: shell.args,
            cwd: target.cwd,
            env: child_env,
            cols,
            rows,
        })
    })
    .await
}

fn spawn(app: &AppHandle, ctx: &Ctx, session: &Session, req: SpawnRequest) -> IpcResult<SessionView> {
    let app_exit = app.clone();
    let ctx_exit = ctx.clone();
    let sid = session.id.clone();
    let on_exit = move |code: Option<i64>| {
        let stopped_by_user = ctx_exit.attention.take_stopped(&sid);
        if let Err(e) = notify::session_exited(&ctx_exit.db(), &sid, code, stopped_by_user) {
            tracing::error!(session = %sid, error = %e, "cannot record session exit");
        }
        // The session's development record (ADR-0038), on its own connection so the UI stays responsive.
        // …and the workspace's sync policy acts on it (ADR-0039; manual = nothing happens).
        // Autonomous sync (ADR-0042) publishes whatever the session changed, record or not.
        let record = nexus_core::db::open(&ctx_exit.data_dir.join(nexus_core::db::DB_FILE)).map_err(|e| e.to_string()).and_then(|c| {
            nexus_core::devrecord::finalize(&c, &ctx_exit.data_dir, &sid).map_err(|e| e.to_string())?;
            nexus_core::workspace_sync::policy::session_ended(&c, &sid).map_err(|e| e.to_string())?;
            let managed = nexus_core::workspace_sync::policy::default_managed_path();
            nexus_core::workspace_sync::policy::run_due(&c, &ctx_exit.data_dir, &ctx_exit.home, &ctx_exit.host.id, &managed).map_err(|e| e.to_string())?;
            Ok(())
        });
        if let Err(e) = record {
            tracing::warn!(session = %sid, error = %e, "development record or sync policy not applied");
        }
        emit(&app_exit, &ctx_exit, &sid);
    };
    match ctx.pty.spawn(&session.id, req, on_exit) {
        Ok(pid) => session::mark_running(&ctx.db(), &session.id, pid)?,
        Err(e) => {
            session::mark_failed(&ctx.db(), &session.id, &e)?;
            emit(app, ctx, &session.id);
            return Err(IpcError::terminal(e));
        }
    }
    emit(app, ctx, &session.id);
    view_of(ctx, &session.id)
}

/// Subscribe a terminal view to a session's output (replays recent output first).
#[tauri::command]
pub fn session_attach(ctx: State<'_, Ctx>, id: String, output: Channel<InvokeResponseBody>) -> IpcResult<bool> {
    ctx.pty.attach(&id, output).map_err(IpcError::terminal)
}

#[tauri::command]
pub fn session_write(ctx: State<'_, Ctx>, id: String, data: String, binary: Option<bool>) -> IpcResult<()> {
    let bytes: Vec<u8> = if binary.unwrap_or(false) {
        data.chars().map(|c| c as u32).map(|c| u8::try_from(c).unwrap_or(b'?')).collect()
    } else {
        data.into_bytes()
    };
    ctx.pty.write(&id, &bytes).map_err(IpcError::terminal)
}

#[tauri::command]
pub fn session_resize(ctx: State<'_, Ctx>, id: String, cols: u16, rows: u16) -> IpcResult<()> {
    ctx.pty.resize(&id, cols, rows).map_err(IpcError::terminal)
}

#[tauri::command]
pub fn session_interrupt(ctx: State<'_, Ctx>, id: String) -> IpcResult<()> {
    ctx.pty.interrupt(&id).map_err(IpcError::terminal)
}

/// Forcefully end a process NEXUS started. The UI asks for confirmation first.
#[tauri::command]
pub fn session_terminate(ctx: State<'_, Ctx>, id: String) -> IpcResult<()> {
    // The user ended it: no "session interrupted" notification for that.
    ctx.attention.mark_stopped(&id);
    ctx.pty.terminate(&id).map_err(IpcError::terminal)
}

/// Free the buffer of an exited terminal (tab closed).
#[tauri::command]
pub fn session_release(ctx: State<'_, Ctx>, id: String) -> bool {
    ctx.pty.release(&id)
}

#[tauri::command]
pub async fn session_rename(app: AppHandle, ctx: State<'_, Ctx>, id: String, title: String) -> IpcResult<SessionView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        session::rename(&ctx.db(), &id, &title)?;
        emit(&app, &ctx, &id);
        view_of(&ctx, &id)
    })
    .await
}

/// Hand a task to a running Claude session of its project (instead of a new session): the session already has the
/// project's context. NEXUS types a fixed message with the task id into its terminal (the task text arrives through
/// `get_task`), starts the task and moves the session's task link.
#[tauri::command]
pub async fn session_continue_task(app: AppHandle, ctx: State<'_, Ctx>, id: String, task_id: String) -> IpcResult<SessionView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        {
            let db = ctx.db();
            let s = session::get(&db, &id)?;
            if s.kind != Kind::Claude || !ctx.pty.is_running(&id) {
                return Err(IpcError::invalid("only a running Claude session in this window can take on a task"));
            }
            let tools_on = s.integrations.as_ref().is_some_and(|i| i["mcp"]["state"] == "enabled");
            if !tools_on {
                return Err(IpcError::invalid("this session has no NEXUS tools, so it cannot read the task — start a new session for it"));
            }
            let ws_id = s.workspace_id.clone().ok_or_else(|| IpcError::invalid("this session belongs to no workspace"))?;
            let ws = workspace::get(&db, &ws_id)?;
            let v = nexus_core::knowledge::tasks::view(&db, &ws, &task_id, 200)?;
            if v.project_id.is_some() && v.project_id != s.project_id {
                return Err(IpcError::invalid("the task belongs to another project"));
            }
            nexus_core::knowledge::tasks::start(&db, &ctx.data_dir, &ws, &v.id)?;
            session::switch_task(&db, &id, &v.id)?;
            let text = nexus_core::knowledge::tasks::handoff(&v.id);
            ctx.pty.write(&id, format!("{text}\r").as_bytes()).map_err(IpcError::terminal)?;
        }
        emit(&app, &ctx, &id);
        view_of(&ctx, &id)
    })
    .await
}

#[tauri::command]
pub async fn session_set_archived(app: AppHandle, ctx: State<'_, Ctx>, id: String, archived: bool) -> IpcResult<SessionView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        if ctx.pty.is_running(&id) {
            return Err(IpcError::invalid("stop the session before archiving it"));
        }
        session::set_archived(&ctx.db(), &id, archived)?;
        emit(&app, &ctx, &id);
        view_of(&ctx, &id)
    })
    .await
}

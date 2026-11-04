//! `nexus launch`: Claude Code in the current terminal, with exactly the same profile isolation, session records
//! and resume semantics as the desktop (all from `nexus-core`). This process is a `cli` session host (ADR-0014).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use clap::Args;

use nexus_core::claude::install;
use nexus_core::claude::hooks;
use nexus_core::claude::mcp::{self, Integrations};
use nexus_core::claude::policy::{self, MachinePolicyLocations};
use nexus_core::claude::launch::{self, LaunchRequest, Mode};
use nexus_core::claude::profile::{self, ClaudeProfile};
use nexus_core::db::{self, Connection};
use nexus_core::host::{self, HostKind};
use nexus_core::paths::{self, Expect};
use nexus_core::session::{self, Filter, Kind, NewSession, Session};
use nexus_core::{CoreError, Result, env, git, new_id, project, resolve_ref, workprofile, workspace};

use crate::Ctx;

#[derive(Args)]
pub struct LaunchArgs {
    /// Claude profile (name or id). Optional when only one profile is registered.
    #[arg(long, short = 'p')]
    profile: Option<String>,
    /// Project of the active workspace (name or id). Default: the project containing the working directory.
    #[arg(long)]
    project: Option<String>,
    /// Working directory when no project is given (default: current directory).
    #[arg(long, value_name = "DIR")]
    cwd: Option<String>,
    /// Engineering rigor: prototype | balanced | professional | critical.
    #[arg(long = "work-profile", short = 'w')]
    work_profile: Option<String>,
    /// Native reasoning effort: low | medium | high | xhigh | max.
    #[arg(long)]
    effort: Option<String>,
    #[arg(long)]
    model: Option<String>,
    /// Session name shown in Claude and NEXUS.
    #[arg(long)]
    name: Option<String>,
    /// Resume a NEXUS session (id or id prefix), or `last` for the most recent resumable one here.
    #[arg(long, value_name = "SESSION|last")]
    resume: Option<String>,
    /// Do not print the one-line launch summary.
    #[arg(long, short = 'q')]
    quiet: bool,
}

struct Plan {
    profile: ClaudeProfile,
    cwd: PathBuf,
    workspace_id: Option<String>,
    project_id: Option<String>,
    title: String,
    mode: Mode,
    claude_session_id: String,
    work_profile: Option<&'static workprofile::WorkProfile>,
    effort: Option<String>,
    model: Option<String>,
    resumed_from: Option<String>,
}

pub fn run(ctx: &Ctx, a: LaunchArgs) -> Result<i32> {
    let conn = ctx.db()?;
    let plan = if let Some(r) = a.resume.as_deref() {
        plan_resume(&conn, ctx, &a, r)?
    } else {
        plan_new(&conn, &a)?
    };
    let exe = install::resolve(plan.profile.executable.as_deref(), &ctx.home)?;
    let me = host::register(&conn, HostKind::Cli)?;
    let nexus_id = new_id();
    let policy = policy::evaluate(&plan.profile.effective_config_dir(&ctx.home), &MachinePolicyLocations::system());
    let exe_self = mcp::nexus_next_to_current_exe();
    let integrations = Integrations {
        mcp: mcp::prepare(&policy, exe_self.as_deref(), &ctx.data_dir, &nexus_id)?,
        hooks: hooks::prepare(&policy, exe_self.as_deref(), &ctx.data_dir, &nexus_id)?,
    };
    let spec = launch::build(
        &LaunchRequest {
            profile: &plan.profile,
            executable: &exe,
            cwd: &plan.cwd,
            mode: plan.mode,
            claude_session_id: &plan.claude_session_id,
            title: (plan.mode == Mode::New).then_some(plan.title.as_str()),
            work_profile: plan.work_profile,
            effort: plan.effort.as_deref(),
            model: plan.model.as_deref(),
            nexus_session_id: &nexus_id,
            workspace_id: plan.workspace_id.as_deref(),
            project_id: plan.project_id.as_deref(),
            mcp_config: integrations.mcp.config_path(),
            settings: integrations.hooks.config_path(),
        },
        &env::current(),
    )?;
    let session = session::insert_starting(
        &conn,
        NewSession {
            id: Some(nexus_id),
            kind: Some(Kind::Claude),
            host: Some(HostKind::Cli),
            host_id: Some(me.id.clone()),
            workspace_id: plan.workspace_id.clone(),
            project_id: plan.project_id.clone(),
            profile_id: Some(plan.profile.id.clone()),
            claude_session_id: Some(plan.claude_session_id.clone()),
            title: plan.title.clone(),
            cwd: plan.cwd.clone(),
            branch: git::status(&plan.cwd).ok().and_then(|s| s.branch),
            work_profile: plan.work_profile.map(|w| w.id.to_string()),
            effort: plan.effort.clone(),
            model: plan.model.clone(),
            program: exe.display().to_string(),
            resumed_from: plan.resumed_from.clone(),
        },
    )?;
    session::set_integrations(&conn, &session.id, &integrations)?;
    profile::touch_used(&conn, &plan.profile.id)?;
    if let Some(pid) = &plan.project_id {
        project::touch_opened(&conn, pid)?;
    }
    if !a.quiet {
        let state = |label: &str, s: &mcp::IntegrationStatus| match s {
            mcp::IntegrationStatus::Enabled { .. } => format!("{label} on"),
            mcp::IntegrationStatus::Blocked { reason } | mcp::IntegrationStatus::Unavailable { reason } => format!("{label} off ({reason})"),
        };
        let tools = format!("{} · {}", state("NEXUS tools", &integrations.mcp), state("hooks", &integrations.hooks));
        eprintln!(
            "nexus: {} Claude · profile {} · {} · {} · {tools} · session {}",
            if plan.mode == Mode::Resume { "resuming" } else { "launching" },
            plan.profile.name,
            plan.work_profile.map(|w| w.name).unwrap_or("no work profile"),
            plan.cwd.display(),
            &session.id[..8]
        );
    }

    // Claude owns Ctrl+C (interrupt / exit); NEXUS must survive it to record the outcome.
    let _ = ctrlc::set_handler(|| {});
    let mut cmd = std::process::Command::new(&spec.program);
    cmd.args(&spec.args).current_dir(&spec.cwd);
    env::apply(&mut cmd, &spec.env);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("cannot start {}: {e}", spec.program.display());
            session::mark_failed(&conn, &session.id, &msg)?;
            host::shutdown(&conn, &me.id)?;
            return Err(CoreError::External {
                program: "claude".into(),
                message: msg,
            });
        }
    };
    session::mark_running(&conn, &session.id, Some(child.id()))?;

    let stop = Arc::new(AtomicBool::new(false));
    let beat = spawn_heartbeat(ctx.data_dir.join(db::DB_FILE), me.id.clone(), session.id.clone(), stop.clone());
    let status = child.wait();
    stop.store(true, Ordering::SeqCst);
    let _ = beat.join();

    let code = status.ok().and_then(|s| s.code()).map(i64::from);
    session::mark_exited(&conn, &session.id, code)?;
    host::shutdown(&conn, &me.id)?;
    Ok(code.map(|c| c as i32).unwrap_or(1))
}

/// Lease refresh on its own connection (a `Connection` is not shared across threads).
fn spawn_heartbeat(db_path: PathBuf, host_id: String, session_id: String, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let Ok(conn) = db::open(&db_path) else { return };
        let mut last = Instant::now();
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(200));
            if last.elapsed() >= host::HEARTBEAT_INTERVAL {
                last = Instant::now();
                let _ = host::heartbeat(&conn, &host_id, std::slice::from_ref(&session_id));
            }
        }
    })
}

fn find_profile(conn: &Connection, query: Option<&str>) -> Result<ClaudeProfile> {
    let list = profile::list(conn, false)?;
    match query {
        Some(q) => resolve_ref(&list, q, |p| &p.id, |p| &p.name, "profile").cloned(),
        None => match list.as_slice() {
            [only] => Ok(only.clone()),
            [] => Err(CoreError::invalid("no Claude profile is registered — `nexus profiles add --name <name> …`")),
            _ => Err(CoreError::invalid(format!(
                "choose a profile with --profile ({})",
                list.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
            ))),
        },
    }
}

fn current_dir(a: &LaunchArgs) -> Result<PathBuf> {
    match a.cwd.as_deref() {
        Some(dir) => paths::validate_user_path(dir, Expect::ExistingDir),
        None => {
            let cwd = std::env::current_dir().map_err(|e| CoreError::io("cannot read", ".", e))?;
            paths::validate_user_path(&cwd.to_string_lossy(), Expect::ExistingDir)
        }
    }
}

/// The deepest registered project of the active workspace that contains `dir`.
fn project_containing(projects: &[project::Project], dir: &Path) -> Option<project::Project> {
    projects
        .iter()
        .filter(|p| p.local_path.as_deref().is_some_and(|lp| starts_with_path(dir, lp)))
        .max_by_key(|p| p.local_path.as_ref().map(|l| l.as_os_str().len()).unwrap_or(0))
        .cloned()
}

fn starts_with_path(path: &Path, prefix: &Path) -> bool {
    if cfg!(windows) {
        let (p, q) = (path.to_string_lossy().to_lowercase(), prefix.to_string_lossy().to_lowercase());
        p == q || p.strip_prefix(&q).is_some_and(|rest| rest.starts_with('\\'))
    } else {
        path.starts_with(prefix)
    }
}

fn plan_new(conn: &Connection, a: &LaunchArgs) -> Result<Plan> {
    let profile = find_profile(conn, a.profile.as_deref())?;
    let ws = workspace::active(conn)?;
    let projects = match &ws {
        Some(w) => project::list(conn, &w.id)?,
        None => Vec::new(),
    };
    let (cwd, proj) = match a.project.as_deref() {
        Some(q) => {
            let p = resolve_ref(&projects, q, |p| &p.id, |p| &p.name, "project")?.clone();
            let lp = p
                .local_path
                .clone()
                .filter(|l| l.is_dir())
                .ok_or_else(|| CoreError::invalid(format!("project '{}' is not on this machine", p.name)))?;
            (lp, Some(p))
        }
        None => {
            let dir = current_dir(a)?;
            let p = project_containing(&projects, &dir);
            (dir, p)
        }
    };
    let wp_id = a.work_profile.clone().or_else(|| profile.default_work_profile.clone());
    let work_profile = match wp_id.as_deref() {
        Some(id) => Some(workprofile::get(id).ok_or_else(|| {
            CoreError::invalid(format!("unknown work profile '{id}' (prototype, balanced, professional, critical)"))
        })?),
        None => None,
    };
    let title = a
        .name
        .as_deref()
        .map(launch::sanitize_title)
        .filter(|t| !t.is_empty())
        .or_else(|| proj.as_ref().map(|p| p.name.clone()))
        .unwrap_or_else(|| cwd.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "session".into()));
    Ok(Plan {
        effort: a.effort.clone().or_else(|| profile.default_effort.clone()),
        model: a.model.clone().or_else(|| profile.default_model.clone()),
        profile,
        workspace_id: proj.as_ref().map(|p| p.workspace_id.clone()).or(ws.map(|w| w.id)),
        project_id: proj.map(|p| p.id),
        cwd,
        title,
        mode: Mode::New,
        claude_session_id: uuid::Uuid::new_v4().to_string(),
        work_profile,
        resumed_from: None,
    })
}

fn plan_resume(conn: &Connection, ctx: &Ctx, a: &LaunchArgs, which: &str) -> Result<Plan> {
    let profiles = profile::list(conn, true)?;
    let recorded = |s: &Session| -> bool {
        let (Some(pid), Some(csid)) = (&s.profile_id, &s.claude_session_id) else { return false };
        profiles
            .iter()
            .find(|p| &p.id == pid)
            .is_some_and(|p| session::conversation_recorded(&p.effective_config_dir(&ctx.home), csid))
    };
    let wanted_profile = match a.profile.as_deref() {
        Some(q) => Some(resolve_ref(&profiles, q, |p| &p.id, |p| &p.name, "profile")?.id.clone()),
        None => None,
    };
    let all = session::list(conn, &Filter { kind: Some(Kind::Claude), include_archived: true, limit: Some(1000), ..Default::default() })?;
    let prev: Session = if which == "last" {
        let dir = current_dir(a)?;
        all.into_iter()
            .filter(|s| !s.state.is_live() && paths::same_path(&s.cwd, &dir) && recorded(s))
            .find(|s| wanted_profile.is_none() || s.profile_id == wanted_profile)
            .ok_or_else(|| CoreError::not_found(format!("no resumable Claude session in {}", dir.display())))?
    } else {
        resolve_ref(&all, which, |s| &s.id, |s| &s.title, "session")?.clone()
    };
    if prev.state.is_live() {
        return Err(CoreError::invalid("that session is still running"));
    }
    if !recorded(&prev) {
        return Err(CoreError::invalid("Claude has no recorded conversation for that session"));
    }
    let profile = prev
        .profile_id
        .as_ref()
        .and_then(|id| profiles.iter().find(|p| &p.id == id))
        .cloned()
        .ok_or_else(|| CoreError::invalid("the session's profile was removed"))?;
    if wanted_profile.as_ref().is_some_and(|w| w != &profile.id) {
        return Err(CoreError::invalid(format!("that conversation belongs to profile '{}'", profile.name)));
    }
    if !prev.cwd.is_dir() {
        return Err(CoreError::invalid(format!("{} no longer exists", prev.cwd.display())));
    }
    Ok(Plan {
        work_profile: prev.work_profile.as_deref().and_then(workprofile::get),
        effort: prev.effort.clone(),
        model: prev.model.clone(),
        claude_session_id: prev.claude_session_id.clone().unwrap_or_default(),
        cwd: prev.cwd.clone(),
        workspace_id: prev.workspace_id.clone(),
        project_id: prev.project_id.clone(),
        title: prev.title.clone(),
        mode: Mode::Resume,
        resumed_from: Some(prev.id),
        profile,
    })
}

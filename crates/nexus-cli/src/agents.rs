//! `nexus agents` (AI agents beyond Claude Code, ADR-0052), `nexus agent-notify` (Codex's turn notifications) and the
//! non-Claude path of `nexus launch --agent <agent>`. Launching uses `nexus_core::agent::launch::prepare` — the same
//! builder as the desktop — and this process is a `cli` session host (ADR-0014).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Subcommand;

use nexus_core::agent::profile::{self, AgentProfile, AgentProfileInput};
use nexus_core::agent::{self, Kind as Agent};
use nexus_core::claude::mcp;
use nexus_core::db;
use nexus_core::host::{self, HostKind};
use nexus_core::paths;
use nexus_core::session::{self, Filter, Kind, NewSession, Session};
use nexus_core::{CoreError, Result, env, git, new_id, project, resolve_ref, workprofile, workspace};

use crate::launch::{LaunchArgs, current_dir_of, find_task, project_containing, spawn_heartbeat};
use crate::{Ctx, out};

#[derive(Subcommand)]
pub enum AgentCmd {
    /// The agents NEXUS can start, whether they are installed, and what their sessions get from NEXUS.
    List,
    /// Agents installed on this computer and whether their default home is registered.
    Detect,
    /// Register an agent profile: the agent's default home, or an existing folder (CODEX_HOME). Nothing is written into it.
    Add {
        /// codex
        agent: String,
        #[arg(long)]
        name: Option<String>,
        /// An existing home folder (default: the agent's own, ~/.codex).
        #[arg(long, value_name = "DIR")]
        home: Option<String>,
        #[arg(long)]
        executable: Option<String>,
        #[arg(long = "work-profile")]
        work_profile: Option<String>,
        #[arg(long)]
        model: Option<String>,
        /// Do not let the agent report finished turns to NEXUS (keeps your own Codex `notify` program).
        #[arg(long)]
        no_turn_notifications: bool,
        /// Keep the agent's API credentials from the environment (OPENAI_API_KEY …).
        #[arg(long)]
        inherit_provider_env: bool,
        /// Extra options for every session (allowlisted), e.g. --arg=--search.
        #[arg(long = "arg", value_name = "OPTION", allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Forget an agent profile (its home folder is not touched).
    Remove { profile: String },
}

pub fn run(ctx: &Ctx, cmd: AgentCmd) -> Result<i32> {
    let conn = ctx.db()?;
    match cmd {
        AgentCmd::List => {
            let agents: Vec<agent::Info> = Agent::ALL.iter().map(|k| agent::info(*k, true)).collect();
            let profiles = profile::list(&conn, false)?;
            if ctx.json {
                out::json(&serde_json::json!({ "agents": agents, "profiles": profiles }));
                return Ok(0);
            }
            for a in &agents {
                let installed = match &a.installed {
                    Some(p) => format!("{} ({})", p.version.as_deref().unwrap_or("?"), p.path.display()),
                    None => format!("not installed — {}", a.install_url),
                };
                println!("{} — {installed}", a.name);
                for c in &a.capabilities {
                    let mark = match c.support {
                        agent::Support::Full => "full",
                        agent::Support::Partial => "partial",
                        agent::Support::Unavailable => "—",
                    };
                    println!("  {mark:<8} {}: {}", c.label, c.detail);
                }
                for p in profiles.iter().filter(|p| p.agent == a.id) {
                    println!("  profile {} — home {}", p.name, p.effective_home(&ctx.home).display());
                }
            }
            println!("Claude Code profiles: `nexus profiles list`.");
            Ok(0)
        }
        AgentCmd::Detect => {
            let d = profile::detect(&conn, &ctx.home)?;
            if ctx.json {
                out::json(&d);
            } else {
                for x in &d {
                    println!(
                        "{}: {} · default home {} ({}){}",
                        x.agent.name(),
                        x.executable.as_ref().map(|e| e.display().to_string()).unwrap_or_else(|| "not on PATH".into()),
                        x.default_home.display(),
                        if x.default_home_exists { "exists" } else { "not created yet" },
                        if x.registered_profile_id.is_some() { " · registered" } else { "" }
                    );
                }
            }
            Ok(0)
        }
        AgentCmd::Add { agent, name, home, executable, work_profile, model, no_turn_notifications, inherit_provider_env, args } => {
            let kind = parse_agent(&agent)?;
            let mut input = AgentProfileInput::default_for(kind);
            if let Some(n) = name {
                input.name = n;
            }
            input.home_dir = home;
            input.executable = executable;
            input.default_work_profile = work_profile;
            input.default_model = model;
            input.turn_notifications = !no_turn_notifications;
            input.inherit_provider_env = inherit_provider_env;
            input.extra_args = args;
            let p = profile::register(&conn, &input, &ctx.home)?;
            if ctx.json {
                out::json(&p);
            } else {
                println!("added {} profile {} (home {})", kind.short(), p.name, p.effective_home(&ctx.home).display());
            }
            Ok(0)
        }
        AgentCmd::Remove { profile: q } => {
            let all = profile::list(&conn, true)?;
            let p = resolve_ref(&all, &q, |p| &p.id, |p| &p.name, "agent profile")?.clone();
            profile::forget(&conn, &p.id)?;
            println!("forgot {} profile {} (its home folder was not touched)", p.agent.short(), p.name);
            Ok(0)
        }
    }
}

pub fn parse_agent(s: &str) -> Result<Agent> {
    Agent::parse(&s.to_ascii_lowercase()).ok_or_else(|| CoreError::invalid(format!("unknown agent '{s}' ({})", Agent::ALL.iter().map(|k| k.id()).collect::<Vec<_>>().join(", "))))
}

/// `nexus agent-notify --agent codex --session <id> <json>`: Codex's notify program. Never disturbs Codex: always exit
/// 0, no output, the database opened without migrations, payload bounded.
pub fn notify(ctx: &Ctx, agent: &str, session: &str, payload: Option<&str>) -> i32 {
    let run = || -> std::result::Result<(), String> {
        if agent != Agent::Codex.id() {
            return Ok(());
        }
        let Some(payload) = payload.filter(|p| p.len() <= 1024 * 1024) else { return Ok(()) };
        let Some(conn) = db::open_existing(&ctx.data_dir.join(db::DB_FILE)).map_err(|e| e.to_string())? else { return Ok(()) };
        nexus_core::codex::notify::handle(&conn, session, payload).map(|_| ()).map_err(|e| e.to_string())
    };
    if let Err(e) = run() {
        eprintln!("nexus agent-notify: {e}");
    }
    0
}

fn find_profile(conn: &db::Connection, kind: Agent, q: Option<&str>) -> Result<AgentProfile> {
    let list: Vec<AgentProfile> = profile::list(conn, false)?.into_iter().filter(|p| p.agent == kind).collect();
    match q {
        Some(q) => resolve_ref(&list, q, |p| &p.id, |p| &p.name, "profile").cloned(),
        None => match list.as_slice() {
            [only] => Ok(only.clone()),
            [] => Err(CoreError::invalid(format!("no {} profile is registered — `nexus agents add {}`", kind.short(), kind.id()))),
            _ => Err(CoreError::invalid(format!("choose a profile with --profile ({})", list.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")))),
        },
    }
}

/// `nexus launch --agent <agent>` for agents other than Claude Code.
pub fn launch(ctx: &Ctx, a: &LaunchArgs, kind: Agent) -> Result<i32> {
    let conn = ctx.db()?;
    if a.effort.is_some() {
        return Err(CoreError::invalid("--effort is a Claude Code option"));
    }
    let (profile, cwd, workspace_id, project_id, title, task_id, resume, prev, wp_id, model) = match a.resume.as_deref() {
        Some(which) => {
            let prev = resumable(&conn, kind, a, which)?;
            let p = prev.agent_profile_id.as_deref().map(|id| profile::get(&conn, id)).transpose()?.ok_or_else(|| CoreError::invalid("the session's profile was removed"))?;
            let thread = prev.claude_session_id.clone();
            (p, prev.cwd.clone(), prev.workspace_id.clone(), prev.project_id.clone(), prev.title.clone(), prev.task_id.clone(), thread, Some(prev.id.clone()), prev.work_profile.clone(), prev.model.clone())
        }
        None => {
            let p = find_profile(&conn, kind, a.profile.as_deref())?;
            let ws = workspace::active(&conn)?;
            let projects = match &ws {
                Some(w) => project::list(&conn, &w.id)?,
                None => Vec::new(),
            };
            let task = a.task.as_deref().map(|q| find_task(&conn, &ctx.data_dir, ws.as_ref(), q)).transpose()?;
            let pq = a.project.clone().or_else(|| task.as_ref().and_then(|t| t.project_id.clone()));
            let (cwd, proj) = match pq.as_deref() {
                Some(q) => {
                    let pr = resolve_ref(&projects, q, |p| &p.id, |p| &p.name, "project")?.clone();
                    let lp = pr.local_path.clone().filter(|l| l.is_dir()).ok_or_else(|| CoreError::invalid(format!("project '{}' is not on this machine", pr.name)))?;
                    (lp, Some(pr))
                }
                None => {
                    let dir = current_dir_of(a.cwd.as_deref())?;
                    let pr = project_containing(&projects, &dir);
                    (dir, pr)
                }
            };
            let title = a
                .name
                .as_deref()
                .map(nexus_core::claude::launch::sanitize_title)
                .filter(|t| !t.is_empty())
                .or_else(|| task.as_ref().map(|t| nexus_core::claude::launch::sanitize_title(&t.title)))
                .or_else(|| proj.as_ref().map(|p| p.name.clone()))
                .unwrap_or_else(|| cwd.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "session".into()));
            let wp = a.work_profile.clone().or_else(|| p.default_work_profile.clone());
            let model = a.model.clone().or_else(|| p.default_model.clone());
            let ws_id = proj.as_ref().map(|x| x.workspace_id.clone()).or(ws.map(|w| w.id));
            (p, cwd, ws_id, proj.map(|x| x.id), title, task.and_then(|t| t.id), None, None, wp, model)
        }
    };
    let work_profile = match wp_id.as_deref() {
        Some(id) => Some(workprofile::get(id).ok_or_else(|| CoreError::invalid(format!("unknown work profile '{id}' (prototype, balanced, professional, critical)")))?),
        None => None,
    };
    let me = host::register(&conn, HostKind::Cli)?;
    let nexus_id = new_id();
    let exe_self = mcp::nexus_next_to_current_exe();
    let base_env = env::current();
    let prepared = agent::launch::prepare(&agent::launch::Request {
        conn: &conn,
        data_dir: &ctx.data_dir,
        profile: &profile,
        nexus_exe: exe_self.as_deref(),
        nexus_session_id: &nexus_id,
        workspace_id: workspace_id.as_deref(),
        project_id: project_id.as_deref(),
        task_id: task_id.as_deref(),
        cwd: &cwd,
        start_task: resume.is_none(),
        work_profile,
        model: model.as_deref(),
        resume: resume.as_deref(),
        kickoff: false,
        base_env: &base_env,
    })?;
    let s = session::insert_starting(
        &conn,
        NewSession {
            id: Some(nexus_id),
            kind: Some(Kind::Claude),
            agent: Some(kind),
            agent_profile_id: Some(profile.id.clone()),
            host: Some(HostKind::Cli),
            host_id: Some(me.id.clone()),
            workspace_id,
            project_id: project_id.clone(),
            claude_session_id: resume.clone(),
            title,
            branch: git::status(&cwd).ok().and_then(|s| s.branch),
            cwd,
            work_profile: work_profile.map(|w| w.id.to_string()),
            model,
            program: prepared.executable.display().to_string(),
            resumed_from: prev,
            task_id: prepared.integrations.task.as_ref().map(|t| t.id.clone()).or(task_id),
            ..Default::default()
        },
    )?;
    session::set_integrations(&conn, &s.id, &prepared.integrations)?;
    profile::touch_used(&conn, &profile.id)?;
    if let Some(pid) = &project_id {
        project::touch_opened(&conn, pid)?;
    }
    if !a.quiet {
        let state = |label: &str, st: &mcp::IntegrationStatus| match st {
            mcp::IntegrationStatus::Enabled { .. } => format!("{label} on"),
            mcp::IntegrationStatus::Blocked { reason } | mcp::IntegrationStatus::Unavailable { reason } => format!("{label} off ({reason})"),
        };
        eprintln!(
            "nexus: {} {} · profile {} · {} · {} · session {}",
            if resume.is_some() { "resuming" } else { "launching" },
            kind.short(),
            profile.name,
            s.cwd.display(),
            state("NEXUS tools", &prepared.integrations.mcp),
            &s.id[..8]
        );
    }
    let _ = ctrlc::set_handler(|| {});
    let spec = &prepared.spec;
    let mut cmd = std::process::Command::new(&spec.program);
    cmd.args(&spec.args).current_dir(&spec.cwd);
    env::apply(&mut cmd, &spec.env);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("cannot start {}: {e}", spec.program.display());
            session::mark_failed(&conn, &s.id, &msg)?;
            host::shutdown(&conn, &me.id)?;
            return Err(CoreError::External { program: kind.id().into(), message: msg });
        }
    };
    session::mark_running(&conn, &s.id, Some(child.id()))?;
    let stop = Arc::new(AtomicBool::new(false));
    let beat = spawn_heartbeat(ctx.data_dir.join(db::DB_FILE), me.id.clone(), s.id.clone(), stop.clone());
    let status = child.wait();
    stop.store(true, Ordering::SeqCst);
    let _ = beat.join();
    let code = status.ok().and_then(|st| st.code()).map(i64::from);
    nexus_core::notify::session_exited(&conn, &s.id, code, false)?;
    if let Err(e) = nexus_core::devrecord::finalize(&conn, &ctx.data_dir, &s.id) {
        eprintln!("nexus: the development record was not written: {e}");
    }
    let _ = nexus_core::workspace_sync::policy::session_ended(&conn, &s.id);
    host::shutdown(&conn, &me.id)?;
    Ok(code.map(|c| c as i32).unwrap_or(1))
}

/// A finished session of this agent whose conversation NEXUS recorded (`last` = the most recent one here).
fn resumable(conn: &db::Connection, kind: Agent, a: &LaunchArgs, which: &str) -> Result<Session> {
    let all: Vec<Session> = session::list(conn, &Filter { kind: Some(Kind::Claude), include_archived: true, limit: Some(1000), ..Default::default() })?
        .into_iter()
        .filter(|s| s.agent == Some(kind))
        .collect();
    let prev = if which == "last" {
        let dir = current_dir_of(a.cwd.as_deref())?;
        all.into_iter()
            .find(|s| !s.state.is_live() && paths::same_path(&s.cwd, &dir) && s.claude_session_id.is_some())
            .ok_or_else(|| CoreError::not_found(format!("no resumable {} session in {}", kind.short(), dir.display())))?
    } else {
        resolve_ref(&all, which, |s| &s.id, |s| &s.title, "session")?.clone()
    };
    if prev.state.is_live() {
        return Err(CoreError::invalid("that session is still running"));
    }
    if prev.claude_session_id.is_none() {
        return Err(CoreError::invalid(format!("{} has not reported a conversation for that session (no finished turn)", kind.short())));
    }
    if !prev.cwd.is_dir() {
        return Err(CoreError::invalid(format!("{} no longer exists", prev.cwd.display())));
    }
    Ok(prev)
}

//! `nexus` — the NEXUS command-line interface. Every command is a thin layer over `nexus-core`; nothing here
//! re-implements behaviour the desktop app has. Works without the desktop app running.

mod backup;
mod constitution;
mod context;
mod diagnose;
mod hook;
mod instructions;
mod kits;
mod knowledge;
mod launch;
mod mcp;
mod notifications;
mod out;
mod sync;
mod templates;
mod usage;
mod workflow;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use serde::Serialize;

use nexus_core::claude::policy::MachinePolicyLocations;
use nexus_core::claude::profile::{self, ProfileInput};
use nexus_core::claude::{auth, install};
use nexus_core::db::{self, Connection};
use nexus_core::doctor::{self, Status};
use nexus_core::session::{self, Filter};
use nexus_core::{CoreError, git, host, now_ms, paths, project, resolve_ref, workspace};

#[derive(Parser)]
#[command(name = "nexus", version, about = "NEXUS — AI-first development environment for Claude Code")]
struct Cli {
    /// Machine-readable JSON output.
    #[arg(long, global = true)]
    json: bool,
    /// NEXUS data directory (default: platform local data dir, or NEXUS_DATA_DIR).
    #[arg(long, global = true, value_name = "DIR")]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check NEXUS, Git, Claude installations, profiles and the active workspace.
    Doctor,
    /// Claude Code setup: is it installed and startable, which version, and the official ways to install it here.
    /// NEXUS never installs it for you.
    Setup,
    /// Workspaces (portable folders with a nexus-workspace.json manifest).
    #[command(subcommand)]
    Workspaces(WorkspaceCmd),
    /// Projects of the active workspace.
    #[command(subcommand)]
    Projects(ProjectCmd),
    /// Claude profiles (separate Claude Code configurations).
    #[command(subcommand)]
    Profiles(ProfileCmd),
    /// Sessions NEXUS started (desktop and `nexus launch`).
    Sessions(SessionsArgs),
    /// Portable workspace knowledge: rules, decisions, notes, specs, workflows (Markdown files, indexed locally).
    #[command(subcommand)]
    Knowledge(knowledge::KnowledgeCmd),
    /// Tasks of the active workspace (Markdown files under tasks/).
    #[command(subcommand)]
    Tasks(knowledge::TasksCmd),
    /// AI Kits: Claude Code plugins or skill folders, enabled per profile for NEXUS sessions.
    #[command(subcommand)]
    Kits(kits::KitCmd),
    /// Workflows: definitions, templates, runs (AI-reported vs verified), reviews.
    #[command(subcommand)]
    Workflow(workflow::WorkflowCmd),
    /// Quality gates: trusted, deterministic checks per project (program + arguments, no shell).
    #[command(subcommand)]
    Gates(workflow::GatesCmd),
    /// Context Manager: a project's context strategy, onboarding analysis and the Context Inspector.
    #[command(subcommand)]
    Context(context::ContextCmd),
    /// Token usage from Claude Code's local session transcripts: measured, estimated, unavailable (ADR-0030).
    Usage(usage::UsageArgs),
    /// Workspace synchronisation through Git: status, fetch, fast-forward pull, reviewed commit and push, clone (ADR-0031).
    #[command(subcommand)]
    Sync(sync::SyncCmd),
    /// Backups of NEXUS's machine-local state (database, global knowledge) and a safe restore (ADR-0032).
    #[command(subcommand)]
    Backup(backup::BackupCmd),
    /// Workspace starter templates: Minimal, Developer, AI Engineering Pro (ADR-0037).
    #[command(subcommand)]
    Templates(templates::TemplateCmd),
    /// Constitutions: structured rules (articles, sections, rules with ids and enforcement) by scope.
    #[command(subcommand)]
    Constitution(constitution::ConstitutionCmd),
    /// Notifications from sessions, tasks and reviews (the desktop's notification center).
    #[command(subcommand)]
    Notifications(notifications::NotificationsCmd),
    /// Show the instruction files Claude loads for a profile and project (CLAUDE.md layers, rules, imports) and what
    /// NEXUS appends. Read-only.
    Instructions(instructions::InstructionsArgs),
    /// Start Claude Code in this terminal with a profile's own configuration.
    Launch(launch::LaunchArgs),
    /// NEXUS MCP server over stdio, bound to one NEXUS session (started by Claude from the session's --mcp-config).
    Mcp {
        #[arg(long)]
        session: String,
    },
    /// Lifecycle hook handler (started by Claude from the session's --settings). Always exits 0.
    #[command(hide = true)]
    Hook {
        #[arg(long)]
        session: String,
        event: String,
    },
}

#[derive(Subcommand)]
enum WorkspaceCmd {
    /// List workspaces registered on this machine.
    List,
    /// Create a workspace in FOLDER (created if missing) and make it active.
    Create {
        folder: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        description: String,
        /// Starter template: minimal, developer or ai-engineering-pro (`nexus templates list`).
        #[arg(long)]
        template: Option<String>,
    },
    /// Register an existing workspace folder (e.g. cloned on a new machine) and make it active.
    Open { folder: String },
    /// Make WORKSPACE (name or id) the active workspace.
    Use { workspace: String },
    /// Would syncing the active workspace folder through Git be safe? Read-only: no fetch, commit or push (ADR-0031).
    /// Exit code 1 when something blocks syncing.
    SyncStatus,
    /// Schema, compatibility, template, features and layout of the active workspace (ADR-0036).
    Info,
    /// Migrate the active workspace to the current schema. Shows the plan; changes nothing without --yes. The files it
    /// changes are backed up to the NEXUS data folder first and put back if a step fails.
    Migrate {
        #[arg(long)]
        yes: bool,
    },
    /// Workspace Doctor: health of the active workspace with recommendations. `--repair ID` shows what that repair
    /// changes; add --yes to apply it. Exit code 1 when something is an error.
    Doctor {
        #[arg(long, value_name = "ID")]
        repair: Option<String>,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum ProjectCmd {
    /// List projects of the active workspace.
    List {
        /// Include live Git status (branch, changes).
        #[arg(long)]
        git: bool,
    },
    /// Register FOLDER as a project of the active workspace (read-only inspection, nothing is executed).
    Add {
        folder: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// Bind a project that is not on this machine to FOLDER.
    Locate { project: String, folder: String },
    /// Repositories in the workspace's clone folder (registered, belonging to a missing project, or new) and projects
    /// missing on this computer. Read-only.
    Scan,
    /// Clone a missing project from its recorded remote into the clone folder (or --to, relative to the workspace).
    Clone {
        project: String,
        #[arg(long, value_name = "FOLDER")]
        to: Option<String>,
    },
    /// Create or refresh a project's portable context (project-context/<project>/): the overview and architecture
    /// you wrote are kept; project.json, environment.json and current-state.md are regenerated.
    Context { project: String },
    /// A project's development history: records (verified / observed / reported, with their sync state), sessions,
    /// tasks, decisions, recent commits, gate results and blockers.
    History { project: String },
    /// A project's environment requirements (runtimes, package managers, containers, environment variable names) from
    /// its files; --check compares them with this computer (tools are only asked for their version).
    Env {
        project: String,
        #[arg(long)]
        check: bool,
    },
    /// Unregister a project (its folder and repository are not touched).
    Remove { project: String },
    /// Where a project stands: verified Git state, saved history (tasks, notes, checkpoints, sessions) and the next
    /// recommended action with the command to run it.
    Resume {
        /// Project (default: the one containing the working directory).
        project: Option<String>,
    },
}

#[derive(Subcommand)]
enum ProfileCmd {
    /// List registered Claude profiles.
    List,
    /// Show Claude configurations found on this machine (read-only).
    Detect,
    /// Register a Claude configuration (read-only), or create a new empty one with --create.
    Add(ProfileAddArgs),
    /// Show the sign-in status of a profile (`claude auth status` with the profile's environment).
    Status { profile: String },
    /// What the profile's configuration contains: skills, agents, MCP servers, hooks, plugins, kits, settings, policy.
    Diagnose { profile: String },
    /// Keep the default profile's personal instructions (~/.claude/CLAUDE.md, ~/.claude/rules) out of this profile's
    /// NEXUS sessions (session-scoped claudeMdExcludes; organisation-managed instructions are never affected).
    Isolate {
        profile: String,
        /// Turn isolation off again.
        #[arg(long)]
        off: bool,
    },
}

#[derive(Args)]
struct ProfileAddArgs {
    #[arg(long)]
    name: String,
    /// Configuration folder. Omit for the default location (~/.claude, launched without CLAUDE_CONFIG_DIR).
    #[arg(long, value_name = "DIR")]
    config_dir: Option<String>,
    /// Create an empty NEXUS-managed folder instead of registering an existing one.
    #[arg(long)]
    create: bool,
    #[arg(long, value_name = "FILE")]
    executable: Option<String>,
    #[arg(long, default_value = "")]
    description: String,
    #[arg(long = "work-profile")]
    work_profile: Option<String>,
    #[arg(long)]
    effort: Option<String>,
    #[arg(long)]
    model: Option<String>,
    /// Keep the default profile's personal instructions out of this profile's NEXUS sessions.
    #[arg(long)]
    isolate_instructions: bool,
}

#[derive(Args)]
struct SessionsArgs {
    /// Include archived sessions and other workspaces.
    #[arg(long)]
    all: bool,
    #[arg(long, default_value_t = 20)]
    limit: u32,
}

/// Everything a command needs; the database is opened lazily per command.
pub struct Ctx {
    pub json: bool,
    pub data_dir: PathBuf,
    pub home: PathBuf,
}

impl Ctx {
    /// Open the database (recovering an unreadable one) and reconcile sessions of dead hosts (ADR-0014).
    pub fn db(&self) -> nexus_core::Result<Connection> {
        let path = self.data_dir.join(db::DB_FILE);
        let (conn, recovery) = db::open_or_recover(&path)?;
        if let Some(r) = recovery {
            eprintln!(
                "warning: the NEXUS database could not be read and was kept as {}; a fresh one was created",
                r.quarantined_to.display()
            );
        }
        host::reconcile(&conn, now_ms())?;
        nexus_core::workflow::run::reconcile(&conn, now_ms())?;
        nexus_core::workspace_sync::history::reconcile(&conn, now_ms())?;
        Ok(conn)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let ctx = match context(&cli) {
        Ok(c) => c,
        Err(e) => return fail(&e),
    };
    let result = match cli.command {
        Command::Doctor => cmd_doctor(&ctx),
        Command::Setup => cmd_setup(&ctx),
        Command::Workspaces(c) => cmd_workspaces(&ctx, c).map(|_| 0),
        Command::Projects(c) => cmd_projects(&ctx, c).map(|_| 0),
        Command::Profiles(c) => cmd_profiles(&ctx, c).map(|_| 0),
        Command::Sessions(a) => cmd_sessions(&ctx, a).map(|_| 0),
        Command::Knowledge(c) => knowledge::cmd_knowledge(&ctx, c).map(|_| 0),
        Command::Tasks(c) => knowledge::cmd_tasks(&ctx, c).map(|_| 0),
        Command::Kits(c) => kits::run(&ctx, c).map(|_| 0),
        Command::Instructions(a) => instructions::run(&ctx, a).map(|_| 0),
        Command::Notifications(c) => notifications::run(&ctx, c).map(|_| 0),
        Command::Constitution(c) => constitution::run(&ctx, c),
        Command::Context(c) => context::run(&ctx, c).map(|_| 0),
        Command::Usage(a) => usage::run(&ctx, a),
        Command::Sync(c) => sync::run(&ctx, c),
        Command::Backup(c) => backup::run(&ctx, c),
        Command::Templates(c) => templates::run(&ctx, c),
        Command::Workflow(c) => workflow::run_workflow(&ctx, c),
        Command::Gates(c) => workflow::run_gates(&ctx, c),
        Command::Launch(a) => launch::run(&ctx, a),
        Command::Mcp { session } => mcp::run(&ctx, &session),
        Command::Hook { session, event } => Ok(hook::run(&ctx, &session, &event)),
    };
    match result {
        Ok(code) => ExitCode::from(u8::try_from(code.clamp(0, 255)).unwrap_or(1)),
        Err(e) => fail(&e),
    }
}

fn context(cli: &Cli) -> nexus_core::Result<Ctx> {
    // Absolute: the MCP server is started by Claude from the project directory with this path in its arguments.
    let data_dir = match &cli.data_dir {
        Some(d) => std::path::absolute(d).map_err(|e| CoreError::io("cannot resolve", d, e))?,
        None => paths::data_dir()?,
    };
    Ok(Ctx {
        json: cli.json,
        data_dir,
        home: paths::home_dir()?,
    })
}

fn fail(e: &CoreError) -> ExitCode {
    eprintln!("error: {e}");
    ExitCode::from(1)
}

fn cmd_setup(ctx: &Ctx) -> nexus_core::Result<i32> {
    let conn = ctx.db()?;
    let s = nexus_core::claude::setup::status(&conn, &ctx.home, true)?;
    if ctx.json {
        out::json(&s);
        return Ok(if s.ready { 0 } else { 1 });
    }
    match (&s.version, s.ready) {
        (Some(v), true) => println!("Claude Code {v} is installed and NEXUS can start it."),
        (_, true) => println!("Claude Code is installed and NEXUS can start it."),
        _ => println!("Claude Code is not installed (or cannot be started)."),
    }
    println!("{} Claude profile(s) registered.", s.profiles);
    if !s.ready {
        println!("Official installation methods (verified {}, {}):", s.verified, s.docs_url);
        for m in &s.methods {
            println!("  {}{} [{}]: {}", m.title, if m.recommended { " (recommended)" } else { "" }, m.shell, m.command);
            println!("      {}", m.notes);
        }
    }
    for n in &s.next {
        println!("→ {n}");
    }
    Ok(if s.ready { 0 } else { 1 })
}

pub(crate) fn active_workspace(conn: &Connection) -> nexus_core::Result<workspace::Workspace> {
    workspace::active(conn)?.ok_or_else(|| {
        CoreError::invalid("no active workspace — create one with `nexus workspaces create <folder> --name <name>`")
    })
}

fn cmd_doctor(ctx: &Ctx) -> nexus_core::Result<i32> {
    let conn = ctx.db()?;
    let machine = MachinePolicyLocations::system();
    let db_path = ctx.data_dir.join(db::DB_FILE);
    let checks = doctor::run(
        &conn,
        &doctor::Context {
            db_path: &db_path,
            home: &ctx.home,
            machine_policy: &machine,
            probe_installations: true,
        },
    )?;
    let worst = doctor::worst(&checks);
    if ctx.json {
        out::json(&checks);
    } else {
        for c in &checks {
            let mark = match c.status {
                Status::Ok => "ok  ",
                Status::Info => "info",
                Status::Warn => "warn",
                Status::Error => "FAIL",
            };
            println!("[{mark}] {}", c.summary);
            if let Some(d) = &c.detail {
                println!("       {d}");
            }
            if let Some(f) = &c.fix {
                println!("       fix: {f}");
            }
        }
    }
    Ok(if worst == Status::Error { 1 } else { 0 })
}

fn cmd_workspaces(ctx: &Ctx, cmd: WorkspaceCmd) -> nexus_core::Result<()> {
    let conn = ctx.db()?;
    let print_one = |ws: &workspace::Workspace| {
        if ctx.json {
            out::json(ws);
        } else {
            println!("active workspace: {} ({})", ws.name, ws.root_path.display());
        }
    };
    match cmd {
        WorkspaceCmd::List => {
            let list = workspace::list(&conn)?;
            let active = workspace::active(&conn)?.map(|w| w.id);
            if ctx.json {
                out::json(&list);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = list
                .iter()
                .map(|w| {
                    vec![
                        if Some(&w.id) == active.as_ref() { "*".into() } else { String::new() },
                        w.name.clone(),
                        w.project_count.to_string(),
                        if w.available { w.root_path.display().to_string() } else { format!("{} (missing)", w.root_path.display()) },
                    ]
                })
                .collect();
            out::table(&["", "NAME", "PROJECTS", "FOLDER"], &rows);
        }
        WorkspaceCmd::Create { folder, name, description, template } => {
            if let Some(t) = &template {
                nexus_core::templates::info(t)?; // unknown templates fail before anything is created
            }
            let ws = workspace::create(&conn, &folder, &name, &description)?;
            if let Some(t) = &template {
                nexus_core::templates::apply(&conn, &ctx.data_dir, &ws, t)?;
            }
            print_one(&workspace::activate(&conn, &ws.id)?);
        }
        WorkspaceCmd::Open { folder } => {
            let ws = workspace::import(&conn, &folder)?;
            print_one(&workspace::activate(&conn, &ws.id)?);
        }
        WorkspaceCmd::Use { workspace: query } => {
            let list = workspace::list(&conn)?;
            let ws = resolve_ref(&list, &query, |w| &w.id, |w| &w.name, "workspace")?;
            print_one(&workspace::activate(&conn, &ws.id)?);
        }
        WorkspaceCmd::SyncStatus => {
            let ws = active_workspace(&conn)?;
            let r = nexus_core::workspace_sync::readiness(&conn, &ws, &ctx.data_dir)?;
            if ctx.json {
                out::json(&r);
            } else {
                println!("{} — {}", ws.name, if r.ready { "ready to sync through Git" } else { "not ready to sync" });
                if r.repo {
                    println!(
                        "  branch {} · upstream {} · {} ahead, {} behind (as of the last fetch) · remote {}",
                        r.branch.as_deref().unwrap_or("–"),
                        r.upstream.as_deref().unwrap_or("none"),
                        r.ahead,
                        r.behind,
                        r.remote.as_deref().unwrap_or("none")
                    );
                    println!("  {} portable file(s) changed{}", r.changed.len(), if r.changed.is_empty() { String::new() } else { format!(": {}", r.changed.iter().take(8).cloned().collect::<Vec<_>>().join(", ")) });
                }
                for s in &r.sensitive {
                    println!("  {}: {} ({})", if s.tracked { "COMMITTED" } else { "not ignored" }, s.path, s.reason);
                }
                for i in &r.issues {
                    println!("  {}: {}", i.severity, i.message);
                }
            }
            std::process::exit(if r.ready { 0 } else { 1 });
        }
        WorkspaceCmd::Info => {
            let ws = active_workspace(&conn)?;
            let m = workspace::read_manifest(&ws.root_path)?;
            let c = workspace::compat(&m);
            if ctx.json {
                out::json(&serde_json::json!({
                    "workspace": ws, "schemaVersion": m.schema_version, "createdWith": m.created_with, "migratedWith": m.migrated_with,
                    "minNexusVersion": m.min_nexus_version, "template": m.template, "features": m.features,
                    "projectsDir": m.projects_dir(), "compat": c, "nexusVersion": workspace::NEXUS_VERSION,
                }));
                return Ok(());
            }
            println!("{} ({})", ws.name, ws.root_path.display());
            println!("  schema {} · {}", m.schema_version, if c.writable { "writable" } else { "READ-ONLY" });
            if let Some(r) = &c.reason {
                println!("  {r}");
            }
            if c.needs_migration {
                println!("  an older schema — `nexus workspaces migrate` shows what moving to schema {} changes", workspace::MANIFEST_SCHEMA);
            }
            println!(
                "  created with NEXUS {} · minimum NEXUS {} · this NEXUS {}",
                m.created_with.as_deref().unwrap_or("(unknown)"),
                m.min_nexus_version.as_deref().unwrap_or("(none)"),
                workspace::NEXUS_VERSION
            );
            match &m.template {
                Some(t) => println!("  template {} {}", t.id, t.version),
                None => println!("  no starter template"),
            }
            println!("  features: {}", if m.features.is_empty() { "(none)".to_string() } else { m.features.join(", ") });
            println!("  project clones: {}/ · {} project(s)", m.projects_dir(), m.projects.len());
        }
        WorkspaceCmd::Doctor { repair, yes } => {
            use nexus_core::workspace_doctor as wd;
            let ws = active_workspace(&conn)?;
            if let Some(id) = repair {
                let plan = wd::repair_plan(&conn, &ctx.data_dir, &ws, &id)?;
                if !yes {
                    if ctx.json {
                        out::json(&serde_json::json!({ "plan": plan, "applied": false }));
                    } else {
                        println!("{}:", plan.title);
                        for c in &plan.changes {
                            println!("  {c}");
                        }
                        for n in &plan.notes {
                            println!("  · {n}");
                        }
                        println!("Repeat with --yes to apply.");
                    }
                    std::process::exit(1);
                }
                let done = wd::repair(&conn, &ctx.data_dir, &ws, &id)?;
                if ctx.json { out::json(&done) } else { println!("{}", done.message) }
                return Ok(());
            }
            let r = wd::run(&conn, &ctx.data_dir, &ws, &wd::Options { home: &ctx.home, probe_tools: true })?;
            if ctx.json {
                out::json(&r);
            } else {
                println!("{} — {} error(s), {} warning(s), {} note(s), {} ok", ws.name, r.counts.error, r.counts.warn, r.counts.info, r.counts.ok);
                for f in &r.findings {
                    let mark = match f.status {
                        Status::Ok => "ok   ",
                        Status::Info => "info ",
                        Status::Warn => "WARN ",
                        Status::Error => "ERROR",
                    };
                    println!("{mark} [{}] {}", f.area, f.summary);
                    for d in &f.detail {
                        println!("        {d}");
                    }
                    if let Some(rec) = &f.recommendation {
                        println!("        → {rec}");
                    }
                    if let Some(rep) = &f.repair {
                        println!("        repair: nexus workspaces doctor --repair {}  ({})", rep.id, rep.title);
                    }
                }
            }
            if r.counts.error > 0 {
                std::process::exit(1);
            }
        }
        WorkspaceCmd::Migrate { yes } => {
            let ws = active_workspace(&conn)?;
            let plan = workspace::migrate::plan(&ws.root_path)?;
            if plan.is_empty() {
                if ctx.json {
                    out::json(&serde_json::json!({ "plan": plan, "applied": false }));
                } else {
                    println!("{} is on workspace schema {} — nothing to migrate.", ws.name, plan.to);
                }
                return Ok(());
            }
            if !yes {
                if ctx.json {
                    out::json(&serde_json::json!({ "plan": plan, "applied": false }));
                } else {
                    println!("Migrating {} from workspace schema {} to {} would:", ws.name, plan.from, plan.to);
                    for c in &plan.changes {
                        println!("  {} {} — {}", c.action, c.path, c.description);
                    }
                    for n in &plan.notes {
                        println!("  · {n}");
                    }
                    println!("Repeat with --yes to apply. The files above are backed up to the NEXUS data folder first.");
                }
                std::process::exit(1);
            }
            let done = workspace::migrate::apply(&conn, &ctx.data_dir, &ws)?;
            if ctx.json {
                out::json(&serde_json::json!({ "plan": done.plan, "applied": true, "backupDir": done.backup_dir }));
            } else {
                println!("{} is now on workspace schema {}.", ws.name, done.plan.to);
                if let Some(b) = &done.backup_dir {
                    println!("  previous versions of the changed files: {}", b.display());
                }
                println!("  review and commit the change through the Sync Center (`nexus sync review`).");
            }
        }
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRow {
    #[serde(flatten)]
    project: project::Project,
    git: Option<git::GitStatus>,
}

fn cmd_projects(ctx: &Ctx, cmd: ProjectCmd) -> nexus_core::Result<()> {
    let conn = ctx.db()?;
    let ws = active_workspace(&conn)?;
    let find = |q: &str| -> nexus_core::Result<project::Project> {
        let list = project::list(&conn, &ws.id)?;
        resolve_ref(&list, q, |p| &p.id, |p| &p.name, "project").cloned()
    };
    match cmd {
        ProjectCmd::List { git: with_git } => {
            let rows: Vec<ProjectRow> = project::list(&conn, &ws.id)?
                .into_iter()
                .map(|p| {
                    let g = if with_git { p.local_path.as_deref().filter(|lp| lp.is_dir()).and_then(|lp| git::status(lp).ok()) } else { None };
                    ProjectRow { project: p, git: g }
                })
                .collect();
            if ctx.json {
                out::json(&rows);
                return Ok(());
            }
            let table: Vec<Vec<String>> = rows
                .iter()
                .map(|r| {
                    let p = &r.project;
                    let git_col = match &r.git {
                        Some(g) if g.is_repo => format!(
                            "{}{}",
                            g.branch.as_deref().unwrap_or("(detached)"),
                            if g.changed() > 0 { format!(" · {} changed", g.changed()) } else { " · clean".into() }
                        ),
                        Some(_) => "not a git repo".into(),
                        None => String::new(),
                    };
                    let mut row = vec![p.name.clone(), p.stack.stacks.iter().take(3).cloned().collect::<Vec<_>>().join(",")];
                    if with_git {
                        row.push(git_col);
                    }
                    row.push(p.local_path.as_ref().map(|l| l.display().to_string()).unwrap_or_else(|| "(not on this machine)".into()));
                    row
                })
                .collect();
            let headers: &[&str] = if with_git { &["NAME", "STACK", "GIT", "PATH"] } else { &["NAME", "STACK", "PATH"] };
            out::table(headers, &table);
        }
        ProjectCmd::Add { folder, name } => {
            let p = project::register_with_context(&conn, &ctx.data_dir, &ws.id, &folder, name.as_deref())?;
            if ctx.json {
                out::json(&p);
            } else {
                println!("added {} [{}] to {}", p.name, p.stack.stacks.join(", "), ws.name);
            }
        }
        ProjectCmd::Scan => {
            let s = project::discovery::scan(&conn, &ws)?;
            if ctx.json {
                out::json(&s);
                return Ok(());
            }
            println!("clone folder: {}/", s.projects_dir);
            for f in &s.found {
                let what = match f.status {
                    "registered" => "registered".to_string(),
                    "known" => "belongs to a project missing here — `nexus projects locate` it".to_string(),
                    _ => "new — review it and `nexus projects add` it".to_string(),
                };
                println!("  {} [{}] {}", f.rel, f.stacks.join(", "), what);
            }
            for m in &s.missing {
                match (&m.clone_to, &m.reason) {
                    (Some(to), _) => println!("  missing: {} — `nexus projects clone \"{}\"` → {to}", m.name, m.name),
                    (None, Some(r)) => println!("  missing: {} — {r}", m.name),
                    _ => println!("  missing: {}", m.name),
                }
            }
            if !s.external.is_empty() {
                println!("  outside the workspace: {}", s.external.join(", "));
            }
        }
        ProjectCmd::Clone { project: q, to } => {
            let p = project::discovery::clone_missing(&conn, &ctx.data_dir, &ws, &find(&q)?.id, to.as_deref())?;
            if ctx.json {
                out::json(&p);
            } else {
                println!("cloned {} into {}", p.name, p.local_path.as_ref().map(|x| x.display().to_string()).unwrap_or_default());
            }
        }
        ProjectCmd::History { project: q } => {
            let p = find(&q)?;
            let _ = nexus_core::devrecord::finalize_pending(&conn, &ctx.data_dir);
            let h = nexus_core::devrecord::history(&conn, &ctx.data_dir, &ws, &p.id)?;
            if ctx.json {
                out::json(&h);
                return Ok(());
            }
            println!("{} — development history", p.name);
            println!("records ({}):", h.records.len());
            for r in &h.records {
                println!("  {} [{}] {}", r.created.as_deref().unwrap_or(""), r.sync, r.title);
            }
            let open: Vec<String> = h.tasks.iter().filter(|t| t.2 != "done").map(|t| format!("[{}] {}", t.2, t.1)).collect();
            if !open.is_empty() {
                println!("open tasks: {}", open.join("; "));
            }
            for (b, at) in &h.blockers {
                println!("blocker ({}): {b}", out::ago(Some(*at)));
            }
            for (oid, s, _) in h.commits.iter().take(5) {
                println!("commit {} {s}", &oid[..oid.len().min(10)]);
            }
        }
        ProjectCmd::Env { project: q, check } => {
            let p = find(&q)?;
            let Some(r) = project::environment::report(&ws, &p, check) else {
                println!("{}: not on this computer and no environment record in the workspace yet", p.name);
                return Ok(());
            };
            if ctx.json {
                out::json(&r);
                return Ok(());
            }
            println!("{} — environment (from the {})", p.name, r.from);
            if r.checks.is_empty() {
                for q in &r.environment.requirements {
                    println!("  {} {}  ({})", q.tool, if q.constraint.is_empty() { "any version" } else { &q.constraint }, q.source);
                }
            } else {
                for c in &r.checks {
                    let have = c.installed.clone().unwrap_or_else(|| "–".into());
                    println!("  {:<9} {} {} — installed {have}  ({})", c.status, c.name, if c.constraint.is_empty() { "(any)" } else { &c.constraint }, c.source);
                    if let Some(cmd) = &c.install_command {
                        println!("            install: {}  (or {})", cmd.join(" "), c.install_url.clone().unwrap_or_default());
                    } else if c.status == "missing" {
                        println!("            install: {}", c.install_url.clone().unwrap_or_default());
                    }
                }
            }
            let cont = &r.environment.containers;
            if !cont.compose_services.is_empty() {
                println!("  local services ({}): {}", cont.compose_file.clone().unwrap_or_default(), cont.compose_services.join(", "));
            }
            if let Some(d) = &cont.devcontainer {
                println!("  dev container: {d}{}", cont.devcontainer_image.as_deref().map(|i| format!(" ({i})")).unwrap_or_default());
            }
            if !r.environment.env_vars.is_empty() {
                println!("  environment variables (names only): {}", r.environment.env_vars.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join(", "));
            }
            if let Some(s) = &r.environment.setup_section {
                println!("  setup instructions: README.md — “{s}”");
            }
        }
        ProjectCmd::Context { project: q } => {
            let r = project::context::init(&conn, &ctx.data_dir, &ws, &find(&q)?.id)?;
            if ctx.json {
                out::json(&r);
            } else {
                println!("{}: {} created, {} updated, {} kept", r.dir, r.created.len(), r.updated.len(), r.kept.len());
            }
        }
        ProjectCmd::Locate { project: q, folder } => {
            let p = project::locate(&conn, &find(&q)?.id, &folder)?;
            if ctx.json {
                out::json(&p);
            } else {
                println!("{} → {}", p.name, p.local_path.map(|l| l.display().to_string()).unwrap_or_default());
            }
        }
        ProjectCmd::Resume { project: q } => {
            let p = match q.as_deref() {
                Some(q) => find(q)?,
                None => {
                    let dir = std::env::current_dir().map_err(|e| CoreError::io("cannot read", ".", e))?;
                    launch::project_containing(&project::list(&conn, &ws.id)?, &dir)
                        .ok_or_else(|| CoreError::invalid("not inside a registered project — name one: `nexus projects resume <project>`"))?
                }
            };
            nexus_core::knowledge::proposals::extract_recent(&conn, &ws.id)?;
            let s = nexus_core::resume::summarize(&conn, &ctx.data_dir, &ctx.home, &p.id)?;
            if ctx.json {
                out::json(&s);
            } else {
                print_resume(&s);
            }
        }
        ProjectCmd::Remove { project: q } => {
            let p = find(&q)?;
            project::unregister(&conn, &p.id)?;
            if !ctx.json {
                println!("removed {} from {} (files untouched)", p.name, ws.name);
            }
        }
    }
    Ok(())
}

fn print_resume(s: &nexus_core::resume::ResumeSummary) {
    use nexus_core::resume::ActionKind;
    let v = &s.verified;
    println!("{} — resume project\n", s.project_name);
    println!("Verified now (Git):");
    if !v.available {
        println!("  folder not available on this machine");
    } else if !v.repo {
        println!("  not a Git repository");
    } else {
        println!(
            "  branch {}{}",
            v.branch.as_deref().unwrap_or("(detached)"),
            v.head.as_ref().map(|h| format!(" @ {} {}", h.oid, h.summary)).unwrap_or_default()
        );
        println!("  {} staged, {} modified, {} untracked, {} conflicted", v.staged, v.unstaged, v.untracked, v.conflicted);
        if let Some(u) = &v.upstream {
            println!("  {} ahead / {} behind {u} (as of the last fetch)", v.ahead, v.behind);
        }
    }
    println!("\nSaved context (history, not re-verified):");
    for t in &s.saved.tasks {
        let check = if t.checklist_total > 0 { format!(" · {}/{}", t.checklist_done, t.checklist_total) } else { String::new() };
        println!("  task  [{}{check}] {} ({})", t.status, t.title, nexus_core::knowledge::service::short_id(&t.id));
        if let Some(l) = &t.last_progress {
            println!("        last: {l}");
        }
    }
    for n in s.saved.notes.iter().chain(&s.saved.decisions) {
        println!("  {:<5} {}{}", n.kind, n.title, n.branch.as_deref().map(|b| format!(" (branch {b})")).unwrap_or_default());
    }
    for c in &s.saved.checkpoints {
        println!("  [{}] {} — {}, {}{}", c.kind, c.summary, c.session_title, out::ago(Some(c.at)), if c.other_branch { " (other branch)" } else { "" });
    }
    if s.saved.hidden_other_branch > 0 {
        println!("  ({} note(s) of other branches hidden)", s.saved.hidden_other_branch);
    }
    for w in &s.warnings {
        println!("\nwarning: {w}");
    }
    println!("\nNext: {}", s.next.label);
    for e in &s.next.evidence {
        println!("  · {e}");
    }
    let cmd = match s.next.kind {
        ActionKind::ResumeSession => s.next.session_id.as_ref().map(|id| format!("nexus launch --resume {}", out::short_id(id))),
        ActionKind::LaunchTask => s.next.task_id.as_ref().map(|id| format!("nexus launch --task {}", nexus_core::knowledge::service::short_id(id))),
        _ => Some(format!("nexus launch --project \"{}\"", s.project_name)),
    };
    if let Some(c) = cmd {
        println!("  run: {c}");
    }
}

fn cmd_profiles(ctx: &Ctx, cmd: ProfileCmd) -> nexus_core::Result<()> {
    let conn = ctx.db()?;
    match cmd {
        ProfileCmd::List => {
            let list = profile::list(&conn, false)?;
            if ctx.json {
                out::json(&list);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = list
                .iter()
                .map(|p| {
                    vec![
                        p.name.clone(),
                        p.config_dir.as_ref().map(|d| d.display().to_string()).unwrap_or_else(|| "(default location)".into()),
                        p.default_work_profile.clone().unwrap_or_default(),
                        out::ago(p.last_used_at),
                    ]
                })
                .collect();
            out::table(&["NAME", "CONFIG", "RIGOR", "LAST USED"], &rows);
        }
        ProfileCmd::Detect => {
            let env_dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
            let found = profile::detect(&conn, &ctx.home, env_dir.as_deref())?;
            if ctx.json {
                out::json(&found);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = found
                .iter()
                .map(|d| {
                    vec![
                        d.suggested_name.clone(),
                        d.path.display().to_string(),
                        if d.registered_profile_id.is_some() { "registered".into() } else { "—".into() },
                        d.markers.join(","),
                    ]
                })
                .collect();
            out::table(&["SUGGESTED", "PATH", "STATUS", "FOUND"], &rows);
        }
        ProfileCmd::Add(a) => {
            let input = ProfileInput {
                name: a.name,
                description: a.description,
                config_dir: a.config_dir,
                executable: a.executable,
                default_work_profile: a.work_profile,
                default_effort: a.effort,
                default_model: a.model,
                isolate_instructions: a.isolate_instructions,
                ..Default::default()
            };
            let p = if a.create {
                profile::create_managed(&conn, &input, &ctx.home)?
            } else {
                profile::register(&conn, &input, &ctx.home)?
            };
            if ctx.json {
                out::json(&p);
            } else {
                println!(
                    "{} profile “{}” → {}",
                    if a.create { "created" } else { "registered" },
                    p.name,
                    p.config_dir.map(|d| d.display().to_string()).unwrap_or_else(|| "default location".into())
                );
            }
        }
        ProfileCmd::Isolate { profile: q, off } => {
            let list = profile::list(&conn, false)?;
            let p = resolve_ref(&list, &q, |p| &p.id, |p| &p.name, "profile")?.clone();
            let input = ProfileInput {
                name: p.name.clone(),
                description: p.description.clone(),
                color: Some(p.color.clone()),
                executable: p.executable.as_ref().map(|e| e.display().to_string()),
                extra_args: p.extra_args.clone(),
                inherit_provider_env: p.inherit_provider_env,
                isolate_instructions: !off,
                default_work_profile: p.default_work_profile.clone(),
                default_effort: p.default_effort.clone(),
                default_model: p.default_model.clone(),
                config_dir: None,
            };
            let updated = profile::update(&conn, &p.id, &input)?;
            if ctx.json {
                out::json(&updated);
            } else if updated.isolate_instructions {
                println!(
                    "{}: NEXUS sessions will not load the default profile's ~/.claude/CLAUDE.md and ~/.claude/rules                      (organisation-managed and project instructions still load; plain `claude` runs are unaffected)",
                    updated.name
                );
            } else {
                println!("{}: isolation off — Claude loads the default profile's home-folder instructions again", updated.name);
            }
        }
        ProfileCmd::Diagnose { profile: q } => diagnose::run(ctx, &q)?,
        ProfileCmd::Status { profile: q } => {
            let list = profile::list(&conn, false)?;
            let p = resolve_ref(&list, &q, |p| &p.id, |p| &p.name, "profile")?;
            let exe = install::resolve(p.executable.as_deref(), &ctx.home)?;
            let s = auth::status(&exe, p)?;
            // Classification only (subscription / API / unknown) — decides whether usage may be shown as spend.
            let _ = nexus_core::usage::record_billing(&conn, &p.id, &s);
            if ctx.json {
                out::json(&s);
            } else if s.logged_in {
                println!(
                    "{}: signed in as {} ({}{})",
                    p.name,
                    s.email.as_deref().unwrap_or("unknown account"),
                    s.auth_method.as_deref().unwrap_or("?"),
                    s.subscription_type.as_deref().map(|t| format!(", {t}")).unwrap_or_default()
                );
            } else {
                println!("{}: not signed in — run `nexus launch --profile \"{}\"` and log in", p.name, p.name);
            }
        }
    }
    Ok(())
}

fn cmd_sessions(ctx: &Ctx, a: SessionsArgs) -> nexus_core::Result<()> {
    let conn = ctx.db()?;
    let workspace_id = if a.all { None } else { workspace::active(&conn)?.map(|w| w.id) };
    let list = session::list(
        &conn,
        &Filter {
            workspace_id,
            include_archived: a.all,
            limit: Some(a.limit),
            ..Default::default()
        },
    )?;
    if ctx.json {
        out::json(&list);
        return Ok(());
    }
    let profiles = profile::list(&conn, true)?;
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|s| {
            let state = match (s.state, s.exit_code) {
                (session::State::Exited, Some(c)) if c != 0 => format!("exited ({c})"),
                (st, _) => format!("{st:?}").to_lowercase(),
            };
            let status = nexus_core::attention::status(s, s.state.is_live());
            let attention = match (status.status, status.certainty) {
                (_, "assumed") if s.state.is_live() => "running?".to_string(),
                (st, _) => serde_json::to_value(st).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
            };
            vec![
                out::short_id(&s.id),
                state,
                attention,
                s.host.clone(),
                s.profile_id
                    .as_ref()
                    .and_then(|id| profiles.iter().find(|p| &p.id == id))
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| format!("{:?}", s.kind).to_lowercase()),
                out::ago(Some(s.started_at)),
                s.title.clone(),
            ]
        })
        .collect();
    out::table(&["ID", "STATE", "ATTENTION", "HOST", "PROFILE", "STARTED", "TITLE"], &rows);
    Ok(())
}

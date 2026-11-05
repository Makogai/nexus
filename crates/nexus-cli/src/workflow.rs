//! `nexus workflow …` and `nexus gates …` (ADR-0029): workflow definitions, runs and the trusted quality gates from
//! the terminal. `nexus workflow verify` is a host: it runs pending gates here when no desktop is running.

use clap::Subcommand;

use nexus_core::host::{self, HostKind};
use nexus_core::workflow::gates::{self, GateInput};
use nexus_core::workflow::run::{self, RunFilter, StartRun};
use nexus_core::workflow::{self, NewWorkflow, templates};
use nexus_core::{CoreError, Result, env};

use crate::knowledge::{find_id, find_project};
use crate::{Ctx, active_workspace, out};

#[derive(Subcommand)]
pub enum WorkflowCmd {
    /// Built-in templates (feature, bugfix, review, refactor, testing, release).
    Templates,
    /// Workflow definitions of the active workspace.
    List,
    /// One workflow: steps, dependencies, gates, criteria.
    Show { workflow: String },
    /// Create a workflow (from a template).
    New {
        #[arg(long)]
        template: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        rigor: Option<String>,
        /// Skill activated only for this workflow's sessions (`kit:skill`, repeatable).
        #[arg(long = "skill")]
        skills: Vec<String>,
    },
    /// Start a run (creates its task unless --task is given). Launch Claude for it with `nexus launch --task <task>`.
    Start {
        workflow: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        task: Option<String>,
        #[arg(long, default_value = "")]
        goal: String,
        #[arg(long)]
        rigor: Option<String>,
    },
    /// Runs of the active workspace.
    Runs {
        #[arg(long)]
        active: bool,
    },
    /// A run's steps: status and how each was verified.
    Status { run: String },
    /// Run the pending quality gates now (this terminal is the host while they run).
    Verify,
    /// Approve or reject a step waiting for review.
    Review {
        run: String,
        step: String,
        #[arg(long)]
        approve: bool,
        #[arg(long)]
        reject: bool,
        #[arg(long, default_value = "")]
        note: String,
    },
    Cancel { run: String },
    Retry { run: String, step: String },
}

#[derive(Subcommand)]
pub enum GatesCmd {
    /// Gates defined for a project.
    List {
        #[arg(long)]
        project: String,
    },
    /// Define a gate: `nexus gates add --project app test -- npm test` (argument vector, no shell).
    Add {
        #[arg(long)]
        project: String,
        name: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        timeout: Option<i64>,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Authorise a gate's command — the version you reviewed in `nexus gates list` (a changed command is a new
    /// version and needs approval again). Separate from approving a workflow.
    Approve {
        #[arg(long)]
        project: String,
        name: String,
        /// The version you reviewed; refused if the command changed since.
        #[arg(long)]
        version: i64,
    },
    /// Withdraw a gate's approval (it stays defined but does not run).
    Revoke {
        #[arg(long)]
        project: String,
        name: String,
    },
    Remove {
        #[arg(long)]
        project: String,
        name: String,
    },
    /// Gates suggested from the project's detected scripts (`--add` defines them, unapproved).
    Suggest {
        #[arg(long)]
        project: String,
        #[arg(long)]
        add: bool,
    },
    /// Run a gate now and record the result.
    Run {
        #[arg(long)]
        project: String,
        name: String,
    },
}

pub(crate) fn find_run(conn: &nexus_core::db::Connection, ws_id: &str, q: &str) -> Result<String> {
    let runs = run::list(conn, ws_id, &RunFilter { limit: Some(500), ..Default::default() })?;
    if let Some(r) = runs.iter().find(|r| r.id == q) {
        return Ok(r.id.clone());
    }
    let hits: Vec<_> = runs.iter().filter(|r| q.len() >= 4 && r.id.ends_with(q)).collect();
    match hits.as_slice() {
        [one] => Ok(one.id.clone()),
        [] => Err(CoreError::not_found(format!("no workflow run {q}"))),
        _ => Err(CoreError::invalid(format!("'{q}' matches {} runs — use more characters", hits.len()))),
    }
}

fn short(id: &str) -> String {
    id.chars().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect()
}

fn print_run(v: &run::RunView) {
    println!("{} — {} ({}, {} rigor) · {}/{} done: {} verified, {} signed off, {} AI-reported, {} skipped", short(&v.run.id), v.run.workflow_title, v.run.status, v.run.rigor, v.progress.done, v.progress.total, v.progress.verified, v.progress.human, v.progress.ai_reported, v.progress.skipped);
    let rows: Vec<Vec<String>> = v
        .steps
        .iter()
        .map(|s| {
            vec![
                s.state.step_id.clone(),
                s.state.kind.clone(),
                s.state.status.clone(),
                s.state.verification.clone().unwrap_or_default(),
                format!("{}/{}", s.state.attempts, s.state.max_attempts),
                s.gates.join(","),
                s.title.clone(),
            ]
        })
        .collect();
    out::table(&["STEP", "KIND", "STATUS", "VERIFIED BY", "TRIES", "GATES", "TITLE"], &rows);
    for s in v.steps.iter().filter(|s| s.state.last_failure.is_some() && !matches!(s.state.status.as_str(), "passed" | "skipped")) {
        println!("\n{}: {}", s.state.step_id, s.state.last_failure.as_deref().unwrap_or_default().lines().take(12).collect::<Vec<_>>().join("\n  "));
    }
}

pub fn run_workflow(ctx: &Ctx, c: WorkflowCmd) -> Result<i32> {
    let conn = ctx.db()?;
    let ws = active_workspace(&conn)?;
    match c {
        WorkflowCmd::Templates => {
            if ctx.json {
                out::json(&templates::TEMPLATES);
            } else {
                let rows: Vec<Vec<String>> = templates::TEMPLATES.iter().map(|t| vec![t.id.into(), t.category.into(), t.title.into(), t.summary.into()]).collect();
                out::table(&["ID", "CATEGORY", "TITLE", "SUMMARY"], &rows);
            }
        }
        WorkflowCmd::List => {
            let list = workflow::list(&conn, &ctx.data_dir, &ws)?;
            if ctx.json {
                out::json(&list);
            } else {
                let rows: Vec<Vec<String>> = list
                    .iter()
                    .map(|w| vec![short(&w.id), w.category.clone(), w.rigor.clone(), w.steps.to_string(), if !w.approved { "needs approval".into() } else if w.errors > 0 { "has errors".into() } else { "ready".into() }, w.title.clone()])
                    .collect();
                out::table(&["ID", "CATEGORY", "RIGOR", "STEPS", "STATE", "TITLE"], &rows);
            }
        }
        WorkflowCmd::Show { workflow: q } => {
            let id = find_id(&conn, &ws, &q)?;
            let v = workflow::get(&conn, &ctx.data_dir, &ws, &id)?;
            if ctx.json {
                out::json(&v);
                return Ok(0);
            }
            println!("{} ({}, default rigor {}) — skills: {}", v.summary.title, v.summary.category, v.summary.rigor, if v.summary.skills.is_empty() { "none".into() } else { v.summary.skills.join(", ") });
            for s in &v.definition.steps {
                println!("  [{}] {} — {}{}{}{}", s.id, s.title, s.kind.as_str(), if s.needs.is_empty() { String::new() } else { format!(", needs {}", s.needs.join(",")) }, if s.gates.is_empty() { String::new() } else { format!(", gates {}", s.gates.join(",")) }, s.rigor.as_deref().map(|r| format!(", from {r}")).unwrap_or_default());
            }
            for p in &v.problems {
                println!("{}: {}", p.severity, p.message);
            }
        }
        WorkflowCmd::New { template, title, rigor, skills } => {
            let v = workflow::create(&conn, &ctx.data_dir, &ws, &NewWorkflow { title: title.unwrap_or_default(), category: String::new(), rigor, skills, template, project_id: None })?;
            if ctx.json {
                out::json(&v);
            } else {
                println!("created “{}” ({} steps) at {}", v.summary.title, v.summary.steps, v.summary.relative_path);
            }
        }
        WorkflowCmd::Start { workflow: q, project, task, goal, rigor } => {
            let wf = find_id(&conn, &ws, &q)?;
            let p = find_project(&conn, &ws, &project)?;
            let task_id = task.map(|t| find_id(&conn, &ws, &t)).transpose()?;
            let v = run::start(&conn, &ctx.data_dir, &ws, &StartRun { workflow_id: wf, project_id: Some(p.id), task_id, goal, rigor, profile_id: None })?;
            if ctx.json {
                out::json(&v);
            } else {
                print_run(&v);
                println!("\nlaunch Claude for it: nexus launch --task {} -w {}", short(&v.run.task_id), v.run.rigor);
            }
        }
        WorkflowCmd::Runs { active } => {
            let list = run::list(&conn, &ws.id, &RunFilter { active_only: active, ..Default::default() })?;
            if ctx.json {
                out::json(&list);
            } else {
                let rows: Vec<Vec<String>> = list.iter().map(|r| vec![short(&r.id), r.status.clone(), r.rigor.clone(), out::ago(Some(r.updated_at)), r.workflow_title.clone()]).collect();
                out::table(&["ID", "STATUS", "RIGOR", "UPDATED", "WORKFLOW"], &rows);
            }
        }
        WorkflowCmd::Status { run: q } => {
            let id = find_run(&conn, &ws.id, &q)?;
            let v = run::view(&conn, &id)?;
            if ctx.json {
                out::json(&v);
            } else {
                print_run(&v);
            }
        }
        WorkflowCmd::Verify => {
            drop(conn);
            let n = verify_here(ctx)?;
            println!("{n} step(s) verified");
        }
        WorkflowCmd::Review { run: q, step, approve, reject, note } => {
            if approve == reject {
                return Err(CoreError::invalid("choose --approve or --reject"));
            }
            let id = find_run(&conn, &ws.id, &q)?;
            let v = run::review(&conn, &ctx.data_dir, &ws, &id, &step, approve, &note)?;
            if ctx.json { out::json(&v) } else { print_run(&v) }
        }
        WorkflowCmd::Cancel { run: q } => {
            let id = find_run(&conn, &ws.id, &q)?;
            let v = run::cancel(&conn, &ctx.data_dir, &ws, &id)?;
            if ctx.json { out::json(&v) } else { print_run(&v) }
        }
        WorkflowCmd::Retry { run: q, step } => {
            let id = find_run(&conn, &ws.id, &q)?;
            let v = run::retry(&conn, &ctx.data_dir, &ws, &id, &step)?;
            if ctx.json { out::json(&v) } else { print_run(&v) }
        }
    }
    Ok(0)
}

/// Run pending gates in this process: registered as a session host so a killed terminal is reconciled (ADR-0014).
fn verify_here(ctx: &Ctx) -> Result<usize> {
    let conn = ctx.db()?;
    let me = host::register(&conn, HostKind::Cli)?;
    let path = ctx.data_dir.join(nexus_core::db::DB_FILE);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let beat = {
        let (stop, id, path) = (stop.clone(), me.id.clone(), path.clone());
        std::thread::spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if let Ok(c) = nexus_core::db::open(&path) {
                    let _ = host::heartbeat(&c, &id, &[]);
                }
            }
        })
    };
    let base = env::current();
    let mut total = 0;
    loop {
        let n = run::verify_pending(|f: &mut dyn FnMut(&nexus_core::db::Connection) -> Result<()>| f(&conn), &ctx.data_dir, &me.id, &base)?;
        total += n;
        if n == 0 {
            break;
        }
    }
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = beat.join();
    host::shutdown(&conn, &me.id)?;
    Ok(total)
}

/// "resumes 2 steps in 1 run (1 more keeps waiting for other gates)" — what approving a gate does to waiting work.
fn impact_text(i: &run::ApprovalImpact, done: bool) -> String {
    let verb = if done { "resumed" } else { "resumes" };
    let mut s = if i.steps > 0 {
        format!("{verb} {} waiting step(s) in {} run(s) automatically", i.steps, i.runs)
    } else {
        "no waiting workflow step resumes".to_string()
    };
    if i.still_waiting > 0 {
        s.push_str(&format!("; {} step(s) keep waiting for other gates", i.still_waiting));
    }
    s
}

pub fn run_gates(ctx: &Ctx, c: GatesCmd) -> Result<i32> {
    let conn = ctx.db()?;
    let ws = active_workspace(&conn)?;
    match c {
        GatesCmd::List { project } => {
            let p = find_project(&conn, &ws, &project)?;
            let list = gates::list(&conn, &p.id)?;
            if ctx.json {
                out::json(&list);
            } else {
                let rows: Vec<Vec<String>> = list
                    .iter()
                    .map(|g| {
                        let last = gates::latest(&conn, &p.id, &g.name).ok().flatten().map(|r| format!("{} {}", r.status, out::ago(Some(r.started_at)))).unwrap_or_else(|| "never run".into());
                        let state = if g.approved { "approved".to_string() } else { "NOT APPROVED".to_string() };
                        let on_approval = if g.approved {
                            "–".to_string()
                        } else {
                            run::approval_impact(&conn, &p.id, &g.name).map(|i| impact_text(&i, false)).unwrap_or_else(|_| "–".into())
                        };
                        vec![g.name.clone(), format!("v{}", g.version), state, g.command_line(), format!("{} s", g.timeout_secs), last, on_approval]
                    })
                    .collect();
                out::table(&["NAME", "VERSION", "APPROVAL", "COMMAND", "TIMEOUT", "LAST RESULT", "ON APPROVAL"], &rows);
                if list.iter().any(|g| !g.approved) {
                    println!("\nreview a command, then: nexus gates approve --project {} <name> --version <n>", p.name);
                }
            }
        }
        GatesCmd::Add { project, name, title, timeout, command } => {
            let p = find_project(&conn, &ws, &project)?;
            let mut it = command.into_iter();
            let program = it.next().ok_or_else(|| CoreError::invalid("give the command after --"))?;
            let g = gates::upsert(&conn, &p.id, &GateInput { name, title: title.unwrap_or_default(), program, args: it.collect(), timeout_secs: timeout })?;
            if ctx.json {
                out::json(&g)
            } else {
                println!("gate {} v{} = {}", g.name, g.version, g.command_line());
                if !g.approved {
                    println!("not approved yet — it runs only after: nexus gates approve --project {} {} --version {}", p.name, g.name, g.version);
                }
            }
        }
        GatesCmd::Approve { project, name, version } => {
            let p = find_project(&conn, &ws, &project)?;
            let g = gates::find(&conn, &p.id, &name)?.ok_or_else(|| CoreError::not_found(format!("no gate {name} for {}", p.name)))?;
            if g.version != version {
                return Err(CoreError::Conflict(format!("gate {name} is version {} now, not {version} — review it again with `nexus gates list`", g.version)));
            }
            let g = gates::approve(&conn, &p.id, &name, &g.command_hash)?;
            // Resuming is part of the one approval (no second confirmation); its effect is reported.
            let resumed = run::resume_gate_waits(&conn, &p.id)?;
            if ctx.json {
                out::json(&serde_json::json!({ "gate": g, "resumed": resumed }))
            } else {
                println!("approved gate {} v{}: {}", g.name, g.version, g.command_line());
                println!("{}", impact_text(&resumed, true));
                if resumed.steps > 0 {
                    println!("`nexus workflow verify` (or the desktop) runs their gates now");
                }
            }
        }
        GatesCmd::Revoke { project, name } => {
            let p = find_project(&conn, &ws, &project)?;
            gates::revoke(&conn, &p.id, &name)?;
            println!("approval of gate {name} withdrawn");
        }
        GatesCmd::Remove { project, name } => {
            let p = find_project(&conn, &ws, &project)?;
            println!("{}", if gates::remove(&conn, &p.id, &name)? { "removed" } else { "no such gate" });
        }
        GatesCmd::Suggest { project, add } => {
            let p = find_project(&conn, &ws, &project)?;
            let s = gates::suggest(&p);
            for g in &s {
                println!("{}: {} {}", g.name, g.program, g.args.join(" "));
                if add {
                    let g = gates::upsert(&conn, &p.id, g)?;
                    if !g.approved {
                        println!("  added as v{}, not approved yet (`nexus gates approve --project {} {} --version {}`)", g.version, p.name, g.name, g.version);
                    }
                }
            }
            if s.is_empty() {
                println!("no scripts detected — define gates with `nexus gates add`");
            }
        }
        GatesCmd::Run { project, name } => {
            let p = find_project(&conn, &ws, &project)?;
            let g = gates::find(&conn, &p.id, &name)?.ok_or_else(|| CoreError::not_found(format!("no gate {name} for {}", p.name)))?;
            let dir = p.local_path.clone().filter(|d| d.is_dir()).ok_or_else(|| CoreError::invalid("the project folder is not available"))?;
            let me = host::register(&conn, HostKind::Cli)?;
            let id = match gates::begin(&conn, &p.id, None, &g, &me.id) {
                Ok(id) => id,
                Err(e) => {
                    host::shutdown(&conn, &me.id)?;
                    return Err(e);
                }
            };
            let o = gates::execute(&g, &dir, &env::current(), &id);
            let r = gates::finish(&conn, &id, &o)?;
            host::shutdown(&conn, &me.id)?;
            if ctx.json {
                out::json(&r);
            } else {
                println!("{} {} in {} ms (exit {})\n{}", r.gate_name, r.status, r.duration_ms.unwrap_or(0), r.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "–".into()), r.output_tail);
            }
            return Ok(if r.status == "passed" { 0 } else { 1 });
        }
    }
    Ok(0)
}

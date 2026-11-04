//! `nexus instructions` — the effective-instructions inspector (ADR-0019): what Claude Code loads by itself for a
//! profile and directory (managed, user, project and local CLAUDE.md layers, rules, imports) in its native order, and
//! what NEXUS would append. Read-only; nothing is launched or changed (a task is not started).

use clap::Args;
use serde::Serialize;

use nexus_core::claude::instructions::Load;
use nexus_core::claude::policy::MachinePolicyLocations;
use nexus_core::claude::prepare::{self, Preview, PreviewRequest};
use nexus_core::knowledge::{Kind, service as knowledge};
use nexus_core::{Result, env, project, resolve_ref, workspace};

use crate::launch::{current_dir_of, find_profile, project_containing};
use crate::{Ctx, out};

#[derive(Args)]
pub struct InstructionsArgs {
    #[arg(long, short = 'p')]
    profile: Option<String>,
    /// Project of the active workspace (default: the project containing the working directory).
    #[arg(long)]
    project: Option<String>,
    #[arg(long, value_name = "DIR")]
    cwd: Option<String>,
    /// Include the brief a session for this task would get (id, its last digits, or title).
    #[arg(long)]
    task: Option<String>,
    #[arg(long = "work-profile", short = 'w')]
    work_profile: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    profile: String,
    cwd: String,
    #[serde(flatten)]
    preview: Preview,
}

pub fn run(ctx: &Ctx, a: InstructionsArgs) -> Result<()> {
    let conn = ctx.db()?;
    let profile = find_profile(&conn, a.profile.as_deref())?;
    let ws = workspace::active(&conn)?;
    let projects = match &ws {
        Some(w) => project::list(&conn, &w.id)?,
        None => Vec::new(),
    };
    let proj = match a.project.as_deref() {
        Some(q) => Some(resolve_ref(&projects, q, |p| &p.id, |p| &p.name, "project")?.clone()),
        None => project_containing(&projects, &current_dir_of(a.cwd.as_deref())?),
    };
    let cwd = match proj.as_ref().and_then(|p| p.local_path.clone()).filter(|p| p.is_dir()) {
        Some(dir) => dir,
        None => current_dir_of(a.cwd.as_deref())?,
    };
    let task_id = match (a.task.as_deref(), &ws) {
        (Some(q), Some(w)) => {
            knowledge::refresh(&conn, &ctx.data_dir, w)?;
            let items: Vec<_> = knowledge::list(&conn, w, &[Kind::Task], false)?.into_iter().filter(|t| t.id.is_some()).collect();
            knowledge::resolve(&items, q)?.id.clone()
        }
        _ => None,
    };
    let base_env = env::current();
    let preview = prepare::preview(&PreviewRequest {
        conn: &conn,
        data_dir: &ctx.data_dir,
        home: &ctx.home,
        profile: &profile,
        machine: &MachinePolicyLocations::system(),
        workspace_id: ws.as_ref().map(|w| w.id.as_str()),
        project_id: proj.as_ref().map(|p| p.id.as_str()),
        task_id: task_id.as_deref(),
        cwd: &cwd,
        base_env: &base_env,
        work_profile: a.work_profile.as_deref(),
    })?;

    if ctx.json {
        out::json(&Report { profile: profile.name.clone(), cwd: cwd.display().to_string(), preview });
        return Ok(());
    }
    let (claude, nexus) = (&preview.claude, &preview.nexus);
    println!("Instructions Claude loads itself — profile {}, {}", profile.name, cwd.display());
    let rows: Vec<Vec<String>> = claude
        .layers
        .iter()
        .enumerate()
        .map(|(n, l)| {
            let load = match l.load {
                Load::AtStart => "at start",
                Load::OnDemand => "on demand",
                Load::Excluded => "excluded",
                Load::NotLoaded => "not loaded",
                Load::Fallback => "fallback",
            };
            let what = l.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(managed settings)".into());
            let note = l.reason.clone().unwrap_or_else(|| if l.paths.is_empty() { String::new() } else { format!("paths: {}", l.paths.join(", ")) });
            vec![
                format!("{}", n + 1),
                format!("{:?}", l.scope).to_lowercase(),
                l.kind.to_string(),
                load.to_string(),
                format!("{} lines", l.lines),
                if note.is_empty() { what } else { format!("{what}  — {note}") },
            ]
        })
        .collect();
    if rows.is_empty() {
        println!("  (no CLAUDE.md, rules or imports apply)");
    } else {
        out::table(&["#", "SCOPE", "KIND", "LOAD", "SIZE", "FILE"], &rows);
    }
    println!("  {} file(s), {} characters at session start; AGENTS.md mode: {}", claude.files_at_start, claude.chars_at_start, claude.agents_mode);
    for w in &claude.warnings {
        println!("  warning: {w}");
    }
    println!("\nNEXUS appends to the system prompt ({} characters):", nexus.appended_chars);
    println!("  work profile: {}", nexus.work_profile.as_deref().unwrap_or("none"));
    if let Some(r) = &nexus.rules {
        println!("  always-on rules: {}", if r.included.is_empty() { "none".into() } else { r.included.join(", ") });
        for (label, list) in [("already in CLAUDE.md (skipped)", &r.already_loaded), ("waiting for approval", &r.pending_approval), ("over budget (on demand)", &r.over_budget)] {
            if !list.is_empty() {
                println!("  {label}: {}", list.join(", "));
            }
        }
    }
    if let Some(t) = &nexus.task {
        println!("  task brief: “{t}” ({} characters; never the task history)", nexus.task_brief_chars);
    }
    for (name, state) in [("NEXUS tools", &nexus.mcp), ("lifecycle hooks", &nexus.hooks)] {
        if let Some(s) = state {
            println!("  {name}: {s}");
        }
    }
    println!("  everything else stays out of the prompt until Claude asks for it through the NEXUS tools");
    Ok(())
}

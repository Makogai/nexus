//! `nexus constitution …`: constitutions from the terminal (ADR-0027). Same files and rules as the desktop's
//! Constitution Manager; editing happens in the desktop or any text editor.

use clap::Subcommand;

use nexus_core::knowledge::constitution::{self, NewConstitution, Target};
use nexus_core::{CoreError, Result};

use crate::knowledge::{find_id, find_project};
use crate::{Ctx, active_workspace, out};

#[derive(Subcommand)]
pub enum ConstitutionCmd {
    /// Constitutions of the active workspace (and global ones), by scope.
    List,
    /// One constitution: its articles, sections and rules, and problems.
    Show { id: String },
    /// The effective rules for a project (and task): scopes applied, overrides, conflicts, pending approvals.
    Effective {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        task: Option<String>,
        /// Preview as if every changed document were approved.
        #[arg(long)]
        pending: bool,
    },
    /// Problems and conflicts of all constitutions; exit code 1 when there are errors or conflicts.
    Check,
    /// Run the gates linked from the project's effective rules (`check=`); exit code 1 when one fails.
    Verify {
        #[arg(long)]
        project: String,
    },
    /// Create a constitution (scope: global, workspace, stack, project, task).
    New {
        title: String,
        #[arg(long, default_value = "workspace")]
        scope: String,
        #[arg(long = "stack")]
        stacks: Vec<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        task: Option<String>,
        /// Rule id prefix (default: initials of the title).
        #[arg(long)]
        prefix: Option<String>,
        /// Start from the starter template.
        #[arg(long)]
        starter: bool,
    },
}

pub fn run(ctx: &Ctx, c: ConstitutionCmd) -> Result<i32> {
    let conn = ctx.db()?;
    let ws = active_workspace(&conn)?;
    match c {
        ConstitutionCmd::List => {
            let list = constitution::list(&conn, &ctx.data_dir, &ws)?;
            if ctx.json {
                out::json(&list);
                return Ok(0);
            }
            let rows: Vec<Vec<String>> = list
                .iter()
                .map(|s| {
                    let state = if !s.approved {
                        "needs approval"
                    } else if s.errors > 0 {
                        "has errors"
                    } else {
                        "in force"
                    };
                    vec![s.id.chars().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect(), s.scope.label().into(), format!("{} ({} mandatory)", s.rules, s.mandatory), state.into(), s.title.clone()]
                })
                .collect();
            if rows.is_empty() {
                println!("no constitutions yet — `nexus constitution new \"Team charter\" --starter`");
            }
            out::table(&["ID", "SCOPE", "RULES", "STATE", "TITLE"], &rows);
        }
        ConstitutionCmd::Show { id } => {
            let id = find_id(&conn, &ws, &id)?;
            let v = constitution::get(&conn, &ctx.data_dir, &ws, &id)?;
            if ctx.json {
                out::json(&v);
                return Ok(0);
            }
            println!("{} — {} · {} · {}", v.summary.title, v.summary.scope.label(), v.summary.relative_path, if v.summary.approved { "approved" } else { "needs approval" });
            for a in &v.structure.articles {
                println!("\n## {}", a.title);
                for s in &a.sections {
                    if !s.title.is_empty() {
                        println!("  ### {}", s.title);
                    }
                    for r in &s.rules {
                        println!("    [{} · {}{}] {}", r.id, r.enforcement.as_str(), if r.enabled { "" } else { ", disabled" }, r.title);
                    }
                }
            }
            for p in &v.problems {
                println!("{}: {}", p.severity, p.message);
            }
        }
        ConstitutionCmd::Effective { project, task, pending } => {
            let p = project.map(|q| find_project(&conn, &ws, &q)).transpose()?;
            let task = task.map(|t| find_id(&conn, &ws, &t)).transpose()?;
            let stacks = p.as_ref().map(|p| p.stack.stacks.clone()).unwrap_or_default();
            let t = Target { project: p.as_ref().map(|p| (p.id.as_str(), stacks.as_slice())), task_id: task.as_deref(), include_pending: pending };
            let e = constitution::effective(&conn, &ctx.data_dir, &ws, &t)?;
            let comp = constitution::compliance(&conn, p.as_ref().map(|p| p.id.as_str()), &e)?;
            if ctx.json {
                out::json(&serde_json::json!({ "effective": e, "compliance": comp }));
                return Ok(0);
            }
            let rows: Vec<Vec<String>> = e
                .rules
                .iter()
                .zip(&comp)
                .map(|(r, c)| {
                    let compliance = match (c.status, &c.check) {
                        ("instructed", _) => "instructed only".to_string(),
                        (s, Some(g)) => format!("{g}: {s}"),
                        (s, None) => s.to_string(),
                    };
                    vec![r.id.clone(), r.enforcement.as_str().into(), r.scope.label().into(), compliance, r.title.clone()]
                })
                .collect();
            out::table(&["ID", "ENFORCEMENT", "FROM", "COMPLIANCE", "TITLE"], &rows);
            println!("\ninstructed = Claude is told; only rules with a check= gate are verified (`nexus constitution verify --project …`).");
            for c in &e.conflicts {
                println!("conflict: {}", c.message);
            }
            for d in &e.pending {
                println!("needs approval: “{}” (`nexus knowledge approve {}` after reviewing it)", d.title, d.id);
            }
        }
        ConstitutionCmd::Check => {
            let list = constitution::list(&conn, &ctx.data_dir, &ws)?;
            let mut bad = 0;
            for s in &list {
                let v = constitution::get(&conn, &ctx.data_dir, &ws, &s.id)?;
                for p in v.problems.iter().filter(|p| p.severity == "error") {
                    println!("{}: {}", s.title, p.message);
                    bad += 1;
                }
            }
            // Conflicts across scopes, per project (and the workspace itself).
            let projects = nexus_core::project::list(&conn, &ws.id)?;
            let mut seen = std::collections::HashSet::new();
            let mut targets: Vec<Option<&nexus_core::project::Project>> = vec![None];
            targets.extend(projects.iter().map(Some));
            for p in targets {
                let t = Target { project: p.map(|p| (p.id.as_str(), p.stack.stacks.as_slice())), ..Default::default() };
                for c in constitution::effective(&conn, &ctx.data_dir, &ws, &t)?.conflicts {
                    if seen.insert(c.message.clone()) {
                        println!("conflict{}: {}", p.map(|p| format!(" ({})", p.name)).unwrap_or_default(), c.message);
                        bad += 1;
                    }
                }
            }
            if bad == 0 {
                println!("{} constitution(s), no problems", list.len());
            }
            return Ok(if bad > 0 { 1 } else { 0 });
        }
        ConstitutionCmd::Verify { project } => {
            let p = find_project(&conn, &ws, &project)?;
            let t = Target { project: Some((p.id.as_str(), p.stack.stacks.as_slice())), ..Default::default() };
            let e = constitution::effective(&conn, &ctx.data_dir, &ws, &t)?;
            let mut names: Vec<String> = e.rules.iter().filter_map(|r| r.check.clone()).collect();
            names.sort();
            names.dedup();
            if names.is_empty() {
                println!("no rule of {} names a check= gate — every rule is instructed only", p.name);
                return Ok(0);
            }
            let dir = p.local_path.clone().filter(|d| d.is_dir()).ok_or_else(|| CoreError::invalid("the project folder is not available"))?;
            let me = nexus_core::host::register(&conn, nexus_core::host::HostKind::Cli)?;
            let mut failed = 0;
            for n in &names {
                let Some(g) = nexus_core::workflow::gates::find(&conn, &p.id, n)? else {
                    println!("{n}: not defined for {} (`nexus gates add --project {} {n} -- …`)", p.name, p.name);
                    failed += 1;
                    continue;
                };
                let id = match nexus_core::workflow::gates::begin(&conn, &p.id, None, &g, &me.id) {
                    Ok(id) => id,
                    Err(e) => {
                        println!("{n}: {e}");
                        failed += 1;
                        continue;
                    }
                };
                let o = nexus_core::workflow::gates::execute(&g, &dir, &nexus_core::env::current(), &id);
                let r = nexus_core::workflow::gates::finish(&conn, &id, &o)?;
                println!("{n}: {} ({} ms)", r.status, r.duration_ms.unwrap_or(0));
                if r.status != "passed" {
                    failed += 1;
                }
            }
            nexus_core::host::shutdown(&conn, &me.id)?;
            return Ok(if failed > 0 { 1 } else { 0 });
        }
        ConstitutionCmd::New { title, scope, stacks, project, task, prefix, starter } => {
            let project_id = project.map(|q| find_project(&conn, &ws, &q).map(|p| p.id)).transpose()?;
            let task_id = task.map(|t| find_id(&conn, &ws, &t)).transpose()?;
            if scope == "global" && project_id.is_some() {
                return Err(CoreError::invalid("a global constitution has no project"));
            }
            let v = constitution::create(&conn, &ctx.data_dir, &ws, &NewConstitution { title, scope, stacks, project_id, task_id, applies: None, prefix, starter })?;
            if ctx.json {
                out::json(&v);
            } else {
                println!("created “{}” ({}, {} rules) at {}", v.summary.title, v.summary.scope.label(), v.summary.rules, v.summary.relative_path);
            }
        }
    }
    Ok(0)
}

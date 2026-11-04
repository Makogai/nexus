//! `nexus context …` — the Context Manager from the terminal (ADR-0028): a project's strategy, the onboarding
//! analysis, and the Context Inspector (configured / predicted / observed / on demand / retrieved).

use clap::Subcommand;

use nexus_core::claude::policy::MachinePolicyLocations;
use nexus_core::claude::prepare::{self, PreviewRequest};
use nexus_core::context::{self, Settings, Strategy, analysis, inspector};
use nexus_core::{CoreError, Result, env};

use crate::knowledge::find_project;
use crate::launch::find_profile;
use crate::{Ctx, active_workspace, out};

#[derive(Subcommand)]
pub enum ContextCmd {
    /// The project's context strategy and the settings it resolves to.
    Show {
        #[arg(long)]
        project: String,
    },
    /// Choose a strategy: minimal, balanced, deep, or custom (with --settings JSON).
    Set {
        #[arg(long)]
        project: String,
        #[arg(long)]
        strategy: String,
        /// Custom settings as JSON (camelCase keys, see `nexus context show --json`).
        #[arg(long)]
        settings: Option<String>,
    },
    /// Onboarding analysis: sources, what each strategy appends, and a recommendation.
    Analyze {
        #[arg(long)]
        project: String,
    },
    /// The Context Inspector for the project (and a profile for the prediction).
    Inspect {
        #[arg(long)]
        project: String,
        #[arg(long, short = 'p')]
        profile: Option<String>,
    },
}

pub fn run(ctx: &Ctx, c: ContextCmd) -> Result<()> {
    let conn = ctx.db()?;
    let ws = active_workspace(&conn)?;
    match c {
        ContextCmd::Show { project } => {
            let p = find_project(&conn, &ws, &project)?;
            let c = context::get(&conn, &p.id)?;
            if ctx.json {
                out::json(&c);
            } else {
                println!("{}: {}{}", p.name, c.strategy.as_str(), if c.configured { "" } else { " (default — not chosen yet; see `nexus context analyze`)" });
                println!("{}", serde_json::to_string_pretty(&c.settings).unwrap_or_default());
            }
        }
        ContextCmd::Set { project, strategy, settings } => {
            let p = find_project(&conn, &ws, &project)?;
            let s = Strategy::parse(&strategy).ok_or_else(|| CoreError::invalid("strategy must be minimal, balanced, deep or custom"))?;
            let custom: Option<Settings> = match settings {
                Some(j) => Some(serde_json::from_str(&j).map_err(|e| CoreError::invalid(format!("settings: {e}")))?),
                None => None,
            };
            let c = context::set(&conn, &p.id, s, custom)?;
            if ctx.json {
                out::json(&c);
            } else {
                println!("{}: {} (applies to new sessions)", p.name, c.strategy.as_str());
            }
        }
        ContextCmd::Analyze { project } => {
            let p = find_project(&conn, &ws, &project)?;
            let a = analysis::analyze(&conn, &ctx.data_dir, &p.id)?;
            if ctx.json {
                out::json(&a);
                return Ok(());
            }
            println!("{}: {} docs ({} KB), {} decisions, {} open tasks, {} constitution rules", a.project, a.docs.files, a.docs.bytes / 1024, a.knowledge.decisions, a.knowledge.open_tasks, a.constitution_rules);
            let rows: Vec<Vec<String>> = a
                .estimates
                .iter()
                .map(|e| vec![e.strategy.as_str().into(), e.rules_chars.to_string(), e.rules_in_prompt.to_string(), e.on_demand.to_string(), e.startup_chars_max.to_string()])
                .collect();
            out::table(&["STRATEGY", "RULES CHARS", "IN PROMPT", "ON DEMAND", "STARTUP ≤"], &rows);
            println!("recommended: {} — {}", a.recommendation.as_str(), a.reasons.join("; "));
        }
        ContextCmd::Inspect { project, profile } => {
            let p = find_project(&conn, &ws, &project)?;
            let predicted = match profile.as_deref() {
                Some(q) => {
                    let profile = find_profile(&conn, Some(q))?;
                    let cwd = p.local_path.clone().filter(|d| d.is_dir()).ok_or_else(|| CoreError::invalid("the project folder is not available"))?;
                    let base_env = env::current();
                    Some(prepare::preview(&PreviewRequest {
                        conn: &conn,
                        data_dir: &ctx.data_dir,
                        home: &ctx.home,
                        profile: &profile,
                        machine: &MachinePolicyLocations::system(),
                        workspace_id: Some(&ws.id),
                        project_id: Some(&p.id),
                        task_id: None,
                        cwd: &cwd,
                        base_env: &base_env,
                        work_profile: None,
                    })?)
                }
                None => None,
            };
            let profile_id = profile.as_deref().map(|q| find_profile(&conn, Some(q)).map(|p| p.id)).transpose()?;
            let r = inspector::inspect(inspector::Request { conn: &conn, workspace_id: &ws.id, project_id: &p.id, stacks: &p.stack.stacks, profile_id: profile_id.as_deref(), predicted })?;
            if ctx.json {
                out::json(&r);
                return Ok(());
            }
            println!("configured: {}{}", r.configured.strategy.as_str(), if r.configured.configured { "" } else { " (default)" });
            if let Some(pr) = &r.predicted {
                println!("predicted: {} instruction file(s) at start, NEXUS appends {} chars", inspector::predicted_paths(&pr.claude).len(), pr.nexus.appended_chars);
            }
            match &r.observed.session {
                Some(s) => {
                    println!("observed in “{}”: {} instruction file(s){}", s.title, r.observed.instructions.len(), if r.observed.instructions_observed { "" } else { " (not observed in that session)" });
                    for f in &r.observed.instructions {
                        println!("  {} {} ({}, {})", if f.predicted { "✓" } else { "+" }, f.path.display(), f.memory_type, f.load_reason);
                    }
                    for m in &r.observed.missing {
                        println!("  − {} predicted but not reported", m.display());
                    }
                }
                None => println!("observed: no session of this project yet"),
            }
            println!("on demand: {} knowledge document(s), {} project doc(s), {} constitution rule(s) by id", r.on_demand.knowledge.decisions + r.on_demand.knowledge.notes + r.on_demand.knowledge.specs + r.on_demand.knowledge.open_tasks, r.on_demand.project_docs, r.on_demand.constitution_rules.len());
            println!("retrieved in the latest session: {} call(s); last 20 sessions: {} retrievals, {} empty", r.retrieved.len(), r.metrics.retrievals, r.metrics.empty);
        }
    }
    Ok(())
}

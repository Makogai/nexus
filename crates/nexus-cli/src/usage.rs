//! `nexus usage …`: local token usage from Claude Code's session transcripts (ADR-0030). Measured counts, estimates
//! and unavailable values are labelled as such; nothing leaves the machine and telemetry settings are never changed.

use clap::{Args, Subcommand};

use nexus_core::claude::policy::MachinePolicyLocations;
use nexus_core::usage::report::{self, Dimension, Filter};
use nexus_core::{CoreError, Result, now_ms};

use crate::knowledge::{find_id, find_project};
use crate::{Ctx, active_workspace, out};

#[derive(Args)]
pub struct UsageArgs {
    #[command(subcommand)]
    cmd: Option<UsageSub>,
    #[command(flatten)]
    filter: FilterArgs,
    /// Breakdown: project, profile, model, strategy, rigor, task, workflow.
    #[arg(long, default_value = "project")]
    by: String,
}

#[derive(Subcommand)]
enum UsageSub {
    /// Usage per Claude session (with its coverage status).
    Sessions {
        #[command(flatten)]
        filter: FilterArgs,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Read new transcript data now (also done before every report).
    Scan,
}

#[derive(Args, Clone, Default)]
struct FilterArgs {
    /// Period in days (0 = everything).
    #[arg(long, default_value_t = 30)]
    days: u32,
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    task: Option<String>,
    /// Workflow run id (or its last characters).
    #[arg(long)]
    workflow: Option<String>,
    /// Context strategy recorded at launch (minimal, balanced, deep, custom, unrecorded).
    #[arg(long)]
    strategy: Option<String>,
    #[arg(long)]
    model: Option<String>,
}

fn filter(conn: &nexus_core::db::Connection, a: &FilterArgs) -> Result<Filter> {
    let ws = active_workspace(conn)?;
    Ok(Filter {
        from: (a.days > 0).then(|| now_ms() - i64::from(a.days) * 86_400_000),
        to: None,
        workspace_id: Some(ws.id.clone()),
        project_id: a.project.as_deref().map(|q| find_project(conn, &ws, q).map(|p| p.id)).transpose()?,
        profile_id: a.profile.as_deref().map(|q| crate::launch::find_profile(conn, Some(q)).map(|p| p.id)).transpose()?,
        task_id: a.task.as_deref().map(|q| find_id(conn, &ws, q)).transpose()?,
        workflow_run_id: a.workflow.as_deref().map(|q| crate::workflow::find_run(conn, &ws.id, q)).transpose()?,
        strategy: a.strategy.clone(),
        work_profile: None,
        model: a.model.clone(),
    })
}

fn num(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

fn opt(v: Option<f64>, f: impl Fn(f64) -> String) -> String {
    v.map(f).unwrap_or_else(|| "unavailable".into())
}

fn pct(v: f64) -> String {
    format!("{:.0}%", v * 100.0)
}

fn usd(c: &report::CostEstimate) -> String {
    match c.usd {
        Some(v) => format!("≈ ${v:.2}{}", if c.incomplete { " (incomplete)" } else { "" }),
        None if c.withheld_subscription > 0 => "not shown (subscription)".into(),
        None if c.withheld_unknown > 0 => "billing unknown".into(),
        None => "unavailable".into(),
    }
}

pub fn run(ctx: &Ctx, a: UsageArgs) -> Result<i32> {
    let conn = ctx.db()?;
    let scanned = nexus_core::usage::scan_due(&conn, &ctx.home, 200)?;
    match a.cmd {
        Some(UsageSub::Scan) => {
            if ctx.json {
                out::json(&serde_json::json!({ "sessions": scanned.sessions, "responses": scanned.messages }));
            } else {
                println!("scanned {} session(s), {} new API response(s)", scanned.sessions, scanned.messages);
            }
        }
        Some(UsageSub::Sessions { filter: fa, limit }) => {
            let f = filter(&conn, &fa)?;
            let rows = report::sessions(&conn, &f, limit)?;
            if ctx.json {
                out::json(&rows);
                return Ok(0);
            }
            let table: Vec<Vec<String>> = rows
                .iter()
                .map(|s| {
                    let measured = if s.status == "measured" { num(s.tokens.total()) } else { "–".into() };
                    vec![
                        out::short_id(&s.session_id),
                        out::ago(Some(s.started_at)),
                        s.project.clone().unwrap_or_else(|| "–".into()),
                        s.profile.clone().unwrap_or_else(|| "–".into()),
                        s.strategy.clone(),
                        match s.status.as_str() {
                            "measured" if s.live => "measured (live)".into(),
                            st => st.to_string(),
                        },
                        num(s.tokens.requests),
                        measured,
                        usd(&s.cost),
                        s.context_estimate.map(|e| format!("≈ {}", num(e as i64))).unwrap_or_else(|| "–".into()),
                    ]
                })
                .collect();
            out::table(&["SESSION", "STARTED", "PROJECT", "PROFILE", "STRATEGY", "USAGE", "REQUESTS", "TOKENS (PARSED)", "COST (CLAUDE EST.)", "NEXUS CONTEXT (EST.)"], &table);
            for s in rows.iter().filter(|s| s.status != "measured").take(5) {
                if let Some(d) = &s.detail {
                    println!("{}: {d}", out::short_id(&s.session_id));
                }
            }
        }
        None => {
            let by = Dimension::parse(&a.by).ok_or_else(|| CoreError::invalid(format!("unknown breakdown '{}' — project, profile, model, strategy, rigor, task, workflow", a.by)))?;
            let f = filter(&conn, &a.filter)?;
            let r = report::report(&conn, &f, by)?;
            let telemetry = report::telemetry(&conn, &ctx.home, &MachinePolicyLocations::system())?;
            if ctx.json {
                out::json(&serde_json::json!({ "report": r, "telemetry": telemetry }));
                return Ok(0);
            }
            let t = &r.totals;
            println!(
                "{} — token counts reported by the Claude API, parsed from Claude Code's local session transcripts{}",
                if a.filter.days == 0 { "All time".to_string() } else { format!("Last {} days", a.filter.days) },
                if r.format_verified { "" } else { " (transcript format of this Claude Code version not verified — values may be inaccurate)" }
            );
            println!(
                "  tokens   {} total · input {} · output {} · cache write {} · cache read {}",
                num(t.total()),
                num(t.input),
                num(t.output),
                num(t.cache_write),
                num(t.cache_read)
            );
            println!(
                "  requests {} in {} session(s){} · context/request {} · cache hit {}",
                num(t.requests),
                r.sessions,
                if t.subagent_requests > 0 { format!(" ({} by subagents)", num(t.subagent_requests)) } else { String::new() },
                opt(r.efficiency.context_per_request, |v| num(v.round() as i64)),
                opt(r.efficiency.cache_hit, pct)
            );
            println!("  estimate cost {} (Claude Code's own estimate at API prices, API-billed profiles only — not a bill)", usd(&r.cost));
            if r.cost.withheld_subscription > 0 {
                println!("           {} session(s) on a subscription are not converted into spend", r.cost.withheld_subscription);
            }
            if r.cost.withheld_unknown > 0 {
                println!("           {} session(s) of profiles with unknown billing — `nexus profiles status <profile>` checks it", r.cost.withheld_unknown);
            }
            if let Some(e) = r.context_estimate.tokens_per_session {
                println!(
                    "  estimate NEXUS-added context ≈ {} tokens/session{} (characters ÷ 4)",
                    num(e as i64),
                    r.context_estimate.share_of_context.map(|s| format!(", ≈ {} of each request's context", pct(s))).unwrap_or_default()
                );
            }
            let c = &r.coverage;
            println!(
                "  coverage {} of {} Claude session(s) measured · {} without API requests · {} pending · {} unavailable{}",
                c.measured,
                c.sessions,
                c.no_requests,
                c.pending,
                c.missing + c.unreadable,
                if c.outside_nexus_requests > 0 { format!(" · {} response(s) from outside NEXUS not attributed", c.outside_nexus_requests) } else { String::new() }
            );
            println!();
            let rows: Vec<Vec<String>> = r
                .groups
                .iter()
                .map(|g| {
                    vec![
                        g.label.clone(),
                        g.sessions.to_string(),
                        num(g.tokens.requests),
                        num(g.tokens.total()),
                        opt(g.efficiency.tokens_per_session, |v| num(v.round() as i64)),
                        opt(g.efficiency.context_per_request, |v| num(v.round() as i64)),
                        opt(g.efficiency.cache_hit, pct),
                        usd(&g.cost),
                    ]
                })
                .collect();
            out::table(&[&a.by.to_uppercase(), "SESSIONS", "REQUESTS", "TOKENS", "PER SESSION", "CONTEXT/REQUEST", "CACHE HIT", "COST (EST.)"], &rows);
            for n in telemetry.iter().filter(|n| n.managed_telemetry) {
                println!("note: telemetry for profile {} is configured by the organisation — NEXUS leaves it unchanged.", n.profile);
            }
        }
    }
    Ok(0)
}

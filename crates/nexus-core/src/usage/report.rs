//! Usage reports (ADR-0030): totals, daily trend, breakdowns with efficiency figures, coverage and sessions.
//!
//! Only responses attributed to a NEXUS session are reported per project, profile, task, workflow and context
//! strategy (the associations come from the session row and its launch record). Responses of the same
//! conversations from outside NEXUS are counted separately.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, ToSql};
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Filter {
    /// Responses at or after this time (ms); sessions overlapping the period.
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    pub profile_id: Option<String>,
    pub task_id: Option<String>,
    pub workflow_run_id: Option<String>,
    /// Context strategy recorded at launch (`unrecorded` for sessions before ADR-0028).
    pub strategy: Option<String>,
    pub work_profile: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Dimension {
    #[default]
    Project,
    Profile,
    Model,
    Strategy,
    WorkProfile,
    Task,
    Workflow,
}

impl Dimension {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "project" => Self::Project,
            "profile" => Self::Profile,
            "model" => Self::Model,
            "strategy" | "context" => Self::Strategy,
            "work-profile" | "rigor" => Self::WorkProfile,
            "task" => Self::Task,
            "workflow" => Self::Workflow,
            _ => return None,
        })
    }
    fn key(self) -> &'static str {
        match self {
            Self::Project => "s.project_id",
            Self::Profile => "s.profile_id",
            Self::Model => "m.model",
            Self::Strategy => "COALESCE(json_extract(s.integrations, '$.context.strategy'), 'unrecorded')",
            Self::WorkProfile => "COALESCE(s.work_profile, 'none')",
            Self::Task => "s.task_id",
            Self::Workflow => "json_extract(s.integrations, '$.workflow.runId')",
        }
    }
    /// Session-level dimensions (costs and estimates are per session, not per model).
    fn per_session(self) -> bool {
        self != Self::Model
    }
}

/// Measured token counts (as reported by the Claude API, recorded by Claude Code).
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    /// API responses.
    pub requests: i64,
    pub input: i64,
    pub output: i64,
    pub cache_write: i64,
    pub cache_read: i64,
    pub web_searches: i64,
    /// Responses produced by subagents.
    pub subagent_requests: i64,
}

impl Tokens {
    pub fn total(&self) -> i64 {
        self.input + self.output + self.cache_write + self.cache_read
    }
    /// Tokens sent as context (uncached input, cache writes and cache reads).
    pub fn context(&self) -> i64 {
        self.input + self.cache_write + self.cache_read
    }
}

/// Derived from measured counts.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Efficiency {
    pub tokens_per_session: Option<f64>,
    pub context_per_request: Option<f64>,
    pub output_per_request: Option<f64>,
    /// Share of context served from the prompt cache.
    pub cache_hit: Option<f64>,
}

fn ratio(a: i64, b: i64) -> Option<f64> {
    (b > 0).then(|| a as f64 / b as f64)
}

fn efficiency(t: &Tokens, sessions: i64) -> Efficiency {
    Efficiency {
        tokens_per_session: ratio(t.total(), sessions),
        context_per_request: ratio(t.context(), t.requests),
        output_per_request: ratio(t.output, t.requests),
        cache_hit: ratio(t.cache_read, t.context()),
    }
}

/// Claude Code's own cost estimate (API list prices; never a bill). Only sessions of profiles billed per API use count:
/// subscription usage is never converted into spend (ADR-0010).
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimate {
    /// `None` = unavailable (no cost snapshot of an API-billed session, or not attributable, e.g. per model).
    pub usd: Option<f64>,
    /// Claude Code reported a model without a known price.
    pub incomplete: bool,
    /// Sessions with a cost snapshot left out because their profile uses a subscription.
    pub withheld_subscription: i64,
    /// … or because the profile's billing is not known yet (check its sign-in under Claude profiles).
    pub withheld_unknown: i64,
}

/// NEXUS's estimate of what it added to sessions itself (appended system prompt, SessionStart context).
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextEstimate {
    pub sessions: i64,
    /// Average estimated tokens per session (characters ÷ 4).
    pub tokens_per_session: Option<f64>,
    /// Estimated share of each request's context that NEXUS added.
    pub share_of_context: Option<f64>,
}

pub const CHARS_PER_TOKEN: f64 = 4.0;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub key: String,
    pub label: String,
    pub sessions: i64,
    pub tokens: Tokens,
    pub efficiency: Efficiency,
    pub cost: CostEstimate,
    pub context_estimate: ContextEstimate,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    /// Local calendar day, `YYYY-MM-DD`.
    pub day: String,
    pub tokens: Tokens,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    /// Claude sessions in the period (matching the filter).
    pub sessions: i64,
    pub measured: i64,
    pub no_requests: i64,
    pub pending: i64,
    pub missing: i64,
    pub unreadable: i64,
    /// Responses in the same conversations from before NEXUS (not in any breakdown).
    pub outside_nexus_requests: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub totals: Tokens,
    pub sessions: i64,
    pub efficiency: Efficiency,
    pub cost: CostEstimate,
    pub context_estimate: ContextEstimate,
    pub coverage: Coverage,
    pub daily: Vec<Day>,
    pub by: Dimension,
    pub groups: Vec<Group>,
    /// Claude Code versions that wrote the transcripts read.
    pub claude_versions: Vec<String>,
    /// All of them use a transcript format NEXUS has verified (otherwise parsed values may be inaccurate).
    pub format_verified: bool,
    /// Provenance of the token counts (ADR-0010): `parsed` — API-reported, read from Claude Code's transcripts.
    pub token_provenance: &'static str,
}

struct Where {
    sql: String,
    params: Vec<Box<dyn ToSql>>,
}

impl Where {
    fn new() -> Self {
        Self { sql: String::new(), params: Vec::new() }
    }
    fn add(&mut self, clause: &str, v: impl ToSql + 'static) {
        let mut sql = std::mem::take(&mut self.sql);
        self.add_into(&mut sql, clause, v);
        self.sql = sql;
    }
    /// Add a parameter whose condition goes into another SQL fragment (numbered placeholders stay unique).
    fn add_into(&mut self, buf: &mut String, clause: &str, v: impl ToSql + 'static) {
        self.params.push(Box::new(v));
        let n = self.params.len();
        buf.push_str(&format!(" AND {}", clause.replace('?', &format!("?{n}"))));
    }
    fn refs(&self) -> Vec<&dyn ToSql> {
        self.params.iter().map(|b| b.as_ref()).collect()
    }
}

/// Session-level conditions (on `s`).
fn session_conditions(w: &mut Where, f: &Filter) {
    if let Some(v) = &f.workspace_id {
        w.add("s.workspace_id = ?", v.clone());
    }
    if let Some(v) = &f.project_id {
        w.add("s.project_id = ?", v.clone());
    }
    if let Some(v) = &f.profile_id {
        w.add("s.profile_id = ?", v.clone());
    }
    if let Some(v) = &f.task_id {
        w.add("s.task_id = ?", v.clone());
    }
    if let Some(v) = &f.workflow_run_id {
        w.add("json_extract(s.integrations, '$.workflow.runId') = ?", v.clone());
    }
    if let Some(v) = &f.strategy {
        w.add(&format!("{} = ?", Dimension::Strategy.key()), v.clone());
    }
    if let Some(v) = &f.work_profile {
        w.add(&format!("{} = ?", Dimension::WorkProfile.key()), v.clone());
    }
}

fn message_where(f: &Filter) -> Where {
    let mut w = Where::new();
    if let Some(v) = f.from {
        w.add("m.ts >= ?", v);
    }
    if let Some(v) = f.to {
        w.add("m.ts < ?", v);
    }
    if let Some(v) = &f.model {
        w.add("m.model = ?", v.clone());
    }
    session_conditions(&mut w, f);
    w
}

fn session_where(f: &Filter) -> Where {
    let mut w = Where::new();
    if let Some(v) = f.from {
        w.add("COALESCE(s.ended_at, s.last_activity_at, s.started_at, 0) >= ?", v);
    }
    if let Some(v) = f.to {
        w.add("s.started_at < ?", v);
    }
    session_conditions(&mut w, f);
    w
}

fn cost_where(f: &Filter) -> Where {
    let mut w = Where::new();
    if let Some(v) = f.from {
        w.add("c.ts >= ?", v);
    }
    if let Some(v) = f.to {
        w.add("c.ts < ?", v);
    }
    session_conditions(&mut w, f);
    w
}

const COST_FROM: &str = "FROM usage_costs c JOIN sessions s ON s.id = c.session_id LEFT JOIN profile_billing b ON b.profile_id = s.profile_id WHERE 1 = 1";
/// Cost of API-billed sessions only, and how many sessions were left out (subscription / unknown billing).
const COST_COLUMNS: &str = "SUM(CASE WHEN b.kind = 'api' THEN c.cost_usd END),
    COALESCE(MAX(CASE WHEN b.kind = 'api' THEN c.unknown_model END), 0),
    COUNT(DISTINCT CASE WHEN b.kind = 'subscription' THEN c.session_id END),
    COUNT(DISTINCT CASE WHEN b.kind IS NULL OR b.kind = 'unknown' THEN c.session_id END)";

fn cost_at(i: usize) -> impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<CostEstimate> {
    move |r| {
        Ok(CostEstimate {
            usd: r.get(i)?,
            incomplete: r.get::<_, i64>(i + 1)? == 1,
            withheld_subscription: r.get(i + 2)?,
            withheld_unknown: r.get(i + 3)?,
        })
    }
}

const SUMS: &str = "COUNT(*), COALESCE(SUM(m.input_tokens), 0), COALESCE(SUM(m.output_tokens), 0), COALESCE(SUM(m.cache_write_tokens), 0),
    COALESCE(SUM(m.cache_read_tokens), 0), COALESCE(SUM(m.web_searches), 0), COALESCE(SUM(m.subagent), 0)";

fn tokens_at(r: &rusqlite::Row<'_>, i: usize) -> rusqlite::Result<Tokens> {
    Ok(Tokens {
        requests: r.get(i)?,
        input: r.get(i + 1)?,
        output: r.get(i + 2)?,
        cache_write: r.get(i + 3)?,
        cache_read: r.get(i + 4)?,
        web_searches: r.get(i + 5)?,
        subagent_requests: r.get(i + 6)?,
    })
}

/// (group key, appended prompt chars, work profile, SessionStart chars) of one session.
type SessionRecord = (Option<String>, Option<i64>, Option<String>, Option<i64>);

/// Per-session estimate of the context NEXUS added (tokens), from launch and hook records.
fn session_estimate(prompt_chars: Option<i64>, work_profile: Option<&str>, start_chars: Option<i64>) -> Option<f64> {
    let wp = work_profile.and_then(crate::workprofile::get).map(|w| w.prompt.chars().count() as i64);
    let chars = prompt_chars.unwrap_or(0) + wp.unwrap_or(0) + start_chars.unwrap_or(0);
    (prompt_chars.is_some() || wp.is_some() || start_chars.is_some()).then(|| (chars as f64 / CHARS_PER_TOKEN).ceil())
}

fn estimate_of(per_session: &[f64], t: &Tokens) -> ContextEstimate {
    let n = per_session.len() as i64;
    let avg = (n > 0).then(|| per_session.iter().sum::<f64>() / n as f64);
    let per_request = ratio(t.context(), t.requests);
    ContextEstimate { sessions: n, tokens_per_session: avg, share_of_context: avg.zip(per_request).filter(|(_, r)| *r > 0.0).map(|(a, r)| (a / r).min(1.0)) }
}

pub fn report(conn: &Connection, f: &Filter, by: Dimension) -> Result<Report> {
    let w = message_where(f);
    let from = "FROM usage_messages m JOIN sessions s ON s.id = m.session_id WHERE 1 = 1";
    let (totals, sessions): (Tokens, i64) = conn.query_row(&format!("SELECT {SUMS}, COUNT(DISTINCT m.session_id) {from}{}", w.sql), w.refs().as_slice(), |r| {
        Ok((tokens_at(r, 0)?, r.get(7)?))
    })?;
    let daily: Vec<Day> = conn
        .prepare(&format!("SELECT date(m.ts / 1000, 'unixepoch', 'localtime') AS d, {SUMS} {from}{} GROUP BY d ORDER BY d", w.sql))?
        .query_map(w.refs().as_slice(), |r| Ok(Day { day: r.get(0)?, tokens: tokens_at(r, 1)? }))?
        .collect::<std::result::Result<_, _>>()?;

    // Session-level records for estimates (and the group each session belongs to).
    let key = by.key();
    let per_session: Vec<SessionRecord> = conn
        .prepare(&format!(
            "SELECT DISTINCT s.id, {}, json_extract(s.integrations, '$.promptChars'), s.work_profile, s.session_start_chars {from}{}",
            if by.per_session() { key } else { "NULL" },
            w.sql
        ))?
        .query_map(w.refs().as_slice(), |r| Ok((r.get::<_, Option<String>>(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut est_all = Vec::new();
    let mut est_by: HashMap<String, Vec<f64>> = HashMap::new();
    for (k, pc, wp, sc) in &per_session {
        if let Some(e) = session_estimate(*pc, wp.as_deref(), *sc) {
            est_all.push(e);
            est_by.entry(k.clone().unwrap_or_default()).or_default().push(e);
        }
    }

    // Costs: per session, so not available per model or with a model filter.
    let costs_ok = f.model.is_none();
    let cw = cost_where(f);
    let cost = if costs_ok {
        conn.query_row(&format!("SELECT {COST_COLUMNS} {COST_FROM}{}", cw.sql), cw.refs().as_slice(), cost_at(0))?
    } else {
        CostEstimate::default()
    };
    let mut cost_by: HashMap<String, CostEstimate> = HashMap::new();
    if costs_ok && by.per_session() {
        let mut st = conn.prepare(&format!("SELECT {key} AS k, {COST_COLUMNS} {COST_FROM}{} GROUP BY k", cw.sql))?;
        let rows = st.query_map(cw.refs().as_slice(), |r| Ok((r.get::<_, Option<String>>(0)?, cost_at(1)(r)?)))?;
        for row in rows {
            let (k, c) = row?;
            cost_by.insert(k.unwrap_or_default(), c);
        }
    }

    let mut groups: Vec<Group> = conn
        .prepare(&format!("SELECT {key} AS k, {SUMS}, COUNT(DISTINCT m.session_id) {from}{} GROUP BY k", w.sql))?
        .query_map(w.refs().as_slice(), |r| Ok((r.get::<_, Option<String>>(0)?, tokens_at(r, 1)?, r.get::<_, i64>(8)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(k, tokens, sessions)| {
            let k = k.unwrap_or_default();
            let est = est_by.get(&k).map(Vec::as_slice).unwrap_or(&[]);
            Group {
                label: String::new(),
                sessions,
                efficiency: efficiency(&tokens, sessions),
                cost: cost_by.get(&k).copied().unwrap_or_default(),
                context_estimate: if by.per_session() { estimate_of(est, &tokens) } else { ContextEstimate::default() },
                tokens,
                key: k,
            }
        })
        .collect();
    groups.sort_by(|a, b| b.tokens.total().cmp(&a.tokens.total()).then_with(|| a.key.cmp(&b.key)));
    groups.truncate(100);
    for g in &mut groups {
        g.label = label(conn, by, &g.key)?;
    }

    let claude_versions: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT u.claude_version FROM usage_sources u WHERE u.claude_version IS NOT NULL
             AND EXISTS (SELECT 1 FROM usage_messages m WHERE m.profile_id = u.profile_id AND m.claude_session_id = u.claude_session_id)
             ORDER BY u.claude_version DESC LIMIT 5",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;

    let format_verified = claude_versions.iter().all(|v| crate::usage::transcript::format_verified(v));
    Ok(Report {
        format_verified,
        token_provenance: "parsed",
        efficiency: efficiency(&totals, sessions),
        context_estimate: estimate_of(&est_all, &totals),
        coverage: coverage(conn, f)?,
        totals,
        sessions,
        cost,
        daily,
        by,
        groups,
        claude_versions,
    })
}

fn coverage(conn: &Connection, f: &Filter) -> Result<Coverage> {
    let w = session_where(f);
    let mut c = Coverage::default();
    let mut st = conn.prepare(&format!(
        "SELECT COALESCE(u.status, 'pending'), COUNT(*) FROM sessions s LEFT JOIN usage_state u ON u.session_id = s.id
         WHERE s.kind = 'claude' AND COALESCE(s.agent, 'claude') = 'claude'{} GROUP BY 1",
        w.sql
    ))?;
    let rows = st.query_map(w.refs().as_slice(), |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (status, n) = row?;
        c.sessions += n;
        match status.as_str() {
            "measured" => c.measured += n,
            "no-requests" => c.no_requests += n,
            "missing" => c.missing += n,
            "unreadable" => c.unreadable += n,
            _ => c.pending += n,
        }
    }
    // Outside NEXUS: only meaningful without session-level filters other than the profile.
    let session_filtered = f.workspace_id.is_some() || f.project_id.is_some() || f.task_id.is_some() || f.workflow_run_id.is_some() || f.strategy.is_some() || f.work_profile.is_some();
    if !session_filtered {
        let mut w = Where::new();
        if let Some(v) = f.from {
            w.add("m.ts >= ?", v);
        }
        if let Some(v) = f.to {
            w.add("m.ts < ?", v);
        }
        if let Some(v) = &f.profile_id {
            w.add("m.profile_id = ?", v.clone());
        }
        if let Some(v) = &f.model {
            w.add("m.model = ?", v.clone());
        }
        c.outside_nexus_requests = conn.query_row(&format!("SELECT COUNT(*) FROM usage_messages m WHERE m.session_id IS NULL{}", w.sql), w.refs().as_slice(), |r| r.get(0))?;
    }
    Ok(c)
}

/// Display label of a group key.
pub fn label(conn: &Connection, by: Dimension, key: &str) -> Result<String> {
    if key.is_empty() {
        return Ok(match by {
            Dimension::Project => "No project",
            Dimension::Profile => "No profile",
            Dimension::Task => "No task",
            Dimension::Workflow => "No workflow",
            _ => "–",
        }
        .into());
    }
    let one = |sql: &str| -> Result<Option<String>> { Ok(conn.query_row(sql, [key], |r| r.get(0)).optional()?) };
    Ok(match by {
        Dimension::Project => one("SELECT name FROM projects WHERE id = ?1")?,
        Dimension::Profile => one("SELECT name FROM claude_profiles WHERE id = ?1")?,
        Dimension::Task => one("SELECT title FROM knowledge_items WHERE id = ?1 AND kind = 'task' AND title <> '' LIMIT 1")?,
        Dimension::Workflow => one("SELECT workflow_title || CASE WHEN goal <> '' THEN ' — ' || goal ELSE '' END FROM workflow_runs WHERE id = ?1")?,
        Dimension::Strategy => Some(match key {
            "unrecorded" => "Not recorded".to_string(),
            k => {
                let mut c = k.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            }
        }),
        Dimension::WorkProfile => Some(crate::workprofile::get(key).map(|w| w.name.to_string()).unwrap_or_else(|| key.to_string())),
        Dimension::Model => Some(key.to_string()),
    }
    .map(|l| l.chars().take(160).collect())
    .unwrap_or_else(|| format!("{key} (removed)")))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
    pub session_id: String,
    pub title: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub live: bool,
    pub project_id: Option<String>,
    pub project: Option<String>,
    pub profile_id: Option<String>,
    pub profile: Option<String>,
    pub task_id: Option<String>,
    pub task: Option<String>,
    pub workflow_run_id: Option<String>,
    pub workflow: Option<String>,
    pub strategy: String,
    pub work_profile: Option<String>,
    /// measured | no-requests | pending | missing | unreadable
    pub status: String,
    pub detail: Option<String>,
    pub tokens: Tokens,
    pub cost: CostEstimate,
    /// Estimated tokens NEXUS added to this session (characters ÷ 4).
    pub context_estimate: Option<f64>,
}

pub fn sessions(conn: &Connection, f: &Filter, limit: usize) -> Result<Vec<SessionUsage>> {
    let mut w = session_where(f);
    // Token sums of the period (and model) per session.
    let mut inner_sql = String::new();
    if let Some(v) = f.from {
        w.add_into(&mut inner_sql, "ts >= ?", v);
    }
    if let Some(v) = f.to {
        w.add_into(&mut inner_sql, "ts < ?", v);
    }
    if let Some(v) = &f.model {
        w.add_into(&mut inner_sql, "model = ?", v.clone());
    }
    let sql = format!(
        "SELECT s.id, s.title, s.started_at, s.ended_at, s.state, s.project_id, s.profile_id, s.task_id,
                json_extract(s.integrations, '$.workflow.runId'), {strategy}, s.work_profile,
                COALESCE(u.status, 'pending'), u.detail,
                COALESCE(a.n, 0), COALESCE(a.inp, 0), COALESCE(a.outp, 0), COALESCE(a.cw, 0), COALESCE(a.cr, 0), COALESCE(a.web, 0), COALESCE(a.sub, 0),
                (SELECT SUM(cost_usd) FROM usage_costs c WHERE c.session_id = s.id), (SELECT COALESCE(MAX(unknown_model), 0) FROM usage_costs c WHERE c.session_id = s.id),
                (SELECT kind FROM profile_billing b WHERE b.profile_id = s.profile_id), (SELECT COUNT(*) FROM usage_costs c WHERE c.session_id = s.id),
                json_extract(s.integrations, '$.promptChars'), s.session_start_chars
         FROM sessions s
         LEFT JOIN usage_state u ON u.session_id = s.id
         LEFT JOIN (SELECT session_id, COUNT(*) n, SUM(input_tokens) inp, SUM(output_tokens) outp, SUM(cache_write_tokens) cw,
                           SUM(cache_read_tokens) cr, SUM(web_searches) web, SUM(subagent) sub
                    FROM usage_messages WHERE session_id IS NOT NULL{inner_sql} GROUP BY session_id) a ON a.session_id = s.id
         WHERE s.kind = 'claude' AND COALESCE(s.agent, 'claude') = 'claude'{where_sql}
         ORDER BY s.started_at DESC LIMIT {limit}",
        strategy = Dimension::Strategy.key(),
        where_sql = w.sql,
        limit = limit.min(1000),
    );
    let params = w.refs();
    let model_filtered = f.model.is_some();
    let rows: Vec<SessionUsage> = conn
        .prepare(&sql)?
        .query_map(params.as_slice(), |r| {
            let state: String = r.get(4)?;
            let work_profile: Option<String> = r.get(10)?;
            Ok(SessionUsage {
                session_id: r.get(0)?,
                title: r.get(1)?,
                started_at: r.get(2)?,
                ended_at: r.get(3)?,
                live: matches!(state.as_str(), "starting" | "running"),
                project_id: r.get(5)?,
                project: None,
                profile_id: r.get(6)?,
                profile: None,
                task_id: r.get(7)?,
                task: None,
                workflow_run_id: r.get(8)?,
                workflow: None,
                strategy: r.get(9)?,
                status: r.get(11)?,
                detail: r.get(12)?,
                tokens: tokens_at(r, 13)?,
                cost: {
                    let billing: Option<String> = r.get(22)?;
                    let snapshots: i64 = r.get(23)?;
                    match (model_filtered, billing.as_deref()) {
                        (true, _) => CostEstimate::default(),
                        (false, Some("api")) => CostEstimate { usd: r.get(20)?, incomplete: r.get::<_, i64>(21)? == 1, ..Default::default() },
                        (false, Some("subscription")) => CostEstimate { withheld_subscription: i64::from(snapshots > 0), ..Default::default() },
                        (false, _) => CostEstimate { withheld_unknown: i64::from(snapshots > 0), ..Default::default() },
                    }
                },
                context_estimate: session_estimate(r.get(24)?, work_profile.as_deref(), r.get(25)?),
                work_profile,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut rows = rows;
    for s in &mut rows {
        s.project = s.project_id.as_deref().map(|k| label(conn, Dimension::Project, k)).transpose()?;
        s.profile = s.profile_id.as_deref().map(|k| label(conn, Dimension::Profile, k)).transpose()?;
        s.task = s.task_id.as_deref().map(|k| label(conn, Dimension::Task, k)).transpose()?;
        s.workflow = s.workflow_run_id.as_deref().map(|k| label(conn, Dimension::Workflow, k)).transpose()?;
    }
    Ok(rows)
}

/// Per profile: is telemetry configured by the organisation? Reported only — NEXUS never changes telemetry.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryNote {
    pub profile_id: String,
    pub profile: String,
    pub organization_managed: bool,
    pub managed_telemetry: bool,
}

pub fn telemetry(conn: &Connection, home: &Path, machine: &crate::claude::policy::MachinePolicyLocations) -> Result<Vec<TelemetryNote>> {
    Ok(crate::claude::profile::list(conn, false)?
        .into_iter()
        .map(|p| {
            let pol = crate::claude::policy::evaluate(&p.effective_config_dir(home), machine);
            TelemetryNote { profile_id: p.id, profile: p.name, organization_managed: pol.organization_managed, managed_telemetry: pol.managed_telemetry }
        })
        .collect())
}

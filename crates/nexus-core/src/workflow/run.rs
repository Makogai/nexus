//! Workflow runs (ADR-0029): a machine-local state machine over a definition snapshot, carried by an ordinary
//! knowledge task (portable progress: status, checklist, log). Claude works in a normal task session and reports
//! steps through MCP (**AI-reported**); NEXUS verifies with quality gates in a host process (**verified**); review
//! steps need a person (**human**). Every transition is SQL — safe from any NEXUS process.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::gates::{self, Gate, GateResult, Outcome, Status as GateStatus};
use super::{Definition, Step, StepKind, rigor_rank};
use crate::error::{CoreError, Result};
use crate::knowledge::service::{self, Author, CreateInput};
use crate::knowledge::tasks::{self, TaskUpdate, escape_untrusted};
use crate::notify::{self, Category, NewEvent, Priority};
use crate::workspace::Workspace;
use crate::{new_id, now_ms};

pub fn max_attempts(rigor: &str) -> i64 {
    match rigor {
        "prototype" | "balanced" => 3,
        _ => 2,
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StartRun {
    pub workflow_id: String,
    pub project_id: Option<String>,
    /// Run on an existing task (else a task is created for the run).
    pub task_id: Option<String>,
    pub goal: String,
    pub rigor: Option<String>,
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub workspace_id: String,
    pub project_id: Option<String>,
    pub workflow_id: String,
    pub workflow_title: String,
    pub workflow_hash: String,
    pub task_id: String,
    pub goal: String,
    pub rigor: String,
    pub profile_id: Option<String>,
    pub skills: Vec<String>,
    pub status: String,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub ended_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StepState {
    pub step_id: String,
    pub position: i64,
    pub kind: String,
    pub status: String,
    pub verification: Option<String>,
    pub attempts: i64,
    pub max_attempts: i64,
    pub report: Option<String>,
    pub criteria_met: Vec<usize>,
    pub last_failure: Option<String>,
    pub updated_at: i64,
    /// `gates` while the step waits for its gates to be defined or approved (it then resumes verification).
    pub blocked_on: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepView {
    #[serde(flatten)]
    pub state: StepState,
    pub title: String,
    pub text: String,
    pub needs: Vec<String>,
    pub gates: Vec<String>,
    pub criteria: Vec<String>,
    pub results: Vec<GateResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    #[serde(flatten)]
    pub run: Run,
    pub steps: Vec<StepView>,
    pub progress: Progress,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub total: usize,
    pub done: usize,
    pub verified: usize,
    pub ai_reported: usize,
    pub human: usize,
    pub skipped: usize,
}

const RUN: &str = "SELECT id, workspace_id, project_id, workflow_id, workflow_title, workflow_hash, task_id, goal, rigor, profile_id, skills,
    status, error, created_at, updated_at, ended_at FROM workflow_runs";

fn run_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    let skills: String = r.get(10)?;
    Ok(Run {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        project_id: r.get(2)?,
        workflow_id: r.get(3)?,
        workflow_title: r.get(4)?,
        workflow_hash: r.get(5)?,
        task_id: r.get(6)?,
        goal: r.get(7)?,
        rigor: r.get(8)?,
        profile_id: r.get(9)?,
        skills: serde_json::from_str(&skills).unwrap_or_default(),
        status: r.get(11)?,
        error: r.get(12)?,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
        ended_at: r.get(15)?,
    })
}

const STEP: &str = "SELECT step_id, position, kind, status, verification, attempts, max_attempts, report, criteria_met, last_failure, updated_at, blocked_on FROM workflow_steps";

fn step_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<StepState> {
    let c: String = r.get(8)?;
    Ok(StepState {
        step_id: r.get(0)?,
        position: r.get(1)?,
        kind: r.get(2)?,
        status: r.get(3)?,
        verification: r.get(4)?,
        attempts: r.get(5)?,
        max_attempts: r.get(6)?,
        report: r.get(7)?,
        criteria_met: serde_json::from_str(&c).unwrap_or_default(),
        last_failure: r.get(9)?,
        updated_at: r.get(10)?,
        blocked_on: r.get(11)?,
    })
}

pub fn get_run(conn: &Connection, id: &str) -> Result<Run> {
    conn.query_row(&format!("{RUN} WHERE id = ?1"), [id], run_row).optional()?.ok_or_else(|| CoreError::not_found(format!("no workflow run {id}")))
}

fn definition(conn: &Connection, run_id: &str) -> Result<Definition> {
    let j: String = conn.query_row("SELECT definition FROM workflow_runs WHERE id = ?1", [run_id], |r| r.get(0))?;
    serde_json::from_str(&j).map_err(|source| CoreError::Json { what: "workflow snapshot".into(), source })
}

fn steps(conn: &Connection, run_id: &str) -> Result<Vec<StepState>> {
    Ok(conn.prepare(&format!("{STEP} WHERE run_id = ?1 ORDER BY rowid"))?.query_map([run_id], step_row)?.collect::<std::result::Result<_, _>>()?)
}

fn step_def<'a>(d: &'a Definition, id: &str) -> Result<&'a Step> {
    d.steps.iter().find(|s| s.id == id).ok_or_else(|| CoreError::not_found(format!("no step {id}")))
}

pub fn view(conn: &Connection, run_id: &str) -> Result<RunView> {
    let run = get_run(conn, run_id)?;
    let d = definition(conn, run_id)?;
    let mut progress = Progress::default();
    let mut out = Vec::new();
    for st in steps(conn, run_id)? {
        let def = step_def(&d, &st.step_id)?;
        progress.total += 1;
        match (st.status.as_str(), st.verification.as_deref()) {
            ("skipped", _) => progress.skipped += 1,
            ("passed", Some("verified")) => progress.verified += 1,
            ("passed", Some("human")) => progress.human += 1,
            ("passed", _) => progress.ai_reported += 1,
            _ => {}
        }
        if matches!(st.status.as_str(), "passed" | "skipped") {
            progress.done += 1;
        }
        out.push(StepView {
            results: gates::results_for_step(conn, run_id, &st.step_id)?,
            title: def.title.clone(),
            text: def.text.clone(),
            needs: def.needs.clone(),
            gates: def.gates.clone(),
            criteria: def.criteria.clone(),
            state: st,
        });
    }
    Ok(RunView { run, steps: out, progress })
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunFilter {
    pub project_id: Option<String>,
    pub active_only: bool,
    pub limit: Option<u32>,
}

pub fn list(conn: &Connection, workspace_id: &str, f: &RunFilter) -> Result<Vec<Run>> {
    let active = if f.active_only { " AND status IN ('running', 'waiting')" } else { "" };
    Ok(conn
        .prepare(&format!("{RUN} WHERE workspace_id = ?1 AND (?2 IS NULL OR project_id = ?2){active} ORDER BY created_at DESC LIMIT ?3"))?
        .query_map(params![workspace_id, f.project_id, f.limit.unwrap_or(100).min(500)], run_row)?
        .collect::<std::result::Result<_, _>>()?)
}

/// The active run carried by a task (one at a time).
pub fn active_for_task(conn: &Connection, task_id: &str) -> Result<Option<Run>> {
    Ok(conn.query_row(&format!("{RUN} WHERE task_id = ?1 AND status IN ('running', 'waiting') ORDER BY created_at DESC LIMIT 1"), [task_id], run_row).optional()?)
}

fn event(run: &Run, category: Category, title: String) -> NewEvent {
    let mut e = NewEvent::new(category, "workflow", title).target("workflow", run.id.clone());
    e.workspace_id = Some(run.workspace_id.clone());
    e.project_id = run.project_id.clone();
    e.profile_id = run.profile_id.clone();
    e.task_id = Some(run.task_id.clone());
    e.workflow_id = Some(run.id.clone());
    e
}

fn task_log(conn: &Connection, data_dir: &Path, ws: &Workspace, run: &Run, update: TaskUpdate<'_>, author: Author) {
    // Best effort: the task file is the portable record; the run state is authoritative for the engine.
    if let Err(e) = tasks::update(conn, data_dir, ws, &run.task_id, &update, author) {
        tracing::debug!(error = %e, "workflow task update skipped");
    }
}

// ---- Start ---------------------------------------------------------------------------------------------------------

pub fn start(conn: &Connection, data_dir: &Path, ws: &Workspace, input: &StartRun) -> Result<RunView> {
    let wf = super::get(conn, data_dir, ws, &input.workflow_id)?;
    if !wf.summary.approved {
        return Err(CoreError::invalid("this workflow changed outside NEXUS — review and approve it before running it"));
    }
    if let Some(e) = wf.problems.iter().find(|p| p.severity == "error") {
        return Err(CoreError::invalid(format!("the workflow has errors: {}", e.message)));
    }
    let rigor = input.rigor.clone().unwrap_or_else(|| wf.summary.rigor.clone());
    if !super::RIGOR.contains(&rigor.as_str()) {
        return Err(CoreError::invalid(format!("rigor must be one of {}", super::RIGOR.join(", "))));
    }
    let project_id = input.project_id.clone().or_else(|| wf.summary.project_id.clone());
    if let Some(pid) = &project_id
        && crate::project::get(conn, pid).map(|p| p.workspace_id != ws.id).unwrap_or(true)
    {
        return Err(CoreError::invalid("the project is not part of this workspace"));
    }
    let order = super::order(&wf.definition).ok_or_else(|| CoreError::invalid("the step dependencies form a cycle"))?;
    let included: Vec<&Step> = order
        .iter()
        .map(|id| step_def(&wf.definition, id))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|s| s.rigor.as_deref().is_none_or(|r| rigor_rank(r) <= rigor_rank(&rigor)))
        .collect();
    let (task_id, own_task) = match &input.task_id {
        Some(t) => {
            let v = tasks::view(conn, ws, t, 100)?;
            if let (Some(tp), Some(p)) = (&v.project_id, &project_id)
                && tp != p
            {
                return Err(CoreError::invalid("the task belongs to another project"));
            }
            if active_for_task(conn, &v.id)?.is_some() {
                return Err(CoreError::invalid("this task already has an active workflow run"));
            }
            (v.id, false)
        }
        None => {
            let goal = input.goal.trim();
            let title: String = if goal.is_empty() { wf.summary.title.clone() } else { format!("{}: {goal}", wf.summary.title) }.chars().take(180).collect();
            let mut body = String::new();
            if !goal.is_empty() {
                body.push_str(goal);
                body.push_str("\n\n");
            }
            body.push_str(&format!("Workflow “{}” at {rigor} rigor. Steps:\n", wf.summary.title));
            for s in &included {
                body.push_str(&format!("- [ ] {}\n", s.title));
            }
            let item = service::create(conn, data_dir, ws, &CreateInput { kind: "task".into(), title, body, project_id: project_id.clone(), labels: vec!["workflow".into()], ..Default::default() }, Author::Human)?;
            (item.id.unwrap_or_default(), true)
        }
    };
    let id = new_id();
    let now = now_ms();
    let max = max_attempts(&rigor);
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO workflow_runs(id, workspace_id, project_id, workflow_id, workflow_title, workflow_hash, definition, task_id, goal, rigor, profile_id, skills,
            status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'running', ?13, ?13)",
        params![
            id,
            ws.id,
            project_id,
            wf.summary.id,
            wf.summary.title,
            wf.summary.hash,
            serde_json::to_string(&wf.definition).map_err(|source| CoreError::Json { what: "workflow snapshot".into(), source })?,
            task_id,
            input.goal.trim().chars().take(2000).collect::<String>(),
            rigor,
            input.profile_id,
            serde_json::to_string(&wf.summary.skills).unwrap_or_else(|_| "[]".into()),
            now
        ],
    )?;
    let mut position = 0;
    for id_ in &order {
        let s = step_def(&wf.definition, id_)?;
        let include = included.iter().any(|x| x.id == s.id);
        if include {
            position += 1;
        }
        tx.execute(
            "INSERT INTO workflow_steps(run_id, step_id, position, kind, status, verification, max_attempts, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                s.id,
                if include && own_task { position } else { 0 },
                s.kind.as_str(),
                if include { "pending" } else { "skipped" },
                if include { None } else { Some("skipped") },
                max,
                now
            ],
        )?;
    }
    tx.commit()?;
    let run = get_run(conn, &id)?;
    task_log(conn, data_dir, ws, &run, TaskUpdate { log: Some(&format!("[nexus] workflow run {} started ({rigor})", service::short_id(&id))), ..Default::default() }, Author::Human);
    advance(conn, data_dir, ws, &id)?;
    view(conn, &id)
}

// ---- Transitions ---------------------------------------------------------------------------------------------------

fn set_step(conn: &Connection, run_id: &str, step_id: &str, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Result<()> {
    let mut all: Vec<&dyn rusqlite::ToSql> = vec![&run_id, &step_id];
    all.extend_from_slice(p);
    let now = now_ms();
    all.push(&now);
    let n = all.len();
    // Any other transition ends a wait for gates.
    let blocked = if sql.contains("blocked_on") { "" } else { ", blocked_on = NULL" };
    conn.execute(&format!("UPDATE workflow_steps SET {sql}{blocked}, updated_at = ?{n} WHERE run_id = ?1 AND step_id = ?2"), all.as_slice())?;
    Ok(())
}

fn set_run(conn: &Connection, run_id: &str, status: &str, error: Option<&str>) -> Result<()> {
    let ended = matches!(status, "completed" | "failed" | "cancelled").then(now_ms);
    conn.execute(
        "UPDATE workflow_runs SET status = ?2, error = ?3, updated_at = ?4, ended_at = COALESCE(?5, ended_at) WHERE id = ?1",
        params![run_id, status, error, now_ms(), ended],
    )?;
    Ok(())
}

fn active(run: &Run) -> bool {
    matches!(run.status.as_str(), "running" | "waiting")
}

/// Open steps whose dependencies are done; finish the run when everything is done or a step failed for good.
pub fn advance(conn: &Connection, data_dir: &Path, ws: &Workspace, run_id: &str) -> Result<()> {
    let run = get_run(conn, run_id)?;
    if !active(&run) {
        return Ok(());
    }
    let d = definition(conn, run_id)?;
    let states = steps(conn, run_id)?;
    let done = |id: &str| states.iter().any(|s| s.step_id == id && matches!(s.status.as_str(), "passed" | "skipped"));
    for st in states.iter().filter(|s| s.status == "pending") {
        let def = step_def(&d, &st.step_id)?;
        if !def.needs.iter().all(|n| done(n)) {
            continue;
        }
        match def.kind {
            StepKind::Claude => set_step(conn, run_id, &st.step_id, "status = 'ready'", &[])?,
            StepKind::Check => set_step(conn, run_id, &st.step_id, "status = 'verifying'", &[])?,
            StepKind::Review => {
                set_step(conn, run_id, &st.step_id, "status = 'awaiting-review'", &[])?;
                notify::publish(
                    conn,
                    event(&run, Category::ReviewRequired, format!("Review needed: {}", def.title))
                        .body(format!("Workflow “{}” waits for your sign-off.", run.workflow_title))
                        .priority(Priority::Action)
                        .dedupe(format!("wf-review:{run_id}:{}", st.step_id)),
                )?;
            }
        }
    }
    let states = steps(conn, run_id)?;
    if let Some(failed) = states.iter().find(|s| s.status == "failed") {
        let title = step_def(&d, &failed.step_id).map(|s| s.title.clone()).unwrap_or_default();
        let why = failed.last_failure.clone().unwrap_or_else(|| "no attempts left".into());
        set_run(conn, run_id, "failed", Some(&format!("{title}: {}", why.chars().take(300).collect::<String>())))?;
        notify::publish(
            conn,
            event(&run, Category::WorkflowFailed, format!("Workflow failed: {}", run.workflow_title))
                .body(format!("Step “{title}” failed after {} attempt(s). Retry it from the run.", failed.attempts))
                .dedupe(format!("wf-failed:{run_id}")),
        )?;
        task_log(conn, data_dir, ws, &run, TaskUpdate { status: Some("blocked"), log: Some(&format!("[nexus] workflow failed at “{title}”")), ..Default::default() }, Author::Human);
        return Ok(());
    }
    if states.iter().all(|s| matches!(s.status.as_str(), "passed" | "skipped")) {
        set_run(conn, run_id, "completed", None)?;
        let p = view(conn, run_id)?.progress;
        let summary = format!("{} verified, {} signed off, {} AI-reported only, {} skipped", p.verified, p.human, p.ai_reported, p.skipped);
        notify::publish(
            conn,
            event(&run, Category::WorkflowCompleted, format!("Workflow finished: {}", run.workflow_title)).body(summary.clone()).dedupe(format!("wf-done:{run_id}")),
        )?;
        notify::resolve_workflow(conn, run_id)?;
        task_log(conn, data_dir, ws, &run, TaskUpdate { status: Some("done"), log: Some(&format!("[nexus] workflow completed — {summary}")), ..Default::default() }, Author::Human);
        crate::checkpoint::add_for_task(conn, &run.task_id, crate::checkpoint::CheckpointKind::Done, &format!("Workflow “{}” completed: {summary}", run.workflow_title))?;
        return Ok(());
    }
    let waiting = !states.iter().any(|s| matches!(s.status.as_str(), "ready" | "verifying"));
    set_run(conn, run_id, if waiting { "waiting" } else { "running" }, None)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Reported {
    Done,
    Blocked,
    NeedsInput,
}

impl Reported {
    pub fn parse(s: &str) -> Option<Reported> {
        Some(match s {
            "done" => Reported::Done,
            "blocked" => Reported::Blocked,
            "needs-input" => Reported::NeedsInput,
            _ => return None,
        })
    }
}

/// Claude reports a step (MCP `report_step`). This is a *claim*: gates (if attached) verify it; at critical rigor a
/// step without gates also needs a person.
/// What Claude reports for a step.
pub struct StepReport<'a> {
    pub status: Reported,
    pub summary: &'a str,
    /// Acceptance criteria Claude says are met (1-based).
    pub criteria: &'a [usize],
}

pub fn report(conn: &Connection, data_dir: &Path, ws: &Workspace, run_id: &str, step_id: &str, r: &StepReport<'_>) -> Result<StepState> {
    let (status, summary, criteria) = (r.status, r.summary, r.criteria);
    let run = get_run(conn, run_id)?;
    if !active(&run) {
        return Err(CoreError::invalid(format!("the run is {}", run.status)));
    }
    let d = definition(conn, run_id)?;
    let def = step_def(&d, step_id)?;
    let st = steps(conn, run_id)?.into_iter().find(|s| s.step_id == step_id).ok_or_else(|| CoreError::not_found("step"))?;
    if def.kind != StepKind::Claude {
        return Err(CoreError::invalid(format!("step `{step_id}` is a {} step — NEXUS or a person completes it", def.kind.as_str())));
    }
    if !matches!(st.status.as_str(), "ready" | "blocked") {
        return Err(CoreError::invalid(format!("step `{step_id}` is {} — only ready or blocked steps can be reported", st.status)));
    }
    let summary: String = escape_untrusted(summary, 500);
    let criteria: Vec<usize> = criteria.iter().copied().filter(|c| *c >= 1 && *c <= def.criteria.len()).collect();
    let crit = serde_json::to_string(&criteria).unwrap_or_else(|_| "[]".into());
    match status {
        Reported::Done => {
            let (next, verification) = if !def.gates.is_empty() {
                ("verifying", None)
            } else if run.rigor == "critical" {
                ("awaiting-review", None)
            } else {
                ("passed", Some("ai-reported"))
            };
            set_step(conn, run_id, step_id, "status = ?3, verification = ?4, attempts = attempts + 1, report = ?5, criteria_met = ?6", &[&next, &verification, &summary, &crit])?;
            if next == "awaiting-review" {
                notify::publish(
                    conn,
                    event(&run, Category::ReviewRequired, format!("Sign-off needed: {}", def.title))
                        .body("Critical rigor: Claude reports this step done; no gate verifies it, so a person signs off.")
                        .priority(Priority::Action)
                        .dedupe(format!("wf-review:{run_id}:{step_id}")),
                )?;
            }
            let how = match next {
                "verifying" => "reported done — NEXUS verifies with its gates",
                "awaiting-review" => "reported done — waits for sign-off",
                _ => "done (AI-reported, not verified)",
            };
            let check: Vec<usize> = if next == "passed" && st.position > 0 { vec![st.position as usize] } else { vec![] };
            task_log(conn, data_dir, ws, &run, TaskUpdate { log: Some(&format!("“{}” {how}: {summary}", def.title)), check: &check, ..Default::default() }, Author::Claude);
        }
        Reported::Blocked | Reported::NeedsInput => {
            set_step(conn, run_id, step_id, "status = 'blocked', report = ?3, criteria_met = ?4", &[&summary, &crit])?;
            let (cat, title) = if status == Reported::NeedsInput {
                (Category::InputRequired, format!("Input needed: {}", def.title))
            } else {
                (Category::TaskBlocked, format!("Workflow step blocked: {}", def.title))
            };
            notify::publish(conn, event(&run, cat, title).body(summary.clone()).priority(Priority::Action).dedupe(format!("wf-blocked:{run_id}:{step_id}")))?;
            task_log(conn, data_dir, ws, &run, TaskUpdate { log: Some(&format!("“{}” blocked: {summary}", def.title)), ..Default::default() }, Author::Claude);
        }
    }
    advance(conn, data_dir, ws, run_id)?;
    steps(conn, run_id)?.into_iter().find(|s| s.step_id == step_id).ok_or_else(|| CoreError::not_found("step"))
}

/// A person approves or rejects a step waiting for review (review steps, critical sign-offs).
pub fn review(conn: &Connection, data_dir: &Path, ws: &Workspace, run_id: &str, step_id: &str, approve: bool, note: &str) -> Result<RunView> {
    let run = get_run(conn, run_id)?;
    if !active(&run) {
        return Err(CoreError::invalid(format!("the run is {}", run.status)));
    }
    let d = definition(conn, run_id)?;
    let def = step_def(&d, step_id)?;
    let all = steps(conn, run_id)?;
    let st = all.iter().find(|s| s.step_id == step_id).ok_or_else(|| CoreError::not_found("step"))?;
    if st.status != "awaiting-review" {
        return Err(CoreError::invalid(format!("step `{step_id}` is not waiting for a review")));
    }
    let note: String = escape_untrusted(note, 500);
    notify::resolve_dedupe(conn, &format!("wf-review:{run_id}:{step_id}"))?;
    if approve {
        set_step(conn, run_id, step_id, "status = 'passed', verification = 'human', report = COALESCE(NULLIF(?3, ''), report)", &[&note])?;
        let check: Vec<usize> = if st.position > 0 { vec![st.position as usize] } else { vec![] };
        task_log(conn, data_dir, ws, &run, TaskUpdate { log: Some(&format!("[nexus] “{}” approved{}", def.title, if note.is_empty() { String::new() } else { format!(": {note}") })), check: &check, ..Default::default() }, Author::Human);
    } else {
        // Changes requested: the Claude steps this one depends on (or the step itself) go back to Claude.
        let attempts = st.attempts + 1;
        let reopen: Vec<String> = if def.kind == StepKind::Review { def.needs.iter().filter(|n| all.iter().any(|s| &s.step_id == *n && s.kind == "claude")).cloned().collect() } else { vec![step_id.to_string()] };
        if attempts >= st.max_attempts || reopen.is_empty() {
            set_step(conn, run_id, step_id, "status = 'failed', attempts = ?3, last_failure = ?4", &[&attempts, &format!("rejected: {note}")])?;
        } else {
            let pending = if def.kind == StepKind::Review { "pending" } else { "ready" };
            set_step(conn, run_id, step_id, "status = ?3, attempts = ?4, last_failure = ?5", &[&pending, &attempts, &format!("rejected: {note}")])?;
            for r in &reopen {
                if r != step_id {
                    set_step(conn, run_id, r, "status = 'ready', verification = NULL, last_failure = ?3", &[&format!("review “{}” requested changes: {note}", def.title)])?;
                }
            }
        }
        task_log(conn, data_dir, ws, &run, TaskUpdate { log: Some(&format!("[nexus] “{}” rejected: {note}", def.title)), ..Default::default() }, Author::Human);
    }
    advance(conn, data_dir, ws, run_id)?;
    view(conn, run_id)
}

// ---- Verification (gates run in a host process) ---------------------------------------------------------------------

/// A step whose gates should run now.
#[derive(Debug, Clone)]
pub struct Pending {
    pub run_id: String,
    pub step_id: String,
    pub project_id: Option<String>,
}

/// Steps in `verifying` with no gate execution in progress.
pub fn pending_verifications(conn: &Connection) -> Result<Vec<Pending>> {
    Ok(conn
        .prepare(
            "SELECT s.run_id, s.step_id, r.project_id FROM workflow_steps s JOIN workflow_runs r ON r.id = s.run_id
             WHERE s.status = 'verifying' AND r.status IN ('running', 'waiting')
               AND NOT EXISTS (SELECT 1 FROM gate_results g WHERE g.run_id = s.run_id AND g.step_id = s.step_id AND g.status = 'running')
             ORDER BY s.updated_at LIMIT 20",
        )?
        .query_map([], |r| Ok(Pending { run_id: r.get(0)?, step_id: r.get(1)?, project_id: r.get(2)? }))?
        .collect::<std::result::Result<_, _>>()?)
}

/// Claimed work: the gates to execute (outside any database lock) and where.
pub struct Claim {
    pub run_id: String,
    pub step_id: String,
    pub cwd: std::path::PathBuf,
    pub gates: Vec<(Gate, String)>,
}

/// Claim a verifying step for this host: resolve its gate names to the project's commands and record executions as
/// running. A missing gate blocks the step with an explanation (and asks the person to define it).
pub fn claim(conn: &Connection, p: &Pending, host_id: &str) -> Result<Option<Claim>> {
    let run = get_run(conn, &p.run_id)?;
    let d = definition(conn, &p.run_id)?;
    let def = step_def(&d, &p.step_id)?;
    let names: Vec<String> = def.gates.iter().flat_map(|g| if g == "constitution" { constitution_gates(conn, &run) } else { vec![g.clone()] }).collect();
    let project = run.project_id.as_deref().map(|id| crate::project::get(conn, id)).transpose()?;
    let Some(project) = project.filter(|p| p.local_path.as_ref().is_some_and(|d| d.is_dir())) else {
        set_step(conn, &p.run_id, &p.step_id, "status = 'blocked', last_failure = ?3", &[&"the run has no project folder on this machine to verify in"])?;
        return Ok(None);
    };
    let mut resolved = Vec::new();
    let mut missing = Vec::new();
    for n in &names {
        match gates::find(conn, &project.id, n)? {
            Some(g) => resolved.push(g),
            None => missing.push(n.clone()),
        }
    }
    if !missing.is_empty() {
        let why = format!("gate(s) {} are not defined for {} — define them under Workflows → Gates", missing.join(", "), project.name);
        set_step(conn, &p.run_id, &p.step_id, "status = 'blocked', blocked_on = 'gates', last_failure = ?3", &[&why])?;
        notify::publish(conn, event(&run, Category::InputRequired, format!("Define gate(s) for {}", project.name)).body(why).dedupe(format!("wf-gates:{}:{}", p.run_id, p.step_id)))?;
        set_run(conn, &p.run_id, "waiting", None)?;
        return Ok(None);
    }
    // Gate commands run only in the version the person approved (separate from approving the workflow).
    let unapproved: Vec<String> = resolved.iter().filter(|g| !g.approved).map(|g| format!("{} (version {})", g.name, g.version)).collect();
    if !unapproved.is_empty() {
        let why = format!("gate command(s) {} are not approved for {} — review and approve them under Workflows → Gates", unapproved.join(", "), project.name);
        set_step(conn, &p.run_id, &p.step_id, "status = 'blocked', blocked_on = 'gates', last_failure = ?3", &[&why])?;
        notify::publish(
            conn,
            event(&run, Category::InputRequired, format!("Approve gate command(s) for {}", project.name)).body(why).priority(Priority::Action).dedupe(format!("wf-approve:{}:{}", p.run_id, p.step_id)),
        )?;
        set_run(conn, &p.run_id, "waiting", None)?;
        return Ok(None);
    }
    let mut gates_out = Vec::new();
    for g in resolved {
        let id = gates::begin(conn, &project.id, Some((&p.run_id, &p.step_id)), &g, host_id)?;
        gates_out.push((g, id));
    }
    Ok(Some(Claim { run_id: p.run_id.clone(), step_id: p.step_id.clone(), cwd: project.local_path.clone().unwrap_or_default(), gates: gates_out }))
}

/// Gates linked from the effective constitution rules (`check=`), for a `gates=constitution` step (ADR-0027).
fn constitution_gates(conn: &Connection, run: &Run) -> Vec<String> {
    let project = run.project_id.as_deref().and_then(|id| crate::project::get(conn, id).ok());
    let stacks = project.as_ref().map(|p| p.stack.stacks.clone()).unwrap_or_default();
    let t = crate::knowledge::constitution::Target { project: project.as_ref().map(|p| (p.id.as_str(), stacks.as_slice())), task_id: Some(&run.task_id), include_pending: false };
    let mut names: Vec<String> = crate::knowledge::constitution::effective_indexed(conn, &run.workspace_id, &t).map(|e| e.rules.into_iter().filter_map(|r| r.check).collect()).unwrap_or_default();
    names.sort();
    names.dedup();
    names
}

/// Record the outcomes of a claim and move the step on: all passed → verified; a failure → back to Claude (or, for a
/// check step, blocked for a retry) until attempts run out → failed.
pub fn complete(conn: &Connection, data_dir: &Path, ws: &Workspace, c: &Claim, outcomes: &[(String, Outcome)]) -> Result<()> {
    let run = get_run(conn, &c.run_id)?;
    for (id, o) in outcomes {
        gates::finish(conn, id, o)?;
    }
    if !active(&run) || outcomes.iter().any(|(_, o)| o.status == GateStatus::Cancelled) {
        return Ok(());
    }
    let d = definition(conn, &c.run_id)?;
    let def = step_def(&d, &c.step_id)?;
    let st = steps(conn, &c.run_id)?.into_iter().find(|s| s.step_id == c.step_id).ok_or_else(|| CoreError::not_found("step"))?;
    let failed: Vec<(String, &Outcome)> = outcomes.iter().filter(|(_, o)| o.status != GateStatus::Passed).map(|(id, o)| (gates::get_result(conn, id).map(|r| r.gate_name).unwrap_or_default(), o)).collect();
    if failed.is_empty() {
        set_step(conn, &c.run_id, &c.step_id, "status = 'passed', verification = 'verified', last_failure = NULL", &[])?;
        let check: Vec<usize> = if st.position > 0 { vec![st.position as usize] } else { vec![] };
        let names: Vec<&str> = c.gates.iter().map(|(g, _)| g.name.as_str()).collect();
        task_log(conn, data_dir, ws, &run, TaskUpdate { log: Some(&format!("[nexus] “{}” verified by {}", def.title, names.join(", "))), check: &check, ..Default::default() }, Author::Human);
    } else {
        let (name, o) = &failed[0];
        let tail: String = o.output_tail.chars().rev().take(1500).collect::<Vec<_>>().into_iter().rev().collect();
        let why = format!("gate {name} {} (exit {}): {tail}", o.status.as_str(), o.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "–".into()));
        let attempts = if def.kind == StepKind::Check { st.attempts + 1 } else { st.attempts };
        if attempts >= st.max_attempts {
            set_step(conn, &c.run_id, &c.step_id, "status = 'failed', attempts = ?3, last_failure = ?4", &[&attempts, &why])?;
        } else if def.kind == StepKind::Claude {
            set_step(conn, &c.run_id, &c.step_id, "status = 'ready', last_failure = ?3", &[&why])?;
            notify::publish(
                conn,
                event(&run, Category::Info, format!("Gate {name} failed — back to Claude: {}", def.title))
                    .body(format!("Attempt {attempts} of {}. Claude sees the output through get_workflow_run.", st.max_attempts))
                    .priority(Priority::Warning)
                    .dedupe(format!("wf-gate:{}:{}", c.run_id, c.step_id)),
            )?;
        } else {
            set_step(conn, &c.run_id, &c.step_id, "status = 'blocked', attempts = ?3, last_failure = ?4", &[&attempts, &why])?;
            notify::publish(
                conn,
                event(&run, Category::ProcessIssue, format!("Gate {name} failed: {}", def.title))
                    .body(format!("Attempt {attempts} of {}. Retry the step after fixing the cause.", st.max_attempts))
                    .dedupe(format!("wf-gate:{}:{}", c.run_id, c.step_id)),
            )?;
        }
        task_log(conn, data_dir, ws, &run, TaskUpdate { log: Some(&format!("[nexus] “{}”: gate {name} {}", def.title, o.status.as_str())), ..Default::default() }, Author::Human);
    }
    advance(conn, data_dir, ws, &c.run_id)
}

/// Claim, execute and complete every pending verification (the host's runner step; also `nexus workflow verify`).
/// Gates execute without holding the database: `with_db` is called for each short transaction.
pub fn verify_pending<F>(with_db: F, data_dir: &Path, host_id: &str, base_env: &[(String, String)]) -> Result<usize>
where
    F: Fn(&mut dyn FnMut(&Connection) -> Result<()>) -> Result<()>,
{
    let mut claims: Vec<(Claim, Workspace)> = Vec::new();
    with_db(&mut |conn| {
        for p in pending_verifications(conn)? {
            let run = get_run(conn, &p.run_id)?;
            let Ok(ws) = crate::workspace::get(conn, &run.workspace_id) else { continue };
            if let Some(c) = claim(conn, &p, host_id)? {
                claims.push((c, ws));
            } else {
                advance(conn, data_dir, &ws, &p.run_id)?;
            }
        }
        Ok(())
    })?;
    let n = claims.len();
    for (c, ws) in claims {
        let outcomes: Vec<(String, Outcome)> = c.gates.iter().map(|(g, id)| (id.clone(), gates::execute(g, &c.cwd, base_env, id))).collect();
        with_db(&mut |conn| complete(conn, data_dir, &ws, &c, &outcomes))?;
    }
    Ok(n)
}

// ---- Person actions ------------------------------------------------------------------------------------------------

pub fn cancel(conn: &Connection, data_dir: &Path, ws: &Workspace, run_id: &str) -> Result<RunView> {
    let run = get_run(conn, run_id)?;
    if !active(&run) {
        return Err(CoreError::invalid(format!("the run is already {}", run.status)));
    }
    for g in gates::running(conn)?.into_iter().filter(|g| g.run_id.as_deref() == Some(run_id)) {
        gates::cancel(&g.id);
        conn.execute("UPDATE gate_results SET status = 'cancelled', ended_at = ?2 WHERE id = ?1 AND status = 'running'", params![g.id, now_ms()])?;
    }
    conn.execute(
        "UPDATE workflow_steps SET status = 'cancelled', updated_at = ?2 WHERE run_id = ?1 AND status IN ('pending', 'ready', 'blocked', 'verifying', 'awaiting-review')",
        params![run_id, now_ms()],
    )?;
    set_run(conn, run_id, "cancelled", None)?;
    notify::resolve_workflow(conn, run_id)?;
    task_log(conn, data_dir, ws, &run, TaskUpdate { log: Some("[nexus] workflow run cancelled"), ..Default::default() }, Author::Human);
    view(conn, run_id)
}

/// Try a failed or blocked step again (a person decides; one more attempt is granted when none are left).
pub fn retry(conn: &Connection, data_dir: &Path, ws: &Workspace, run_id: &str, step_id: &str) -> Result<RunView> {
    let run = get_run(conn, run_id)?;
    if run.status == "cancelled" {
        return Err(CoreError::invalid("the run was cancelled — start a new run"));
    }
    let d = definition(conn, run_id)?;
    let def = step_def(&d, step_id)?;
    let st = steps(conn, run_id)?.into_iter().find(|s| s.step_id == step_id).ok_or_else(|| CoreError::not_found("step"))?;
    if !matches!(st.status.as_str(), "failed" | "blocked") {
        return Err(CoreError::invalid(format!("step `{step_id}` is {} — only failed or blocked steps are retried", st.status)));
    }
    let next = match def.kind {
        // Reported work that only waited for its gates is verified again, not handed back to Claude.
        _ if st.blocked_on.as_deref() == Some("gates") => "verifying",
        StepKind::Claude => "ready",
        StepKind::Check => "verifying",
        StepKind::Review => "awaiting-review",
    };
    set_step(conn, run_id, step_id, "status = ?3, max_attempts = MAX(max_attempts, attempts + 1)", &[&next])?;
    if run.status == "failed" {
        set_run(conn, run_id, "running", None)?;
        notify::resolve_dedupe(conn, &format!("wf-failed:{run_id}"))?;
    }
    task_log(conn, data_dir, ws, &run, TaskUpdate { log: Some(&format!("[nexus] retrying “{}”", def.title)), ..Default::default() }, Author::Human);
    advance(conn, data_dir, ws, run_id)?;
    view(conn, run_id)
}

/// A step whose reported work waits for its gates, and the gate names it still needs (not defined or not approved).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GateWait {
    pub run_id: String,
    pub step_id: String,
    pub needs: Vec<String>,
}

/// Steps of the project's active runs waiting for gates, with what each still needs.
pub fn gate_waits(conn: &Connection, project_id: &str) -> Result<Vec<GateWait>> {
    let waiting: Vec<(String, String)> = conn
        .prepare(
            "SELECT s.run_id, s.step_id FROM workflow_steps s JOIN workflow_runs r ON r.id = s.run_id
             WHERE s.status = 'blocked' AND s.blocked_on = 'gates' AND r.project_id = ?1 AND r.status IN ('running', 'waiting')
             ORDER BY r.created_at, s.position",
        )?
        .query_map([project_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut out = Vec::new();
    for (run_id, step_id) in waiting {
        let run = get_run(conn, &run_id)?;
        let d = definition(conn, &run_id)?;
        let Ok(def) = step_def(&d, &step_id) else { continue };
        let mut names: Vec<String> = def.gates.iter().flat_map(|g| if g == "constitution" { constitution_gates(conn, &run) } else { vec![g.clone()] }).collect();
        names.sort();
        names.dedup();
        let mut needs = Vec::new();
        for n in names {
            if !gates::find(conn, project_id, &n)?.is_some_and(|g| g.approved) {
                needs.push(n);
            }
        }
        out.push(GateWait { run_id, step_id, needs });
    }
    Ok(out)
}

/// What approving one gate changes for waiting work — shown before the approval is confirmed (desktop dialog,
/// `nexus gates list`) and reported after it. Purely informational: the approval itself is `gates::approve`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalImpact {
    /// Steps that resume verification automatically once this gate is approved …
    pub steps: usize,
    /// … in this many runs.
    pub runs: usize,
    /// Steps that also wait for this gate but keep waiting for other gates.
    pub still_waiting: usize,
}

pub fn approval_impact(conn: &Connection, project_id: &str, gate: &str) -> Result<ApprovalImpact> {
    let mut impact = ApprovalImpact::default();
    let mut runs = std::collections::HashSet::new();
    for w in gate_waits(conn, project_id)?.into_iter().filter(|w| w.needs.iter().any(|n| n == gate)) {
        if w.needs.iter().all(|n| n == gate) {
            impact.steps += 1;
            runs.insert(w.run_id);
        } else {
            impact.still_waiting += 1;
        }
    }
    impact.runs = runs.len();
    Ok(impact)
}

/// After a gate approval: steps whose gates are now all defined and approved verify again (no new report from
/// Claude); steps still missing something keep waiting. Returns how many steps resumed and in how many runs.
pub fn resume_gate_waits(conn: &Connection, project_id: &str) -> Result<ApprovalImpact> {
    let mut impact = ApprovalImpact::default();
    let mut runs = std::collections::HashSet::new();
    for w in gate_waits(conn, project_id)? {
        if !w.needs.is_empty() {
            impact.still_waiting += 1;
            continue;
        }
        set_step(conn, &w.run_id, &w.step_id, "status = 'verifying', last_failure = NULL", &[])?;
        set_run(conn, &w.run_id, "running", None)?;
        notify::resolve_dedupe(conn, &format!("wf-approve:{}:{}", w.run_id, w.step_id))?;
        notify::resolve_dedupe(conn, &format!("wf-gates:{}:{}", w.run_id, w.step_id))?;
        impact.steps += 1;
        runs.insert(w.run_id);
    }
    impact.runs = runs.len();
    Ok(impact)
}

/// After a crash: gate executions of dead hosts are interrupted; their steps stay `verifying` and are run again.
pub fn reconcile(conn: &Connection, now: i64) -> Result<usize> {
    gates::reconcile(conn, now)
}

/// Bounded, escaped brief of the run for Claude (MCP and SessionStart): data, never the system prompt.
pub fn brief(v: &RunView, max_chars: usize) -> String {
    let mut t = format!(
        "<nexus-workflow-run id=\"{}\" status=\"{}\" rigor=\"{}\">\nworkflow: {}\n",
        v.run.id,
        v.run.status,
        v.run.rigor,
        escape_untrusted(&v.run.workflow_title, 120)
    );
    for s in &v.steps {
        if s.state.status == "skipped" {
            continue;
        }
        t.push_str(&format!("- [{}] {} ({}, {})", s.state.step_id, escape_untrusted(&s.title, 100), s.state.kind, s.state.status));
        if !s.gates.is_empty() {
            t.push_str(&format!(" gates: {}", s.gates.join(",")));
        }
        t.push('\n');
    }
    t.push_str("</nexus-workflow-run>\nUse get_workflow_run for the current step's instructions and report_step when done. NEXUS runs the gates itself.");
    t.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests;

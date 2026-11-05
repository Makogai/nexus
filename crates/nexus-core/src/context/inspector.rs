//! Context Inspector (ADR-0028): for a project (and a profile), what context is **configured**, **predicted** for the
//! next session, **observed** in the latest session (instruction files Claude reported loading through the
//! InstructionsLoaded hook, and what NEXUS appended), **available on demand**, and **actually retrieved** through the
//! NEXUS MCP tools — plus retrieval metrics. Facts only: nothing is estimated where it can be observed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::{Value, json};

use super::{Config, docs};
use crate::claude::instructions::{Inspection, Load};
use crate::error::Result;
use crate::knowledge::{constitution, context as kctx};
use crate::session::{self, Filter, Kind, Session};
use crate::{new_id, now_ms};

/// Upper bound of retrieval/instruction events per session.
pub const MAX_EVENTS_PER_SESSION: i64 = 500;

/// Tools that retrieve context (recorded with result counts and sizes; queries are not stored).
pub const RETRIEVAL_TOOLS: &[&str] = &[
    "get_context_index",
    "search_knowledge",
    "get_document",
    "get_constitution",
    "get_task",
    "list_tasks",
    "get_resume_summary",
    "get_checkpoints",
    "get_session_activity",
    "get_project",
    "get_git_status",
    "get_workflow_run",
];

fn events(conn: &Connection, session_id: &str, kind: &str) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM session_events WHERE session_id = ?1 AND kind = ?2", params![session_id, kind], |r| r.get(0))?)
}

/// Record one retrieval by the session's MCP server: tool, number of results, characters returned, document ids.
pub fn record_retrieval(conn: &Connection, session_id: &str, tool: &str, results: usize, chars: usize, ids: &[String]) -> Result<()> {
    if !RETRIEVAL_TOOLS.contains(&tool) || events(conn, session_id, "retrieval")? >= MAX_EVENTS_PER_SESSION {
        return Ok(());
    }
    let ids: Vec<&String> = ids.iter().take(20).collect();
    conn.execute(
        "INSERT INTO session_events(id, session_id, kind, summary, data, source, created_at) VALUES (?1, ?2, 'retrieval', ?3, ?4, 'nexus', ?5)",
        params![new_id(), session_id, tool, json!({ "results": results, "chars": chars, "ids": ids }).to_string(), now_ms()],
    )?;
    Ok(())
}

/// Record an instruction file Claude reported loading (InstructionsLoaded hook). Deduplicated per session.
pub fn record_instructions(conn: &Connection, session_id: &str, path: &str, memory_type: &str, load_reason: &str) -> Result<()> {
    if path.len() > 2048 || !Path::new(path).is_absolute() || events(conn, session_id, "instructions")? >= MAX_EVENTS_PER_SESSION {
        return Ok(());
    }
    let seen: i64 = conn.query_row(
        "SELECT COUNT(*) FROM session_events WHERE session_id = ?1 AND kind = 'instructions' AND json_extract(data, '$.path') = ?2",
        params![session_id, path],
        |r| r.get(0),
    )?;
    if seen > 0 {
        return Ok(());
    }
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let clean = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(40).collect::<String>();
    conn.execute(
        "INSERT INTO session_events(id, session_id, kind, summary, data, source, created_at) VALUES (?1, ?2, 'instructions', ?3, ?4, 'hook', ?5)",
        params![new_id(), session_id, name, json!({ "path": path, "memoryType": clean(memory_type), "loadReason": clean(load_reason) }).to_string(), now_ms()],
    )?;
    Ok(())
}

/// Cross-profile instruction bleed (threat T21) seen live: the session of one profile loaded an instruction file from
/// *another* profile's configuration folder — a registered profile's, or the default location `~/.claude` (the
/// default profile, registered or not). Publishes one warning per session and file; returns whether it did.
pub fn check_bleed(conn: &Connection, session_id: &str, path: &str, home: &Path) -> Result<bool> {
    let Ok(s) = session::get(conn, session_id) else { return Ok(false) };
    let profiles = crate::claude::profile::list(conn, true)?;
    let Some(own) = s.profile_id.as_deref().and_then(|id| profiles.iter().find(|p| p.id == id)) else { return Ok(false) };
    // Canonical forms on both sides: Claude, the profile record and the OS may spell the same folder differently
    // (8.3 short names such as `RUNNER~1`, symlinks, case).
    let canon = |p: &Path| crate::paths::canonical_or_given(p);
    let own_dir = canon(&own.effective_config_dir(home));
    let file_canon = canon(Path::new(path));
    let file = file_canon.as_path();
    if crate::paths::is_within(file, &own_dir) {
        return Ok(false);
    }
    let default_dir = canon(&home.join(".claude"));
    let mut others: Vec<(String, std::path::PathBuf)> =
        profiles.iter().filter(|p| p.id != own.id).map(|p| (p.name.clone(), canon(&p.effective_config_dir(home)))).collect();
    if !others.iter().any(|(_, d)| crate::paths::same_path(d, &default_dir)) {
        others.push(("the default profile".into(), default_dir));
    }
    let Some((other, _)) = others.into_iter().find(|(_, dir)| crate::paths::is_within(file, dir) && !crate::paths::is_within(&own_dir, dir)) else {
        return Ok(false);
    };
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let published = crate::notify::publish(
        conn,
        crate::notify::NewEvent::for_session(
            crate::notify::Category::ProcessIssue,
            "hook",
            &s,
            format!("“{}” loaded instructions of {other}", s.title),
        )
        .body(format!("{name} from {other}’s configuration reached a {} session. Instruction isolation for {} keeps it out.", own.name, own.name))
        .priority(crate::notify::Priority::Warning)
        .target("profile", own.id.clone())
        .dedupe(format!("bleed:{}:{}", s.id, path)),
    )?;
    Ok(matches!(published, crate::notify::Published::Created(_)))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedFile {
    pub path: PathBuf,
    pub memory_type: String,
    pub load_reason: String,
    /// The inspector predicted this file at session start.
    pub predicted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    pub id: String,
    pub title: String,
    pub started_at: i64,
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Observed {
    pub session: Option<SessionRef>,
    /// `false` when that session had no instruction observation (hooks off, or a strategy without it).
    pub instructions_observed: bool,
    pub instructions: Vec<ObservedFile>,
    /// Predicted at start but not reported by Claude in that session.
    pub missing: Vec<PathBuf>,
    /// What NEXUS appended to that session (rules, constitution, task brief) as recorded at launch.
    pub nexus: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnDemand {
    pub knowledge: kctx::Summary,
    pub project_docs: usize,
    pub constitution_rules: Vec<String>,
    pub tools: Vec<&'static str>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Retrieval {
    pub tool: String,
    pub results: i64,
    pub chars: i64,
    pub ids: Vec<String>,
    pub at: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub sessions: usize,
    pub sessions_with_retrieval: usize,
    pub retrievals: usize,
    pub chars: i64,
    /// Retrievals that returned nothing.
    pub empty: usize,
    pub by_tool: Vec<(String, usize)>,
    /// Most retrieved document ids / doc paths.
    pub top_documents: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub configured: Config,
    /// Prediction for the next session with the chosen profile (instruction layers + NEXUS append).
    pub predicted: Option<crate::claude::prepare::Preview>,
    pub observed: Observed,
    pub on_demand: OnDemand,
    pub retrieved: Vec<Retrieval>,
    pub metrics: Metrics,
}

fn retrievals(conn: &Connection, session_id: &str) -> Result<Vec<Retrieval>> {
    Ok(conn
        .prepare("SELECT summary, data, created_at FROM session_events WHERE session_id = ?1 AND kind = 'retrieval' ORDER BY created_at, rowid")?
        .query_map([session_id], |r| {
            let data: Option<String> = r.get(1)?;
            let v: Value = data.and_then(|d| serde_json::from_str(&d).ok()).unwrap_or(Value::Null);
            Ok(Retrieval {
                tool: r.get(0)?,
                results: v["results"].as_i64().unwrap_or(0),
                chars: v["chars"].as_i64().unwrap_or(0),
                ids: v["ids"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(),
                at: r.get(2)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?)
}

fn observed_files(conn: &Connection, session_id: &str) -> Result<Vec<(PathBuf, String, String)>> {
    Ok(conn
        .prepare("SELECT data FROM session_events WHERE session_id = ?1 AND kind = 'instructions' ORDER BY created_at, rowid")?
        .query_map([session_id], |r| r.get::<_, Option<String>>(0))?
        .filter_map(|d| d.ok().flatten())
        .filter_map(|d| serde_json::from_str::<Value>(&d).ok())
        .filter_map(|v| Some((PathBuf::from(v["path"].as_str()?), v["memoryType"].as_str().unwrap_or("").to_string(), v["loadReason"].as_str().unwrap_or("").to_string())))
        .collect())
}

fn same(a: &Path, b: &Path) -> bool {
    crate::paths::same_path(a, b)
}

/// Retrieval metrics over the project's last `n` Claude sessions.
pub fn metrics(conn: &Connection, project_id: &str, n: u32) -> Result<Metrics> {
    let list = session::list(conn, &Filter { project_id: Some(project_id.into()), kind: Some(Kind::Claude), limit: Some(n), include_archived: true, ..Default::default() })?;
    let mut m = Metrics { sessions: list.len(), ..Default::default() };
    let mut tools: std::collections::BTreeMap<String, usize> = Default::default();
    let mut docs: std::collections::HashMap<String, usize> = Default::default();
    for s in &list {
        let r = retrievals(conn, &s.id)?;
        if !r.is_empty() {
            m.sessions_with_retrieval += 1;
        }
        for x in r {
            m.retrievals += 1;
            m.chars += x.chars;
            if x.results == 0 {
                m.empty += 1;
            }
            *tools.entry(x.tool).or_default() += 1;
            for id in x.ids {
                *docs.entry(id).or_default() += 1;
            }
        }
    }
    m.by_tool = tools.into_iter().collect();
    m.by_tool.sort_by_key(|t| std::cmp::Reverse(t.1));
    let mut top: Vec<(String, usize)> = docs.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top.truncate(10);
    m.top_documents = top;
    Ok(m)
}

pub struct Request<'a> {
    pub conn: &'a Connection,
    pub workspace_id: &'a str,
    pub project_id: &'a str,
    pub stacks: &'a [String],
    pub profile_id: Option<&'a str>,
    /// Instruction prediction for the chosen profile (computed by the caller with `prepare::preview`).
    pub predicted: Option<crate::claude::prepare::Preview>,
}

pub fn inspect(r: Request<'_>) -> Result<Report> {
    let configured = super::get(r.conn, r.project_id)?;
    // The latest Claude session of this project (and profile, if chosen).
    let latest: Option<Session> = session::list(r.conn, &Filter { project_id: Some(r.project_id.into()), kind: Some(Kind::Claude), limit: Some(30), include_archived: true, ..Default::default() })?
        .into_iter()
        .find(|s| r.profile_id.is_none_or(|p| s.profile_id.as_deref() == Some(p)));
    let at_start: Vec<PathBuf> = r
        .predicted
        .as_ref()
        .map(|p: &crate::claude::prepare::Preview| predicted_paths(&p.claude))
        .unwrap_or_default();
    let mut observed = Observed { session: None, instructions_observed: false, instructions: vec![], missing: vec![], nexus: None };
    let mut retrieved = Vec::new();
    if let Some(s) = &latest {
        observed.session = Some(SessionRef { id: s.id.clone(), title: s.title.clone(), started_at: s.started_at, profile_id: s.profile_id.clone() });
        let files = observed_files(r.conn, &s.id)?;
        observed.instructions_observed = !files.is_empty() || s.integrations.as_ref().is_some_and(|i| i["context"]["observeInstructions"] == json!(true));
        let mut seen: HashSet<usize> = HashSet::new();
        for (path, memory_type, load_reason) in files {
            let hit = at_start.iter().position(|p| same(p, &path));
            if let Some(i) = hit {
                seen.insert(i);
            }
            observed.instructions.push(ObservedFile { path, memory_type, load_reason, predicted: hit.is_some() });
        }
        if observed.instructions_observed {
            observed.missing = at_start.iter().enumerate().filter(|(i, _)| !seen.contains(i)).map(|(_, p)| p.clone()).collect();
        }
        observed.nexus = s.integrations.as_ref().map(|i| json!({ "rules": i["rules"], "task": i["task"], "kits": i["kits"], "context": i["context"] }));
        retrieved = retrievals(r.conn, &s.id)?;
    }
    let scope = Some((r.project_id, r.stacks));
    let eff = constitution::effective_indexed(r.conn, r.workspace_id, &constitution::Target { project: scope, ..Default::default() })?;
    let delivery = constitution::delivery(&eff, configured.settings.rules_budget, true, configured.settings.constitution);
    Ok(Report {
        on_demand: OnDemand {
            knowledge: kctx::summary(r.conn, r.workspace_id, scope)?,
            project_docs: if configured.settings.project_docs { docs::list(r.conn, r.project_id)?.len() } else { 0 },
            constitution_rules: delivery.on_demand,
            tools: RETRIEVAL_TOOLS.to_vec(),
        },
        metrics: metrics(r.conn, r.project_id, 20)?,
        configured,
        predicted: r.predicted,
        observed,
        retrieved,
    })
}

/// Files the inspector predicts Claude loads at session start.
pub fn predicted_paths(i: &Inspection) -> Vec<PathBuf> {
    i.layers.iter().filter(|l| l.load == Load::AtStart).filter_map(|l| l.path.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::NewSession;

    #[test]
    fn retrievals_and_instructions_are_recorded_bounded_and_summarised() {
        let conn = crate::db::open_in_memory().unwrap();
        let t = tempfile::tempdir().unwrap();
        let ws = crate::workspace::create(&conn, t.path().join("ws").to_str().unwrap(), "W", "").unwrap();
        std::fs::create_dir_all(t.path().join("p")).unwrap();
        let p = crate::project::register(&conn, &ws.id, t.path().join("p").to_str().unwrap(), None).unwrap();
        let s = session::insert_starting(&conn, NewSession { title: "S".into(), cwd: t.path().join("p"), program: "claude".into(), workspace_id: Some(ws.id.clone()), project_id: Some(p.id.clone()), ..Default::default() }).unwrap();
        record_retrieval(&conn, &s.id, "search_knowledge", 3, 900, &["a".into(), "b".into()]).unwrap();
        record_retrieval(&conn, &s.id, "search_knowledge", 0, 20, &[]).unwrap();
        record_retrieval(&conn, &s.id, "get_document", 1, 4000, &["a".into()]).unwrap();
        record_retrieval(&conn, &s.id, "checkpoint", 1, 10, &[]).unwrap(); // not a retrieval tool
        let claude_md = t.path().join("p").join("CLAUDE.md");
        for _ in 0..2 {
            record_instructions(&conn, &s.id, claude_md.to_str().unwrap(), "Project", "session_start").unwrap();
        }
        record_instructions(&conn, &s.id, "relative/CLAUDE.md", "Project", "session_start").unwrap();
        let m = metrics(&conn, &p.id, 20).unwrap();
        assert_eq!((m.retrievals, m.empty, m.chars, m.sessions_with_retrieval), (3, 1, 4920, 1));
        assert_eq!(m.by_tool[0], ("search_knowledge".to_string(), 2));
        assert_eq!(m.top_documents[0], ("a".to_string(), 2));
        // A file of another profile's configuration folder in this session: a bleed warning (once).
        let home = t.path().join("home");
        let mk = |name: &str, dir: Option<std::path::PathBuf>| crate::claude::profile::create_managed(&conn, &crate::claude::profile::ProfileInput { name: name.into(), config_dir: dir.map(|d| d.to_string_lossy().into_owned()), ..Default::default() }, &home);
        let work = mk("Work", Some(t.path().join("cfg-work"))).unwrap();
        let personal = mk("Personal", Some(t.path().join("cfg-personal"))).unwrap();
        conn.execute("UPDATE sessions SET profile_id = ?2 WHERE id = ?1", rusqlite::params![s.id, work.id]).unwrap();
        let foreign = t.path().join("cfg-personal").join("CLAUDE.md");
        assert!(check_bleed(&conn, &s.id, foreign.to_str().unwrap(), &home).unwrap());
        assert!(!check_bleed(&conn, &s.id, foreign.to_str().unwrap(), &home).unwrap(), "once per file and session");
        assert!(!check_bleed(&conn, &s.id, t.path().join("cfg-work").join("CLAUDE.md").to_str().unwrap(), &home).unwrap(), "own profile is fine");
        assert!(!check_bleed(&conn, &s.id, claude_md.to_str().unwrap(), &home).unwrap(), "project files are fine");
        let n = crate::notify::list(&conn, &crate::notify::Filter::default()).unwrap();
        assert!(n.iter().any(|x| x.title.contains("instructions of Personal") && x.target_kind.as_deref() == Some("profile")), "{n:?}");
        let _ = personal;

        let report = inspect(Request { conn: &conn, workspace_id: &ws.id, project_id: &p.id, stacks: &[], profile_id: None, predicted: None }).unwrap();
        assert_eq!(report.observed.instructions.len(), 1, "deduplicated, absolute paths only");
        assert!(report.observed.instructions_observed);
        assert_eq!(report.retrieved.len(), 3);
        assert!(!report.configured.configured);
    }
}

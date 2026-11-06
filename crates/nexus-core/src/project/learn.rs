//! Learn a project (W4): build a project's knowledge base in three reviewed steps.
//!
//! 1. **NEXUS's own map** ([`map`]) — read without any model: tracked files by folder and language, documentation in
//!    the repository, stack and commands, history size, what the portable context already holds, and whether a Graphify
//!    graph exists. Nothing is executed; Graphify is only looked up on the PATH.
//! 2. **A guided Claude session** ([`start`]) — a task whose brief carries the map (as data) and asks Claude to read the
//!    code without changing it, draft the overview and the architecture summary ([`record_draft`], through the MCP tool
//!    `draft_project_document`) and record durable memories (`record_memory`: notes are unverified, decisions start as
//!    proposals). NEXUS calls no model itself; the session runs on the person's Claude plan.
//! 3. **The person decides** — a draft becomes the overview or fills the architecture summary only through
//!    [`apply_draft`]; [`discard_draft`] keeps it as a record but out of the way.
//!
//! Graphify stays optional and the person's choice: NEXUS types its offline, code-only build command into a terminal in
//! the project folder for the person to run, and [`exclude_graph_output`] keeps `graphify-out/` out of Git through the
//! clone's local exclude file (never a committed file).

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::knowledge::frontmatter::Value;
use crate::knowledge::index::{self, Filter, Item};
use crate::knowledge::service::{self, Author, CreateInput};
use crate::knowledge::{Kind, store};
use crate::workspace::Workspace;
use crate::{git, project, toolchain};

pub const GRAPH_DIR: &str = "graphify-out";
/// The build NEXUS offers, both steps offline (Graphify 0.9.93 help, run on a synthetic repository 2026-11-05):
/// `extract --code-only` writes `graph.json` from the code alone (local AST, no API key); `cluster-only --no-label`
/// writes `GRAPH_REPORT.md` and keeps placeholder community names — without `--no-label` Graphify asks a model to name
/// them. `;` runs both in PowerShell, bash and zsh.
pub const GRAPH_COMMAND: &str = "graphify extract . --code-only; graphify cluster-only . --no-label";
pub const LEARN_LABEL: &str = "learn";
pub const DRAFT_LABEL: &str = "draft";
pub const TOPICS: [&str; 2] = ["overview", "architecture"];
const MAX_TRACKED: usize = 50_000;
const MAX_DRAFT_CHARS: usize = 30_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Count {
    pub name: String,
    pub files: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContextStatus {
    pub overview_id: Option<String>,
    /// Still the text NEXUS generated from the README (not written or accepted yet).
    pub overview_generated: bool,
    pub architecture_id: Option<String>,
    /// The architecture summary has been written (or a draft accepted).
    pub architecture_summary: bool,
    pub notes: usize,
    pub decisions: usize,
    pub open_tasks: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphStatus {
    /// `graphify` is on this computer's PATH (looked up, not run).
    pub installed: bool,
    pub built: bool,
    pub report: Option<String>,
    pub built_ms: Option<i64>,
    /// `graphify-out/` is ignored by Git in this clone.
    pub ignored: bool,
    pub command: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearnTask {
    pub id: String,
    pub title: String,
    pub status: Option<String>,
    pub depth: Depth,
}

/// How much a learning session may spend (ADR-0041): one Claude session in every case; the depth limits how many
/// files Claude reads and how long the drafts are. Stored as a task label (`learn-quick` …), so the person can change
/// it before (re)starting the session.
#[derive(Debug, Clone, Copy, Serialize, serde::Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Depth {
    Quick,
    #[default]
    Standard,
    Deep,
}

/// A depth as the Learn tab shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DepthInfo {
    pub id: Depth,
    pub files: Option<u32>,
    pub overview_words: u32,
    pub architecture_words: u32,
    pub memories: (u32, u32),
    pub rules: String,
}

impl DepthInfo {
    fn of(d: Depth) -> DepthInfo {
        let b = d.budget();
        DepthInfo { id: d, files: b.files, overview_words: b.overview_words, architecture_words: b.architecture_words, memories: b.memories, rules: rules(d) }
    }
}

/// What a depth allows: files Claude may open (`None` = no limit, still selective), words per draft, memories.
pub struct Budget {
    pub files: Option<u32>,
    pub overview_words: u32,
    pub architecture_words: u32,
    pub memories: (u32, u32),
}

impl Depth {
    pub const ALL: [Depth; 3] = [Depth::Quick, Depth::Standard, Depth::Deep];

    pub fn id(self) -> &'static str {
        match self {
            Depth::Quick => "quick",
            Depth::Standard => "standard",
            Depth::Deep => "deep",
        }
    }

    pub fn label(self) -> String {
        format!("{LEARN_LABEL}-{}", self.id())
    }

    pub fn parse(s: &str) -> Option<Depth> {
        Depth::ALL.into_iter().find(|d| d.id() == s.trim().to_ascii_lowercase())
    }

    /// The depth a task's labels name — `None` when the task is not a learning task.
    pub fn of_labels(labels: &[String]) -> Option<Depth> {
        labels.iter().any(|l| l == LEARN_LABEL).then(|| Depth::ALL.into_iter().find(|d| labels.contains(&d.label())).unwrap_or_default())
    }

    pub fn budget(self) -> Budget {
        match self {
            Depth::Quick => Budget { files: Some(20), overview_words: 250, architecture_words: 400, memories: (3, 5) },
            Depth::Standard => Budget { files: Some(50), overview_words: 400, architecture_words: 700, memories: (3, 8) },
            Depth::Deep => Budget { files: None, overview_words: 700, architecture_words: 1200, memories: (5, 10) },
        }
    }
}

/// The learning session's rules (ADR-0041) for the system prompt: NEXUS-authored, built from the validated depth only
/// — no task text (T22). Stable ids so they can be referred to; they hold even where the work profile asks for more.
pub fn rules(depth: Depth) -> String {
    let b = depth.budget();
    let files = match b.files {
        Some(n) => format!("open at most {n} files"),
        None => "no file limit, but read selectively".to_string(),
    };
    format!(
        "NEXUS learning session — rules LEARN-1 to LEARN-7 (they apply even where the work profile suggests reviews or more depth):\n\
         LEARN-1 Follow the {skill} skill for this session's task.\n\
         LEARN-2 Work alone in this session: do not start subagents, agent teams or multi-agent workflows unless the user asks for them.\n\
         LEARN-3 Read only: do not change, create or delete files in the repository, and do not run builds, tests or installs.\n\
         LEARN-4 Budget (depth: {depth}): {files}; overview at most {ow} words; architecture summary at most {aw} words; {m0} to {m1} memories.\n\
         LEARN-5 Mark every statement in a draft as verified (seen in the code, with the file) or inferred.\n\
         LEARN-6 Save drafts only with draft_project_document and memories only with record_memory; the user decides what enters the knowledge base.\n\
         LEARN-7 Stop when the drafts and memories are saved: report what you skipped and ask before reading further.",
        skill = crate::claude::builtin::LEARN_SKILL,
        depth = depth.id(),
        ow = b.overview_words,
        aw = b.architecture_words,
        m0 = b.memories.0,
        m1 = b.memories.1,
    )
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMap {
    pub project_id: String,
    pub name: String,
    pub files: usize,
    pub files_truncated: bool,
    pub languages: Vec<Count>,
    pub folders: Vec<Count>,
    pub docs: Vec<String>,
    pub stacks: Vec<String>,
    pub package_managers: Vec<String>,
    pub scripts: Vec<(String, String)>,
    pub history: Option<git::HistoryStats>,
    pub context: ContextStatus,
    pub graph: GraphStatus,
    pub task: Option<LearnTask>,
    pub drafts: usize,
    /// The depths to choose from, with their budgets and the exact rules a learning session gets (ADR-0041).
    pub depths: Vec<DepthInfo>,
    /// The built-in skill learning sessions get (`nexus:learn-project`).
    pub skill: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub id: String,
    pub topic: String,
    pub title: String,
    pub updated: Option<String>,
    pub body: String,
}

fn language(path: &str) -> &'static str {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "rs" => "Rust",
        "ts" | "tsx" | "mts" | "cts" => "TypeScript",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript",
        "vue" => "Vue",
        "svelte" => "Svelte",
        "py" => "Python",
        "php" => "PHP",
        "go" => "Go",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "cs" => "C#",
        "dart" => "Dart",
        "rb" => "Ruby",
        "swift" => "Swift",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" => "C++",
        "sql" => "SQL",
        "css" | "scss" | "sass" | "less" => "CSS",
        "html" | "htm" => "HTML",
        "md" | "mdx" => "Markdown",
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "sh" | "bash" => "Shell",
        "ps1" => "PowerShell",
        _ => "Other",
    }
}

fn is_doc(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let top = !lower.contains('/');
    (top && ["readme", "about", "changelog", "contributing", "architecture", "security", "claude.md", "agents.md"].iter().any(|p| name.starts_with(p)))
        || ((lower.starts_with("docs/") || lower.starts_with("doc/") || lower.contains("/adr/") || lower.contains("/decisions/")) && (name.ends_with(".md") || name.ends_with(".mdx")))
        || lower == ".github/copilot-instructions.md"
}

fn top_counts(map: BTreeMap<String, usize>, n: usize) -> Vec<Count> {
    let mut v: Vec<Count> = map.into_iter().map(|(name, files)| Count { name, files }).collect();
    v.sort_by(|a, b| b.files.cmp(&a.files).then_with(|| a.name.cmp(&b.name)));
    v.truncate(n);
    v
}

/// Labels of a knowledge document (read from its file; the index does not keep them).
fn labels(item: &Item) -> Vec<String> {
    store::read(&item.path)
        .ok()
        .and_then(|(d, _)| match d.fm.get("labels") {
            Some(Value::List(l)) => Some(l.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn project_items(conn: &Connection, ws: &Workspace, project_id: &str, kind: Kind) -> Result<Vec<Item>> {
    index::list(conn, &ws.id, &Filter { kinds: vec![kind], project_id: Some(project_id.to_string()), ..Default::default() })
}

fn open_draft_notes(conn: &Connection, ws: &Workspace, project_id: &str) -> Result<Vec<(Item, String)>> {
    Ok(project_items(conn, ws, project_id, Kind::Note)?
        .into_iter()
        .filter_map(|i| {
            let l = labels(&i);
            let topic = TOPICS.iter().find(|t| l.iter().any(|x| x == *t))?.to_string();
            (l.iter().any(|x| x == DRAFT_LABEL)).then_some((i, topic))
        })
        .collect())
}

/// NEXUS's own map of the project — files, docs, stack, history, context, graph — without any model or execution.
pub fn map(conn: &Connection, data_dir: &Path, ws: &Workspace, project_id: &str) -> Result<ProjectMap> {
    let p = project::get(conn, project_id)?;
    let root = p.local_path.clone().filter(|_| p.available).ok_or_else(|| CoreError::invalid(format!("{} is not on this computer", p.name)))?;
    // The portable context exists and is current (overview/architecture are kept; current state is regenerated).
    let _ = project::context::refresh_state(conn, data_dir, ws, project_id);
    service::refresh(conn, data_dir, ws)?;

    let (files, files_truncated) = git::tracked_files_capped(&root, MAX_TRACKED).unwrap_or_default();
    let mut langs: BTreeMap<String, usize> = BTreeMap::new();
    let mut folders: BTreeMap<String, usize> = BTreeMap::new();
    let mut docs = Vec::new();
    for f in &files {
        *langs.entry(language(f).to_string()).or_default() += 1;
        if let Some((top, _)) = f.split_once('/') {
            *folders.entry(top.to_string()).or_default() += 1;
        }
        if docs.len() < 40 && is_doc(f) {
            docs.push(f.clone());
        }
    }
    langs.remove("Other");

    let mut context = ContextStatus::default();
    if let Some(dir) = project::context::context_dir(ws, project_id) {
        for (file, is_overview) in [("overview.md", true), ("architecture.md", false)] {
            if let Ok((d, _)) = store::read(&dir.join(file)) {
                if is_overview {
                    context.overview_id = d.id().map(str::to_string);
                    context.overview_generated = d.body.contains("_Created by NEXUS from the project's files");
                } else {
                    context.architecture_id = d.id().map(str::to_string);
                    context.architecture_summary = !d.body.contains("_Not written yet._");
                }
            }
        }
    }
    context.notes = project_items(conn, ws, project_id, Kind::Note)?.len();
    context.decisions = project_items(conn, ws, project_id, Kind::Decision)?.len();
    let tasks = project_items(conn, ws, project_id, Kind::Task)?;
    context.open_tasks = tasks.iter().filter(|t| t.status.as_deref() != Some("done")).count();
    let task = tasks
        .iter()
        .filter(|t| t.status.as_deref() != Some("done") && labels(t).iter().any(|l| l == LEARN_LABEL))
        .find_map(|t| Some(LearnTask { id: t.id.clone()?, title: t.title.clone(), status: t.status.clone(), depth: Depth::of_labels(&labels(t)).unwrap_or_default() }));

    let report = root.join(GRAPH_DIR).join("GRAPH_REPORT.md");
    let built_ms = std::fs::metadata(&report).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64);
    let graph = GraphStatus {
        installed: toolchain::find("graphify").is_some(),
        built: built_ms.is_some(),
        report: built_ms.map(|_| format!("{GRAPH_DIR}/GRAPH_REPORT.md")),
        built_ms,
        ignored: git::ignored_paths(&root, &[format!("{GRAPH_DIR}/")]).contains(&format!("{GRAPH_DIR}/")),
        command: GRAPH_COMMAND,
    };
    let drafts = open_draft_notes(conn, ws, project_id)?.len();

    Ok(ProjectMap {
        project_id: p.id.clone(),
        name: p.name.clone(),
        files: files.len(),
        files_truncated,
        languages: top_counts(langs, 8),
        folders: top_counts(folders, 12),
        docs,
        stacks: p.stack.stacks.clone(),
        package_managers: p.stack.package_managers.clone(),
        scripts: p.stack.scripts.iter().map(|s| (s.kind.clone(), s.command.clone())).collect(),
        history: git::history_stats(&root),
        context,
        graph,
        task,
        drafts,
        depths: Depth::ALL.into_iter().map(DepthInfo::of).collect(),
        skill: crate::claude::builtin::LEARN_SKILL,
    })
}

/// The checklist step for a built graph (also added to a learning task created before the graph existed).
const GRAPH_STEP: &str = "- [ ] Read the Graphify report (graphify-out/GRAPH_REPORT.md); use `/graphify query` if the skill is available";

/// The learning session's brief: what to do, with NEXUS's map as data.
fn brief(m: &ProjectMap) -> String {
    let mut b = String::from(
        "Build the knowledge base for this project so future sessions start informed. Read the code and the documents; \
         do not change any file in the repository.\n\n",
    );
    b.push_str("- [ ] Read NEXUS's map below and the project's own documentation\n");
    if m.graph.built {
        b.push_str(GRAPH_STEP);
        b.push('\n');
    }
    b.push_str("- [ ] Draft the overview with `draft_project_document` (topic overview): purpose, users, main features, how to run and test it\n");
    b.push_str("- [ ] Draft the architecture summary with `draft_project_document` (topic architecture): components, data flow, boundaries, key decisions\n");
    b.push_str("- [ ] Record 3–10 durable facts with `record_memory` (notes; proposed decisions for choices that were made)\n");
    b.push_str("- [ ] Log open questions and gaps with `update_task`\n\n");
    b.push_str("Say in every draft which parts you verified in the code and which you inferred. The person reviews the drafts and decides what enters the knowledge base.\n\n");
    b.push_str("## NEXUS's map (read from the repository by NEXUS, no model)\n\n");
    b.push_str(&format!("- Tracked files: {}{}\n", m.files, if m.files_truncated { "+" } else { "" }));
    if !m.languages.is_empty() {
        b.push_str(&format!("- Languages: {}\n", m.languages.iter().map(|c| format!("{} ({})", c.name, c.files)).collect::<Vec<_>>().join(", ")));
    }
    if !m.folders.is_empty() {
        b.push_str(&format!("- Top-level folders: {}\n", m.folders.iter().map(|c| format!("{}/ ({})", c.name, c.files)).collect::<Vec<_>>().join(", ")));
    }
    if !m.stacks.is_empty() {
        b.push_str(&format!("- Stacks: {}\n", m.stacks.join(", ")));
    }
    if !m.scripts.is_empty() {
        b.push_str(&format!("- Commands: {}\n", m.scripts.iter().map(|(k, c)| format!("{k}: `{c}`")).collect::<Vec<_>>().join("; ")));
    }
    if !m.docs.is_empty() {
        b.push_str(&format!("- Documentation: {}\n", m.docs.iter().take(20).cloned().collect::<Vec<_>>().join(", ")));
    }
    if let Some(h) = &m.history {
        b.push_str(&format!("- History: {} commits by {} author(s)\n", h.commits, h.contributors));
    }
    b
}

/// Create (or reuse the open) learning task for the project at `depth`; the desktop then launches Claude on it.
pub fn start(conn: &Connection, data_dir: &Path, ws: &Workspace, project_id: &str, depth: Depth) -> Result<LearnTask> {
    let m = map(conn, data_dir, ws, project_id)?;
    if let Some(mut t) = m.task.clone() {
        if m.graph.built {
            add_graph_step(conn, data_dir, ws, &t.id)?;
        }
        if t.depth != depth {
            set_depth(conn, data_dir, ws, &t.id, depth)?;
            t.depth = depth;
        }
        return Ok(t);
    }
    let input = CreateInput {
        kind: "task".into(),
        title: format!("Learn {} — build its knowledge base", m.name),
        body: brief(&m),
        project_id: Some(project_id.to_string()),
        priority: Some("medium".into()),
        status: Some("ready".into()),
        labels: vec![LEARN_LABEL.into(), depth.label()],
        ..Default::default()
    };
    let item = service::create(conn, data_dir, ws, &input, Author::Human)?;
    Ok(LearnTask { id: item.id.clone().unwrap_or_default(), title: item.title, status: item.status, depth })
}

/// The learning task's depth label, replaced (the person chose another depth before starting again).
fn set_depth(conn: &Connection, data_dir: &Path, ws: &Workspace, task_id: &str, depth: Depth) -> Result<()> {
    let item = index::get(conn, &ws.id, task_id)?;
    let privacy = index::root_privacy(conn, &item.path)?;
    store::update(&item.path, None, privacy, |d| {
        let mut labels: Vec<String> = d.fm.list("labels").into_iter().filter(|l| !Depth::ALL.iter().any(|x| &x.label() == l)).collect();
        labels.push(depth.label());
        d.fm.set("labels", Value::List(labels));
        Ok(())
    })?;
    service::refresh(conn, data_dir, ws)?;
    Ok(())
}

/// A graph built after the learning task was created joins the task's checklist — once, after its first step (or at
/// the end when the list has no checklist lines). Written through the knowledge store like every task change.
fn add_graph_step(conn: &Connection, data_dir: &Path, ws: &Workspace, task_id: &str) -> Result<()> {
    let item = index::get(conn, &ws.id, task_id)?;
    let (doc, _) = store::read(&item.path)?;
    if doc.body.contains(&format!("{GRAPH_DIR}/GRAPH_REPORT.md")) {
        return Ok(());
    }
    let privacy = index::root_privacy(conn, &item.path)?;
    store::update(&item.path, None, privacy, |d| {
        d.body = with_step_after_first(&d.body, GRAPH_STEP);
        Ok(())
    })?;
    service::refresh(conn, data_dir, ws)?;
    Ok(())
}

/// `body` with `step` as a new line after its first checklist line, or at the end when it has none.
fn with_step_after_first(body: &str, step: &str) -> String {
    let mut lines: Vec<&str> = body.lines().collect();
    match lines.iter().position(|l| l.trim_start().starts_with("- [")) {
        Some(i) => lines.insert(i + 1, step),
        None => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push("");
            }
            lines.push(step);
        }
    }
    format!("{}\n", lines.join("\n"))
}

/// Claude's draft of the overview or the architecture summary (MCP `draft_project_document`): saved as an unverified
/// note labelled `draft` + topic, for the person to review.
pub fn record_draft(conn: &Connection, data_dir: &Path, ws: &Workspace, project_id: &str, topic: &str, body: &str) -> Result<Item> {
    if !TOPICS.contains(&topic) {
        return Err(CoreError::invalid("`topic` must be overview or architecture"));
    }
    let body = body.trim();
    if body.is_empty() || body.chars().count() > MAX_DRAFT_CHARS {
        return Err(CoreError::invalid(format!("a draft needs 1–{MAX_DRAFT_CHARS} characters")));
    }
    let p = project::get(conn, project_id)?;
    let input = CreateInput {
        kind: "note".into(),
        title: format!("{} — {topic} draft", p.name),
        body: body.to_string(),
        project_id: Some(project_id.to_string()),
        labels: vec![DRAFT_LABEL.into(), topic.into()],
        ..Default::default()
    };
    service::create(conn, data_dir, ws, &input, Author::Claude)
}

/// Drafts waiting for the person (newest first).
pub fn drafts(conn: &Connection, ws: &Workspace, project_id: &str) -> Result<Vec<Draft>> {
    let mut out: Vec<Draft> = open_draft_notes(conn, ws, project_id)?
        .into_iter()
        .filter_map(|(i, topic)| {
            let (d, _) = store::read(&i.path).ok()?;
            Some(Draft { id: i.id.clone()?, topic, title: i.title.clone(), updated: i.updated.clone(), body: d.body.trim().to_string() })
        })
        .collect();
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(out)
}

fn relabel(item: &Item, conn: &Connection, from: &str, to: &str) -> Result<()> {
    let privacy = index::root_privacy(conn, &item.path)?;
    store::update(&item.path, None, privacy, |d| {
        let mut l = match d.fm.get("labels") {
            Some(Value::List(l)) => l.clone(),
            _ => Vec::new(),
        };
        l.retain(|x| x != from);
        if !l.iter().any(|x| x == to) {
            l.push(to.into());
        }
        d.fm.set("labels", Value::List(l));
        Ok(())
    })?;
    Ok(())
}

fn draft_item(conn: &Connection, ws: &Workspace, project_id: &str, draft_id: &str) -> Result<(Item, String)> {
    open_draft_notes(conn, ws, project_id)?
        .into_iter()
        .find(|(i, _)| i.id.as_deref() == Some(draft_id))
        .ok_or_else(|| CoreError::not_found("no such draft waiting for review"))
}

/// The person accepts a draft: it becomes the overview, or fills the architecture summary (the verified structure
/// section stays). The draft note is kept, labelled `applied`.
pub fn apply_draft(conn: &Connection, data_dir: &Path, ws: &Workspace, project_id: &str, draft_id: &str) -> Result<()> {
    let (note, topic) = draft_item(conn, ws, project_id, draft_id)?;
    let (draft, _) = store::read(&note.path)?;
    let dir = project::context::context_dir(ws, project_id).ok_or_else(|| CoreError::invalid("the workspace keeps no project context (schema 2 needed)"))?;
    let today = crate::knowledge::timestamp(crate::now_ms()).chars().take(10).collect::<String>();
    let target = dir.join(format!("{topic}.md"));
    let privacy = index::root_privacy(conn, &target)?;
    let text = draft.body.trim();
    store::update(&target, None, privacy, |d| {
        let source = format!("_Drafted by Claude in a NEXUS session and accepted on {today}. Facts marked as inferred still need checking._");
        if topic == "overview" {
            d.body = format!("{source}\n\n{text}\n");
        } else {
            let summary = text.strip_prefix("## Architecture summary").map(str::trim_start).unwrap_or(text);
            let head = d.body.split("\n## Architecture summary").next().unwrap_or(&d.body).trim_end().to_string();
            d.body = format!("{head}\n\n## Architecture summary\n\n{source}\n\n{summary}\n");
            d.fm.set("status", Value::Str("active".into()));
        }
        Ok(())
    })?;
    relabel(&note, conn, DRAFT_LABEL, "applied")?;
    service::refresh(conn, data_dir, ws)?;
    // The person accepted this content here: the new version is approved on this machine.
    if let Some(id) = store::read(&target).ok().and_then(|(d, _)| d.id().map(str::to_string))
        && let Ok(it) = index::get(conn, &ws.id, &id)
    {
        index::approve(conn, &it.path, &it.hash)?;
    }
    finish_if_reviewed(conn, data_dir, ws, project_id)?;
    Ok(())
}

/// The person sets a draft aside: kept as a note, labelled `discarded`.
pub fn discard_draft(conn: &Connection, data_dir: &Path, ws: &Workspace, project_id: &str, draft_id: &str) -> Result<()> {
    let (note, _) = draft_item(conn, ws, project_id, draft_id)?;
    relabel(&note, conn, DRAFT_LABEL, "discarded")?;
    service::refresh(conn, data_dir, ws)?;
    finish_if_reviewed(conn, data_dir, ws, project_id)?;
    Ok(())
}

/// The learning task is done once the person reviewed the result: no draft waits, the overview is no longer NEXUS's
/// generated text and the architecture summary is written. Returns whether a task was closed.
pub fn finish_if_reviewed(conn: &Connection, data_dir: &Path, ws: &Workspace, project_id: &str) -> Result<bool> {
    let m = map(conn, data_dir, ws, project_id)?;
    let Some(task) = m.task else { return Ok(false) };
    if m.drafts > 0 || m.context.overview_generated || !m.context.architecture_summary {
        return Ok(false);
    }
    let log = "Learning reviewed: overview and architecture summary accepted";
    let u = crate::knowledge::tasks::TaskUpdate { status: Some("done"), log: Some(log), ..Default::default() };
    crate::knowledge::tasks::update(conn, data_dir, ws, &task.id, &u, Author::Human)?;
    Ok(true)
}

/// Keep `graphify-out/` out of Git in this clone: one line in the local exclude file (`.git/info/exclude`), which is
/// never committed. Returns whether a line was added.
pub fn exclude_graph_output(conn: &Connection, project_id: &str) -> Result<bool> {
    let p = project::get(conn, project_id)?;
    let root = p.local_path.filter(|_| p.available).ok_or_else(|| CoreError::invalid("the project is not on this computer"))?;
    if git::ignored_paths(&root, &[format!("{GRAPH_DIR}/")]).contains(&format!("{GRAPH_DIR}/")) {
        return Ok(false);
    }
    let file = git::local_exclude_file(&root).ok_or_else(|| CoreError::invalid("the project is not a Git repository"))?;
    let mut text = std::fs::read_to_string(&file).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("# Graphify output (added by NEXUS; this file is local to this clone)\n/{GRAPH_DIR}/\n"));
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| CoreError::io("cannot create", parent, e))?;
    }
    crate::workspace::write_atomic(&file, text.as_bytes())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(["-c", "user.name=Ann", "-c", "user.email=ann@example.invalid", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
    }

    /// W4: NEXUS maps the project without a model; the learning task carries the map; Claude's drafts wait for the
    /// person, who applies one to the overview and the architecture summary or discards it; graphify-out/ is kept out
    /// of Git locally.
    #[test]
    fn a_project_is_learned_through_reviewed_drafts() {
        let t = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let data = base.join("data");
        let ws = crate::workspace::create(&conn, base.join("ws").to_str().unwrap(), "W", "").unwrap();
        let repo = base.join("shop");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(repo.join("docs")).unwrap();
        std::fs::write(repo.join("src").join("main.ts"), "export {}\n").unwrap();
        std::fs::write(repo.join("src").join("cart.ts"), "export {}\n").unwrap();
        std::fs::write(repo.join("docs").join("setup.md"), "# Setup\n").unwrap();
        std::fs::write(repo.join("README.md"), "# Shop\n\nSells things.\n").unwrap();
        std::fs::write(repo.join("package.json"), r#"{"dependencies":{"vue":"^3"},"scripts":{"test":"vitest"}}"#).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "init"]);
        let p = project::register_with_context(&conn, &data, &ws.id, repo.to_str().unwrap(), None).unwrap();

        let m = map(&conn, &data, &ws, &p.id).unwrap();
        assert_eq!(m.files, 5);
        assert_eq!(m.languages.iter().find(|c| c.name == "TypeScript").map(|c| c.files), Some(2));
        assert_eq!(m.folders.first().map(|c| (c.name.as_str(), c.files)), Some(("src", 2)));
        assert!(m.docs.contains(&"README.md".to_string()) && m.docs.contains(&"docs/setup.md".to_string()), "{:?}", m.docs);
        assert_eq!(m.history.as_ref().map(|h| (h.commits, h.contributors)), Some((1, 1)));
        assert!(m.context.overview_generated && !m.context.architecture_summary);
        assert!(!m.graph.built && m.task.is_none());

        // The learning task carries the map as its brief; starting again reuses it.
        let task = start(&conn, &data, &ws, &p.id, Depth::Standard).unwrap();
        let brief = service::read(&conn, &ws, &task.id, 50_000).unwrap().body;
        assert!(brief.contains("draft_project_document") && brief.contains("TypeScript (2)") && brief.contains("do not change any file"), "{brief}");
        assert_eq!(start(&conn, &data, &ws, &p.id, Depth::Standard).unwrap().id, task.id);

        // Claude drafts; the person applies one and discards the other.
        let ov = record_draft(&conn, &data, &ws, &p.id, "overview", "## What it is\n\nA web shop (verified: src/main.ts).").unwrap();
        let ar = record_draft(&conn, &data, &ws, &p.id, "architecture", "## Architecture summary\n\nA Vue front end; cart logic in src/cart.ts (inferred).").unwrap();
        record_draft(&conn, &data, &ws, &p.id, "architecture", "Second opinion.").unwrap();
        assert!(record_draft(&conn, &data, &ws, &p.id, "security", "x").is_err());
        assert_eq!(drafts(&conn, &ws, &p.id).unwrap().len(), 3);
        assert_eq!(map(&conn, &data, &ws, &p.id).unwrap().drafts, 3);

        apply_draft(&conn, &data, &ws, &p.id, ov.id.as_deref().unwrap()).unwrap();
        apply_draft(&conn, &data, &ws, &p.id, ar.id.as_deref().unwrap()).unwrap();
        let ctx = project::context::context_dir(&ws, &p.id).unwrap();
        let overview = std::fs::read_to_string(ctx.join("overview.md")).unwrap();
        assert!(overview.contains("A web shop") && overview.contains("accepted on") && !overview.contains("_Created by NEXUS from the project's files"));
        let arch = std::fs::read_to_string(ctx.join("architecture.md")).unwrap();
        assert!(arch.contains("## Structure (verified from the file system)"), "the verified structure stays");
        assert!(arch.contains("cart logic in src/cart.ts (inferred)") && !arch.contains("_Not written yet._") && arch.contains("status: active"), "{arch}");
        assert_eq!(arch.matches("## Architecture summary").count(), 1);
        let left = drafts(&conn, &ws, &p.id).unwrap();
        assert_eq!(left.len(), 1);
        discard_draft(&conn, &data, &ws, &p.id, &left[0].id).unwrap();
        assert!(drafts(&conn, &ws, &p.id).unwrap().is_empty());
        assert!(apply_draft(&conn, &data, &ws, &p.id, ov.id.as_deref().unwrap()).is_err(), "an applied draft is not applied twice");
        let m = map(&conn, &data, &ws, &p.id).unwrap();
        assert!(!m.context.overview_generated && m.context.architecture_summary);
        // The review finished the learning: its task is done (and no longer the open learning task).
        assert!(m.task.is_none(), "the review closed the learning task");
        assert_eq!(service::read(&conn, &ws, &task.id, 50_000).unwrap().item.status.as_deref(), Some("done"));
        assert!(!finish_if_reviewed(&conn, &data, &ws, &p.id).unwrap(), "nothing left to close");

        // Learning again starts a new task (before any graph exists).
        let task = start(&conn, &data, &ws, &p.id, Depth::Standard).unwrap();
        assert!(!service::read(&conn, &ws, &task.id, 50_000).unwrap().body.contains("GRAPH_REPORT"));

        // graphify-out/ is excluded in this clone only (nothing committed changes).
        assert!(exclude_graph_output(&conn, &p.id).unwrap());
        assert!(!exclude_graph_output(&conn, &p.id).unwrap(), "only once");
        assert!(map(&conn, &data, &ws, &p.id).unwrap().graph.ignored);
        let status = Command::new("git").args(["status", "--porcelain"]).current_dir(&repo).output().unwrap();
        assert!(String::from_utf8_lossy(&status.stdout).trim().is_empty(), "no tracked or untracked file changed");
        std::fs::create_dir_all(repo.join(GRAPH_DIR)).unwrap();
        std::fs::write(repo.join(GRAPH_DIR).join("GRAPH_REPORT.md"), "# Report\n").unwrap();
        let m = map(&conn, &data, &ws, &p.id).unwrap();
        assert!(m.graph.built && m.graph.report.as_deref() == Some("graphify-out/GRAPH_REPORT.md"));

        // A graph built after the task was created joins its checklist, after the first step, once.
        assert_eq!(start(&conn, &data, &ws, &p.id, Depth::Standard).unwrap().id, task.id);
        assert_eq!(start(&conn, &data, &ws, &p.id, Depth::Standard).unwrap().id, task.id);
        let brief = service::read(&conn, &ws, &task.id, 50_000).unwrap().body;
        assert_eq!(brief.matches("graphify-out/GRAPH_REPORT.md").count(), 1, "{brief}");
        let steps: Vec<&str> = brief.lines().filter(|l| l.starts_with("- [")).collect();
        assert!(steps[0].contains("NEXUS's map") && steps[1].contains("Graphify report"), "{steps:?}");
        assert_eq!(crate::knowledge::tasks::checklist(crate::knowledge::tasks::split_log(&brief).0).len(), steps.len(), "every step is a checklist item");

        // The depth is a label on the task; choosing another one replaces it (one depth label, the learn label stays).
        let labels_of = |id: &str| index::get(&conn, &ws.id, id).map(|i| labels(&i)).unwrap();
        assert_eq!(Depth::of_labels(&labels_of(&task.id)), Some(Depth::Standard));
        let t = start(&conn, &data, &ws, &p.id, Depth::Quick).unwrap();
        assert_eq!((t.id.as_str(), t.depth), (task.id.as_str(), Depth::Quick));
        let l = labels_of(&task.id);
        assert!(l.contains(&LEARN_LABEL.to_string()) && l.contains(&"learn-quick".to_string()) && !l.contains(&"learn-standard".to_string()), "{l:?}");
        assert_eq!(map(&conn, &data, &ws, &p.id).unwrap().task.unwrap().depth, Depth::Quick);
    }

    #[test]
    fn depths_have_budgets_and_rules_carry_them_without_task_text() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(Depth::of_labels(&s(&["bug"])), None, "not a learning task");
        assert_eq!(Depth::of_labels(&s(&["learn"])), Some(Depth::Standard), "older learning tasks: the default");
        assert_eq!(Depth::of_labels(&s(&["learn", "learn-deep"])), Some(Depth::Deep));
        assert_eq!(Depth::parse(" Quick "), Some(Depth::Quick));
        assert_eq!(Depth::parse("huge"), None);
        let quick = rules(Depth::Quick);
        assert!(quick.contains("LEARN-2") && quick.contains("open at most 20 files") && quick.contains("at most 250 words") && quick.contains("nexus:learn-project"), "{quick}");
        assert!(rules(Depth::Deep).contains("no file limit"));
        assert_eq!((1..=7).filter(|i| quick.contains(&format!("LEARN-{i} "))).count(), 7, "seven stable rule ids");
        assert!(quick.chars().count() < 1200, "compact: {} chars", quick.chars().count());
    }

    #[test]
    fn a_step_goes_after_the_first_checklist_line_or_at_the_end() {
        assert_eq!(with_step_after_first("Intro\n\n- [ ] a\n- [x] b\n", "- [ ] g"), "Intro\n\n- [ ] a\n- [ ] g\n- [x] b\n");
        assert_eq!(with_step_after_first("Only text.", "- [ ] g"), "Only text.\n\n- [ ] g\n");
        assert_eq!(with_step_after_first("", "- [ ] g"), "- [ ] g\n");
    }
}

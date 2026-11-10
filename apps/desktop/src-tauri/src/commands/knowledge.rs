//! Portable workspace knowledge and tasks (ADR-0013/0017). The desktop is a human author: it may create every kind
//! and approve always-on rules; Claude goes through the MCP server with narrower rights.

use tauri::State;

use serde::Serialize;

use nexus_core::claude::prepare::{self, Preview, PreviewRequest};
use nexus_core::claude::profile;
use nexus_core::knowledge::Kind;
use nexus_core::knowledge::branches::{self, GitRef, Relevance};
use nexus_core::knowledge::index::{self as kindex, Hit, Item};
use nexus_core::knowledge::service::{self as knowledge, Author, CreateInput, DocView, ItemView, Overview};
use nexus_core::knowledge::tasks::{self, TaskUpdate, TaskView};
use nexus_core::workspace::{self, Workspace};
use nexus_core::{env, paths, project};

use crate::error::{IpcError, IpcResult, blocking};
use crate::state::Ctx;

fn available(ctx: &Ctx, workspace_id: &str) -> IpcResult<Workspace> {
    let ws = workspace::get(&ctx.db(), workspace_id)?;
    if !ws.available {
        return Err(IpcError::invalid("the workspace folder is not available on this machine"));
    }
    Ok(ws)
}

/// Refresh the index from the files (or rebuild it from scratch) and return every document, including invalid ones.
#[tauri::command]
pub async fn knowledge_overview(ctx: State<'_, Ctx>, workspace_id: String, rebuild: Option<bool>) -> IpcResult<Overview> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(knowledge::overview(&ctx.db(), &ctx.data_dir, &ws, rebuild.unwrap_or(false))?)
    })
    .await
}

#[tauri::command]
pub async fn knowledge_read(ctx: State<'_, Ctx>, workspace_id: String, id: String) -> IpcResult<DocView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(knowledge::read(&ctx.db(), &ws, &id, 200_000)?)
    })
    .await
}

#[tauri::command]
pub async fn knowledge_search(ctx: State<'_, Ctx>, workspace_id: String, query: String, kinds: Option<Vec<String>>) -> IpcResult<Vec<Hit>> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        let kinds = kinds
            .unwrap_or_default()
            .iter()
            .map(|k| Kind::parse(k).ok_or_else(|| IpcError::invalid(format!("unknown kind {k}"))))
            .collect::<IpcResult<Vec<_>>>()?;
        let db = ctx.db();
        knowledge::refresh(&db, &ctx.data_dir, &ws)?;
        Ok(kindex::search(&db, &ws.id, &query, &kinds, 30)?)
    })
    .await
}

#[tauri::command]
pub async fn knowledge_create(ctx: State<'_, Ctx>, workspace_id: String, input: CreateInput) -> IpcResult<Item> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(knowledge::create(&ctx.db(), &ctx.data_dir, &ws, &input, Author::Human)?)
    })
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn knowledge_update_task(
    ctx: State<'_, Ctx>,
    workspace_id: String,
    id: String,
    status: Option<String>,
    log: Option<String>,
    expected_hash: Option<String>,
    check: Option<Vec<usize>>,
    uncheck: Option<Vec<usize>>,
) -> IpcResult<Item> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        let (check, uncheck) = (check.unwrap_or_default(), uncheck.unwrap_or_default());
        let u = TaskUpdate {
            status: status.as_deref(),
            log: log.as_deref(),
            check: &check,
            uncheck: &uncheck,
            link: None,
            expected_hash: expected_hash.as_deref(),
        };
        Ok(tasks::update(&ctx.db(), &ctx.data_dir, &ws, &id, &u, Author::Human)?)
    })
    .await
}

/// Compact task view: status, checklist, recent log (ADR-0018).
#[tauri::command]
pub async fn knowledge_task(ctx: State<'_, Ctx>, workspace_id: String, id: String) -> IpcResult<TaskView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        let db = ctx.db();
        knowledge::refresh(&db, &ctx.data_dir, &ws)?;
        Ok(tasks::view(&db, &ws, &id, 20_000)?)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDoc {
    #[serde(flatten)]
    pub view: ItemView,
    pub relevance: Relevance,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectKnowledge {
    /// Live Git state (Git is authoritative for branches).
    pub git: GitRef,
    /// Open tasks of the project (and unscoped ones): ready for review first, then in progress, then the rest.
    pub tasks: Vec<ItemView>,
    /// The most recently finished tasks (to reopen one).
    pub done_tasks: Vec<ItemView>,
    /// Notes and decisions visible on the checked-out branch, with their relevance.
    pub notes: Vec<ProjectDoc>,
    /// Documents of other branches, hidden here.
    pub hidden_other_branch: usize,
}

/// Knowledge of one project for its checked-out branch (ADR-0018).
#[tauri::command]
pub async fn project_knowledge(ctx: State<'_, Ctx>, project_id: String) -> IpcResult<ProjectKnowledge> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let p = project::get(&ctx.db(), &project_id)?;
        let ws = available(&ctx, &p.workspace_id)?;
        let repo = p.local_path.clone().filter(|l| l.is_dir());
        let (git, history) = match &repo {
            Some(r) => (GitRef::of(r), branches::history(r)),
            None => (GitRef::default(), Default::default()),
        };
        let db = ctx.db();
        let o = knowledge::overview(&db, &ctx.data_dir, &ws, false)?;
        let mine = |v: &ItemView| v.item.valid && v.item.conflict.is_none() && (v.item.project_id.is_none() || v.item.project_id.as_deref() == Some(p.id.as_str()));
        let mut tasks = Vec::new();
        let mut done_tasks = Vec::new();
        let mut notes = Vec::new();
        let mut hidden = 0;
        for v in o.items.into_iter().filter(mine) {
            let rel = branches::relevance(&v.item, &git, &history);
            match v.item.kind.as_deref() {
                Some("task") if v.item.status.as_deref() == Some("done") => done_tasks.push(v),
                Some("task") => tasks.push(v),
                Some("note" | "decision") if rel.visible() => notes.push(ProjectDoc { view: v, relevance: rel }),
                Some("note" | "decision") => hidden += 1,
                _ => {}
            }
        }
        notes.sort_by_key(|n| n.relevance != Relevance::ThisBranch);
        // What needs the person first: work Claude says is complete, then work in progress; urgent before the rest.
        let rank = |s: Option<&str>| match s {
            Some("review") => 0,
            Some("in-progress") => 1,
            Some("blocked") => 2,
            Some("ready") => 3,
            _ => 4,
        };
        let urgency = |p: Option<&str>| match p {
            Some("urgent") => 0,
            Some("high") => 1,
            _ => 2,
        };
        tasks.sort_by_key(|v| (rank(v.item.status.as_deref()), urgency(v.item.priority.as_deref())));
        done_tasks.sort_by(|a, b| b.item.updated.cmp(&a.item.updated));
        done_tasks.truncate(5);
        Ok(ProjectKnowledge { git, tasks, done_tasks, notes, hidden_other_branch: hidden })
    })
    .await
}

/// What waits for the person in the workspace, or in one project (ADR-0048): one list, each item naming what to open.
#[tauri::command]
pub async fn needs_you(ctx: State<'_, Ctx>, workspace_id: String, project_id: Option<String>) -> IpcResult<Vec<nexus_core::inbox::Item>> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(nexus_core::inbox::needs_you(&ctx.db(), &ctx.data_dir, &ws, project_id.as_deref())?)
    })
    .await
}

/// Memory proposals of the workspace (ADR-0022); `extract` first scans recent sessions.
#[tauri::command]
pub async fn knowledge_proposals(ctx: State<'_, Ctx>, workspace_id: String, extract: Option<bool>) -> IpcResult<Vec<nexus_core::knowledge::proposals::Proposal>> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        let db = ctx.db();
        if extract.unwrap_or(true) {
            knowledge::refresh(&db, &ctx.data_dir, &ws)?;
            nexus_core::knowledge::proposals::extract_recent(&db, &ws.id)?;
        }
        Ok(nexus_core::knowledge::proposals::list(&db, &ws.id, Some("pending"))?)
    })
    .await
}

/// A person approves a proposal: only now does it become a knowledge document.
#[tauri::command]
pub async fn knowledge_proposal_approve(
    ctx: State<'_, Ctx>,
    workspace_id: String,
    id: String,
    approval: nexus_core::knowledge::proposals::Approval,
) -> IpcResult<Item> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(nexus_core::knowledge::proposals::approve(&ctx.db(), &ctx.data_dir, &ws, &id, &approval)?)
    })
    .await
}

#[tauri::command]
pub fn knowledge_proposal_reject(ctx: State<'_, Ctx>, id: String) -> IpcResult<()> {
    Ok(nexus_core::knowledge::proposals::reject(&ctx.db(), &id)?)
}

/// "Resume project" (ADR-0021): verified Git state, saved history and one recommended next action.
#[tauri::command]
pub async fn project_resume(ctx: State<'_, Ctx>, project_id: String) -> IpcResult<nexus_core::resume::ResumeSummary> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let db = ctx.db();
        // Sessions that ended without NEXUS hooks (e.g. NEXUS was closed) still get their proposals surfaced.
        let ws = nexus_core::project::get(&db, &project_id)?.workspace_id;
        nexus_core::knowledge::proposals::extract_recent(&db, &ws)?;
        Ok(nexus_core::resume::summarize(&db, &ctx.data_dir, &ctx.home, &project_id)?)
    })
    .await
}

/// Effective-instructions inspector (ADR-0019): what Claude loads for this profile and folder, and what NEXUS
/// would append. Read-only.
#[tauri::command]
pub async fn instructions_inspect(
    ctx: State<'_, Ctx>,
    profile_id: String,
    project_id: Option<String>,
    cwd: Option<String>,
    task_id: Option<String>,
    work_profile: Option<String>,
) -> IpcResult<Preview> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let db = ctx.db();
        let prof = profile::get(&db, &profile_id)?;
        let (cwd, workspace_id) = match project_id.as_deref() {
            Some(pid) => {
                let p = project::get(&db, pid)?;
                let dir = p.local_path.clone().filter(|l| l.is_dir()).ok_or_else(|| IpcError::invalid("project folder is not available on this machine"))?;
                (dir, Some(p.workspace_id))
            }
            None => {
                let raw = cwd.ok_or_else(|| IpcError::invalid("choose a project or a folder"))?;
                (paths::validate_user_path(&raw, paths::Expect::ExistingDir)?, workspace::active(&db)?.map(|w| w.id))
            }
        };
        let base_env = env::current();
        Ok(prepare::preview(&PreviewRequest {
            conn: &db,
            data_dir: &ctx.data_dir,
            home: &ctx.home,
            profile: &prof,
            machine: &ctx.machine_policy,
            workspace_id: workspace_id.as_deref(),
            project_id: project_id.as_deref(),
            task_id: task_id.as_deref(),
            cwd: &cwd,
            base_env: &base_env,
            work_profile: work_profile.as_deref(),
        })?)
    })
    .await
}

/// The person's edit of a document's title and body (W2): optimistic (`expected_hash`), validated; a constitution goes
/// through its parser; the saved version is approved on this machine because the person wrote it here.
#[tauri::command]
pub async fn knowledge_update(
    ctx: State<'_, Ctx>,
    workspace_id: String,
    id: String,
    title: Option<String>,
    body: String,
    expected_hash: Option<String>,
) -> IpcResult<Item> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(knowledge::update_doc(&ctx.db(), &ctx.data_dir, &ws, &id, title.as_deref(), &body, expected_hash.as_deref())?)
    })
    .await
}

/// Approve the current version of an always-on rule on this machine (threat T19).
#[tauri::command]
pub async fn knowledge_approve(ctx: State<'_, Ctx>, workspace_id: String, id: String) -> IpcResult<Item> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let ws = available(&ctx, &workspace_id)?;
        Ok(knowledge::approve(&ctx.db(), &ctx.data_dir, &ws, &id)?)
    })
    .await
}

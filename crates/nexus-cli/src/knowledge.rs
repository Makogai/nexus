//! `nexus knowledge …` and `nexus tasks …` — portable workspace knowledge (ADR-0013/0017) from the terminal. Thin
//! layer over `nexus_core::knowledge::service`; people are the author here, so every kind can be created.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use serde::Serialize;

use nexus_core::knowledge::branches::{self, GitRef};
use nexus_core::knowledge::index::{self as kindex, Item};
use nexus_core::knowledge::proposals::{self, Approval};
use nexus_core::knowledge::tasks::{self, TaskUpdate};
use nexus_core::knowledge::service::{self as knowledge, Author, CreateInput, ItemView};
use nexus_core::knowledge::{Kind, MAX_FILE_BYTES, context};
use nexus_core::project::{self, Project};
use nexus_core::workspace::Workspace;
use nexus_core::{CoreError, resolve_ref};

use crate::{Ctx, active_workspace, out};

#[derive(Subcommand)]
pub enum KnowledgeCmd {
    /// Re-scan the knowledge files into the index; --rebuild drops the index and recreates it from the files.
    Reindex {
        #[arg(long)]
        rebuild: bool,
    },
    /// List documents of the active workspace (and global ones).
    List {
        /// rule, task, decision, note, spec or workflow (repeatable).
        #[arg(long = "kind", value_name = "KIND")]
        kinds: Vec<String>,
        /// Also list files that failed validation or share an id (index health).
        #[arg(long)]
        invalid: bool,
        /// Only documents for this project (and unscoped ones), with their relevance for its checked-out branch.
        #[arg(long)]
        project: Option<String>,
    },
    /// Full-text search.
    Search {
        query: String,
        #[arg(long = "kind", value_name = "KIND")]
        kinds: Vec<String>,
        #[arg(long, default_value_t = 10)]
        limit: u32,
    },
    /// Show one document (id, its last digits, or exact title).
    Show { id: String },
    /// Rules that apply to a project, and what a Claude launch receives in its system prompt.
    Rules {
        #[arg(long)]
        project: Option<String>,
    },
    /// Approve the current version of an always-on workspace rule on this machine (threat T19).
    Approve { id: String },
    /// Create a document.
    New(Box<NewArgs>),
    /// Memory proposed from session activity — reviewed and approved before it becomes knowledge (ADR-0022).
    #[command(subcommand)]
    Proposals(ProposalsCmd),
}

#[derive(Subcommand)]
pub enum ProposalsCmd {
    /// Pending proposals (extracts from recent sessions first).
    List {
        /// Also approved and rejected ones.
        #[arg(long)]
        all: bool,
    },
    /// Approve a proposal: it becomes a knowledge document (optionally edited).
    Approve {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
        /// workspace | project (default) | branch (the session's branch)
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        local: bool,
    },
    /// Reject a proposal (it is not proposed again).
    Reject { id: String },
}

#[derive(Args)]
pub struct NewArgs {
    /// rule, task, decision, note, spec or workflow.
    kind: String,
    #[arg(long)]
    title: String,
    #[arg(long, default_value = "")]
    body: String,
    /// Read the Markdown body from FILE.
    #[arg(long, value_name = "FILE", conflicts_with = "body")]
    body_file: Option<PathBuf>,
    /// Limit to a project of the active workspace.
    #[arg(long)]
    project: Option<String>,
    /// Keep on this machine only (`.nexus-local/`, never synced).
    #[arg(long)]
    local: bool,
    /// Global knowledge for every workspace on this machine (e.g. a constitution).
    #[arg(long, conflicts_with_all = ["project", "local"])]
    global: bool,
    /// Rules: always | on-demand.
    #[arg(long)]
    applies: Option<String>,
    /// Rules: limit to projects with this stack (repeatable).
    #[arg(long = "stack", value_name = "STACK")]
    stacks: Vec<String>,
    /// Tasks: low | medium | high | urgent.
    #[arg(long)]
    priority: Option<String>,
    #[arg(long)]
    status: Option<String>,
    #[arg(long = "label", value_name = "LABEL")]
    labels: Vec<String>,
    /// Tie the document to a Git branch: a branch name, or `current` for the project's checked-out branch (also
    /// records its HEAD commit). Requires --project.
    #[arg(long, requires = "project")]
    branch: Option<String>,
}

#[derive(Subcommand)]
pub enum TasksCmd {
    /// Tasks of the active workspace.
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        project: Option<String>,
    },
    /// Create a task.
    Add {
        title: String,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        local: bool,
        #[arg(long, default_value = "")]
        body: String,
    },
    /// Change a task's status (backlog, ready, in-progress, review, blocked, done), optionally logging a line.
    Move {
        id: String,
        status: String,
        #[arg(long)]
        log: Option<String>,
        /// Tick checklist items (1-based numbers, repeatable).
        #[arg(long = "check", value_name = "N")]
        check: Vec<usize>,
    },
    /// Show a task: status, checklist, recent log and the sessions that worked on it.
    Show { id: String },
}

fn kinds(names: &[String]) -> nexus_core::Result<Vec<Kind>> {
    names
        .iter()
        .map(|k| Kind::parse(k).ok_or_else(|| CoreError::invalid(format!("unknown kind '{k}' (rule, task, decision, note, spec, workflow)"))))
        .collect()
}

fn find_project(conn: &nexus_core::db::Connection, ws: &Workspace, q: &str) -> nexus_core::Result<Project> {
    let list = project::list(conn, &ws.id)?;
    resolve_ref(&list, q, |p| &p.id, |p| &p.name, "project").cloned()
}

/// Resolve an id, unique end/start of an id, or exact title to a document id (duplicates are reported by the service).
fn find_id(conn: &nexus_core::db::Connection, ws: &Workspace, q: &str) -> nexus_core::Result<String> {
    let items: Vec<Item> = knowledge::list(conn, ws, &[], false)?.into_iter().filter(|i| i.id.is_some()).collect();
    Ok(knowledge::resolve(&items, q)?.id.clone().unwrap_or_default())
}

fn rows(items: &[ItemView]) -> Vec<Vec<String>> {
    items.iter().map(rows_for).collect()
}

fn rows_for(v: &ItemView) -> Vec<String> {
    [v].into_iter()
        .map(|v| {
            let i = &v.item;
            let state = match (&i.error, &i.conflict) {
                (Some(e), _) => format!("INVALID: {e}"),
                (_, Some(c)) => format!("CONFLICT: {c}"),
                _ if v.needs_approval => format!("{} · needs approval", i.status.as_deref().unwrap_or("-")),
                _ => i.status.clone().unwrap_or_else(|| "-".into()),
            };
            let title = if i.valid { i.title.clone() } else { i.path.display().to_string() };
            vec![i.kind.clone().unwrap_or_else(|| "?".into()), knowledge::short_id(i.id.as_deref().unwrap_or("-")), v.scope.into(), state, title]
        })
        .next()
        .unwrap_or_default()
}

pub fn cmd_knowledge(ctx: &Ctx, cmd: KnowledgeCmd) -> nexus_core::Result<()> {
    let conn = ctx.db()?;
    let ws = active_workspace(&conn)?;
    match cmd {
        KnowledgeCmd::Proposals(p) => {
            let find = |q: &str| -> nexus_core::Result<proposals::Proposal> {
                let all = proposals::list(&conn, &ws.id, None)?;
                resolve_ref(&all, q, |p| &p.id, |p| &p.title, "proposal").cloned()
            };
            match p {
                ProposalsCmd::List { all } => {
                    knowledge::refresh(&conn, &ctx.data_dir, &ws)?;
                    proposals::extract_recent(&conn, &ws.id)?;
                    let list = proposals::list(&conn, &ws.id, if all { None } else { Some("pending") })?;
                    if ctx.json {
                        out::json(&list);
                    } else if list.is_empty() {
                        println!("no memory proposals waiting for review");
                    } else {
                        for p in &list {
                            println!("{} {:<8} {:<8} {}", out::short_id(&p.id), p.status, p.kind, p.title);
                            println!("         from “{}”{} — {}", p.session_title, p.branch.as_deref().map(|b| format!(" on {b}")).unwrap_or_default(), out::ago(Some(p.created_at)));
                        }
                        println!("\napprove with `nexus knowledge proposals approve <id> [--title …] [--scope workspace|project|branch]`, or reject");
                    }
                }
                ProposalsCmd::Approve { id, title, body, scope, local } => {
                    knowledge::refresh(&conn, &ctx.data_dir, &ws)?;
                    let p = find(&id)?;
                    let item = proposals::approve(&conn, &ctx.data_dir, &ws, &p.id, &Approval { title, body, scope, local_only: local })?;
                    if ctx.json {
                        out::json(&item);
                    } else {
                        println!("approved: {} “{}” → {}", item.kind.as_deref().unwrap_or("?"), item.title, item.path.display());
                    }
                }
                ProposalsCmd::Reject { id } => {
                    let p = find(&id)?;
                    proposals::reject(&conn, &p.id)?;
                    if !ctx.json {
                        println!("rejected “{}” (it will not be proposed again)", p.title);
                    }
                }
            }
        }
        KnowledgeCmd::Reindex { rebuild } => {
            let o = knowledge::overview(&conn, &ctx.data_dir, &ws, rebuild)?;
            if ctx.json {
                out::json(&o.report);
            } else {
                let r = &o.report;
                println!(
                    "{} {} files: {} changed, {} removed, {} invalid, {} conflicts",
                    if rebuild { "rebuilt from" } else { "scanned" },
                    r.scanned,
                    r.changed,
                    r.removed,
                    r.invalid,
                    r.conflicts
                );
            }
        }
        KnowledgeCmd::List { kinds: k, invalid, project } => {
            let wanted = kinds(&k)?;
            let o = knowledge::overview(&conn, &ctx.data_dir, &ws, false)?;
            let p = project.as_deref().map(|q| find_project(&conn, &ws, q)).transpose()?;
            let items: Vec<ItemView> = o
                .items
                .into_iter()
                .filter(|v| invalid || (v.item.valid && v.item.conflict.is_none()))
                .filter(|v| wanted.is_empty() || wanted.iter().any(|w| v.item.kind.as_deref() == Some(w.as_str())))
                .filter(|v| p.as_ref().is_none_or(|p| v.item.project_id.is_none() || v.item.project_id.as_ref() == Some(&p.id)))
                .collect();
            if let Some(p) = &p {
                // Branch relevance from the project's live Git state (Git stays authoritative).
                let repo = p.local_path.clone().unwrap_or_default();
                let git = GitRef::of(&repo);
                let history = branches::history(&repo);
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Row<'a> {
                    #[serde(flatten)]
                    view: &'a ItemView,
                    relevance: branches::Relevance,
                }
                let rows: Vec<Row> = items.iter().map(|v| Row { relevance: branches::relevance(&v.item, &git, &history), view: v }).collect();
                if ctx.json {
                    out::json(&rows);
                } else {
                    println!("{} on {}", p.name, git.branch.as_deref().unwrap_or("(no branch)"));
                    let table: Vec<Vec<String>> = rows
                        .iter()
                        .map(|r| {
                            let mut row = rows_for(r.view);
                            row.insert(3, match r.relevance {
                                branches::Relevance::Unbound => String::new(),
                                branches::Relevance::ThisBranch => "this branch".into(),
                                branches::Relevance::Inherited => format!("from {}", r.view.item.branch.as_deref().unwrap_or("history")),
                                branches::Relevance::OtherBranch => format!("other branch ({})", r.view.item.branch.as_deref().unwrap_or("?")),
                            });
                            row
                        })
                        .collect();
                    out::table(&["KIND", "ID", "SCOPE", "BRANCH", "STATUS", "TITLE"], &table);
                }
                return Ok(());
            }
            if ctx.json {
                out::json(&items);
            } else if items.is_empty() {
                println!("no knowledge documents in {} — create one with `nexus knowledge new <kind> --title …`", ws.name);
            } else {
                out::table(&["KIND", "ID", "SCOPE", "STATUS", "TITLE"], &rows(&items));
            }
        }
        KnowledgeCmd::Search { query, kinds: k, limit } => {
            let wanted = kinds(&k)?;
            knowledge::refresh(&conn, &ctx.data_dir, &ws)?;
            let hits = kindex::search(&conn, &ws.id, &query, &wanted, limit.clamp(1, 50))?;
            if ctx.json {
                out::json(&hits);
                return Ok(());
            }
            for h in &hits {
                println!("{} {:<8} {}", knowledge::short_id(h.item.id.as_deref().unwrap_or("-")), h.item.kind.as_deref().unwrap_or("?"), h.item.title);
                println!("         {}", h.snippet.replace('\n', " "));
            }
            if hits.is_empty() {
                println!("no matches");
            }
        }
        KnowledgeCmd::Show { id } => {
            knowledge::refresh(&conn, &ctx.data_dir, &ws)?;
            let id = find_id(&conn, &ws, &id)?;
            let v = knowledge::read(&conn, &ws, &id, 200_000)?;
            if ctx.json {
                out::json(&v);
                return Ok(());
            }
            println!("{} — {} ({})", v.item.title, v.item.kind.as_deref().unwrap_or("?"), v.relative_path);
            for (k, val) in &v.fields {
                if !matches!(k.as_str(), "nexus" | "title") {
                    println!("  {k}: {}", val.as_str().map(str::to_string).unwrap_or_else(|| val.to_string()));
                }
            }
            println!("\n{}", v.body.trim_end());
        }
        KnowledgeCmd::Rules { project } => {
            knowledge::refresh(&conn, &ctx.data_dir, &ws)?;
            let p = project.as_deref().map(|q| find_project(&conn, &ws, q)).transpose()?;
            let stacks = p.as_ref().map(|p| p.stack.stacks.clone()).unwrap_or_default();
            let scope = p.as_ref().map(|p| (p.id.as_str(), stacks.as_slice()));
            let rules = kindex::effective_rules(&conn, &ws.id, scope)?
                .into_iter()
                .map(|r| knowledge::view(&conn, r))
                .collect::<nexus_core::Result<Vec<_>>>()?;
            let launch = context::launch_rules(&conn, &ws.id, scope)?;
            if ctx.json {
                #[derive(Serialize)]
                struct RulesOut<'a> {
                    rules: &'a [ItemView],
                    launch: &'a context::LaunchRules,
                }
                out::json(&RulesOut { rules: &rules, launch: &launch });
                return Ok(());
            }
            let table: Vec<Vec<String>> = rules
                .iter()
                .map(|v| {
                    let applies = v.item.applies.as_deref().unwrap_or("on-demand");
                    let state = if v.needs_approval {
                        "needs approval"
                    } else if launch.included.contains(&v.item.title) {
                        "in system prompt"
                    } else if applies == "always" {
                        "over budget (on demand)"
                    } else {
                        "on demand (MCP)"
                    };
                    vec![knowledge::short_id(v.item.id.as_deref().unwrap_or("-")), v.scope.into(), applies.into(), state.into(), v.item.title.clone()]
                })
                .collect();
            if table.is_empty() {
                println!("no rules apply{}", p.map(|p| format!(" to {}", p.name)).unwrap_or_default());
            } else {
                out::table(&["ID", "SCOPE", "APPLIES", "DELIVERY", "TITLE"], &table);
            }
            if !launch.pending_approval.is_empty() {
                println!("\napprove with `nexus knowledge approve <id>` after reviewing the rule (`nexus knowledge show <id>`)");
            }
        }
        KnowledgeCmd::Approve { id } => {
            knowledge::refresh(&conn, &ctx.data_dir, &ws)?;
            let id = find_id(&conn, &ws, &id)?;
            let item = knowledge::approve(&conn, &ctx.data_dir, &ws, &id)?;
            if ctx.json {
                out::json(&item);
            } else {
                println!("approved “{}” (this version, on this machine)", item.title);
            }
        }
        KnowledgeCmd::New(a) => {
            let body = match &a.body_file {
                Some(f) => {
                    let len = std::fs::metadata(f).map_err(|e| CoreError::io("cannot read", f, e))?.len();
                    if len > MAX_FILE_BYTES {
                        return Err(CoreError::invalid("the body file is larger than 2 MiB"));
                    }
                    std::fs::read_to_string(f).map_err(|e| CoreError::io("cannot read", f, e))?
                }
                None => a.body.clone(),
            };
            let proj = a.project.as_deref().map(|q| find_project(&conn, &ws, q)).transpose()?;
            let (branch, commit) = match (a.branch.as_deref(), &proj) {
                (Some("current"), Some(p)) => {
                    let g = GitRef::of(p.local_path.as_deref().unwrap_or(Path::new("")));
                    if g.branch.is_none() {
                        return Err(CoreError::invalid(format!("{} has no checked-out branch", p.name)));
                    }
                    (g.branch, g.head)
                }
                (Some(name), _) => (Some(name.to_string()), None),
                (None, _) => (None, None),
            };
            let project_id = proj.map(|p| p.id);
            let input = CreateInput {
                kind: a.kind,
                title: a.title,
                body,
                project_id,
                local_only: a.local,
                priority: a.priority,
                status: a.status,
                applies: a.applies,
                stacks: a.stacks,
                labels: a.labels,
                global: a.global,
                branch,
                commit,
            };
            let item = knowledge::create(&conn, &ctx.data_dir, &ws, &input, Author::Human)?;
            if ctx.json {
                out::json(&item);
            } else {
                println!("created {} “{}” → {}", item.kind.as_deref().unwrap_or("?"), item.title, item.path.display());
            }
        }
    }
    Ok(())
}

pub fn cmd_tasks(ctx: &Ctx, cmd: TasksCmd) -> nexus_core::Result<()> {
    let conn = ctx.db()?;
    let ws = active_workspace(&conn)?;
    match cmd {
        TasksCmd::List { status, project } => {
            if let Some(s) = &status
                && !Kind::Task.statuses().contains(&s.as_str())
            {
                return Err(CoreError::invalid(format!("status must be one of {}", Kind::Task.statuses().join(", "))));
            }
            let pid = project.as_deref().map(|q| find_project(&conn, &ws, q).map(|p| p.id)).transpose()?;
            knowledge::refresh(&conn, &ctx.data_dir, &ws)?;
            let tasks: Vec<ItemView> = knowledge::list(&conn, &ws, &[Kind::Task], false)?
                .into_iter()
                .filter(|t| status.as_ref().is_none_or(|s| t.status.as_ref() == Some(s)))
                .filter(|t| pid.is_none() || t.project_id == pid || t.project_id.is_none())
                .map(|t| knowledge::view(&conn, t))
                .collect::<nexus_core::Result<_>>()?;
            if ctx.json {
                out::json(&tasks);
                return Ok(());
            }
            let order = Kind::Task.statuses();
            let mut sorted: Vec<&ItemView> = tasks.iter().collect();
            sorted.sort_by_key(|t| order.iter().position(|s| t.item.status.as_deref() == Some(*s)).unwrap_or(order.len()));
            let table: Vec<Vec<String>> = sorted
                .iter()
                .map(|t| {
                    vec![
                        knowledge::short_id(t.item.id.as_deref().unwrap_or("-")),
                        t.item.status.clone().unwrap_or_default(),
                        t.item.priority.clone().unwrap_or_default(),
                        t.item.title.clone(),
                    ]
                })
                .collect();
            if table.is_empty() {
                println!("no tasks — add one with `nexus tasks add \"<title>\"`");
            } else {
                out::table(&["ID", "STATUS", "PRIORITY", "TITLE"], &table);
            }
        }
        TasksCmd::Add { title, priority, project, local, body } => {
            let project_id = project.as_deref().map(|q| find_project(&conn, &ws, q).map(|p| p.id)).transpose()?;
            let input = CreateInput { kind: "task".into(), title, body, project_id, local_only: local, priority, ..Default::default() };
            let item = knowledge::create(&conn, &ctx.data_dir, &ws, &input, Author::Human)?;
            if ctx.json {
                out::json(&item);
            } else {
                println!("added task {} “{}”", knowledge::short_id(item.id.as_deref().unwrap_or("-")), item.title);
            }
        }
        TasksCmd::Move { id, status, log, check } => {
            knowledge::refresh(&conn, &ctx.data_dir, &ws)?;
            let id = find_id(&conn, &ws, &id)?;
            let u = TaskUpdate { status: Some(&status), log: log.as_deref(), check: &check, ..Default::default() };
            let item = tasks::update(&conn, &ctx.data_dir, &ws, &id, &u, Author::Human)?;
            if ctx.json {
                out::json(&item);
            } else {
                println!("“{}” → {}", item.title, item.status.as_deref().unwrap_or("?"));
            }
        }
        TasksCmd::Show { id } => {
            knowledge::refresh(&conn, &ctx.data_dir, &ws)?;
            let id = find_id(&conn, &ws, &id)?;
            let v = tasks::view(&conn, &ws, &id, 20_000)?;
            let sessions: Vec<nexus_core::session::Session> = nexus_core::session::list(
                &conn,
                &nexus_core::session::Filter { workspace_id: Some(ws.id.clone()), include_archived: true, limit: Some(500), ..Default::default() },
            )?
            .into_iter()
            .filter(|s| s.task_id.as_deref() == Some(v.id.as_str()))
            .collect();
            if ctx.json {
                #[derive(Serialize)]
                struct ShowOut<'a> {
                    task: &'a tasks::TaskView,
                    sessions: &'a [nexus_core::session::Session],
                }
                out::json(&ShowOut { task: &v, sessions: &sessions });
                return Ok(());
            }
            println!("{} — {}{}", v.title, v.status, v.priority.as_deref().map(|p| format!(" · {p}")).unwrap_or_default());
            if let Some(b) = &v.branch {
                println!("branch {b}{}", v.commit.as_deref().map(|c| format!(" @ {}", &c[..c.len().min(10)])).unwrap_or_default());
            }
            if !v.description.is_empty() {
                println!("\n{}", v.description);
            }
            if !v.recent_log.is_empty() {
                println!("\nrecent log ({} of {}):", v.recent_log.len(), v.log_entries);
                for l in &v.recent_log {
                    println!("  {l}");
                }
            }
            if !sessions.is_empty() {
                println!("\nsessions:");
                for s in &sessions {
                    println!("  {} {:<11} {} {}", out::short_id(&s.id), format!("{:?}", s.state).to_lowercase(), out::ago(Some(s.started_at)), s.title);
                }
            }
        }
    }
    Ok(())
}

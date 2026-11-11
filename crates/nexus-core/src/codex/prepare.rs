//! What a Codex session gets from NEXUS (ADR-0052), shared by every session host like `claude::prepare`: the NEXUS MCP
//! server, turn notifications, and NEXUS-authored context — the same approved rules, commit-message note, task pointer
//! and workflow line a Claude session gets (one builder, `claude::prepare::build_append`). The task's own text is
//! untrusted and reaches Codex only through `get_task` (T22). Nothing is written into the person's Codex home; the
//! session folder records what was given.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::agent::profile::AgentProfile;
use crate::claude::mcp::{IntegrationStatus, Integrations};
use crate::claude::prepare::{self as cprep, AppendInput, ContextLaunch, LearnLaunch, TaskLaunch, WorkflowLaunch};
use crate::error::{CoreError, Result};
use crate::knowledge::service;
use crate::{project, workspace};

pub struct Request<'a> {
    pub conn: &'a Connection,
    pub data_dir: &'a Path,
    pub profile: &'a AgentProfile,
    /// The `nexus` program Codex starts for MCP and notify (None → both unavailable).
    pub nexus_exe: Option<&'a Path>,
    pub nexus_session_id: &'a str,
    pub workspace_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    /// New session for the task: move a `backlog`/`ready` task to `in-progress`.
    pub start_task: bool,
}

pub struct Prepared {
    pub integrations: Integrations,
    /// NEXUS-authored context for `developer_instructions` after the work profile; no workspace-controlled text.
    pub context: Option<String>,
    /// The `nexus` program bound for MCP / notify.
    pub mcp_exe: Option<PathBuf>,
    pub notify_exe: Option<PathBuf>,
    /// A neutral session name for task sessions (the task title is untrusted).
    pub task_title: Option<String>,
}

const NO_HOOKS: &str = "Codex runs only hooks you trusted in your own Codex configuration; NEXUS does not change it (ADR-0052)";
const NO_KITS: &str = "AI Kits are Claude Code plugins";

pub fn prepare(r: &Request<'_>) -> Result<Prepared> {
    let exe = r.nexus_exe.filter(|e| e.is_file()).map(Path::to_path_buf);
    let mcp = match &exe {
        None => IntegrationStatus::Unavailable { reason: "the nexus command-line tool was not found next to NEXUS".into() },
        Some(e) => {
            uuid::Uuid::parse_str(r.nexus_session_id).map_err(|_| CoreError::invalid("session id must be a UUID"))?;
            let dir = crate::claude::mcp::session_dir(r.data_dir, r.nexus_session_id);
            std::fs::create_dir_all(&dir).map_err(|e| CoreError::io("cannot create", &dir, e))?;
            let config = dir.join("codex.json");
            // A record of the overrides (what Codex was given), not something Codex reads.
            let body = serde_json::json!({
                "agent": "codex",
                "mcpServers": { crate::claude::mcp::SERVER_NAME: { "command": e.display().to_string(), "args": ["--data-dir", r.data_dir.display().to_string(), "mcp", "--session", r.nexus_session_id] } },
                "notify": r.profile.turn_notifications,
            });
            crate::workspace::write_atomic(&config, &serde_json::to_vec_pretty(&body).unwrap_or_default())?;
            IntegrationStatus::Enabled { config }
        }
    };
    let mcp_on = matches!(mcp, IntegrationStatus::Enabled { .. });
    let ws = r.workspace_id.and_then(|id| workspace::get(r.conn, id).ok()).filter(|w| w.available);
    let project = r.project_id.and_then(|id| project::get(r.conn, id).ok());
    let ctx = crate::context::settings_for(r.conn, r.project_id);
    let strategy = r.project_id.and_then(|p| crate::context::get(r.conn, p).ok()).map(|c| c.strategy).unwrap_or_default();
    let run = r.task_id.and_then(|t| crate::workflow::run::active_for_task(r.conn, t).ok().flatten());
    let (mut parts, mut rules, mut task, mut learn, mut task_title) = (Vec::new(), None, None, None, None);
    match &ws {
        Some(ws) => {
            service::refresh(r.conn, r.data_dir, ws)?;
            let a = cprep::build_append(&AppendInput {
                conn: r.conn,
                data_dir: r.data_dir,
                ws,
                project: project.as_ref(),
                task_id: r.task_id,
                inspection: None,
                ctx: &ctx,
                mcp_on,
                hooks_on: false,
                run: run.as_ref(),
                start_task: r.start_task,
            })?;
            parts = a.parts;
            rules = a.rules;
            if let Some(v) = a.task {
                if let Some(depth) = a.learn {
                    // The rules (LEARN-1…8) are in the context; the built-in skill is a Claude plugin. Codex can read
                    // the extra folders without `--add-dir` (which would make them writable).
                    learn = Some(LearnLaunch { depth, skill: None, withheld: Some(NO_KITS.into()), folders: a.add_dirs.iter().map(|d| d.display().to_string()).collect() });
                }
                task_title = Some(format!("NEXUS task {}", service::short_id(&v.id)));
                task = Some(TaskLaunch { id: v.id.clone(), title: v.title.clone(), status: v.status.clone(), delivery: cprep::task_delivery(false, mcp_on) });
            }
        }
        None if r.task_id.is_some() => return Err(CoreError::invalid("a task can only be launched in its workspace")),
        None => {}
    }
    let prompt_chars = (!parts.is_empty()).then(|| parts.iter().map(|p| p.chars().count()).sum::<usize>() + 2 * (parts.len() - 1));
    Ok(Prepared {
        integrations: Integrations {
            mcp,
            hooks: IntegrationStatus::Unavailable { reason: NO_HOOKS.into() },
            rules,
            task,
            isolation: None,
            kits: Vec::new(),
            context: Some(ContextLaunch { strategy, observe_instructions: false }),
            workflow: run.map(|x| WorkflowLaunch { run_id: x.id, workflow: x.workflow_title, rigor: x.rigor, skills: Vec::new(), refused: x.skills.iter().map(|s| format!("{s} ({NO_KITS})")).collect() }),
            learn,
            prompt_chars,
        },
        context: (!parts.is_empty()).then(|| parts.join("\n\n")),
        mcp_exe: if mcp_on { exe.clone() } else { None },
        notify_exe: if r.profile.turn_notifications { exe } else { None },
        task_title,
    })
}

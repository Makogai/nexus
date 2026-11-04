//! `nexus mcp` — the NEXUS MCP server (ADR-0009): newline-delimited JSON-RPC 2.0 over stdio implementing the MCP
//! subset NEXUS needs (initialize, ping, tools/list, tools/call).
//!
//! Authorization: the server is bound at launch to one NEXUS session (`--session <id>` in the session-scoped
//! `--mcp-config` NEXUS generated). Every tool is confined to that session's workspace; project references from
//! other workspaces simply do not resolve. There is no shell, file or network access, outputs are bounded, and
//! everything Claude sends is treated as data.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use std::path::PathBuf;

use nexus_core::activity;
use nexus_core::knowledge::service::{self as knowledge, Author, CreateInput};
use nexus_core::knowledge::{Kind, context, index as kindex, skills};
use nexus_core::checkpoint::{self, CheckpointKind, Source};
use nexus_core::claude::profile;
use nexus_core::db::Connection;
use nexus_core::project::{self, Project};
use nexus_core::session::{self, Filter, Session};
use nexus_core::{git, resolve_ref, workspace};

use crate::Ctx;

/// Protocol revisions whose tools subset this server implements identically.
const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_LINE: usize = 1024 * 1024;
const MAX_REF: usize = 200;

const INSTRUCTIONS: &str = "NEXUS tools describe the current workspace, project and session and let you record checkpoints. \
Call get_session_context when resuming work. Record a checkpoint only for meaningful moments (a decision, a blocker, \
a finished task, notable progress) — not for every step.";

pub fn run(ctx: &Ctx, session_id: &str) -> nexus_core::Result<i32> {
    let conn = ctx.db()?;
    let mut server = Server::new(conn, session_id, ctx.data_dir.clone());
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if let Some(reply) = server.handle_line(&line)
            && writeln!(stdout, "{reply}").and_then(|_| stdout.flush()).is_err()
        {
            break;
        }
    }
    Ok(0)
}

struct Binding {
    session: Session,
    workspace_id: String,
    workspace_name: String,
    workspace: workspace::Workspace,
}

pub struct Server {
    conn: Connection,
    binding: Result<Binding, String>,
    data_dir: PathBuf,
}

impl Server {
    pub fn new(conn: Connection, session_id: &str, data_dir: PathBuf) -> Server {
        let binding = bind(&conn, session_id);
        Server { conn, binding, data_dir }
    }

    /// One input line → at most one output line (notifications produce none).
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        if line.len() > MAX_LINE {
            return Some(error(Value::Null, -32600, "message too large").to_string());
        }
        match serde_json::from_str::<Value>(line) {
            Ok(msg) => self.handle(msg).map(|v| v.to_string()),
            Err(_) => Some(error(Value::Null, -32700, "parse error").to_string()),
        }
    }

    pub fn handle(&mut self, msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or_default();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = id else {
            return None; // notification (initialized, cancelled, …)
        };
        if msg.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error(id, -32600, "invalid request: jsonrpc must be \"2.0\""));
        }
        let result = match method {
            "initialize" => Ok(initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => self.call_tool(&params),
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => error(id, code, &message),
        })
    }

    fn call_tool(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !tool_definitions().iter().any(|t| t["name"] == name) {
            return Err((-32602, format!("unknown tool: {name}")));
        }
        let outcome = match &self.binding {
            Err(reason) => Err(reason.clone()),
            Ok(b) => match name {
                "get_session_context" => self.session_context(b),
                "list_projects" => self.list_projects(b),
                "get_project" => self.get_project(b, &args),
                "get_git_status" => self.git_status(b, &args),
                "list_recent_sessions" => self.recent_sessions(b, &args),
                "checkpoint" => self.checkpoint(b, &args),
                "get_checkpoints" => self.get_checkpoints(b, &args),
                "get_session_activity" => self.session_activity(b, &args),
                "get_context_index" => self.context_index(b),
                "search_knowledge" => self.search_knowledge(b, &args),
                "get_document" => self.get_document(b, &args),
                "list_tasks" => self.list_tasks(b, &args),
                "create_task" => self.create_task(b, &args),
                "update_task" => self.update_task(b, &args),
                "record_memory" => self.record_memory(b, &args),
                _ => unreachable!("checked above"),
            },
        };
        Ok(match outcome {
            Ok(structured) => json!({
                "content": [{ "type": "text", "text": structured.to_string() }],
                "structuredContent": structured,
                "isError": false,
            }),
            Err(message) => json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
        })
    }

    fn projects(&self, b: &Binding) -> Result<Vec<Project>, String> {
        project::list(&self.conn, &b.workspace_id).map_err(|e| e.to_string())
    }

    /// `project` argument (name or id within this workspace), or the session's own project.
    fn target_project(&self, b: &Binding, args: &Value) -> Result<Project, String> {
        let projects = self.projects(b)?;
        match args.get("project").and_then(Value::as_str) {
            Some(q) if q.chars().count() > MAX_REF => Err("project reference is too long".into()),
            Some(q) => resolve_ref(&projects, q, |p| &p.id, |p| &p.name, "project in this workspace")
                .cloned()
                .map_err(|e| e.to_string()),
            None => {
                let pid = b.session.project_id.as_deref().ok_or("this session has no project; pass `project`")?;
                projects.into_iter().find(|p| p.id == pid).ok_or_else(|| "the session's project is no longer registered".into())
            }
        }
    }

    fn session_context(&self, b: &Binding) -> Result<Value, String> {
        let s = &b.session;
        let project = s
            .project_id
            .as_deref()
            .and_then(|pid| self.projects(b).ok()?.into_iter().find(|p| p.id == pid));
        let profile_name = s
            .profile_id
            .as_deref()
            .and_then(|pid| profile::get(&self.conn, pid).ok())
            .map(|p| p.name);
        let recent = match &project {
            Some(p) => checkpoint::for_project(&self.conn, &p.id, 5),
            None => checkpoint::for_session(&self.conn, &s.id, 5),
        }
        .map_err(|e| e.to_string())?;
        Ok(json!({
            "session": {
                "id": s.id, "title": s.title, "workProfile": s.work_profile, "effort": s.effort, "model": s.model,
                "branch": s.branch, "startedAt": s.started_at, "resumedFrom": s.resumed_from,
            },
            "workspace": { "id": b.workspace_id, "name": b.workspace_name },
            "project": project.as_ref().map(project_summary),
            "profile": profile_name,
            "recentCheckpoints": recent.iter().map(checkpoint_json).collect::<Vec<_>>(),
        }))
    }

    fn list_projects(&self, b: &Binding) -> Result<Value, String> {
        let current = b.session.project_id.as_deref();
        let projects: Vec<Value> = self
            .projects(b)?
            .iter()
            .map(|p| {
                let mut v = project_summary(p);
                v["current"] = json!(Some(p.id.as_str()) == current);
                v
            })
            .collect();
        Ok(json!({ "workspace": b.workspace_name, "projects": projects }))
    }

    fn get_project(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let p = self.target_project(b, args)?;
        let mut v = project_summary(&p);
        v["remoteUrl"] = json!(p.remote_url);
        v["git"] = match p.local_path.as_deref().filter(|lp| lp.is_dir()) {
            Some(lp) => git::status(lp).map(|g| json!(g)).unwrap_or(Value::Null),
            None => Value::Null,
        };
        Ok(v)
    }

    fn git_status(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let p = self.target_project(b, args)?;
        let lp = p.local_path.filter(|lp| lp.is_dir()).ok_or("the project is not available on this machine")?;
        git::status(&lp).map(|g| json!({ "project": p.name, "status": g })).map_err(|e| e.to_string())
    }

    fn recent_sessions(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let limit = limit_arg(args, 10, 20)?;
        let project_id = match args.get("project") {
            Some(_) => Some(self.target_project(b, args)?.id),
            None => None,
        };
        let list = session::list(
            &self.conn,
            &Filter {
                workspace_id: Some(b.workspace_id.clone()),
                project_id,
                limit: Some(limit),
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())?;
        let sessions: Vec<Value> = list
            .iter()
            .map(|s| {
                json!({
                    "id": s.id, "title": s.title, "kind": s.kind, "state": s.state, "exitCode": s.exit_code,
                    "workProfile": s.work_profile, "branch": s.branch, "startedAt": s.started_at, "endedAt": s.ended_at,
                    "current": s.id == b.session.id,
                })
            })
            .collect();
        Ok(json!({ "sessions": sessions }))
    }

    fn checkpoint(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let kind = args
            .get("kind")
            .and_then(Value::as_str)
            .and_then(CheckpointKind::parse)
            .ok_or("`kind` must be one of progress, decision, blocker, done, note")?;
        let summary = args.get("summary").and_then(Value::as_str).ok_or("`summary` is required")?;
        let c = checkpoint::add(&self.conn, &b.session.id, kind, summary, Source::Claude).map_err(|e| e.to_string())?;
        Ok(json!({ "recorded": checkpoint_json(&c) }))
    }

    fn get_checkpoints(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let limit = limit_arg(args, 20, 50)?;
        let list = if args.get("project").is_some() || b.session.project_id.is_some() {
            checkpoint::for_project(&self.conn, &self.target_project(b, args)?.id, limit)
        } else {
            checkpoint::for_session(&self.conn, &b.session.id, limit)
        }
        .map_err(|e| e.to_string())?;
        Ok(json!({ "checkpoints": list.iter().map(checkpoint_json).collect::<Vec<_>>() }))
    }
}

impl Server {
    /// Structured activity (files, command categories, turns, compactions) of this session and, optionally, the
    /// previous sessions of its project — raw material for consolidating durable memory on request.
    fn session_activity(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let previous = limit_arg_named(args, "previous", 0, 5)?;
        let mut ids = vec![b.session.id.clone()];
        if previous > 0
            && let Some(pid) = &b.session.project_id
        {
            let earlier = session::list(
                &self.conn,
                &Filter {
                    workspace_id: Some(b.workspace_id.clone()),
                    project_id: Some(pid.clone()),
                    limit: Some(previous + 1),
                    ..Default::default()
                },
            )
            .map_err(|e| e.to_string())?;
            ids.extend(earlier.into_iter().map(|s| s.id).filter(|id| id != &b.session.id).take(previous as usize));
        }
        let sessions: Vec<Value> = ids
            .iter()
            .filter_map(|id| activity::activity(&self.conn, id).ok())
            .map(|a| json!(a))
            .collect();
        Ok(json!({ "sessions": sessions }))
    }
}

/// Knowledge tools (ADR-0017): lazy, bounded access to the workspace's portable knowledge.
impl Server {
    fn refresh_knowledge(&self, b: &Binding) -> Result<(), String> {
        knowledge::refresh(&self.conn, &self.data_dir, &b.workspace).map(|_| ()).map_err(|e| e.to_string())
    }

    fn session_project(&self, b: &Binding) -> Option<Project> {
        b.session.project_id.as_deref().and_then(|pid| project::get(&self.conn, pid).ok())
    }

    fn context_index(&self, b: &Binding) -> Result<Value, String> {
        self.refresh_knowledge(b)?;
        let project = self.session_project(b);
        let stacks = project.as_ref().map(|p| p.stack.stacks.clone()).unwrap_or_default();
        let scope = project.as_ref().map(|p| (p.id.as_str(), stacks.as_slice()));
        let e = |e: nexus_core::CoreError| e.to_string();
        let summary = context::summary(&self.conn, &b.workspace_id, scope).map_err(e)?;
        let rules: Vec<Value> = kindex::effective_rules(&self.conn, &b.workspace_id, scope)
            .map_err(e)?
            .iter()
            .map(|r| {
                json!({
                    "id": r.id, "title": r.title, "applies": r.applies.as_deref().unwrap_or("on-demand"), "scope": knowledge::scope_label(r),
                    "stacks": r.stacks, "inSystemPrompt": r.applies.as_deref() == Some("always") && kindex::is_approved(&self.conn, r).unwrap_or(false),
                })
            })
            .collect();
        let brief = |kind: Kind, limit: usize, open_only: bool| -> Result<Vec<Value>, String> {
            Ok(knowledge::list(&self.conn, &b.workspace, &[kind], false)
                .map_err(e)?
                .into_iter()
                .filter(|i| !open_only || i.status.as_deref() != Some("done"))
                .filter(|i| i.project_id.is_none() || i.project_id == b.session.project_id)
                .take(limit)
                .map(|i| json!({ "id": i.id, "title": i.title, "status": i.status, "priority": i.priority }))
                .collect())
        };
        let health: Vec<Value> = knowledge::list(&self.conn, &b.workspace, &[], true)
            .map_err(e)?
            .into_iter()
            .filter(|i| !i.valid || i.conflict.is_some())
            .take(5)
            .map(|i| json!({ "file": i.path.file_name().map(|n| n.to_string_lossy().into_owned()), "problem": i.error.or(i.conflict) }))
            .collect();
        let skills = skills::catalog(&b.workspace.root_path, project.as_ref().and_then(|p| p.local_path.as_deref()));
        Ok(json!({
            "summary": summary,
            "rules": rules,
            "openTasks": brief(Kind::Task, 15, true)?,
            "decisions": brief(Kind::Decision, 10, false)?,
            "workflows": brief(Kind::Workflow, 10, false)?,
            "specs": brief(Kind::Spec, 10, false)?,
            "skills": skills,
            "indexProblems": health,
            "hint": "Fetch full text only when relevant: get_document(id). Search: search_knowledge(query).",
        }))
    }

    fn search_knowledge(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let query = args.get("query").and_then(Value::as_str).filter(|q| !q.trim().is_empty()).ok_or("`query` is required")?;
        if query.chars().count() > 300 {
            return Err("query is too long".into());
        }
        let kinds = kinds_arg(args)?;
        let limit = limit_arg(args, 10, 20)?;
        self.refresh_knowledge(b)?;
        let hits = kindex::search(&self.conn, &b.workspace_id, query, &kinds, limit).map_err(|e| e.to_string())?;
        let hits: Vec<Value> = hits
            .iter()
            .map(|h| json!({ "id": h.item.id, "kind": h.item.kind, "title": h.item.title, "status": h.item.status, "snippet": h.snippet }))
            .collect();
        Ok(json!({ "hits": hits }))
    }

    fn get_document(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let id = args.get("id").and_then(Value::as_str).ok_or("`id` is required")?;
        self.refresh_knowledge(b)?;
        let view = knowledge::read(&self.conn, &b.workspace, id, 20_000).map_err(|e| e.to_string())?;
        Ok(json!({
            "id": view.item.id, "kind": view.item.kind, "title": view.item.title, "fields": view.fields,
            "body": view.body, "truncated": view.truncated, "path": view.relative_path, "hash": view.item.hash,
        }))
    }

    fn list_tasks(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let status = args.get("status").and_then(Value::as_str);
        if let Some(s) = status && !Kind::Task.statuses().contains(&s) {
            return Err(format!("`status` must be one of {}", Kind::Task.statuses().join(", ")));
        }
        let limit = limit_arg(args, 30, 50)? as usize;
        self.refresh_knowledge(b)?;
        let tasks: Vec<Value> = knowledge::list(&self.conn, &b.workspace, &[Kind::Task], false)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|t| status.is_none_or(|s| t.status.as_deref() == Some(s)))
            .take(limit)
            .map(|t| json!({ "id": t.id, "title": t.title, "status": t.status, "priority": t.priority, "project": t.project_id, "hash": t.hash }))
            .collect();
        Ok(json!({ "tasks": tasks }))
    }

    fn create_task(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let title = args.get("title").and_then(Value::as_str).ok_or("`title` is required")?;
        let project_id = match args.get("project") {
            Some(_) => Some(self.target_project(b, args)?.id),
            None => b.session.project_id.clone(),
        };
        let input = CreateInput {
            kind: "task".into(),
            title: title.into(),
            body: args.get("body").and_then(Value::as_str).unwrap_or_default().chars().take(20_000).collect(),
            project_id,
            local_only: args.get("localOnly").and_then(Value::as_bool).unwrap_or(false),
            priority: args.get("priority").and_then(Value::as_str).map(str::to_string),
            ..Default::default()
        };
        let item = knowledge::create(&self.conn, &self.data_dir, &b.workspace, &input, Author::Claude).map_err(|e| e.to_string())?;
        Ok(json!({ "created": { "id": item.id, "title": item.title, "status": item.status, "hash": item.hash } }))
    }

    fn update_task(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let id = args.get("id").and_then(Value::as_str).ok_or("`id` is required")?;
        let status = args.get("status").and_then(Value::as_str);
        let log = args.get("log").and_then(Value::as_str);
        if status.is_none() && log.is_none() {
            return Err("pass `status` and/or `log`".into());
        }
        let expected = args.get("expectedHash").and_then(Value::as_str);
        let item = knowledge::update_task(&self.conn, &self.data_dir, &b.workspace, id, status, log, expected).map_err(|e| e.to_string())?;
        Ok(json!({ "updated": { "id": item.id, "status": item.status, "hash": item.hash } }))
    }

    fn record_memory(&self, b: &Binding, args: &Value) -> Result<Value, String> {
        let kind = match args.get("kind").and_then(Value::as_str) {
            Some("note") => "note",
            Some("decision") => "decision",
            _ => return Err("`kind` must be note or decision".into()),
        };
        let title = args.get("title").and_then(Value::as_str).ok_or("`title` is required")?;
        let body = args.get("body").and_then(Value::as_str).ok_or("`body` is required")?;
        let input = CreateInput {
            kind: kind.into(),
            title: title.into(),
            body: body.chars().take(20_000).collect(),
            project_id: b.session.project_id.clone(),
            local_only: args.get("localOnly").and_then(Value::as_bool).unwrap_or(false),
            ..Default::default()
        };
        let item = knowledge::create(&self.conn, &self.data_dir, &b.workspace, &input, Author::Claude).map_err(|e| e.to_string())?;
        Ok(json!({ "recorded": { "id": item.id, "kind": item.kind, "title": item.title, "status": item.status } }))
    }
}

fn kinds_arg(args: &Value) -> Result<Vec<Kind>, String> {
    match args.get("kinds") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|k| k.as_str().and_then(Kind::parse).ok_or_else(|| "`kinds` entries must be rule, task, decision, note, spec or workflow".to_string()))
            .collect(),
        Some(_) => Err("`kinds` must be an array".into()),
    }
}

fn limit_arg_named(args: &Value, key: &str, default: u32, max: u32) -> Result<u32, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_u64()
            .filter(|n| *n <= u64::from(max))
            .map(|n| n as u32)
            .ok_or_else(|| format!("`{key}` must be an integer from 0 to {max}")),
    }
}

fn bind(conn: &Connection, session_id: &str) -> Result<Binding, String> {
    let unbound = |why: &str| format!("This NEXUS MCP server is not bound to a usable NEXUS session ({why}).");
    let session = session::get(conn, session_id).map_err(|_| unbound("unknown session"))?;
    let workspace_id = session.workspace_id.clone().ok_or_else(|| unbound("the session has no workspace"))?;
    let ws = workspace::get(conn, &workspace_id).map_err(|_| unbound("its workspace is no longer registered"))?;
    Ok(Binding {
        session,
        workspace_id,
        workspace_name: ws.name.clone(),
        workspace: ws,
    })
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
    let version = if PROTOCOL_VERSIONS.contains(&requested) { requested } else { PROTOCOL_VERSIONS[0] };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "nexus", "title": "NEXUS", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn limit_arg(args: &Value, default: u32, max: u32) -> Result<u32, String> {
    match args.get("limit") {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_u64()
            .filter(|n| (1..=u64::from(max)).contains(n))
            .map(|n| n as u32)
            .ok_or_else(|| format!("`limit` must be an integer from 1 to {max}")),
    }
}

fn project_summary(p: &Project) -> Value {
    json!({
        "id": p.id, "name": p.name, "path": p.local_path, "available": p.available,
        "stacks": p.stack.stacks, "packageManagers": p.stack.package_managers,
        "scripts": p.stack.scripts, "hasClaudeMd": p.stack.has_claude_md, "hasAgentsMd": p.stack.has_agents_md,
    })
}

fn checkpoint_json(c: &checkpoint::Checkpoint) -> Value {
    json!({ "kind": c.kind, "summary": c.summary, "session": c.session_title, "createdAt": c.created_at })
}

fn tool_definitions() -> Vec<Value> {
    let project_arg = json!({ "type": "string", "maxLength": MAX_REF, "description": "Project name or id in this workspace (default: the session's project)" });
    let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
    vec![
        json!({
            "name": "get_session_context", "title": "Session context",
            "description": "Current NEXUS session, workspace, project (stack, scripts) and recent checkpoints. Call when starting or resuming work.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": read_only,
        }),
        json!({
            "name": "list_projects", "title": "Workspace projects",
            "description": "Projects registered in the current NEXUS workspace (related repositories).",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": read_only,
        }),
        json!({
            "name": "get_project", "title": "Project details",
            "description": "Stack, scripts, remote and Git status of a project in this workspace.",
            "inputSchema": { "type": "object", "properties": { "project": project_arg }, "additionalProperties": false },
            "annotations": read_only,
        }),
        json!({
            "name": "get_git_status", "title": "Git status",
            "description": "Branch, upstream, ahead/behind and change counts of a project in this workspace.",
            "inputSchema": { "type": "object", "properties": { "project": project_arg }, "additionalProperties": false },
            "annotations": read_only,
        }),
        json!({
            "name": "list_recent_sessions", "title": "Recent sessions",
            "description": "Recent NEXUS sessions in this workspace (or one project): title, state, rigor, branch, times.",
            "inputSchema": { "type": "object", "properties": {
                "project": project_arg,
                "limit": { "type": "integer", "minimum": 1, "maximum": 20 }
            }, "additionalProperties": false },
            "annotations": read_only,
        }),
        json!({
            "name": "checkpoint", "title": "Record checkpoint",
            "description": "Record a short checkpoint on the current session so work can be resumed later. Use for decisions, blockers, completed work and notable progress.",
            "inputSchema": { "type": "object", "properties": {
                "kind": { "type": "string", "enum": ["progress", "decision", "blocker", "done", "note"] },
                "summary": { "type": "string", "minLength": 1, "maxLength": checkpoint::MAX_SUMMARY_CHARS }
            }, "required": ["kind", "summary"], "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
        }),
        json!({
            "name": "get_session_activity", "title": "Session activity",
            "description": "Files changed, notable command categories (tests, builds, commits), turns and compactions of this session — optionally also of up to 5 previous sessions of the project. Use it to summarise or consolidate work.",
            "inputSchema": { "type": "object", "properties": {
                "previous": { "type": "integer", "minimum": 0, "maximum": 5 }
            }, "additionalProperties": false },
            "annotations": read_only,
        }),
        json!({
            "name": "get_context_index", "title": "Knowledge index",
            "description": "Start here for workspace knowledge: counts, applicable rules (always-on ones are already in your system prompt), open tasks, decisions, workflows, specs and skills — titles and ids only.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": read_only,
        }),
        json!({
            "name": "search_knowledge", "title": "Search knowledge",
            "description": "Full-text search over this workspace's rules, tasks, decisions, notes, specs and workflows.",
            "inputSchema": { "type": "object", "properties": {
                "query": { "type": "string", "minLength": 1, "maxLength": 300 },
                "kinds": { "type": "array", "items": { "type": "string", "enum": ["rule", "task", "decision", "note", "spec", "workflow"] } },
                "limit": { "type": "integer", "minimum": 1, "maximum": 20 }
            }, "required": ["query"], "additionalProperties": false },
            "annotations": read_only,
        }),
        json!({
            "name": "get_document", "title": "Read document",
            "description": "Full text and fields of one knowledge document by id (from get_context_index or search_knowledge).",
            "inputSchema": { "type": "object", "properties": { "id": { "type": "string", "maxLength": 64 } }, "required": ["id"], "additionalProperties": false },
            "annotations": read_only,
        }),
        json!({
            "name": "list_tasks", "title": "Tasks",
            "description": "Tasks of this workspace, optionally by status.",
            "inputSchema": { "type": "object", "properties": {
                "status": { "type": "string", "enum": ["backlog", "ready", "in-progress", "review", "blocked", "done"] },
                "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
            }, "additionalProperties": false },
            "annotations": read_only,
        }),
        json!({
            "name": "create_task", "title": "Create task",
            "description": "Create a task file in the workspace (defaults to this session's project).",
            "inputSchema": { "type": "object", "properties": {
                "title": { "type": "string", "minLength": 1, "maxLength": 200 },
                "body": { "type": "string", "maxLength": 20000 },
                "priority": { "type": "string", "enum": ["low", "medium", "high", "urgent"] },
                "project": project_arg.clone(),
                "localOnly": { "type": "boolean", "description": "Keep on this machine only (never synced)" }
            }, "required": ["title"], "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
        }),
        json!({
            "name": "update_task", "title": "Update task",
            "description": "Change a task's status and/or append a dated log line. Pass expectedHash (from list_tasks/get_document) to avoid overwriting concurrent edits.",
            "inputSchema": { "type": "object", "properties": {
                "id": { "type": "string", "maxLength": 64 },
                "status": { "type": "string", "enum": ["backlog", "ready", "in-progress", "review", "blocked", "done"] },
                "log": { "type": "string", "maxLength": 500 },
                "expectedHash": { "type": "string", "maxLength": 32 }
            }, "required": ["id"], "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
        }),
        json!({
            "name": "record_memory", "title": "Record memory",
            "description": "Save durable project memory as a file: a note (recorded as unverified) or a proposed decision. Use for things worth knowing in future sessions — not for routine progress (use checkpoint).",
            "inputSchema": { "type": "object", "properties": {
                "kind": { "type": "string", "enum": ["note", "decision"] },
                "title": { "type": "string", "minLength": 1, "maxLength": 200 },
                "body": { "type": "string", "minLength": 1, "maxLength": 20000 },
                "localOnly": { "type": "boolean" }
            }, "required": ["kind", "title", "body"], "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
        }),
        json!({
            "name": "get_checkpoints", "title": "Checkpoints",
            "description": "Recent checkpoints of a project in this workspace (across its sessions), newest first.",
            "inputSchema": { "type": "object", "properties": {
                "project": project_arg,
                "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
            }, "additionalProperties": false },
            "annotations": read_only,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_core::session::NewSession;

    struct Fixture {
        _t: tempfile::TempDir,
        server: Server,
        other_project_id: String,
    }

    /// Two workspaces; the server is bound to a session of workspace A's "app" project.
    fn fixture() -> Fixture {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("nexus.db");
        let conn = nexus_core::db::open(&path).unwrap();
        let mut ids = Vec::new();
        for (ws_name, proj) in [("A", "app"), ("B", "secret-app")] {
            let ws = workspace::create(&conn, t.path().join(ws_name).to_str().unwrap(), ws_name, "").unwrap();
            let dir = t.path().join(proj);
            std::fs::create_dir_all(&dir).unwrap();
            let p = project::register(&conn, &ws.id, dir.to_str().unwrap(), None).unwrap();
            ids.push((ws.id, p.id));
        }
        let s = session::insert_starting(
            &conn,
            NewSession {
                title: "Build boss fight".into(),
                cwd: t.path().join("app"),
                program: "claude".into(),
                workspace_id: Some(ids[0].0.clone()),
                project_id: Some(ids[0].1.clone()),
                work_profile: Some("prototype".into()),
                ..Default::default()
            },
        )
        .unwrap();
        Fixture {
            server: Server::new(conn, &s.id, t.path().join("data")),
            other_project_id: ids[1].1.clone(),
            _t: t,
        }
    }

    fn call(server: &mut Server, name: &str, args: Value) -> Value {
        let reply = server
            .handle(json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": name, "arguments": args } }))
            .unwrap();
        reply["result"].clone()
    }

    #[test]
    fn initialize_negotiates_known_versions_and_announces_tools() {
        let mut f = fixture();
        let r = f.server.handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } })).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(r["result"]["capabilities"]["tools"]["listChanged"], false);
        let r = f.server.handle(json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize", "params": { "protocolVersion": "2099-01-01" } })).unwrap();
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0], "unknown version → our latest");
        assert!(f.server.handle(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let tools = f.server.handle(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" })).unwrap();
        let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, [
            "get_session_context", "list_projects", "get_project", "get_git_status", "list_recent_sessions", "checkpoint",
            "get_session_activity", "get_context_index", "search_knowledge", "get_document", "list_tasks", "create_task",
            "update_task", "record_memory", "get_checkpoints",
        ]);
        assert!(names.iter().all(|n| !n.starts_with("nexus_")), "Claude already prefixes mcp__nexus__");
    }

    #[test]
    fn session_context_and_checkpoints_round_trip() {
        let mut f = fixture();
        let r = call(&mut f.server, "checkpoint", json!({ "kind": "decision", "summary": "Use a state machine for the boss" }));
        assert_eq!(r["isError"], false);
        let ctx = call(&mut f.server, "get_session_context", json!({}));
        let s = &ctx["structuredContent"];
        assert_eq!(s["workspace"]["name"], "A");
        assert_eq!(s["project"]["name"], "app");
        assert_eq!(s["session"]["workProfile"], "prototype");
        assert_eq!(s["recentCheckpoints"][0]["summary"], "Use a state machine for the boss");
        assert_eq!(ctx["content"][0]["type"], "text");
        let cps = call(&mut f.server, "get_checkpoints", json!({ "limit": 5 }));
        assert_eq!(cps["structuredContent"]["checkpoints"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn other_workspaces_are_invisible() {
        let mut f = fixture();
        let list = call(&mut f.server, "list_projects", json!({}));
        let names: Vec<&str> = list["structuredContent"]["projects"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["app"]);
        for q in [f.other_project_id.clone(), "secret-app".to_string(), f.other_project_id[..8].to_string()] {
            let r = call(&mut f.server, "get_project", json!({ "project": q }));
            assert_eq!(r["isError"], true, "cross-workspace reference {q} must not resolve");
            assert!(r["content"][0]["text"].as_str().unwrap().contains("no project in this workspace matches"));
        }
    }

    #[test]
    fn invalid_input_is_rejected_as_tool_errors_or_protocol_errors() {
        let mut f = fixture();
        for (name, args) in [
            ("checkpoint", json!({ "kind": "rm -rf /", "summary": "x" })),
            ("checkpoint", json!({ "kind": "note" })),
            ("checkpoint", json!({ "kind": "note", "summary": "x".repeat(checkpoint::MAX_SUMMARY_CHARS + 1) })),
            ("get_checkpoints", json!({ "limit": 9999 })),
            ("get_project", json!({ "project": "x".repeat(500) })),
        ] {
            assert_eq!(call(&mut f.server, name, args.clone())["isError"], true, "{name} {args}");
        }
        let unknown = f.server.handle(json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": { "name": "run_shell", "arguments": {} } })).unwrap();
        assert_eq!(unknown["error"]["code"], -32602);
        let no_method = f.server.handle(json!({ "jsonrpc": "2.0", "id": 10, "method": "resources/list" })).unwrap();
        assert_eq!(no_method["error"]["code"], -32601);
        assert_eq!(serde_json::from_str::<Value>(&f.server.handle_line("{not json").unwrap()).unwrap()["error"]["code"], -32700);
    }

    #[test]
    fn knowledge_tools_are_lazy_bounded_and_workspace_scoped() {
        let mut f = fixture();
        let created = call(&mut f.server, "create_task", json!({ "title": "Add boss music", "priority": "high" }));
        assert_eq!(created["isError"], false, "{created}");
        let task_id = created["structuredContent"]["created"]["id"].as_str().unwrap().to_string();
        let mem = call(&mut f.server, "record_memory", json!({ "kind": "decision", "title": "Use FMOD for audio", "body": "Chosen for its Godot plugin." }));
        assert_eq!(mem["structuredContent"]["recorded"]["status"], "proposed", "Claude's decisions start as proposals");

        let idx = call(&mut f.server, "get_context_index", json!({}));
        let sc = &idx["structuredContent"];
        assert_eq!(sc["openTasks"][0]["title"], "Add boss music");
        assert_eq!(sc["decisions"][0]["title"], "Use FMOD for audio");
        assert!(sc.to_string().len() < 8_000, "the index is a catalog, not content");
        assert!(!sc.to_string().contains("Godot plugin"), "bodies are not in the index");

        let hits = call(&mut f.server, "search_knowledge", json!({ "query": "godot audio" }));
        assert_eq!(hits["structuredContent"]["hits"][0]["title"], "Use FMOD for audio");
        let doc = call(&mut f.server, "get_document", json!({ "id": task_id }));
        let hash = doc["structuredContent"]["hash"].as_str().unwrap().to_string();
        let moved = call(&mut f.server, "update_task", json!({ "id": task_id, "status": "in-progress", "log": "Picked up", "expectedHash": hash }));
        assert_eq!(moved["structuredContent"]["updated"]["status"], "in-progress");
        let stale = call(&mut f.server, "update_task", json!({ "id": task_id, "status": "done", "expectedHash": hash }));
        assert_eq!(stale["isError"], true, "a stale hash is a conflict, not an overwrite");
        assert_eq!(call(&mut f.server, "list_tasks", json!({ "status": "in-progress" }))["structuredContent"]["tasks"].as_array().unwrap().len(), 1);

        // Claude cannot create rules; inputs are validated.
        let rule = call(&mut f.server, "record_memory", json!({ "kind": "rule", "title": "x", "body": "y" }));
        assert_eq!(rule["isError"], true);
        assert_eq!(call(&mut f.server, "search_knowledge", json!({ "query": "x", "kinds": ["passwords"] }))["isError"], true);
        assert_eq!(call(&mut f.server, "get_document", json!({ "id": "not-an-id" }))["isError"], true);
    }

    #[test]
    fn unbound_server_refuses_every_tool() {
        let t = tempfile::tempdir().unwrap();
        let conn = nexus_core::db::open(&t.path().join("nexus.db")).unwrap();
        let mut server = Server::new(conn, "00000000-0000-0000-0000-000000000000", t.path().join("data"));
        let r = call(&mut server, "list_projects", json!({}));
        assert_eq!(r["isError"], true);
        assert!(r["content"][0]["text"].as_str().unwrap().contains("not bound"));
    }
}

//! Session preparation shared by every session host (desktop, `nexus launch`): the session-scoped NEXUS
//! integrations (MCP, hooks — ADR-0009/0016) and the bounded context appended to Claude's system prompt (approved
//! always-on rules not already in a loaded CLAUDE.md — ADR-0017/0019 — and the brief of the session's task —
//! ADR-0018). One implementation, so the desktop and the CLI cannot drift apart.

use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use super::instructions::{self, Inspection};
use super::isolation;
use super::mcp::{self, IntegrationStatus, Integrations};
use super::policy::{self, MachinePolicyLocations, PolicySummary};
use super::profile::ClaudeProfile;
use super::hooks;
use crate::error::{CoreError, Result};
use crate::knowledge::{context, service, tasks};
use crate::{project, workspace};

pub struct Request<'a> {
    pub conn: &'a Connection,
    pub data_dir: &'a Path,
    pub home: &'a Path,
    pub profile: &'a ClaudeProfile,
    pub machine: &'a MachinePolicyLocations,
    /// The `nexus` executable Claude starts for MCP and hooks (None → those integrations are unavailable).
    pub nexus_exe: Option<&'a Path>,
    pub nexus_session_id: &'a str,
    pub workspace_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub cwd: &'a Path,
    /// Environment the host passes on (checked for `CLAUDE_CODE_DISABLE_CLAUDE_MDS`).
    pub base_env: &'a [(String, String)],
    /// New session for the task: move a `backlog`/`ready` task to `in-progress`.
    pub start_task: bool,
}

/// What a session was given for its task (recorded with the session's integrations).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskLaunch {
    pub id: String,
    pub title: String,
    pub status: String,
    /// How the task's (untrusted) text reaches Claude: `session-start` (hook context), `tools` (MCP only) or
    /// `none`. It never goes into the system prompt (threat T22, ADR-0018).
    pub delivery: &'static str,
}

/// Profile instruction isolation applied to a session (ADR-0020).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Isolation {
    pub claude_md_excludes: Vec<String>,
}

pub struct Prepared {
    pub integrations: Integrations,
    /// Text for `--append-system-prompt` after the work profile (approved rules, task pointer), if any. Contains no
    /// workspace-controlled task text.
    pub prompt: Option<String>,
    /// The session's `--settings` file (hooks and/or isolation excludes), if any.
    pub settings: Option<std::path::PathBuf>,
    /// AI Kit overlays to pass as `--plugin-dir` (profile-specific, ADR-0024).
    pub plugin_dirs: Vec<std::path::PathBuf>,
    /// Name to give Claude (`--name`) instead of the host's title: task sessions get a neutral `NEXUS task <id>` so the
    /// (untrusted) task title is never handed to Claude as session metadata (T22).
    pub claude_name: Option<String>,
    pub policy: PolicySummary,
}

pub fn claude_mds_disabled(env: &[(String, String)]) -> bool {
    env.iter().any(|(k, v)| k.eq_ignore_ascii_case("CLAUDE_CODE_DISABLE_CLAUDE_MDS") && !matches!(v.trim(), "" | "0" | "false"))
}

/// `claudeMdExcludes` the profile's isolation adds to a session in `cwd` (empty when isolation is off).
pub fn isolation_excludes(home: &Path, profile: &ClaudeProfile, cwd: &Path) -> Vec<String> {
    if profile.isolate_instructions { isolation::excludes(home, &profile.effective_config_dir(home), cwd) } else { Vec::new() }
}

/// What Claude itself loads for this profile and directory, including NEXUS's isolation excludes (read-only;
/// ADR-0019, ADR-0020).
pub fn inspect_instructions(home: &Path, profile: &ClaudeProfile, machine: &MachinePolicyLocations, cwd: &Path, base_env: &[(String, String)]) -> (Inspection, PolicySummary) {
    let config_dir = profile.effective_config_dir(home);
    let pol = policy::evaluate(&config_dir, machine);
    let session_excludes = isolation_excludes(home, profile, cwd);
    let i = instructions::inspect(&instructions::Input {
        managed_dir: machine.managed_dir(),
        policy: &pol,
        config_dir: &config_dir,
        home,
        cwd,
        env_disabled: claude_mds_disabled(base_env),
        session_excludes: &session_excludes,
    });
    (i, pol)
}

/// What NEXUS would append for a launch, next to what Claude loads itself (the effective-instructions inspector,
/// ADR-0019). Read-only: nothing is written, a task is not started.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub claude: Inspection,
    pub nexus: NexusAppend,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusAppend {
    pub work_profile: Option<String>,
    pub work_profile_chars: usize,
    pub rules: Option<context::LaunchRules>,
    pub task: Option<String>,
    pub task_brief_chars: usize,
    /// Total characters appended to Claude's system prompt.
    pub appended_chars: usize,
    pub mcp: Option<String>,
    pub hooks: Option<String>,
    /// `claudeMdExcludes` added by profile isolation (ADR-0020).
    pub isolation_excludes: Vec<String>,
}

/// The context strategy a session was launched with (ADR-0028).
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextLaunch {
    pub strategy: crate::context::Strategy,
    pub observe_instructions: bool,
}

pub struct PreviewRequest<'a> {
    pub conn: &'a Connection,
    pub data_dir: &'a Path,
    pub home: &'a Path,
    pub profile: &'a ClaudeProfile,
    pub machine: &'a MachinePolicyLocations,
    pub workspace_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub cwd: &'a Path,
    pub base_env: &'a [(String, String)],
    pub work_profile: Option<&'a str>,
}

pub fn preview(r: &PreviewRequest<'_>) -> Result<Preview> {
    let (claude, pol) = inspect_instructions(r.home, r.profile, r.machine, r.cwd, r.base_env);
    let wp = r.work_profile.map(str::to_string).or_else(|| r.profile.default_work_profile.clone()).and_then(|id| crate::workprofile::get(&id));
    let mut nexus = NexusAppend {
        work_profile: wp.map(|w| w.name.to_string()),
        work_profile_chars: wp.map(|w| w.prompt.chars().count()).unwrap_or(0),
        mcp: (!pol.sideload_allowed() || pol.allow_managed_mcp_servers_only || pol.strict_plugin_only_customization)
            .then(|| "blocked by organisation policy".to_string()),
        hooks: (!pol.session_hooks_allowed()).then(|| "blocked by organisation policy".to_string()),
        isolation_excludes: isolation_excludes(r.home, r.profile, r.cwd),
        ..Default::default()
    };
    if let Some(ws) = r.workspace_id.and_then(|id| workspace::get(r.conn, id).ok()).filter(|w| w.available) {
        service::refresh(r.conn, r.data_dir, &ws)?;
        let project = r.project_id.and_then(|id| project::get(r.conn, id).ok());
        let stacks = project.as_ref().map(|p| p.stack.stacks.clone()).unwrap_or_default();
        let scope = project.as_ref().map(|p| (p.id.as_str(), stacks.as_slice()));
        let ctx = crate::context::settings_for(r.conn, r.project_id);
        let rules = context::launch_rules_with(r.conn, &ws.id, scope, Some(&claude), r.task_id, &context::RuleOptions::from(&ctx))?;
        if let Some(tid) = r.task_id {
            let v = tasks::view(r.conn, &ws, tid, 4000)?;
            nexus.task_brief_chars = tasks::pointer(&v.id, "session-start").chars().count();
            nexus.task = Some(v.title);
        }
        nexus.appended_chars = rules.text.as_deref().map(|t| t.chars().count()).unwrap_or(0) + nexus.task_brief_chars;
        nexus.rules = Some(rules);
    }
    nexus.appended_chars += nexus.work_profile_chars;
    Ok(Preview { claude, nexus })
}

pub fn prepare(r: &Request<'_>) -> Result<Prepared> {
    let (inspection, pol) = inspect_instructions(r.home, r.profile, r.machine, r.cwd, r.base_env);
    let mcp = mcp::prepare(&pol, r.nexus_exe, r.data_dir, r.nexus_session_id)?;
    let excludes = isolation_excludes(r.home, r.profile, r.cwd);
    let ctx = crate::context::settings_for(r.conn, r.project_id);
    let strategy = r.project_id.and_then(|p| crate::context::get(r.conn, p).ok()).map(|c| c.strategy).unwrap_or_default();
    let (hooks, settings) = hooks::prepare_session_settings(&pol, r.nexus_exe, r.data_dir, r.nexus_session_id, &excludes, ctx.observe_instructions)?;
    let isolation = (!excludes.is_empty()).then(|| Isolation { claude_md_excludes: excludes.clone() });
    let (plugin_dirs, kit_launches) = crate::kits::session_plugin_dirs(r.conn, r.data_dir, &r.profile.id, &pol)?;
    let ws = r.workspace_id.and_then(|id| workspace::get(r.conn, id).ok()).filter(|w| w.available);
    let project = r.project_id.and_then(|id| project::get(r.conn, id).ok());
    let mut parts: Vec<String> = Vec::new();
    let (mut rules, mut task) = (None, None);
    let mut claude_name = None;
    match &ws {
        Some(ws) => {
            service::refresh(r.conn, r.data_dir, ws)?;
            let stacks = project.as_ref().map(|p| p.stack.stacks.clone()).unwrap_or_default();
            let scope = project.as_ref().map(|p| (p.id.as_str(), stacks.as_slice()));
            let lr = context::launch_rules_with(r.conn, &ws.id, scope, Some(&inspection), r.task_id, &context::RuleOptions::from(&ctx))?;
            parts.extend(lr.text.clone());
            if !lr.pending_approval.is_empty() {
                // Synced rule changes are not used until reviewed (T19): make the review queue visible.
                let mut e = crate::notify::NewEvent::new(
                    crate::notify::Category::ReviewRequired,
                    "knowledge",
                    format!("{} rule document(s) changed and wait for your approval", lr.pending_approval.len()),
                )
                .body(lr.pending_approval.join(", "))
                .target("rules", ws.id.clone())
                .dedupe(format!("rules:{}", ws.id));
                e.workspace_id = Some(ws.id.clone());
                e.project_id = r.project_id.map(str::to_string);
                let _ = crate::notify::publish(r.conn, e);
            }
            rules = Some(lr);
            if let Some(tid) = r.task_id {
                let v = tasks::view(r.conn, ws, tid, 4000)?;
                if let (Some(tp), Some(p)) = (&v.project_id, &project)
                    && tp != &p.id
                {
                    return Err(CoreError::invalid(format!("task “{}” belongs to another project", v.title)));
                }
                let v = if r.start_task {
                    tasks::start(r.conn, r.data_dir, ws, tid)?;
                    tasks::view(r.conn, ws, tid, 4000)?
                } else {
                    v
                };
                let (hooks_on, mcp_on) = (matches!(hooks, IntegrationStatus::Enabled { .. }), matches!(mcp, IntegrationStatus::Enabled { .. }));
                let delivery = match (hooks_on, mcp_on) {
                    (true, _) => "session-start",
                    (false, true) => "tools",
                    (false, false) => "none",
                };
                // Only NEXUS-authored text and the validated id go into the system prompt; the task's own text is
                // untrusted workspace content and arrives as data (SessionStart context / tool output).
                parts.push(tasks::pointer(&v.id, delivery));
                claude_name = Some(format!("NEXUS task {}", service::short_id(&v.id)));
                task = Some(TaskLaunch { id: v.id.clone(), title: v.title.clone(), status: v.status.clone(), delivery });
            }
        }
        None if r.task_id.is_some() => return Err(CoreError::invalid("a task can only be launched in its workspace")),
        None => {}
    }
    let observing = ctx.observe_instructions && matches!(hooks, IntegrationStatus::Enabled { .. });
    Ok(Prepared {
        integrations: Integrations {
            mcp,
            hooks,
            rules,
            task,
            isolation,
            kits: kit_launches,
            context: Some(ContextLaunch { strategy, observe_instructions: observing }),
        },
        prompt: (!parts.is_empty()).then(|| parts.join("\n\n")),
        settings,
        plugin_dirs,
        claude_name,
        policy: pol,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::profile::{self, ProfileInput};
    use crate::knowledge::service::{Author, CreateInput};

    #[test]
    fn task_sessions_get_a_brief_and_no_duplicate_rules() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let (home, data) = (root.join("home"), root.join("data"));
        let ws = workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("CLAUDE.md"), "# Conventions\n- Keep commits small and focused.\n").unwrap();
        let p = project::register(&conn, &ws.id, repo.to_str().unwrap(), Some("repo")).unwrap();
        std::fs::create_dir_all(root.join("repo2")).unwrap();
        let other = project::register(&conn, &ws.id, root.join("repo2").to_str().unwrap(), Some("other")).unwrap();
        let input = ProfileInput { name: "P".into(), config_dir: Some(root.join("cfg").to_string_lossy().into_owned()), ..Default::default() };
        let prof = profile::create_managed(&conn, &input, &home).unwrap();
        let rule = |title: &str, body: &str| CreateInput { kind: "rule".into(), title: title.into(), body: body.into(), applies: Some("always".into()), ..Default::default() };
        service::create(&conn, &data, &ws, &rule("Small commits", "Keep commits small and focused."), Author::Human).unwrap();
        service::create(&conn, &data, &ws, &rule("Tests", "Every change ships with a test."), Author::Human).unwrap();
        let task = service::create(
            &conn, &data, &ws,
            &CreateInput { kind: "task".into(), title: "Add login".into(), body: "Form.\n- [ ] Email\n".into(), project_id: Some(p.id.clone()), ..Default::default() },
            Author::Human,
        ).unwrap();
        let tid = task.id.clone().unwrap();
        let machine = MachinePolicyLocations::default();
        let req = |task_id: Option<&'static str>, project_id: &str| -> Result<Prepared> {
            let tid_owned: Option<String> = task_id.map(|_| tid.clone());
            prepare(&Request {
                conn: &conn, data_dir: &data, home: &home, profile: &prof, machine: &machine, nexus_exe: None,
                nexus_session_id: "00000000-0000-4000-8000-000000000001", workspace_id: Some(&ws.id), project_id: Some(project_id), task_id: tid_owned.as_deref(),
                cwd: &repo, base_env: &[], start_task: true,
            })
        };
        let prepared = req(Some("task"), &p.id).unwrap();
        let prompt = prepared.prompt.unwrap();
        assert!(prompt.contains("Every change ships with a test."));
        assert!(!prompt.contains("Keep commits small"), "already in the project's CLAUDE.md: not sent twice");
        assert_eq!(prepared.integrations.rules.unwrap().already_loaded, ["Small commits (in CLAUDE.md)"]);
        assert!(prompt.contains(&format!("NEXUS: this session works on NEXUS task {tid}.")), "{prompt}");
        assert!(!prompt.contains("Add login") && !prompt.contains("Email"), "no task text in the system prompt");
        let t = prepared.integrations.task.unwrap();
        assert_eq!((t.status.as_str(), t.delivery), ("in-progress", "none"), "launching started the task; no hooks/MCP here");
        assert!(matches!(prepared.integrations.mcp, mcp::IntegrationStatus::Unavailable { .. }));
        assert!(req(Some("task"), &other.id).is_err(), "a task launches only in its own project");
        let disabled = [("CLAUDE_CODE_DISABLE_CLAUDE_MDS".to_string(), "1".to_string())];
        assert!(claude_mds_disabled(&disabled) && !claude_mds_disabled(&[]));
    }
}

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use nexus_core::claude::auth::{self, AuthStatus};
use nexus_core::claude::install::{self, InstallSource, Installation};
use nexus_core::claude::policy::{self, PolicySummary};
use nexus_core::claude::profile::{self, ClaudeProfile, DetectedProfile, ProfileInput};
use nexus_core::claude::EFFORT_LEVELS;
use nexus_core::workprofile::{self, WorkProfile};

use crate::error::{IpcResult, blocking};
use crate::state::Ctx;

#[tauri::command]
pub async fn claude_installations(ctx: State<'_, Ctx>) -> IpcResult<Vec<Installation>> {
    let home = ctx.home.clone();
    blocking(move || Ok(install::discover(&home))).await
}

#[tauri::command]
pub fn profile_list(ctx: State<'_, Ctx>, include_archived: Option<bool>) -> IpcResult<Vec<ClaudeProfile>> {
    Ok(profile::list(&ctx.db(), include_archived.unwrap_or(false))?)
}

#[tauri::command]
pub async fn profile_detect(ctx: State<'_, Ctx>) -> IpcResult<Vec<DetectedProfile>> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let env_dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
        Ok(profile::detect(&ctx.db(), &ctx.home, env_dir.as_deref())?)
    })
    .await
}

#[tauri::command]
pub fn profile_register(ctx: State<'_, Ctx>, input: ProfileInput) -> IpcResult<ClaudeProfile> {
    Ok(profile::register(&ctx.db(), &input, &ctx.home)?)
}

#[tauri::command]
pub fn profile_create(ctx: State<'_, Ctx>, input: ProfileInput) -> IpcResult<ClaudeProfile> {
    Ok(profile::create_managed(&ctx.db(), &input, &ctx.home)?)
}

#[tauri::command]
pub fn profile_update(ctx: State<'_, Ctx>, id: String, input: ProfileInput) -> IpcResult<ClaudeProfile> {
    Ok(profile::update(&ctx.db(), &id, &input)?)
}

#[tauri::command]
pub fn profile_set_archived(ctx: State<'_, Ctx>, id: String, archived: bool) -> IpcResult<ClaudeProfile> {
    Ok(profile::set_archived(&ctx.db(), &id, archived)?)
}

#[tauri::command]
pub fn profile_forget(ctx: State<'_, Ctx>, id: String) -> IpcResult<()> {
    Ok(profile::forget(&ctx.db(), &id)?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDetails {
    pub effective_config_dir: PathBuf,
    pub config_dir_exists: bool,
    pub policy: PolicySummary,
    pub executable: Option<Installation>,
    pub executable_error: Option<String>,
}

/// Fast, read-only facts about a profile: effective directory, organisation policy, executable.
#[tauri::command]
pub async fn profile_details(ctx: State<'_, Ctx>, id: String) -> IpcResult<ProfileDetails> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let p = profile::get(&ctx.db(), &id)?;
        let dir = p.effective_config_dir(&ctx.home);
        let policy = policy::evaluate(&dir, &ctx.machine_policy);
        let (executable, executable_error) = match install::resolve(p.executable.as_deref(), &ctx.home) {
            Ok(path) => {
                let source = if p.executable.is_some() { InstallSource::Custom } else { InstallSource::Path };
                (Some(install::inspect(&path, source)), None)
            }
            Err(e) => (None, Some(e.to_string())),
        };
        Ok(ProfileDetails {
            config_dir_exists: dir.is_dir(),
            effective_config_dir: dir,
            policy,
            executable,
            executable_error,
        })
    })
    .await
}

/// `claude auth status --json` under the profile's environment. Not cached or logged (contains identity).
#[tauri::command]
pub async fn profile_auth_status(ctx: State<'_, Ctx>, id: String) -> IpcResult<AuthStatus> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let p = profile::get(&ctx.db(), &id)?;
        let exe = install::resolve(p.executable.as_deref(), &ctx.home)?;
        let s = auth::status(&exe, &p)?;
        // Classification only (subscription / API / unknown) — decides whether usage may be shown as spend.
        let _ = nexus_core::usage::record_billing(&ctx.db(), &p.id, &s);
        Ok(s)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    pub work_profiles: &'static [WorkProfile],
    pub default_work_profile: &'static str,
    pub effort_levels: &'static [&'static str],
    /// The first message NEXUS sends when the person asks Claude to start on a task right away (shown verbatim).
    pub task_kickoff: &'static str,
}

#[tauri::command]
pub fn launch_options() -> LaunchOptions {
    LaunchOptions {
        work_profiles: workprofile::builtins(),
        default_work_profile: workprofile::DEFAULT,
        effort_levels: EFFORT_LEVELS,
        task_kickoff: nexus_core::knowledge::tasks::KICKOFF,
    }
}

/// Claude Code setup: installation, version, profiles and the official installation methods (Portable Workspace P5).
#[tauri::command]
pub async fn claude_setup_status(ctx: State<'_, Ctx>) -> IpcResult<nexus_core::claude::setup::Status> {
    let ctx = ctx.inner().clone();
    crate::error::blocking(move || {
        let c = nexus_core::db::open(&ctx.data_dir.join(nexus_core::db::DB_FILE))?;
        Ok(nexus_core::claude::setup::status(&c, &ctx.home, true)?)
    })
    .await
}

/// What NEXUS gives Claude sessions: its built-in skills, the MCP server's tools (asked from the real `nexus mcp`
/// server next to this program), the hook events it subscribes to, and whether this machine's policy allows them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusTools {
    /// The `nexus` program sessions start for MCP and hooks (None = not found next to NEXUS).
    pub cli: Option<String>,
    pub tools: Vec<serde_json::Value>,
    /// Why the tool list could not be read, if it could not.
    pub tools_error: Option<String>,
    pub hooks: Vec<&'static str>,
    pub skills: Vec<nexus_core::claude::builtin::Skill>,
}

#[tauri::command]
pub async fn nexus_tools() -> IpcResult<NexusTools> {
    crate::error::blocking(move || {
        let exe = nexus_core::claude::mcp::nexus_next_to_current_exe();
        let (tools, tools_error) = match &exe {
            None => (Vec::new(), Some("the nexus program was not found next to NEXUS".to_string())),
            Some(p) => {
                let mut cmd = nexus_core::proc::command(p);
                cmd.args(["mcp", "--list-tools"]);
                match nexus_core::proc::run(cmd, std::time::Duration::from_secs(15)) {
                    Ok(o) if o.success() => match serde_json::from_str::<Vec<serde_json::Value>>(o.stdout_str().trim()) {
                        Ok(t) => (t, None),
                        Err(e) => (Vec::new(), Some(format!("unexpected tool list: {e}"))),
                    },
                    Ok(o) => (Vec::new(), Some(format!("nexus mcp --list-tools failed: {}", o.stderr_str().lines().next().unwrap_or_default()))),
                    Err(e) => (Vec::new(), Some(e.to_string())),
                }
            }
        };
        Ok(NexusTools {
            cli: exe.map(|p| p.display().to_string()),
            tools,
            tools_error,
            hooks: nexus_core::claude::hooks::SUBSCRIPTIONS.iter().map(|(e, _, _)| *e).collect(),
            skills: nexus_core::claude::builtin::skills(),
        })
    })
    .await
}

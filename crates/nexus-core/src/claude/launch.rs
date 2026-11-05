//! Building the exact process specification for a Claude session (ADR-0003, ADR-0004, ADR-0008).
//!
//! This is a pure function of its inputs so that profile-aware launching can be tested exhaustively
//! without spawning anything. The PTY layer only executes the returned [`LaunchSpec`].

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::claude::profile::{ClaudeProfile, validate_effort, validate_model};
use crate::env::{self, EnvChange, Policy, Reason};
use crate::error::{CoreError, Result};
use crate::workprofile::WorkProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// Start a new conversation with a NEXUS-assigned `--session-id`.
    New,
    /// Continue an existing conversation with `--resume`.
    Resume,
}

#[derive(Debug, Clone)]
pub struct LaunchRequest<'a> {
    pub profile: &'a ClaudeProfile,
    pub executable: &'a Path,
    pub cwd: &'a Path,
    pub mode: Mode,
    pub claude_session_id: &'a str,
    pub title: Option<&'a str>,
    pub work_profile: Option<&'a WorkProfile>,
    pub effort: Option<&'a str>,
    pub model: Option<&'a str>,
    pub nexus_session_id: &'a str,
    pub workspace_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    /// Session-scoped NEXUS MCP config (`--mcp-config`), when policy allows it (see `claude::mcp`).
    pub mcp_config: Option<&'a Path>,
    /// Session-scoped lifecycle-hook settings (`--settings`), when policy allows it (see `claude::hooks`).
    pub settings: Option<&'a Path>,
    /// NEXUS context (approved always-on rules, task brief — `claude::prepare`), appended after the work profile.
    pub context_prompt: Option<&'a str>,
    /// AI Kit overlays for this profile (`--plugin-dir`, ADR-0024).
    pub plugin_dirs: &'a [PathBuf],
    /// A first message for a new conversation, sent on the person's request (e.g. "start the task now"). A `'static`
    /// NEXUS constant by type — never workspace text (threat T22). Ignored when resuming.
    pub first_prompt: Option<&'static str>,
}

/// Everything needed to spawn the process. `env` is the *complete* environment (the spawner clears the
/// inherited one first), so nothing reaches the child by accident.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    #[serde(skip)]
    pub env: Vec<(String, String)>,
    /// Human-readable summary of environment changes (no secret values).
    pub env_changes: Vec<EnvChange>,
}

pub fn sanitize_title(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            c if c.is_control() => ' ',
            '<' => '‹',
            '>' => '›',
            c => c,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect()
}

/// The environment a Claude process for `profile` must see: inherited env minus scrubbed variables,
/// with `CLAUDE_CONFIG_DIR` set for explicit profiles and *absent* for the default location.
pub fn profile_env(
    profile: &ClaudeProfile,
    base_env: &[(String, String)],
    extra: &[(String, String, Reason)],
) -> (Vec<(String, String)>, Vec<EnvChange>) {
    let mut set: Vec<(String, String, Reason)> = Vec::new();
    if let Some(dir) = &profile.config_dir {
        set.push(("CLAUDE_CONFIG_DIR".into(), dir.to_string_lossy().into_owned(), Reason::ProfileManaged));
    }
    set.extend(extra.iter().cloned());
    let policy = Policy {
        claude: true,
        inherit_provider_env: profile.inherit_provider_env,
    };
    env::compose(base_env, &set, policy)
}

pub fn build(req: &LaunchRequest<'_>, base_env: &[(String, String)]) -> Result<LaunchSpec> {
    uuid::Uuid::parse_str(req.claude_session_id)
        .map_err(|_| CoreError::invalid("Claude session id must be a UUID"))?;
    if !req.cwd.is_absolute() {
        return Err(CoreError::invalid("working directory must be absolute"));
    }
    if !req.cwd.is_dir() {
        return Err(CoreError::invalid(format!("working directory {} does not exist", req.cwd.display())));
    }

    let mut args: Vec<String> = req.profile.extra_args.clone();
    match req.mode {
        Mode::New => args.extend(["--session-id".to_string(), req.claude_session_id.to_string()]),
        Mode::Resume => args.extend(["--resume".to_string(), req.claude_session_id.to_string()]),
    }
    if let Some(title) = req.title.map(sanitize_title).filter(|t| !t.is_empty()) {
        args.extend(["--name".to_string(), title]);
    }
    if let Some(effort) = req.effort {
        validate_effort(effort)?;
        args.extend(["--effort".to_string(), effort.to_string()]);
    }
    if let Some(model) = req.model {
        validate_model(model)?;
        args.extend(["--model".to_string(), model.to_string()]);
    }
    let appended: Vec<&str> = [req.work_profile.map(|w| w.prompt), req.context_prompt.filter(|r| !r.trim().is_empty())]
        .into_iter()
        .flatten()
        .collect();
    if !appended.is_empty() {
        args.extend(["--append-system-prompt".to_string(), appended.join("\n\n")]);
    }
    if let Some(cfg) = req.mcp_config {
        if !cfg.is_absolute() {
            return Err(CoreError::invalid("MCP config path must be absolute"));
        }
        args.extend(["--mcp-config".to_string(), cfg.display().to_string()]);
    }
    if let Some(settings) = req.settings {
        if !settings.is_absolute() {
            return Err(CoreError::invalid("settings path must be absolute"));
        }
        args.extend(["--settings".to_string(), settings.display().to_string()]);
    }
    for dir in req.plugin_dirs {
        if !dir.is_absolute() {
            return Err(CoreError::invalid("plugin folder must be absolute"));
        }
        args.extend(["--plugin-dir".to_string(), dir.display().to_string()]);
    }
    // After `--`, or a variadic option before it (`--mcp-config <configs...>`) would take the message as a file.
    if let (Some(first), Mode::New) = (req.first_prompt, &req.mode) {
        args.extend(["--".to_string(), first.to_string()]);
    }

    let mut extra = vec![
        ("NEXUS_SESSION_ID".to_string(), req.nexus_session_id.to_string(), Reason::NexusScope),
        ("TERM".to_string(), "xterm-256color".to_string(), Reason::Terminal),
        ("COLORTERM".to_string(), "truecolor".to_string(), Reason::Terminal),
    ];
    if let Some(ws) = req.workspace_id {
        extra.push(("NEXUS_WORKSPACE_ID".into(), ws.into(), Reason::NexusScope));
    }
    if let Some(p) = req.project_id {
        extra.push(("NEXUS_PROJECT_ID".into(), p.into(), Reason::NexusScope));
    }
    let (env, env_changes) = profile_env(req.profile, base_env, &extra);

    Ok(LaunchSpec {
        program: req.executable.to_path_buf(),
        args,
        cwd: req.cwd.to_path_buf(),
        env,
        env_changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workprofile;

    fn profile(dir: Option<&str>) -> ClaudeProfile {
        ClaudeProfile {
            id: "p1".into(),
            name: "P".into(),
            description: String::new(),
            color: "#000000".into(),
            config_dir: dir.map(PathBuf::from),
            executable: None,
            extra_args: vec!["--permission-mode".into(), "acceptEdits".into()],
            managed_by_nexus: false,
            inherit_provider_env: false,
            isolate_instructions: false,
            default_work_profile: None,
            default_effort: None,
            default_model: None,
            created_at: 0,
            updated_at: 0,
            last_used_at: None,
            archived_at: None,
        }
    }

    const SID: &str = "3b241101-e2bb-4255-8caf-4136c566a962";

    fn base_env() -> Vec<(String, String)> {
        vec![
            ("PATH".into(), "/usr/bin".into()),
            ("CLAUDE_CONFIG_DIR".into(), "/from/parent".into()),
            ("CLAUDECODE".into(), "1".into()),
            ("ANTHROPIC_API_KEY".into(), "sk-ambient".into()),
        ]
    }

    fn get_env<'a>(spec: &'a LaunchSpec, k: &str) -> Option<&'a str> {
        spec.env.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str())
    }

    fn request<'a>(p: &'a ClaudeProfile, cwd: &'a Path, mode: Mode) -> LaunchRequest<'a> {
        LaunchRequest {
            profile: p,
            executable: Path::new("claude"),
            cwd,
            mode,
            claude_session_id: SID,
            title: Some("Fix\nlogin  bug"),
            work_profile: workprofile::get("prototype"),
            effort: Some("high"),
            model: None,
            nexus_session_id: "nx-1",
            workspace_id: Some("ws-1"),
            project_id: Some("pr-1"),
            mcp_config: None,
            settings: None,
            context_prompt: None,
            plugin_dirs: &[],
            first_prompt: None,
        }
    }

    #[test]
    fn a_first_message_comes_last_after_the_options_and_only_for_new_conversations() {
        let t = tempfile::tempdir().unwrap();
        let p = profile(None);
        let mcp = t.path().join("mcp.json");
        let req = LaunchRequest { mcp_config: Some(&mcp), first_prompt: Some("Start now."), ..request(&p, t.path(), Mode::New) };
        let args = build(&req, &base_env()).unwrap().args;
        assert_eq!(&args[args.len() - 4..], ["--mcp-config", &mcp.display().to_string(), "--", "Start now."]);
        let resumed = LaunchRequest { first_prompt: Some("Start now."), ..request(&p, t.path(), Mode::Resume) };
        let args = build(&resumed, &base_env()).unwrap().args;
        assert!(!args.iter().any(|a| a == "--" || a == "Start now."), "{args:?}");
    }

    #[test]
    fn default_location_profile_removes_config_dir() {
        let t = tempfile::tempdir().unwrap();
        let p = profile(None);
        let spec = build(&request(&p, t.path(), Mode::New), &base_env()).unwrap();
        assert_eq!(get_env(&spec, "CLAUDE_CONFIG_DIR"), None, "inherited CLAUDE_CONFIG_DIR must be removed");
        assert_eq!(get_env(&spec, "CLAUDECODE"), None);
        assert_eq!(get_env(&spec, "ANTHROPIC_API_KEY"), None);
        assert_eq!(get_env(&spec, "NEXUS_SESSION_ID"), Some("nx-1"));
        assert_eq!(get_env(&spec, "NEXUS_PROJECT_ID"), Some("pr-1"));
        assert_eq!(get_env(&spec, "PATH"), Some("/usr/bin"));
    }

    #[test]
    fn explicit_profile_sets_config_dir() {
        let t = tempfile::tempdir().unwrap();
        let p = profile(Some("/home/u/.claude-personal"));
        let spec = build(&request(&p, t.path(), Mode::New), &base_env()).unwrap();
        assert_eq!(get_env(&spec, "CLAUDE_CONFIG_DIR"), Some("/home/u/.claude-personal"));
        let n = spec.env.iter().filter(|(k, _)| k.eq_ignore_ascii_case("CLAUDE_CONFIG_DIR")).count();
        assert_eq!(n, 1);
    }

    #[test]
    fn args_for_new_and_resume() {
        let t = tempfile::tempdir().unwrap();
        let p = profile(None);
        let spec = build(&request(&p, t.path(), Mode::New), &base_env()).unwrap();
        assert_eq!(&spec.args[..2], ["--permission-mode", "acceptEdits"], "profile args first");
        let joined = spec.args.join(" ");
        assert!(joined.contains(&format!("--session-id {SID}")));
        assert!(!joined.contains("--resume"));
        assert!(joined.contains("--name Fix login bug"), "title sanitized: {joined}");
        assert!(joined.contains("--effort high"));
        assert!(spec.args.iter().any(|a| a.starts_with("NEXUS work profile: PROTOTYPE")));

        let spec = build(&request(&p, t.path(), Mode::Resume), &base_env()).unwrap();
        let joined = spec.args.join(" ");
        assert!(joined.contains(&format!("--resume {SID}")));
        assert!(!joined.contains("--session-id"), "never both flags");
    }

    /// Golden argv: any change to what NEXUS passes to Claude must be deliberate (update this test with it).
    #[test]
    fn argv_is_exactly_as_specified() {
        let t = tempfile::tempdir().unwrap();
        let p = profile(Some("/cfg"));
        let mcp = t.path().join("mcp.json");
        let hooks = t.path().join("settings.json");
        let mut r = request(&p, t.path(), Mode::New);
        r.model = Some("opus");
        r.mcp_config = Some(&mcp);
        r.settings = Some(&hooks);
        let spec = build(&r, &base_env()).unwrap();
        let prompt = workprofile::get("prototype").unwrap().prompt;
        let mcp_arg = mcp.display().to_string();
        let hooks_arg = hooks.display().to_string();
        assert_eq!(
            spec.args,
            [
                "--permission-mode", "acceptEdits",
                "--session-id", SID,
                "--name", "Fix login bug",
                "--effort", "high",
                "--model", "opus",
                "--append-system-prompt", prompt,
                "--mcp-config", mcp_arg.as_str(),
                "--settings", hooks_arg.as_str(),
            ]
        );
        assert_eq!(spec.program, Path::new("claude"));
        assert_eq!(spec.cwd, t.path());
    }

    #[test]
    fn rules_follow_the_work_profile_in_one_appended_prompt() {
        let t = tempfile::tempdir().unwrap();
        let p = profile(None);
        let mut r = request(&p, t.path(), Mode::New);
        r.context_prompt = Some("NEXUS rules (always apply):\n\n### Tests\nAlways add tests.");
        let spec = build(&r, &base_env()).unwrap();
        let i = spec.args.iter().position(|a| a == "--append-system-prompt").unwrap();
        assert!(spec.args[i + 1].starts_with("NEXUS work profile: PROTOTYPE"));
        assert!(spec.args[i + 1].ends_with("Always add tests."));
        assert_eq!(spec.args.iter().filter(|a| *a == "--append-system-prompt").count(), 1);
        r.work_profile = None;
        let spec = build(&r, &base_env()).unwrap();
        let i = spec.args.iter().position(|a| a == "--append-system-prompt").unwrap();
        assert!(spec.args[i + 1].starts_with("NEXUS rules"));
    }

    #[test]
    fn rejects_invalid_requests() {
        let t = tempfile::tempdir().unwrap();
        let p = profile(None);
        let mut r = request(&p, t.path(), Mode::New);
        r.claude_session_id = "not-a-uuid; rm -rf /";
        assert!(build(&r, &base_env()).is_err());
        let mut r = request(&p, t.path(), Mode::New);
        r.effort = Some("ultra");
        assert!(build(&r, &base_env()).is_err());
        let mut r = request(&p, t.path(), Mode::New);
        r.model = Some("opus && calc.exe");
        assert!(build(&r, &base_env()).is_err());
        let missing = t.path().join("missing");
        let r = request(&p, &missing, Mode::New);
        assert!(build(&r, &base_env()).is_err());
    }

    #[test]
    fn hostile_title_is_inert_argument() {
        let t = tempfile::tempdir().unwrap();
        let p = profile(None);
        let mut r = request(&p, t.path(), Mode::New);
        r.title = Some("x\" & calc.exe & echo \"$(whoami)`id`");
        let spec = build(&r, &base_env()).unwrap();
        // It stays one argv element; nothing is interpreted because no shell is involved.
        let idx = spec.args.iter().position(|a| a == "--name").unwrap();
        assert_eq!(spec.args[idx + 1], "x\" & calc.exe & echo \"$(whoami)`id`");
    }

    #[test]
    fn env_changes_never_contain_removed_values() {
        let t = tempfile::tempdir().unwrap();
        let p = profile(None);
        let spec = build(&request(&p, t.path(), Mode::New), &base_env()).unwrap();
        let s = serde_json::to_string(&spec).unwrap();
        assert!(!s.contains("sk-ambient") && !s.contains("/from/parent"));
        assert!(s.contains("ANTHROPIC_API_KEY"), "removal is still visible by name");
    }
}

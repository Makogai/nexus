//! The exact process specification for a Codex session (ADR-0052): a pure function of its inputs, like
//! `claude::launch`. Everything NEXUS gives the session is a `-c key=value` override for this one run (TOML, escaped
//! here); nothing is written into the person's Codex configuration.

use std::path::Path;

use crate::agent::Kind;
use crate::agent::profile::AgentProfile;
use crate::claude::launch::LaunchSpec;
use crate::env::{self, Policy, Reason};
use crate::error::{CoreError, Result};
use crate::workprofile::WorkProfile;

/// The NEXUS program Codex starts for MCP and turn notifications, and the data dir it must use.
#[derive(Debug, Clone, Copy)]
pub struct NexusBinding<'a> {
    pub nexus_exe: &'a Path,
    pub data_dir: &'a Path,
}

#[derive(Debug, Clone)]
pub struct LaunchRequest<'a> {
    pub profile: &'a AgentProfile,
    pub executable: &'a Path,
    pub cwd: &'a Path,
    /// Continue this Codex conversation (its thread id, a UUID); `None` starts a new one.
    pub resume: Option<&'a str>,
    pub work_profile: Option<&'a WorkProfile>,
    pub model: Option<&'a str>,
    pub nexus_session_id: &'a str,
    pub workspace_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    /// Bind the NEXUS MCP server (`-c mcp_servers.nexus.*`).
    pub mcp: Option<NexusBinding<'a>>,
    /// Report finished turns to NEXUS (`-c notify=[…]`), when the profile keeps turn notifications on.
    pub notify: Option<NexusBinding<'a>>,
    /// NEXUS-authored text after the work profile (approved rules, task pointer) — never workspace text (T22).
    pub context: Option<&'a str>,
    /// A first message for a new conversation: a NEXUS constant by type (T22). Ignored when resuming.
    pub first_prompt: Option<&'static str>,
}

/// A TOML basic string (`"…"`) for a `-c` value: backslashes, quotes and control characters escaped.
pub fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn toml_array(items: &[String]) -> String {
    format!("[{}]", items.iter().map(|s| toml_string(s)).collect::<Vec<_>>().join(","))
}

/// `nexus --data-dir <dir> <args…>` as one argument list.
fn nexus_args(b: &NexusBinding<'_>, rest: &[&str]) -> Vec<String> {
    let mut v = vec!["--data-dir".to_string(), b.data_dir.display().to_string()];
    v.extend(rest.iter().map(|s| s.to_string()));
    v
}

/// Options a Codex profile may add. Everything else is refused: what NEXUS manages (`-c`, `-C`, `-m`, `--remote`),
/// what removes protections, and anything that could be read as a subcommand or prompt.
const VALUE_FLAGS: &[&str] = &["-p", "--profile", "-s", "--sandbox", "-a", "--ask-for-approval", "--local-provider", "--enable", "--disable"];
const SWITCHES: &[&str] = &["--search", "--oss", "--no-alt-screen", "--no-daemon"];

pub fn validate_extra_args(args: &[String]) -> Result<()> {
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a.is_empty() || a.len() > 512 || a.contains('\0') {
            return Err(CoreError::invalid("launcher arguments must be 1–512 characters without NUL"));
        }
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v)),
            _ => (a.as_str(), None),
        };
        if SWITCHES.contains(&flag) && inline.is_none() {
            i += 1;
            continue;
        }
        if !VALUE_FLAGS.contains(&flag) {
            return Err(CoreError::invalid(format!(
                "'{flag}' is not an option NEXUS passes to Codex (allowed: {}, {})",
                VALUE_FLAGS.join(" "),
                SWITCHES.join(" ")
            )));
        }
        let value = match inline {
            Some(v) => v.to_string(),
            None => {
                i += 1;
                args.get(i).cloned().ok_or_else(|| CoreError::invalid(format!("'{flag}' needs a value")))?
            }
        };
        if value.is_empty() || value.starts_with('-') {
            return Err(CoreError::invalid(format!("'{flag}' needs a value")));
        }
        if matches!(flag, "-s" | "--sandbox") && value == "danger-full-access" {
            return Err(CoreError::invalid("NEXUS does not start Codex without its sandbox (danger-full-access)"));
        }
        i += 1;
    }
    Ok(())
}

pub fn build(req: &LaunchRequest<'_>, base_env: &[(String, String)]) -> Result<LaunchSpec> {
    if req.profile.agent != Kind::Codex {
        return Err(CoreError::invalid("not a Codex profile"));
    }
    if !req.cwd.is_absolute() || !req.cwd.is_dir() {
        return Err(CoreError::invalid(format!("working directory {} does not exist", req.cwd.display())));
    }
    if let Some(id) = req.resume {
        uuid::Uuid::parse_str(id).map_err(|_| CoreError::invalid("Codex conversation id must be a UUID"))?;
    }
    validate_extra_args(&req.profile.extra_args)?;

    let mut args: Vec<String> = Vec::new();
    if req.resume.is_some() {
        args.push("resume".into());
    }
    args.extend(req.profile.extra_args.iter().cloned());
    if let Some(m) = req.model {
        crate::claude::profile::validate_model(m)?;
        args.extend(["-m".to_string(), m.to_string()]);
    }
    let mut overrides: Vec<String> = Vec::new();
    if let Some(b) = &req.mcp {
        if !b.nexus_exe.is_absolute() || !b.data_dir.is_absolute() {
            return Err(CoreError::invalid("the nexus program and data folder must be absolute"));
        }
        uuid::Uuid::parse_str(req.nexus_session_id).map_err(|_| CoreError::invalid("session id must be a UUID"))?;
        overrides.push(format!("mcp_servers.{}.command={}", crate::claude::mcp::SERVER_NAME, toml_string(&b.nexus_exe.display().to_string())));
        overrides.push(format!("mcp_servers.{}.args={}", crate::claude::mcp::SERVER_NAME, toml_array(&nexus_args(b, &["mcp", "--session", req.nexus_session_id]))));
    }
    let text: Vec<&str> = [req.work_profile.map(|w| w.prompt), req.context.filter(|c| !c.trim().is_empty())].into_iter().flatten().collect();
    if !text.is_empty() {
        overrides.push(format!("developer_instructions={}", toml_string(&text.join("\n\n"))));
    }
    if let Some(b) = &req.notify {
        if !b.nexus_exe.is_absolute() || !b.data_dir.is_absolute() {
            return Err(CoreError::invalid("the nexus program and data folder must be absolute"));
        }
        let mut argv = vec![b.nexus_exe.display().to_string()];
        argv.extend(nexus_args(b, &["agent-notify", "--agent", "codex", "--session", req.nexus_session_id]));
        overrides.push(format!("notify={}", toml_array(&argv)));
    }
    for o in overrides {
        args.extend(["-c".to_string(), o]);
    }
    match (req.resume, req.first_prompt) {
        (Some(id), _) => args.push(id.to_string()),
        (None, Some(first)) => args.extend(["--".to_string(), first.to_string()]),
        (None, None) => {}
    }

    let mut set = vec![
        ("NEXUS_SESSION_ID".to_string(), req.nexus_session_id.to_string(), Reason::NexusScope),
        ("TERM".to_string(), "xterm-256color".to_string(), Reason::Terminal),
        ("COLORTERM".to_string(), "truecolor".to_string(), Reason::Terminal),
    ];
    if let Some(ws) = req.workspace_id {
        set.push(("NEXUS_WORKSPACE_ID".into(), ws.into(), Reason::NexusScope));
    }
    if let Some(p) = req.project_id {
        set.push(("NEXUS_PROJECT_ID".into(), p.into(), Reason::NexusScope));
    }
    if let Some(home) = &req.profile.home_dir {
        set.push((Kind::Codex.home_var().into(), home.display().to_string(), Reason::ProfileManaged));
    }
    let (env, env_changes) = env::compose(base_env, &set, Policy::agent(Kind::Codex, req.profile.inherit_provider_env));
    Ok(LaunchSpec { program: req.executable.to_path_buf(), args, cwd: req.cwd.to_path_buf(), env, env_changes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::profile::AgentProfileInput;
    use std::path::PathBuf;

    const SID: &str = "3b241101-e2bb-4255-8caf-4136c566a962";
    const THREAD: &str = "0199a213-81c0-7800-8aa1-bbab2a035a53";

    fn profile(home: Option<&str>, extra: &[&str]) -> AgentProfile {
        let i = AgentProfileInput::default_for(Kind::Codex);
        AgentProfile {
            id: "p".into(),
            agent: Kind::Codex,
            name: i.name,
            home_dir: home.map(PathBuf::from),
            executable: None,
            extra_args: extra.iter().map(|s| s.to_string()).collect(),
            inherit_provider_env: false,
            turn_notifications: true,
            default_work_profile: None,
            default_model: None,
            created_at: 0,
            updated_at: 0,
            last_used_at: None,
            archived_at: None,
        }
    }

    fn base_env() -> Vec<(String, String)> {
        [("PATH", "/bin"), ("CODEX_HOME", "/parent"), ("OPENAI_API_KEY", "sk-ambient"), ("CLAUDECODE", "1")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn overrides(spec: &LaunchSpec) -> Vec<String> {
        spec.args.windows(2).filter(|w| w[0] == "-c").map(|w| w[1].clone()).collect()
    }

    #[test]
    fn a_new_session_gets_nexus_through_overrides_only() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dunce::canonicalize(dir.path()).unwrap();
        let exe = cwd.join("nexus.exe");
        let data = cwd.join("data dir");
        let b = NexusBinding { nexus_exe: &exe, data_dir: &data };
        let p = profile(Some("/homes/codex-work"), &["--search", "--sandbox", "workspace-write"]);
        let wp = crate::workprofile::get("professional").unwrap();
        let spec = build(
            &LaunchRequest {
                profile: &p,
                executable: Path::new("/bin/codex"),
                cwd: &cwd,
                resume: None,
                work_profile: Some(wp),
                model: Some("gpt-6"),
                nexus_session_id: SID,
                workspace_id: Some("w"),
                project_id: Some("p1"),
                mcp: Some(b),
                notify: Some(b),
                context: Some("NEXUS: rules \"quoted\" C:\\path"),
                first_prompt: Some(crate::knowledge::tasks::KICKOFF),
            },
            &base_env(),
        )
        .unwrap();
        assert_eq!(&spec.args[..3], ["--search", "--sandbox", "workspace-write"], "the profile's options first");
        assert!(spec.args.windows(2).any(|w| w == ["-m", "gpt-6"]));
        let o = overrides(&spec);
        assert_eq!(o[0], format!("mcp_servers.nexus.command={}", toml_string(&exe.display().to_string())));
        assert!(o[1].starts_with("mcp_servers.nexus.args=[\"--data-dir\",") && o[1].contains(&format!("\"mcp\",\"--session\",\"{SID}\"")), "{}", o[1]);
        assert!(o[2].starts_with("developer_instructions=\"") && o[2].contains("\\\"quoted\\\"") && o[2].contains("C:\\\\path"), "{}", o[2]);
        assert!(o[2].contains(&toml_string(wp.prompt)[1..20]), "the work profile comes first");
        assert!(o[3].starts_with("notify=[") && o[3].contains("\"agent-notify\",\"--agent\",\"codex\",\"--session\""), "{}", o[3]);
        assert_eq!(&spec.args[spec.args.len() - 2..], ["--", crate::knowledge::tasks::KICKOFF], "the kickoff is the prompt, after --");
        let env = |k: &str| spec.env.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone());
        assert_eq!(env("CODEX_HOME").as_deref(), Some("/homes/codex-work"));
        assert_eq!(env("NEXUS_SESSION_ID").as_deref(), Some(SID));
        assert_eq!((env("OPENAI_API_KEY"), env("CLAUDECODE")), (None, None));
        assert!(!format!("{:?}", spec.env_changes).contains("sk-ambient"));
    }

    #[test]
    fn resume_continues_the_thread_and_the_default_home_is_codexs_own() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dunce::canonicalize(dir.path()).unwrap();
        let p = profile(None, &[]);
        let req = LaunchRequest {
            profile: &p,
            executable: Path::new("/bin/codex"),
            cwd: &cwd,
            resume: Some(THREAD),
            work_profile: None,
            model: None,
            nexus_session_id: SID,
            workspace_id: None,
            project_id: None,
            mcp: None,
            notify: None,
            context: None,
            first_prompt: Some(crate::knowledge::tasks::KICKOFF),
        };
        let spec = build(&req, &base_env()).unwrap();
        assert_eq!(spec.args, ["resume", THREAD], "no first prompt when resuming; nothing NEXUS was not asked for");
        assert!(!spec.env.iter().any(|(k, _)| k.eq_ignore_ascii_case("CODEX_HOME")), "the default home: an inherited CODEX_HOME is removed");
        let bad = LaunchRequest { resume: Some("--last"), ..req.clone() };
        assert!(build(&bad, &base_env()).is_err(), "only a recorded UUID");
    }

    #[test]
    fn extra_arguments_are_an_allowlist() {
        let ok = |a: &[&str]| validate_extra_args(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>()).is_ok();
        assert!(ok(&["--search", "-p", "work", "--sandbox=workspace-write", "-a", "on-request", "--enable", "fast_mode"]));
        for bad in [
            &["-c", "notify=[]"][..],
            &["--config=mcp_servers.nexus.command=\"x\""],
            &["-C", "/elsewhere"],
            &["-m", "o3"],
            &["--dangerously-bypass-approvals-and-sandbox"],
            &["--dangerously-bypass-hook-trust"],
            &["--sandbox", "danger-full-access"],
            &["-s=danger-full-access"],
            &["--sandbox=danger-full-access"],
            &["exec"],
            &["resume"],
            &["--profile"],
            &["--profile", "--search"],
            &["--search=yes"],
            &["--remote", "ws://x"],
        ] {
            assert!(!ok(bad), "{bad:?} must be refused");
        }
    }

    #[test]
    fn toml_strings_escape_what_toml_requires() {
        assert_eq!(toml_string(r#"C:\a "b""#), r#""C:\\a \"b\"""#);
        assert_eq!(toml_string("a\nb\tc\u{1}"), "\"a\\nb\\tc\\u0001\"");
        assert_eq!(toml_string("ünïcode ✓"), "\"ünïcode ✓\"");
        assert_eq!(toml_array(&["a b".into(), "c".into()]), "[\"a b\",\"c\"]");
    }
}

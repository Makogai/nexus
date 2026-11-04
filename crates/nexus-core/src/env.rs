//! Child-environment hygiene (ADR-0003, threats T2/T3).
//!
//! Children start from NEXUS's own environment, minus variables that would make them misbehave or
//! leak across profiles:
//!
//! * **Session markers** that Claude Code sets for its own children. If NEXUS itself was started from a
//!   Claude session they would make every launched `claude` a "nested" session (hidden from `--resume`),
//!   and the messaging token is a secret of the parent session.
//! * **Telemetry** (`OTEL_*`, `CLAUDE_CODE_ENABLE_TELEMETRY`). Telemetry belongs to the profile (managed or
//!   profile settings), never to the ambient environment: an organisation's exporter inherited from a work
//!   session must not receive a personal profile's events.
//! * **Provider credentials** (`ANTHROPIC_API_KEY`, …) unless the profile opts in: an ambient key silently
//!   overrides the profile's own login.
//! * **Profile-managed** `CLAUDE_CONFIG_DIR` and stale `NEXUS_*` scope variables: NEXUS sets them explicitly.
//!
//! Legitimate user configuration such as `CLAUDE_CODE_GIT_BASH_PATH` or `CLAUDE_CODE_USE_BEDROCK` is kept.

use serde::Serialize;

pub const SESSION_MARKERS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_CODE_SIMPLE",
    "CLAUDE_CODE_SAFE_MODE",
    "CLAUDE_PROJECT_DIR",
    "CLAUDE_PLUGIN_ROOT",
    "CLAUDE_PLUGIN_DATA",
    "CLAUDE_ENV_FILE",
];

pub const PROVIDER_CREDENTIALS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_CUSTOM_HEADERS",
];

pub const NEXUS_SCOPE: &[&str] = &["NEXUS_SESSION_ID", "NEXUS_WORKSPACE_ID", "NEXUS_PROJECT_ID"];

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    SessionMarker,
    Telemetry,
    ProviderCredential,
    ProfileManaged,
    NexusScope,
    Terminal,
}

/// Which scrub rules apply.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    /// Claude sessions: also scrub telemetry, `CLAUDE_CONFIG_DIR` and (unless allowed) provider credentials.
    pub claude: bool,
    /// Keep `ANTHROPIC_*` credentials (profile opt-in).
    pub inherit_provider_env: bool,
}

impl Policy {
    pub const CLAUDE: Policy = Policy {
        claude: true,
        inherit_provider_env: false,
    };
    /// Plain shells keep the user's environment except Claude session markers and NEXUS scope.
    pub const SHELL: Policy = Policy {
        claude: false,
        inherit_provider_env: true,
    };
}

pub fn scrub_reason(name: &str, policy: Policy) -> Option<Reason> {
    let up = name.to_ascii_uppercase();
    if SESSION_MARKERS.contains(&up.as_str()) {
        return Some(Reason::SessionMarker);
    }
    if NEXUS_SCOPE.contains(&up.as_str()) {
        return Some(Reason::NexusScope);
    }
    if !policy.claude {
        return None;
    }
    if up == "CLAUDE_CONFIG_DIR" {
        return Some(Reason::ProfileManaged);
    }
    if up.starts_with("OTEL_") || up == "CLAUDE_CODE_ENABLE_TELEMETRY" {
        return Some(Reason::Telemetry);
    }
    if !policy.inherit_provider_env && PROVIDER_CREDENTIALS.contains(&up.as_str()) {
        return Some(Reason::ProviderCredential);
    }
    None
}

/// A visible record of what NEXUS changed. Values are only included for variables NEXUS set itself
/// (never secret); removed variables are listed by name only.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvChange {
    pub name: String,
    pub action: EnvAction,
    pub value: Option<String>,
    pub reason: Reason,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EnvAction {
    Set,
    Removed,
}

/// Build the complete child environment: `base` minus scrubbed variables, plus `set` (which wins,
/// case-insensitively).
pub fn compose(
    base: &[(String, String)],
    set: &[(String, String, Reason)],
    policy: Policy,
) -> (Vec<(String, String)>, Vec<EnvChange>) {
    let mut env = Vec::with_capacity(base.len() + set.len());
    let mut changes = Vec::new();
    for (k, v) in base {
        if set.iter().any(|(sk, _, _)| sk.eq_ignore_ascii_case(k)) {
            continue;
        }
        if let Some(reason) = scrub_reason(k, policy) {
            changes.push(EnvChange {
                name: k.clone(),
                action: EnvAction::Removed,
                value: None,
                reason,
            });
            continue;
        }
        env.push((k.clone(), v.clone()));
    }
    for (k, v, reason) in set {
        env.push((k.clone(), v.clone()));
        changes.push(EnvChange {
            name: k.clone(),
            action: EnvAction::Set,
            value: Some(v.clone()),
            reason: *reason,
        });
    }
    env.sort_by_key(|(k, _)| k.to_ascii_uppercase());
    (env, changes)
}

/// Environment for an interactive shell terminal: the user's environment minus Claude session markers and stale
/// NEXUS scope variables, plus terminal capabilities. Provider credentials and `CLAUDE_CONFIG_DIR` are the user's
/// own shell configuration and are kept.
pub fn shell_env(base: &[(String, String)]) -> Vec<(String, String)> {
    let set = [
        ("TERM".to_string(), "xterm-256color".to_string(), Reason::Terminal),
        ("COLORTERM".to_string(), "truecolor".to_string(), Reason::Terminal),
    ];
    compose(base, &set, Policy::SHELL).0
}

/// Snapshot of this process's environment (skipping non-UTF-8 entries, which cannot be passed on reliably).
pub fn current() -> Vec<(String, String)> {
    std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect()
}

/// Apply a fully composed environment to a `Command` (clears everything else).
pub fn apply(cmd: &mut std::process::Command, env: &[(String, String)]) {
    cmd.env_clear();
    cmd.envs(env.iter().map(|(k, v)| (k, v)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Vec<(String, String)> {
        [
            ("PATH", "/bin"),
            ("CLAUDECODE", "1"),
            ("CLAUDE_CODE_MESSAGING_TOKEN", "secret"),
            ("CLAUDE_CODE_CHILD_SESSION", "1"),
            ("CLAUDE_CODE_GIT_BASH_PATH", "C:/git/bin/bash.exe"),
            ("CLAUDE_CODE_USE_BEDROCK", "1"),
            ("claude_config_dir", "/wrong"),
            ("ANTHROPIC_API_KEY", "sk-ambient"),
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://corp"),
            ("CLAUDE_CODE_ENABLE_TELEMETRY", "1"),
            ("NEXUS_SESSION_ID", "stale"),
            ("NEXUS_DATA_DIR", "/tmp/nx"),
            ("HOME", "/home/u"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    fn names(env: &[(String, String)]) -> Vec<String> {
        env.iter().map(|(k, _)| k.to_ascii_uppercase()).collect()
    }

    #[test]
    fn claude_policy_scrubs_markers_telemetry_and_credentials() {
        let set = vec![("CLAUDE_CONFIG_DIR".to_string(), "/p".to_string(), Reason::ProfileManaged)];
        let (env, changes) = compose(&base(), &set, Policy::CLAUDE);
        let n = names(&env);
        for gone in [
            "CLAUDECODE",
            "CLAUDE_CODE_MESSAGING_TOKEN",
            "CLAUDE_CODE_CHILD_SESSION",
            "ANTHROPIC_API_KEY",
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            "CLAUDE_CODE_ENABLE_TELEMETRY",
            "NEXUS_SESSION_ID",
        ] {
            assert!(!n.contains(&gone.to_string()), "{gone} leaked");
        }
        for kept in ["PATH", "HOME", "CLAUDE_CODE_GIT_BASH_PATH", "CLAUDE_CODE_USE_BEDROCK", "NEXUS_DATA_DIR"] {
            assert!(n.contains(&kept.to_string()), "{kept} dropped");
        }
        let cfg: Vec<_> = env.iter().filter(|(k, _)| k.eq_ignore_ascii_case("CLAUDE_CONFIG_DIR")).collect();
        assert_eq!(cfg.len(), 1, "exactly one CLAUDE_CONFIG_DIR regardless of case");
        assert_eq!(cfg[0].1, "/p");
        assert!(changes.iter().all(|c| c.action == EnvAction::Set || c.value.is_none()), "no removed values");
        assert!(!format!("{changes:?}").contains("secret") && !format!("{changes:?}").contains("sk-ambient"));
    }

    fn variants(name: &str) -> Vec<String> {
        let lower = name.to_ascii_lowercase();
        let mixed: String = name
            .chars()
            .enumerate()
            .map(|(i, c)| if i % 2 == 0 { c.to_ascii_lowercase() } else { c })
            .collect();
        vec![name.to_string(), lower, mixed]
    }

    #[test]
    fn every_scrubbed_name_is_removed_in_any_case() {
        let mut must_go: Vec<&str> = SESSION_MARKERS.to_vec();
        must_go.extend(PROVIDER_CREDENTIALS);
        must_go.extend(NEXUS_SCOPE);
        must_go.extend(["CLAUDE_CONFIG_DIR", "CLAUDE_CODE_ENABLE_TELEMETRY", "OTEL_EXPORTER_OTLP_HEADERS", "OTEL_RESOURCE_ATTRIBUTES"]);
        for name in must_go {
            for v in variants(name) {
                assert!(scrub_reason(&v, Policy::CLAUDE).is_some(), "{v} must not reach Claude");
                let base = vec![(v.clone(), "x".to_string())];
                let (env, _) = compose(&base, &[], Policy::CLAUDE);
                assert!(env.is_empty(), "{v} survived compose");
            }
        }
    }

    #[test]
    fn legitimate_configuration_is_kept() {
        for name in [
            "PATH",
            "Path",
            "HOME",
            "USERPROFILE",
            "SystemRoot",
            "TEMP",
            "CLAUDE_CODE_GIT_BASH_PATH",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "NEXUS_DATA_DIR",
            "NEXUS_LOG",
            "ANTHROPIC_MODEL",
            "HTTPS_PROXY",
        ] {
            assert_eq!(scrub_reason(name, Policy::CLAUDE), None, "{name} is user configuration and must be kept");
        }
    }

    #[test]
    fn shell_env_keeps_user_config_but_drops_markers() {
        let env = shell_env(&base());
        let n = names(&env);
        for gone in ["CLAUDECODE", "CLAUDE_CODE_MESSAGING_TOKEN", "CLAUDE_CODE_CHILD_SESSION", "NEXUS_SESSION_ID"] {
            assert!(!n.contains(&gone.to_string()), "{gone} reached a shell");
        }
        for kept in ["ANTHROPIC_API_KEY", "CLAUDE_CONFIG_DIR", "NEXUS_DATA_DIR", "PATH"] {
            assert!(n.contains(&kept.to_string()), "{kept} is the user's shell configuration");
        }
        assert!(env.iter().any(|(k, v)| k == "TERM" && v == "xterm-256color"));
    }

    #[test]
    fn provider_opt_in_keeps_key() {
        let policy = Policy {
            claude: true,
            inherit_provider_env: true,
        };
        let (env, _) = compose(&base(), &[], policy);
        assert!(names(&env).contains(&"ANTHROPIC_API_KEY".to_string()));
    }

    #[test]
    fn shell_policy_only_removes_markers() {
        let (env, _) = compose(&base(), &[], Policy::SHELL);
        let n = names(&env);
        assert!(!n.contains(&"CLAUDECODE".to_string()));
        assert!(!n.contains(&"CLAUDE_CODE_MESSAGING_TOKEN".to_string()));
        assert!(n.contains(&"ANTHROPIC_API_KEY".to_string()));
        assert!(n.contains(&"CLAUDE_CONFIG_DIR".to_string()));
    }
}

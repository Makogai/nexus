//! Environment-isolation regression suite for every place NEXUS spawns a Claude executable outside a session
//! (auth status, version probe). Sessions are covered by `apps/desktop/src-tauri/tests/pty_launch.rs` and the E2E
//! smoke test. Each test runs `fake-claude` under a deliberately hostile parent environment — the one NEXUS has when
//! it is started from inside an organisation-managed Claude session.

use std::path::{Path, PathBuf};

use nexus_core::claude::{auth, install, profile::ClaudeProfile};

fn fake_claude() -> PathBuf {
    let exe = if cfg!(windows) { "fake-claude.exe" } else { "fake-claude" };
    let dir = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_path_buf();
    let path = dir.join(exe);
    if !path.is_file() {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let ok = std::process::Command::new(cargo).args(["build", "-q", "-p", "fake-claude"]).status().unwrap();
        assert!(ok.success());
    }
    path
}

/// Parent environment polluted with every class of variable that must not reach Claude.
fn hostile_env() -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = nexus_core::env::current()
        .into_iter()
        .filter(|(k, _)| {
            let up = k.to_ascii_uppercase();
            !up.starts_with("CLAUDE") && !up.starts_with("ANTHROPIC") && !up.starts_with("OTEL_") && !up.starts_with("NEXUS_")
        })
        .collect();
    for (k, v) in [
        ("CLAUDECODE", "1"),
        ("CLAUDE_CODE_SESSION_ID", "parent"),
        ("CLAUDE_CODE_CHILD_SESSION", "1"),
        ("CLAUDE_CODE_MESSAGING_TOKEN", "parent-secret"),
        ("CLAUDE_CODE_ENTRYPOINT", "cli"),
        ("CLAUDE_CODE_ENABLE_TELEMETRY", "1"),
        ("claude_config_dir", "/parent/config"), // odd case on purpose
        ("ANTHROPIC_API_KEY", "sk-ambient"),
        ("ANTHROPIC_AUTH_TOKEN", "ambient-token"),
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://corp.example.invalid"),
        ("NEXUS_SESSION_ID", "stale-parent-session"),
    ] {
        env.push((k.into(), v.into()));
    }
    env
}

fn profile(config_dir: Option<&Path>, inherit_provider_env: bool) -> ClaudeProfile {
    ClaudeProfile {
        id: "p".into(),
        name: "P".into(),
        description: String::new(),
        color: "#000000".into(),
        config_dir: config_dir.map(Path::to_path_buf),
        executable: None,
        extra_args: vec![],
        managed_by_nexus: false,
        inherit_provider_env,
        default_work_profile: None,
        default_effort: None,
        default_model: None,
        created_at: 0,
        updated_at: 0,
        last_used_at: None,
        archived_at: None,
    }
}

#[test]
fn auth_status_for_default_profile_sees_no_parent_configuration() {
    let s = auth::status_with_env(&fake_claude(), &profile(None, false), &hostile_env()).unwrap();
    assert_eq!(s.email.as_deref(), Some("default"), "inherited CLAUDE_CONFIG_DIR (any case) must be removed");
    assert_eq!(s.org_name.as_deref(), Some("leaked:"), "nothing isolation-relevant may leak");
}

#[test]
fn auth_status_for_explicit_profile_uses_only_its_directory() {
    let t = tempfile::tempdir().unwrap();
    let s = auth::status_with_env(&fake_claude(), &profile(Some(t.path()), false), &hostile_env()).unwrap();
    assert_eq!(s.email.as_deref(), Some(t.path().to_str().unwrap()));
    assert_eq!(s.org_name.as_deref(), Some("leaked:"));
}

#[test]
fn provider_opt_in_passes_credentials_but_still_scrubs_markers_and_telemetry() {
    let s = auth::status_with_env(&fake_claude(), &profile(None, true), &hostile_env()).unwrap();
    assert_eq!(s.org_name.as_deref(), Some("leaked:ANTHROPIC_API_KEY,ANTHROPIC_AUTH_TOKEN"));
}

#[test]
fn version_probe_is_isolated() {
    let t = tempfile::tempdir().unwrap();
    let report = t.path().join("report.json");
    let mut env = hostile_env();
    env.push(("FAKE_CLAUDE_REPORT".into(), report.display().to_string()));
    assert_eq!(install::version_with_env(&fake_claude(), &env).as_deref(), Some("9.9.9"));
    let r: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&report).unwrap()).unwrap();
    for (k, v) in r["env"].as_object().unwrap() {
        if k == "TERM" {
            continue;
        }
        assert!(v.is_null(), "{k} reached the version probe: {v}");
    }
}

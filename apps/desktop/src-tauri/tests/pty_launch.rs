//! End-to-end launch tests: a real `LaunchSpec` from `nexus-core`, spawned through the real PTY manager,
//! executing the `fake-claude` test double that reports exactly what it received.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use nexus_core::claude::launch::{self, LaunchRequest, Mode};
use nexus_core::claude::profile::ClaudeProfile;
use nexus_core::workprofile;
use nexus_desktop_lib::pty::{PtyManager, SpawnRequest};

const SID: &str = "0f8b1c2e-6a4d-4e2a-9d4b-2f1c3b5a7e90";

fn fake_claude() -> PathBuf {
    let exe = if cfg!(windows) { "fake-claude.exe" } else { "fake-claude" };
    // Test binary lives in target/<profile>/deps/; the helper binary in target/<profile>/.
    let dir = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_path_buf();
    let path = dir.join(exe);
    if !path.is_file() {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status = std::process::Command::new(cargo)
            .args(["build", "-q", "-p", "fake-claude"])
            .status()
            .expect("build fake-claude");
        assert!(status.success());
    }
    assert!(path.is_file(), "fake-claude not found at {}", path.display());
    path
}

fn profile(config_dir: Option<&Path>) -> ClaudeProfile {
    ClaudeProfile {
        id: "p".into(),
        name: "Test".into(),
        description: String::new(),
        color: "#7c6cf2".into(),
        config_dir: config_dir.map(Path::to_path_buf),
        executable: None,
        extra_args: vec![],
        managed_by_nexus: true,
        inherit_provider_env: false,
        default_work_profile: None,
        default_effort: None,
        default_model: None,
        created_at: 0,
        updated_at: 0,
        last_used_at: None,
        archived_at: None,
    }
}

/// The NEXUS process environment, polluted the way it is when NEXUS is started from inside a work
/// Claude session.
fn hostile_base_env(report: &Path, stay: bool) -> Vec<(String, String)> {
    let mut env = nexus_core::env::current();
    env.retain(|(k, _)| !k.to_ascii_uppercase().starts_with("CLAUDE") && !k.to_ascii_uppercase().starts_with("ANTHROPIC"));
    for (k, v) in [
        ("CLAUDECODE", "1"),
        ("CLAUDE_CODE_SESSION_ID", "parent-session"),
        ("CLAUDE_CODE_CHILD_SESSION", "1"),
        ("CLAUDE_CODE_MESSAGING_TOKEN", "parent-secret"),
        ("CLAUDE_CONFIG_DIR", "/parent/config"),
        ("ANTHROPIC_API_KEY", "sk-ambient"),
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://corp.example.invalid"),
    ] {
        env.push((k.into(), v.into()));
    }
    env.push(("FAKE_CLAUDE_REPORT".into(), report.display().to_string()));
    if stay {
        env.push(("FAKE_CLAUDE_STAY".into(), "1".into()));
    }
    env
}

fn launch(
    mgr: &PtyManager,
    id: &str,
    p: &ClaudeProfile,
    cwd: &Path,
    base: &[(String, String)],
) -> mpsc::Receiver<Option<i64>> {
    let exe = fake_claude();
    let spec = launch::build(
        &LaunchRequest {
            profile: p,
            executable: &exe,
            cwd,
            mode: Mode::New,
            claude_session_id: SID,
            title: Some("PTY test"),
            work_profile: workprofile::get("prototype"),
            effort: Some("high"),
            model: None,
            nexus_session_id: id,
            workspace_id: Some("ws-1"),
            project_id: Some("pr-1"),
            mcp_config: None,
            settings: None,
            context_prompt: None,
        },
        base,
    )
    .unwrap();
    let (tx, rx) = mpsc::channel();
    mgr.spawn(
        id,
        SpawnRequest {
            program: spec.program,
            args: spec.args,
            cwd: spec.cwd,
            env: spec.env,
            cols: 200,
            rows: 40,
        },
        move |code| {
            let _ = tx.send(code);
        },
    )
    .expect("spawn");
    rx
}

fn read_report(path: &Path) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(raw) = std::fs::read_to_string(path)
            && let Ok(v) = serde_json::from_str(&raw)
        {
            return v;
        }
        assert!(Instant::now() < deadline, "fake-claude never wrote its report");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn explicit_profile_gets_its_config_dir_and_a_clean_environment() {
    let t = tempfile::tempdir().unwrap();
    let cwd = t.path().join("project");
    let cfg = t.path().join(".claude-personal");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&cfg).unwrap();
    let report = t.path().join("report.json");

    let mgr = PtyManager::default();
    let rx = launch(&mgr, "nx-explicit", &profile(Some(&cfg)), &cwd, &hostile_base_env(&report, false));
    let code = rx.recv_timeout(Duration::from_secs(30)).expect("process exits");
    assert_eq!(code, Some(0));

    let r = read_report(&report);
    let env = &r["env"];
    assert_eq!(env["CLAUDE_CONFIG_DIR"], cfg.display().to_string());
    assert_eq!(env["NEXUS_SESSION_ID"], "nx-explicit");
    assert_eq!(env["NEXUS_PROJECT_ID"], "pr-1");
    assert_eq!(env["TERM"], "xterm-256color");
    for leaked in [
        "CLAUDECODE",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_MESSAGING_TOKEN",
        "ANTHROPIC_API_KEY",
        "OTEL_EXPORTER_OTLP_ENDPOINT",
    ] {
        assert!(env[leaked].is_null(), "{leaked} leaked into the Claude process: {}", env[leaked]);
    }
    let args: Vec<String> = serde_json::from_value(r["args"].clone()).unwrap();
    let pos = args.iter().position(|a| a == "--session-id").expect("--session-id passed");
    assert_eq!(args[pos + 1], SID);
    assert!(args.iter().any(|a| a.starts_with("NEXUS work profile: PROTOTYPE")));
    assert!(nexus_core::paths::same_path(
        &dunce_canon(Path::new(r["cwd"].as_str().unwrap())),
        &dunce_canon(&cwd)
    ));

    // The PTY carried the program's output.
    let out = String::from_utf8_lossy(&mgr.snapshot("nx-explicit")).into_owned();
    assert!(out.contains("FAKE-CLAUDE-READY"), "terminal output missing: {out:?}");
    assert!(!mgr.is_running("nx-explicit"));
}

#[test]
fn default_location_profile_runs_without_config_dir() {
    let t = tempfile::tempdir().unwrap();
    let report = t.path().join("report.json");
    let mgr = PtyManager::default();
    let rx = launch(&mgr, "nx-default", &profile(None), t.path(), &hostile_base_env(&report, false));
    assert_eq!(rx.recv_timeout(Duration::from_secs(30)).unwrap(), Some(0));
    let r = read_report(&report);
    assert!(r["env"]["CLAUDE_CONFIG_DIR"].is_null(), "inherited CLAUDE_CONFIG_DIR must be removed");
}

#[test]
fn input_reaches_the_process_and_exit_codes_are_reported() {
    let t = tempfile::tempdir().unwrap();
    let report = t.path().join("report.json");
    let mut base = hostile_base_env(&report, true);
    base.push(("FAKE_CLAUDE_EXIT".into(), "3".into()));
    let mgr = PtyManager::default();
    let rx = launch(&mgr, "nx-input", &profile(None), t.path(), &base);
    read_report(&report); // started and waiting for input
    assert!(mgr.is_running("nx-input"));
    mgr.write("nx-input", b"hello nexus\r").unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(30)).unwrap(), Some(3));
    let out = String::from_utf8_lossy(&mgr.snapshot("nx-input")).into_owned();
    assert!(out.contains("received: hello nexus"), "echo missing: {out:?}");
    assert!(mgr.write("nx-input", b"x").is_err(), "writing to an exited process fails cleanly");
    assert!(mgr.release("nx-input"));
    assert!(!mgr.is_attached("nx-input"));
}

#[test]
fn terminate_stops_a_running_process() {
    let t = tempfile::tempdir().unwrap();
    let report = t.path().join("report.json");
    let mgr = PtyManager::default();
    let rx = launch(&mgr, "nx-kill", &profile(None), t.path(), &hostile_base_env(&report, true));
    read_report(&report);
    assert!(!mgr.release("nx-kill"), "running terminals are not released");
    mgr.terminate("nx-kill").unwrap();
    let code = rx.recv_timeout(Duration::from_secs(30)).expect("terminated process reports exit");
    assert_ne!(code, Some(0));
    assert!(!mgr.is_running("nx-kill"));
}

fn dunce_canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p)
        .map(|c| PathBuf::from(c.to_string_lossy().trim_start_matches(r"\\?\")))
        .unwrap_or_else(|_| p.to_path_buf())
}

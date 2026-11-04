//! End-to-end tests of the real `nexus` binary: isolated data dir, synthetic workspace/repo/profile, and the
//! `fake-claude` test double — never a real Claude profile.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

const NEXUS: &str = env!("CARGO_BIN_EXE_nexus");

fn fake_claude() -> PathBuf {
    let exe = if cfg!(windows) { "fake-claude.exe" } else { "fake-claude" };
    let path = Path::new(NEXUS).parent().unwrap().join(exe);
    if !path.is_file() {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        assert!(Command::new(cargo).args(["build", "-q", "-p", "fake-claude"]).status().unwrap().success());
    }
    path
}

struct Env {
    _tmp: tempfile::TempDir,
    data: PathBuf,
    repo: PathBuf,
    cfg: PathBuf,
}

impl Env {
    fn new() -> Env {
        let tmp = tempfile::tempdir().unwrap();
        let root = dunce(tmp.path());
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        let git = Command::new("git").args(["init", "-q", "-b", "main"]).current_dir(&repo).status().unwrap();
        assert!(git.success());
        Env {
            data: root.join("data"),
            cfg: root.join("cfg"),
            repo,
            _tmp: tmp,
        }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(NEXUS);
        c.arg("--data-dir").arg(&self.data).args(args);
        // A parent environment as polluted as inside an organisation-managed Claude session.
        for (k, v) in [
            ("CLAUDECODE", "1"),
            ("CLAUDE_CODE_SESSION_ID", "parent"),
            ("CLAUDE_CODE_CHILD_SESSION", "1"),
            ("CLAUDE_CODE_MESSAGING_TOKEN", "parent-secret"),
            ("CLAUDE_CONFIG_DIR", "/parent/config"),
            ("ANTHROPIC_API_KEY", "sk-ambient"),
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://corp.example.invalid"),
        ] {
            c.env(k, v);
        }
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(out.status.success(), "nexus {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn json(&self, args: &[&str]) -> Value {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        serde_json::from_str(&self.ok(&all)).unwrap()
    }

    /// Workspace with the repo registered and a fake-claude profile.
    fn setup(&self) {
        self.ok(&["workspaces", "create", self.data.parent().unwrap().join("ws").to_str().unwrap(), "--name", "Test"]);
        let added = self.ok(&["projects", "add", self.repo.to_str().unwrap()]);
        assert!(added.contains("added repo [rust]"), "{added}");
        self.ok(&[
            "profiles", "add", "--name", "Fake", "--create",
            "--config-dir", self.cfg.to_str().unwrap(),
            "--executable", fake_claude().to_str().unwrap(),
            "--work-profile", "prototype",
        ]);
    }
}

fn dunce(p: &Path) -> PathBuf {
    let c = std::fs::canonicalize(p).unwrap();
    PathBuf::from(c.to_string_lossy().trim_start_matches(r"\\?\"))
}

fn read_report(path: &Path) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(raw) = std::fs::read_to_string(path)
            && let Ok(v) = serde_json::from_str(&raw)
        {
            return v;
        }
        assert!(Instant::now() < deadline, "fake-claude never ran");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn launch_is_isolated_recorded_and_propagates_the_exit_code() {
    let e = Env::new();
    e.setup();
    let report = e.data.parent().unwrap().join("report.json");
    let out = e
        .cmd(&["launch", "--profile", "fake", "--project", "repo", "--name", "First try", "-q"])
        .env("FAKE_CLAUDE_REPORT", &report)
        .env("FAKE_CLAUDE_EXIT", "3")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3), "exit code of Claude is the exit code of nexus launch");

    let r = read_report(&report);
    let env = &r["env"];
    assert_eq!(env["CLAUDE_CONFIG_DIR"], e.cfg.display().to_string());
    for k in ["CLAUDECODE", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_MESSAGING_TOKEN", "ANTHROPIC_API_KEY", "OTEL_EXPORTER_OTLP_ENDPOINT"] {
        assert!(env[k].is_null(), "{k} leaked: {}", env[k]);
    }
    assert!(env["NEXUS_SESSION_ID"].is_string() && env["NEXUS_PROJECT_ID"].is_string());
    let args: Vec<String> = serde_json::from_value(r["args"].clone()).unwrap();
    assert_eq!(args[0], "--session-id");
    assert!(args.windows(2).any(|w| w == ["--name", "First try"]));
    assert!(args.iter().any(|a| a.starts_with("NEXUS work profile: PROTOTYPE")), "profile default rigor applied");
    assert!(nexus_core::paths::same_path(Path::new(r["cwd"].as_str().unwrap()), &e.repo));

    let sessions = e.json(&["sessions"]);
    let s = &sessions[0];
    assert_eq!(s["host"], "cli");
    assert_eq!(s["state"], "exited");
    assert_eq!(s["exitCode"], 3);
    assert_eq!(s["claudeSessionId"], args[1]);
    assert_eq!(s["title"], "First try");
}

#[test]
fn resume_last_continues_the_recorded_conversation() {
    let e = Env::new();
    e.setup();
    let out = e.cmd(&["launch", "--project", "repo", "-q"]).output().unwrap(); // the only profile is chosen
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let first = e.json(&["sessions"])[0].clone();
    let csid = first["claudeSessionId"].as_str().unwrap().to_string();

    // Without a transcript there is nothing to resume.
    let none = e.run(&["launch", "--resume", "last", "--cwd", e.repo.to_str().unwrap(), "-q"]);
    assert_eq!(none.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&none.stderr).contains("no resumable Claude session"));

    let dir = e.cfg.join("projects").join("demo");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{csid}.jsonl")), "{}\n").unwrap();
    let report = e.data.parent().unwrap().join("resume.json");
    let out = e
        .cmd(&["launch", "--resume", "last", "--cwd", e.repo.to_str().unwrap(), "-q"])
        .env("FAKE_CLAUDE_REPORT", &report)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let args: Vec<String> = serde_json::from_value(read_report(&report)["args"].clone()).unwrap();
    assert!(args.windows(2).any(|w| w == ["--resume", csid.as_str()]), "{args:?}");
    assert!(!args.contains(&"--session-id".to_string()));
    let latest = e.json(&["sessions"])[0].clone();
    assert_eq!(latest["resumedFrom"], first["id"]);
}

#[test]
fn a_killed_cli_host_is_reconciled_to_interrupted() {
    let e = Env::new();
    e.setup();
    let report = e.data.parent().unwrap().join("stay.json");
    let mut child = e
        .cmd(&["launch", "--project", "repo", "-q"])
        .env("FAKE_CLAUDE_REPORT", &report)
        .env("FAKE_CLAUDE_STAY", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    read_report(&report); // Claude (fake) is running and waiting for input
    assert_eq!(e.json(&["sessions"])[0]["state"], "running");

    child.kill().unwrap(); // the terminal running `nexus launch` is gone
    let _ = child.wait();
    // Simulate the lease expiring (60 s) instead of waiting for it.
    {
        let conn = nexus_core::db::open(&e.data.join(nexus_core::db::DB_FILE)).unwrap();
        conn.execute("UPDATE session_hosts SET heartbeat_at = 0 WHERE kind = 'cli'", []).unwrap();
    }
    let s = e.json(&["sessions"])[0].clone();
    assert_eq!(s["state"], "interrupted");
    // Release the orphaned test double (it inherited our stdin pipe).
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"bye\n");
    }
}

#[test]
fn errors_are_reported_with_exit_code_1() {
    let e = Env::new();
    e.setup();
    let out = e.run(&["launch", "--profile", "nope"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no profile matches 'nope'"));
    let out = e.run(&["profiles", "add", "--name", "Bad", "--config-dir", e.cfg.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "duplicate config dir is rejected");
    assert!(String::from_utf8_lossy(&out.stderr).contains("already uses this configuration directory"));
}

#[test]
fn lists_and_doctor_work_without_the_desktop() {
    let e = Env::new();
    e.setup();
    let ws = e.json(&["workspaces", "list"]);
    assert_eq!(ws[0]["name"], "Test");
    let projects = e.json(&["projects", "list", "--git"]);
    assert_eq!(projects[0]["name"], "repo");
    assert_eq!(projects[0]["git"]["branch"], "main");
    let profiles = e.json(&["profiles", "list"]);
    assert_eq!(profiles[0]["managedByNexus"], true);
    let status = e.json(&["profiles", "status", "Fake"]);
    assert_eq!(status["email"], e.cfg.display().to_string(), "auth status ran under the profile's directory");
    assert_eq!(status["orgName"], "leaked:", "and saw none of the parent's variables");

    let doctor = e.run(&["--json", "doctor"]);
    let checks: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let summaries: Vec<&str> = checks.as_array().unwrap().iter().map(|c| c["summary"].as_str().unwrap()).collect();
    assert!(summaries.iter().any(|s| s.starts_with("database healthy")), "{summaries:?}");
    assert!(summaries.iter().any(|s| s.contains("profile “Fake”: ready")), "{summaries:?}");
}

fn launch_report(e: &Env, extra: &[&str], report_name: &str) -> (Value, Value) {
    let report = e.data.parent().unwrap().join(report_name);
    let mut args = vec!["launch", "--project", "repo", "-q"];
    args.extend_from_slice(extra);
    let out = e.cmd(&args).env("FAKE_CLAUDE_REPORT", &report).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    (read_report(&report), e.json(&["sessions"])[0].clone())
}

#[test]
fn launch_binds_a_session_scoped_mcp_server() {
    let e = Env::new();
    e.setup();
    let (report, session) = launch_report(&e, &[], "mcp.json");
    let args: Vec<String> = serde_json::from_value(report["args"].clone()).unwrap();
    let cfg_path = args.windows(2).find(|w| w[0] == "--mcp-config").map(|w| PathBuf::from(&w[1])).expect("--mcp-config passed");
    let cfg: Value = serde_json::from_slice(&std::fs::read(&cfg_path).unwrap()).unwrap();
    let server = &cfg["mcpServers"]["nexus"];
    assert!(nexus_core::paths::same_path(Path::new(server["command"].as_str().unwrap()), Path::new(NEXUS)));
    let server_args: Vec<&str> = server["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    assert_eq!(server_args[2..], ["mcp", "--session", session["id"].as_str().unwrap()], "scope = this session");
    assert_eq!(session["integrations"]["mcp"]["state"], "enabled");
}

#[test]
fn mcp_server_speaks_json_rpc_over_stdio() {
    let e = Env::new();
    e.setup();
    let (_, session) = launch_report(&e, &["--name", "Boss fight"], "for-mcp.json");
    let sid = session["id"].as_str().unwrap();
    let mut child = Command::new(NEXUS)
        .args(["--data-dir", e.data.to_str().unwrap(), "mcp", "--session", sid])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        for msg in [
            json(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#),
            json(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            json(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#),
            json(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"checkpoint","arguments":{"kind":"done","summary":"Boss fight playable"}}}"#),
            json(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_session_context","arguments":{}}}"#),
        ] {
            writeln!(stdin, "{msg}").unwrap();
        }
    } // closing stdin ends the server
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let replies: Vec<Value> = String::from_utf8_lossy(&out.stdout).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(replies.len(), 4, "the notification gets no reply: {replies:#?}");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "nexus");
    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), 7);
    assert_eq!(replies[2]["result"]["isError"], false);
    let ctx = &replies[3]["result"]["structuredContent"];
    assert_eq!(ctx["session"]["title"], "Boss fight");
    assert_eq!(ctx["project"]["name"], "repo");
    assert_eq!(ctx["recentCheckpoints"][0]["summary"], "Boss fight playable");
}

fn json(s: &str) -> Value {
    serde_json::from_str(s).unwrap()
}

#[test]
fn organisation_policy_blocking_sideload_flags_omits_mcp_and_says_why() {
    let e = Env::new();
    e.setup();
    std::fs::write(e.cfg.join("remote-settings.json"), r#"{"disableSideloadFlags": true}"#).unwrap();
    let (report, session) = launch_report(&e, &[], "blocked.json");
    let args: Vec<String> = serde_json::from_value(report["args"].clone()).unwrap();
    assert!(!args.contains(&"--mcp-config".to_string()), "Claude would refuse to start with a sideload flag");
    assert_eq!(session["integrations"]["mcp"]["state"], "blocked");
    assert!(session["integrations"]["mcp"]["reason"].as_str().unwrap().contains("disableSideloadFlags"));
}

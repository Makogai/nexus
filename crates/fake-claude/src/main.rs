//! `fake-claude` — a deterministic stand-in for the `claude` executable used by launch and isolation tests.
//!
//! Every invocation first builds a report `{"fakeClaude":true,"args":[…],"cwd":"…","env":{…}}` where `env` contains
//! the variables relevant to profile isolation (present → value, absent → null). With `FAKE_CLAUDE_REPORT=<path>`
//! the report is written to that file (PTY output is re-rendered by ConPTY, a file is exact).
//!
//! * `--version` prints `9.9.9 (Fake Claude)` like the real CLI.
//! * `auth status --json` prints a logged-in status whose `email` reflects `CLAUDE_CONFIG_DIR` and whose
//!   `orgName` is `leaked:<names>` — every isolation-relevant variable that should not have reached Claude.
//! * Otherwise (a session) prints `FAKE-CLAUDE-READY` and the report; `FAKE_CLAUDE_STAY=1` then waits for one line of
//!   input and echoes it; `FAKE_CLAUDE_EXIT=<n>` sets the exit code.

use std::io::{BufRead, Write};

const WATCHED: &[&str] = &[
    "CLAUDE_CONFIG_DIR",
    "NEXUS_SESSION_ID",
    "NEXUS_WORKSPACE_ID",
    "NEXUS_PROJECT_ID",
    "CLAUDECODE",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_ENABLE_TELEMETRY",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "OTEL_EXPORTER_OTLP_ENDPOINT",
    "TERM",
];

/// Variables that legitimately reach Claude (set by NEXUS) and are therefore not "leaks".
fn expected(name: &str) -> bool {
    matches!(name, "CLAUDE_CONFIG_DIR" | "TERM") || name.starts_with("NEXUS_")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut env = serde_json::Map::new();
    for k in WATCHED {
        env.insert((*k).to_string(), std::env::var(k).map(serde_json::Value::String).unwrap_or(serde_json::Value::Null));
    }
    let report = serde_json::json!({
        "fakeClaude": true,
        "args": args,
        "cwd": std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(),
        "env": env,
    });
    if let Ok(path) = std::env::var("FAKE_CLAUDE_REPORT") {
        let _ = std::fs::write(path, report.to_string());
    }

    if args.first().map(String::as_str) == Some("--version") {
        println!("9.9.9 (Fake Claude)");
        return;
    }
    if args.iter().map(String::as_str).collect::<Vec<_>>() == ["auth", "status", "--json"] {
        let who = std::env::var("CLAUDE_CONFIG_DIR").unwrap_or_else(|_| "default".into());
        let leaked: Vec<&str> = WATCHED
            .iter()
            .copied()
            .filter(|k| !expected(k) && std::env::var_os(k).is_some())
            .collect();
        let status = serde_json::json!({
            "loggedIn": true, "authMethod": "fake", "subscriptionType": "test", "email": who,
            "orgName": format!("leaked:{}", leaked.join(",")),
        });
        println!("{status}");
        return;
    }

    println!("FAKE-CLAUDE-READY");
    println!("{report}");
    let _ = std::io::stdout().flush();
    if std::env::var("FAKE_CLAUDE_STAY").as_deref() == Ok("1") {
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
        println!("received: {}", line.trim());
    }
    let code = std::env::var("FAKE_CLAUDE_EXIT").ok().and_then(|c| c.parse().ok()).unwrap_or(0);
    std::process::exit(code);
}

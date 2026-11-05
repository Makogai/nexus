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
//! * `FAKE_CLAUDE_TRANSCRIPT=<fixture>` (sessions only, and only when `CLAUDE_CONFIG_DIR` is set — never the default
//!   `~/.claude`): appends the fixture's lines to `$CLAUDE_CONFIG_DIR/projects/<cwd as Claude encodes it>/<id>.jsonl`
//!   like the real CLI, for `--session-id <id>` and `--resume <id>`. Placeholders: `{SESSION}`, `{TS}` (now, ISO),
//!   `{TS+n}` (n seconds later), `{START}` (now, ms). `FAKE_CLAUDE_SUBAGENT=<fixture>` writes a subagent file too.

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
        // `FAKE_CLAUDE_AUTH_METHOD=api_key` / `claude.ai` lets tests choose how the profile is billed.
        let method = std::env::var("FAKE_CLAUDE_AUTH_METHOD").unwrap_or_else(|_| "fake".into());
        let subscription = if method == "claude.ai" { serde_json::json!("max") } else { serde_json::Value::Null };
        let status = serde_json::json!({
            "loggedIn": true, "authMethod": method, "subscriptionType": subscription, "email": who,
            "orgName": format!("leaked:{}", leaked.join(",")),
        });
        println!("{status}");
        return;
    }

    write_transcripts(&args);
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

/// Emulate Claude Code's transcript files (see the module docs).
fn write_transcripts(args: &[String]) {
    let Some(cfg) = std::env::var_os("CLAUDE_CONFIG_DIR").map(std::path::PathBuf::from) else { return };
    let id = args.windows(2).find(|w| w[0] == "--session-id" || w[0] == "--resume").map(|w| w[1].clone());
    let Some(id) = id.filter(|i| i.len() == 36 && i.chars().all(|c| c.is_ascii_hexdigit() || c == '-')) else { return };
    let cwd = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default();
    let encoded: String = cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let dir = cfg.join("projects").join(encoded);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    let fill = |fixture: &str| -> String {
        let mut out = fixture.replace("{SESSION}", &id).replace("{START}", &now.to_string()).replace("{TS}", &iso(now));
        while let Some(i) = out.find("{TS+") {
            let Some(j) = out[i..].find('}') else { break };
            let secs: i64 = out[i + 4..i + j].parse().unwrap_or(0);
            out.replace_range(i..i + j + 1, &iso(now + secs * 1000));
        }
        out
    };
    let append = |path: std::path::PathBuf, text: String| {
        use std::io::Write as _;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(text.as_bytes());
        }
    };
    if let Some(fixture) = std::env::var_os("FAKE_CLAUDE_TRANSCRIPT").and_then(|p| std::fs::read_to_string(p).ok()) {
        append(dir.join(format!("{id}.jsonl")), fill(&fixture));
    }
    if let Some(fixture) = std::env::var_os("FAKE_CLAUDE_SUBAGENT").and_then(|p| std::fs::read_to_string(p).ok()) {
        append(dir.join(&id).join("subagents").join("agent-fake.jsonl"), fill(&fixture));
    }
}

/// Milliseconds since the epoch → `YYYY-MM-DDTHH:MM:SS.mmmZ` (civil from days).
fn iso(ms: i64) -> String {
    let (secs, millis) = (ms.div_euclid(1000), ms.rem_euclid(1000));
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

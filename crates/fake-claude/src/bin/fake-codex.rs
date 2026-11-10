//! `fake-codex` — a deterministic stand-in for the `codex` executable (ADR-0052 tests). It reads NEXUS's `-c` overrides
//! the way Codex does (TOML values), so a test proves the binding end to end:
//!
//! * `--version` prints `codex-cli 9.9.9`.
//! * A session writes the report `{"fakeCodex":true,"args":[…],"cwd":"…","env":{…},"overrides":{…}}` to
//!   `FAKE_CODEX_REPORT` (if set) and prints `FAKE-CODEX-READY`. `overrides` holds the parsed `-c` values.
//! * `FAKE_CODEX_MCP=1`: starts the `mcp_servers.nexus` server from the overrides, asks it for `initialize` and
//!   `tools/list`, and records `mcpTools` (the tool names) in the report — or `mcpError`.
//! * `FAKE_CODEX_TURN=1`: runs the `notify` program from the overrides once, like Codex after a turn, with the payload as
//!   the last argument (`thread-id` = the resumed id, else `FAKE_CODEX_THREAD` or a fixed UUID; the payload carries
//!   prompt and answer text that NEXUS must not store).
//! * `FAKE_CODEX_STAY=1` waits for one line of input; `FAKE_CODEX_EXIT=<n>` sets the exit code.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

const WATCHED: &[&str] = &["CODEX_HOME", "OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_SANDBOX", "NEXUS_SESSION_ID", "NEXUS_WORKSPACE_ID", "NEXUS_PROJECT_ID", "CLAUDECODE", "TERM"];
const THREAD: &str = "0199a213-81c0-7800-8aa1-bbab2a035a53";

/// A TOML value as NEXUS writes it: a basic string or an array of basic strings (anything else stays raw).
fn toml_value(v: &str) -> serde_json::Value {
    let mut chars = v.trim().chars().peekable();
    fn string(c: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
        if c.next()? != '"' {
            return None;
        }
        let mut s = String::new();
        loop {
            match c.next()? {
                '"' => return Some(s),
                '\\' => match c.next()? {
                    'n' => s.push('\n'),
                    'r' => s.push('\r'),
                    't' => s.push('\t'),
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    'u' => {
                        let hex: String = (0..4).filter_map(|_| c.next()).collect();
                        s.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    _ => return None,
                },
                ch => s.push(ch),
            }
        }
    }
    match chars.peek() {
        Some('"') => string(&mut chars).map(serde_json::Value::String).unwrap_or_else(|| serde_json::json!({ "raw": v })),
        Some('[') => {
            chars.next();
            let mut items = Vec::new();
            loop {
                while chars.peek().is_some_and(|c| c.is_whitespace() || *c == ',') {
                    chars.next();
                }
                if chars.peek() == Some(&']') {
                    break serde_json::Value::Array(items);
                }
                match string(&mut chars) {
                    Some(s) => items.push(serde_json::Value::String(s)),
                    None => break serde_json::json!({ "raw": v }),
                }
            }
        }
        _ => serde_json::json!({ "raw": v }),
    }
}

fn overrides(args: &[String]) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    for w in args.windows(2).filter(|w| w[0] == "-c") {
        if let Some((k, v)) = w[1].split_once('=') {
            m.insert(k.to_string(), toml_value(v));
        }
    }
    m
}

fn strings(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// initialize + tools/list against the overrides' NEXUS MCP server.
fn ask_mcp(o: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<String>, String> {
    let cmd = o.get("mcp_servers.nexus.command").and_then(|v| v.as_str()).ok_or("no mcp_servers.nexus.command")?;
    let args = strings(o.get("mcp_servers.nexus.args"));
    let mut child = Command::new(cmd).args(&args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    for msg in [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fake-codex","version":"9.9.9"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    ] {
        writeln!(stdin, "{msg}").map_err(|e| e.to_string())?;
    }
    drop(stdin);
    let out = BufReader::new(child.stdout.take().ok_or("no stdout")?);
    let mut tools = Vec::new();
    for line in out.lines().map_while(Result::ok) {
        let v: serde_json::Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if v["id"] == 2 {
            tools = v["result"]["tools"].as_array().map(|a| a.iter().filter_map(|t| t["name"].as_str().map(str::to_string)).collect()).unwrap_or_default();
        }
    }
    let _ = child.wait();
    Ok(tools)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        println!("codex-cli 9.9.9");
        return;
    }
    let o = overrides(&args);
    let mut env = serde_json::Map::new();
    for k in WATCHED {
        env.insert((*k).to_string(), std::env::var(k).map(serde_json::Value::String).unwrap_or(serde_json::Value::Null));
    }
    let mut report = serde_json::json!({
        "fakeCodex": true,
        "args": args,
        "cwd": std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(),
        "env": env,
        "overrides": o,
    });
    if std::env::var("FAKE_CODEX_MCP").as_deref() == Ok("1") {
        match ask_mcp(&o) {
            Ok(t) => report["mcpTools"] = serde_json::json!(t),
            Err(e) => report["mcpError"] = serde_json::json!(e),
        }
    }
    if std::env::var("FAKE_CODEX_TURN").as_deref() == Ok("1") {
        let argv = strings(o.get("notify"));
        let resumed = (args.first().map(String::as_str) == Some("resume")).then(|| args.last().cloned()).flatten();
        let thread = resumed.or_else(|| std::env::var("FAKE_CODEX_THREAD").ok()).unwrap_or_else(|| THREAD.into());
        let payload = serde_json::json!({
            "type": "agent-turn-complete", "thread-id": thread, "turn-id": "turn-1",
            "cwd": report["cwd"], "input-messages": ["FAKE PROMPT TEXT"], "last-assistant-message": "FAKE ANSWER TEXT",
        });
        report["notified"] = match argv.split_first() {
            Some((program, rest)) => serde_json::json!(Command::new(program).args(rest).arg(payload.to_string()).status().map(|s| s.success()).unwrap_or(false)),
            None => serde_json::json!(false),
        };
    }
    if let Ok(path) = std::env::var("FAKE_CODEX_REPORT") {
        let _ = std::fs::write(path, report.to_string());
    }
    println!("FAKE-CODEX-READY");
    let _ = std::io::stdout().flush();
    if std::env::var("FAKE_CODEX_STAY").as_deref() == Ok("1") {
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
        println!("received: {}", line.trim());
    }
    let code = std::env::var("FAKE_CODEX_EXIT").ok().and_then(|c| c.parse().ok()).unwrap_or(0);
    std::process::exit(code);
}

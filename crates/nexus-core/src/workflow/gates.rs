//! Quality gates (ADR-0029): the *trusted* way NEXUS verifies work. A gate is a command (program + argument vector,
//! never a shell string) the user defined for a project **on this machine**; workflow documents only name gates.
//! Gates run in a NEXUS host process (desktop runner or `nexus workflow verify`) — never inside the MCP server and
//! never on Claude's say-so alone — with the user's environment minus Claude session markers, a timeout, captured
//! (bounded, redacted) output and an exit code. That is the difference between *AI-reported* and *verified*.
//!
//! **Approval** (ADR-0029 amendment): a gate executes only when its current command — program and arguments — was
//! approved by the person. Defining or changing a command creates a new *version* that is unapproved until approved
//! again; [`begin`] (the start of every execution) refuses anything else. This is separate from approving a workflow
//! definition: a workflow says *which* gates verify a step, the gate approval says *what may run* on this machine.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::{new_id, now_ms};

pub const MAX_TIMEOUT_SECS: i64 = 3 * 3600;
const OUTPUT_TAIL: usize = 8 * 1024;
const MAX_CAPTURE: usize = 2 * 1024 * 1024;

pub fn valid_name(n: &str) -> bool {
    n.chars().next().is_some_and(|c| c.is_ascii_lowercase()) && n.len() <= 32 && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Gate {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub title: String,
    pub program: String,
    pub args: Vec<String>,
    pub timeout_secs: i64,
    pub updated_at: i64,
    /// Increments whenever the command (program or arguments) changes.
    pub version: i64,
    /// Hash of the current command; approval pins it.
    pub command_hash: String,
    /// The current command (this version) was approved by the person.
    pub approved: bool,
    pub approved_at: Option<i64>,
}

impl Gate {
    pub fn command_line(&self) -> String {
        std::iter::once(self.program.as_str()).chain(self.args.iter().map(String::as_str)).collect::<Vec<_>>().join(" ")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GateInput {
    pub name: String,
    pub title: String,
    pub program: String,
    pub args: Vec<String>,
    pub timeout_secs: Option<i64>,
}

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Gate> {
    let args: String = r.get(5)?;
    let program: String = r.get(4)?;
    let args: Vec<String> = serde_json::from_str(&args).unwrap_or_default();
    let stored: String = r.get(9)?;
    // Gates defined before migration 0013 have no stored hash: they are shown with their computed hash and stay
    // unapproved until the person approves them once (`approve` then stores the hash).
    let command_hash = if stored.is_empty() { self::command_hash(&program, &args) } else { stored };
    let approved_hash: Option<String> = r.get(10)?;
    Ok(Gate {
        id: r.get(0)?,
        project_id: r.get(1)?,
        name: r.get(2)?,
        title: r.get(3)?,
        program,
        args,
        timeout_secs: r.get(6)?,
        updated_at: r.get(7)?,
        version: r.get(8)?,
        approved: !command_hash.is_empty() && approved_hash.as_deref() == Some(command_hash.as_str()),
        approved_at: r.get(11)?,
        command_hash,
    })
}

const SELECT: &str = "SELECT id, project_id, name, title, program, args, timeout_secs, updated_at, version, command_hash, approved_hash, approved_at FROM quality_gates";

/// What approval pins: the program and every argument, in order (the timeout is not part of the command).
pub fn command_hash(program: &str, args: &[String]) -> String {
    let mut s = program.to_string();
    for a in args {
        s.push('\0');
        s.push_str(a);
    }
    crate::knowledge::content_hash(s.as_bytes())
}

pub fn list(conn: &Connection, project_id: &str) -> Result<Vec<Gate>> {
    Ok(conn.prepare(&format!("{SELECT} WHERE project_id = ?1 ORDER BY name"))?.query_map([project_id], from_row)?.collect::<std::result::Result<_, _>>()?)
}

pub fn find(conn: &Connection, project_id: &str, name: &str) -> Result<Option<Gate>> {
    Ok(conn.query_row(&format!("{SELECT} WHERE project_id = ?1 AND name = ?2"), params![project_id, name], from_row).optional()?)
}

/// Define (or replace) a gate. Only the person does this — the desktop and CLI, never MCP or a workflow file.
pub fn upsert(conn: &Connection, project_id: &str, g: &GateInput) -> Result<Gate> {
    crate::project::get(conn, project_id)?;
    if !valid_name(&g.name) {
        return Err(CoreError::invalid("gate names use lowercase letters, digits and `-` (e.g. test, lint)"));
    }
    let program = g.program.trim();
    if program.is_empty() || program.len() > 400 || program.contains(['\n', '\r', '\0']) {
        return Err(CoreError::invalid("the program is a file name or path"));
    }
    if g.args.len() > 64 || g.args.iter().any(|a| a.len() > 1000 || a.contains('\0')) {
        return Err(CoreError::invalid("at most 64 arguments of ≤ 1000 characters"));
    }
    let timeout = g.timeout_secs.unwrap_or(900).clamp(5, MAX_TIMEOUT_SECS);
    let title = if g.title.trim().is_empty() { g.name.clone() } else { g.title.trim().chars().take(120).collect() };
    let now = now_ms();
    // A changed command is a new version; the earlier approval no longer matches it.
    conn.execute(
        "INSERT INTO quality_gates(id, project_id, name, title, program, args, timeout_secs, created_at, updated_at, version, command_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1, ?9)
         ON CONFLICT(project_id, name) DO UPDATE SET title = excluded.title, program = excluded.program, args = excluded.args,
            timeout_secs = excluded.timeout_secs, updated_at = excluded.updated_at,
            version = CASE WHEN quality_gates.command_hash = excluded.command_hash THEN quality_gates.version ELSE quality_gates.version + 1 END,
            command_hash = excluded.command_hash",
        params![new_id(), project_id, g.name, title, program, serde_json::to_string(&g.args).unwrap_or_else(|_| "[]".into()), timeout, now, command_hash(program, &g.args)],
    )?;
    find(conn, project_id, &g.name)?.ok_or_else(|| CoreError::not_found("gate"))
}

/// The person authorises the gate's current command. `expected_hash` is the command they reviewed: if it changed in
/// the meantime the approval is refused (review the new version).
pub fn approve(conn: &Connection, project_id: &str, name: &str, expected_hash: &str) -> Result<Gate> {
    let g = find(conn, project_id, name)?.ok_or_else(|| CoreError::not_found(format!("no gate {name}")))?;
    if g.command_hash != expected_hash {
        return Err(CoreError::Conflict(format!("gate {name} changed since it was reviewed (now version {}): review it again", g.version)));
    }
    conn.execute(
        "UPDATE quality_gates SET command_hash = ?4, approved_hash = ?4, approved_at = ?3
         WHERE project_id = ?1 AND name = ?2 AND (command_hash = ?4 OR command_hash = '')",
        params![project_id, name, now_ms(), expected_hash],
    )?;
    find(conn, project_id, name)?.ok_or_else(|| CoreError::not_found("gate"))
}

/// Withdraw the approval (the gate stays defined but does not run).
pub fn revoke(conn: &Connection, project_id: &str, name: &str) -> Result<()> {
    conn.execute("UPDATE quality_gates SET approved_hash = NULL, approved_at = NULL WHERE project_id = ?1 AND name = ?2", params![project_id, name])?;
    Ok(())
}

pub fn remove(conn: &Connection, project_id: &str, name: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM quality_gates WHERE project_id = ?1 AND name = ?2", params![project_id, name])? > 0)
}

/// Suggestions from the project's detected scripts (never run or saved automatically).
pub fn suggest(p: &crate::project::Project) -> Vec<GateInput> {
    let mut out = Vec::new();
    for s in &p.stack.scripts {
        if !matches!(s.kind.as_str(), "test" | "build" | "lint" | "typecheck") || out.iter().any(|g: &GateInput| g.name == s.kind) {
            continue;
        }
        let mut words = s.command.split_whitespace().map(str::to_string);
        let Some(program) = words.next() else { continue };
        out.push(GateInput { name: s.kind.clone(), title: format!("{} ({})", s.kind, s.command), program, args: words.collect(), timeout_secs: Some(900) });
    }
    out
}

// ---- Execution ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Passed,
    Failed,
    Timeout,
    Error,
    Cancelled,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Passed => "passed",
            Status::Failed => "failed",
            Status::Timeout => "timeout",
            Status::Error => "error",
            Status::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub status: Status,
    pub exit_code: Option<i32>,
    pub duration_ms: i64,
    pub output_tail: String,
}

static CANCEL: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = LazyLock::new(Default::default);

/// Ask a running gate execution to stop (the process is killed within ~50 ms).
pub fn cancel(result_id: &str) -> bool {
    match CANCEL.lock().unwrap_or_else(|p| p.into_inner()).get(result_id) {
        Some(flag) => {
            flag.store(true, Ordering::SeqCst);
            true
        }
        None => false,
    }
}

fn read_capped(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        while let Ok(n) = r.read(&mut chunk) {
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() > MAX_CAPTURE {
                let cut = buf.len() - MAX_CAPTURE / 2;
                buf.drain(..cut);
            }
        }
        buf
    })
}

/// Tokens that look like credentials are replaced before output is stored or shown.
pub fn redact(s: &str) -> String {
    const MARKERS: &[&str] = &["sk-ant-", "ghp_", "github_pat_", "xoxb-", "xoxp-", "AKIA", "glpat-", "Bearer "];
    let mut out = s.to_string();
    for m in MARKERS {
        let mut from = 0;
        while let Some(i) = out[from..].find(m) {
            let start = from + i;
            let end = out[start + m.len()..].find(|c: char| c.is_whitespace() || c == '"' || c == '\'').map(|e| start + m.len() + e).unwrap_or(out.len());
            if end - start - m.len() >= 8 {
                out.replace_range(start..end, "[redacted]");
                from = start + "[redacted]".len();
            } else {
                from = start + m.len();
            }
        }
    }
    out
}

fn tail(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    let s = s.trim_end();
    let start = s.char_indices().rev().nth(OUTPUT_TAIL).map(|(i, _)| i).unwrap_or(0);
    redact(&s[start..])
}

fn resolve(program: &str, cwd: &Path) -> Option<PathBuf> {
    let p = Path::new(program);
    if p.components().count() > 1 || p.is_absolute() {
        let full = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
        return full.is_file().then_some(full);
    }
    which::which(program).ok()
}

/// Run one gate in `cwd` (the project folder). `result_id` registers it for [`cancel`]. No database access.
pub fn execute(gate: &Gate, cwd: &Path, base_env: &[(String, String)], result_id: &str) -> Outcome {
    let started = Instant::now();
    let done = |status, exit_code, out: &[u8]| Outcome { status, exit_code, duration_ms: started.elapsed().as_millis() as i64, output_tail: tail(out) };
    let Some(program) = resolve(&gate.program, cwd) else {
        return done(Status::Error, None, format!("program `{}` was not found", gate.program).as_bytes());
    };
    let flag = Arc::new(AtomicBool::new(false));
    CANCEL.lock().unwrap_or_else(|p| p.into_inner()).insert(result_id.to_string(), flag.clone());
    let _guard = scopeguard(result_id.to_string());
    let mut cmd = crate::proc::command(&program);
    cmd.args(&gate.args).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let (env, _) = crate::env::compose(base_env, &[("NEXUS_GATE".into(), gate.name.clone(), crate::env::Reason::NexusScope)], crate::env::Policy::SHELL);
    crate::env::apply(&mut cmd, &env);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return done(Status::Error, None, format!("could not start {}: {e}", program.display()).as_bytes()),
    };
    let out_t = read_capped(child.stdout.take().expect("piped"));
    let err_t = read_capped(child.stderr.take().expect("piped"));
    let limit = Duration::from_secs(gate.timeout_secs.clamp(1, MAX_TIMEOUT_SECS) as u64);
    let (status, code) = loop {
        match child.try_wait() {
            Ok(Some(s)) => break (if s.success() { Status::Passed } else { Status::Failed }, s.code()),
            Ok(None) if flag.load(Ordering::SeqCst) => {
                let _ = child.kill();
                let _ = child.wait();
                break (Status::Cancelled, None);
            }
            Ok(None) if started.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                break (Status::Timeout, None);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(_) => break (Status::Error, None),
        }
    };
    let mut out = out_t.join().unwrap_or_default();
    let err = err_t.join().unwrap_or_default();
    if !err.is_empty() {
        if !out.is_empty() {
            out.push(b'\n');
        }
        out.extend_from_slice(&err);
    }
    if status == Status::Timeout {
        out.extend_from_slice(format!("\n[NEXUS: stopped after {} s]", limit.as_secs()).as_bytes());
    }
    done(status, code, &out)
}

struct Unregister(String);
impl Drop for Unregister {
    fn drop(&mut self) {
        CANCEL.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.0);
    }
}
fn scopeguard(id: String) -> Unregister {
    Unregister(id)
}

// ---- Results -----------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GateResult {
    pub id: String,
    pub project_id: Option<String>,
    pub run_id: Option<String>,
    pub step_id: Option<String>,
    pub gate_name: String,
    pub command: String,
    pub status: String,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub output_tail: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    /// The gate version (approved command) that produced this result.
    pub gate_version: Option<i64>,
}

const RESULT: &str = "SELECT id, project_id, run_id, step_id, gate_name, command, status, exit_code, duration_ms, output_tail, started_at, ended_at, gate_version FROM gate_results";

fn result_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<GateResult> {
    Ok(GateResult {
        id: r.get(0)?,
        project_id: r.get(1)?,
        run_id: r.get(2)?,
        step_id: r.get(3)?,
        gate_name: r.get(4)?,
        command: r.get(5)?,
        status: r.get(6)?,
        exit_code: r.get(7)?,
        duration_ms: r.get(8)?,
        output_tail: r.get(9)?,
        started_at: r.get(10)?,
        ended_at: r.get(11)?,
        gate_version: r.get(12)?,
    })
}

/// Record the start of an execution (owned by `host_id`, so a crash can be reconciled). **The authorisation check:**
/// refused unless the gate's current command is the approved one — every execution path starts here.
pub fn begin(conn: &Connection, project_id: &str, run: Option<(&str, &str)>, gate: &Gate, host_id: &str) -> Result<String> {
    let current = find(conn, project_id, &gate.name)?.ok_or_else(|| CoreError::not_found(format!("no gate {}", gate.name)))?;
    if !current.approved || current.command_hash != gate.command_hash || command_hash(&gate.program, &gate.args) != current.command_hash {
        return Err(CoreError::invalid(format!(
            "gate {} (version {}) is not approved — review its command and approve it before it runs",
            gate.name, current.version
        )));
    }
    let id = new_id();
    conn.execute(
        "INSERT INTO gate_results(id, project_id, run_id, step_id, gate_name, command, status, host_id, started_at, gate_version, command_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running', ?7, ?8, ?9, ?10)",
        params![id, project_id, run.map(|r| r.0), run.map(|r| r.1), gate.name, gate.command_line(), host_id, now_ms(), current.version, current.command_hash],
    )?;
    Ok(id)
}

pub fn finish(conn: &Connection, id: &str, o: &Outcome) -> Result<GateResult> {
    conn.execute(
        "UPDATE gate_results SET status = ?2, exit_code = ?3, duration_ms = ?4, output_tail = ?5, ended_at = ?6 WHERE id = ?1 AND status = 'running'",
        params![id, o.status.as_str(), o.exit_code, o.duration_ms, o.output_tail, now_ms()],
    )?;
    get_result(conn, id)
}

pub fn get_result(conn: &Connection, id: &str) -> Result<GateResult> {
    conn.query_row(&format!("{RESULT} WHERE id = ?1"), [id], result_row).optional()?.ok_or_else(|| CoreError::not_found("gate result"))
}

pub fn results_for_step(conn: &Connection, run_id: &str, step_id: &str) -> Result<Vec<GateResult>> {
    Ok(conn.prepare(&format!("{RESULT} WHERE run_id = ?1 AND step_id = ?2 ORDER BY started_at DESC, rowid DESC LIMIT 30"))?.query_map(params![run_id, step_id], result_row)?.collect::<std::result::Result<_, _>>()?)
}

/// The latest finished result of a gate for a project (constitution compliance, ADR-0027).
pub fn latest(conn: &Connection, project_id: &str, name: &str) -> Result<Option<GateResult>> {
    Ok(conn
        .query_row(&format!("{RESULT} WHERE project_id = ?1 AND gate_name = ?2 AND status != 'running' ORDER BY started_at DESC, rowid DESC LIMIT 1"), params![project_id, name], result_row)
        .optional()?)
}

pub fn running(conn: &Connection) -> Result<Vec<GateResult>> {
    Ok(conn.prepare(&format!("{RESULT} WHERE status = 'running'"))?.query_map([], result_row)?.collect::<std::result::Result<_, _>>()?)
}

/// Executions whose host died (crash, killed terminal) are `interrupted`; their steps are verified again.
pub fn reconcile(conn: &Connection, now: i64) -> Result<usize> {
    let stale: Vec<(String, Option<String>)> = conn
        .prepare("SELECT id, host_id FROM gate_results WHERE status = 'running'")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut n = 0;
    for (id, host) in stale {
        let alive = match host.as_deref() {
            Some(h) => crate::host::is_alive(conn, h, now)?,
            None => false,
        };
        if !alive {
            n += conn.execute("UPDATE gate_results SET status = 'interrupted', ended_at = ?2 WHERE id = ?1 AND status = 'running'", params![id, now])?;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Connection, tempfile::TempDir, crate::project::Project) {
        let conn = crate::db::open_in_memory().unwrap();
        let t = tempfile::tempdir().unwrap();
        let ws = crate::workspace::create(&conn, t.path().join("ws").to_str().unwrap(), "W", "").unwrap();
        let repo = t.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git").args(["init", "-q"]).current_dir(&repo).status().unwrap();
        let p = crate::project::register(&conn, &ws.id, repo.to_str().unwrap(), None).unwrap();
        (conn, t, p)
    }

    #[test]
    fn gates_are_validated_executed_with_exit_codes_timeouts_and_redaction() {
        let (conn, _t, p) = fixture();
        assert!(upsert(&conn, &p.id, &GateInput { name: "Bad Name".into(), program: "git".into(), ..Default::default() }).is_err());
        let ok = upsert(&conn, &p.id, &GateInput { name: "version".into(), program: "git".into(), args: vec!["--version".into()], ..Default::default() }).unwrap();
        let fail = upsert(&conn, &p.id, &GateInput { name: "fail".into(), program: "git".into(), args: vec!["rev-parse".into(), "--verify".into(), "no-such-ref".into()], ..Default::default() }).unwrap();
        let missing = upsert(&conn, &p.id, &GateInput { name: "missing".into(), program: "definitely-not-a-program-xyz".into(), ..Default::default() }).unwrap();
        let dir = p.local_path.clone().unwrap();
        let env = crate::env::current();
        assert!(!ok.approved, "a new gate is not approved");
        assert!(begin(&conn, &p.id, None, &ok, "host-x").is_err(), "an unapproved gate does not run");
        let ok = approve(&conn, &p.id, "version", &ok.command_hash).unwrap();
        assert!(ok.approved && ok.version == 1);
        let o = execute(&ok, &dir, &env, "a");
        assert_eq!((o.status, o.exit_code), (Status::Passed, Some(0)));
        assert!(o.output_tail.contains("git version"));
        let f = execute(&fail, &dir, &env, "b");
        assert_eq!(f.status, Status::Failed);
        assert!(f.exit_code.unwrap_or(0) != 0);
        assert_eq!(execute(&missing, &dir, &env, "c").status, Status::Error);

        let id = begin(&conn, &p.id, None, &ok, "host-x").unwrap();
        let r = finish(&conn, &id, &o).unwrap();
        assert_eq!((r.status.as_str(), r.command.as_str(), r.gate_version), ("passed", "git --version", Some(1)));
        assert_eq!(latest(&conn, &p.id, "version").unwrap().unwrap().id, id);
        // A running record whose host is gone is reconciled.
        let stale = begin(&conn, &p.id, None, &find(&conn, &p.id, "version").unwrap().unwrap(), "dead-host").unwrap();
        assert_eq!(reconcile(&conn, now_ms()).unwrap(), 1);
        assert_eq!(get_result(&conn, &stale).unwrap().status, "interrupted");

        // Changing the command makes a new version that needs approval again; the old review cannot approve it.
        let same = upsert(&conn, &p.id, &GateInput { name: "version".into(), title: "renamed".into(), program: "git".into(), args: vec!["--version".into()], timeout_secs: Some(60) }).unwrap();
        assert!(same.approved && same.version == 1, "title and timeout are not the command");
        let changed = upsert(&conn, &p.id, &GateInput { name: "version".into(), program: "git".into(), args: vec!["status".into()], ..Default::default() }).unwrap();
        assert!(!changed.approved && changed.version == 2);
        assert!(begin(&conn, &p.id, None, &changed, "host-x").is_err());
        assert!(approve(&conn, &p.id, "version", &ok.command_hash).is_err(), "approving a hash that is no longer current is refused");
        let mut forged = approve(&conn, &p.id, "version", &changed.command_hash).unwrap();
        forged.args = vec!["--danger".into()];
        assert!(begin(&conn, &p.id, None, &forged, "host-x").is_err(), "only the approved command, as stored, runs");

        // A gate defined before migration 0013 (no stored hash) is unapproved until approved once, then runs.
        conn.execute("UPDATE quality_gates SET command_hash = '', approved_hash = NULL WHERE name = 'fail'", []).unwrap();
        let legacy = find(&conn, &p.id, "fail").unwrap().unwrap();
        assert!(!legacy.approved && legacy.command_hash == command_hash("git", &legacy.args));
        assert!(begin(&conn, &p.id, None, &legacy, "host-x").is_err());
        assert!(approve(&conn, &p.id, "fail", &"0".repeat(64)).is_err(), "only the shown command can be approved");
        let legacy = approve(&conn, &p.id, "fail", &legacy.command_hash).unwrap();
        assert!(legacy.approved);
        assert!(begin(&conn, &p.id, None, &legacy, "host-x").is_ok());
        revoke(&conn, &p.id, "version").unwrap();
        assert!(!find(&conn, &p.id, "version").unwrap().unwrap().approved);

        assert_eq!(redact(&format!("token {}{} end", "ghp_", "abcdefghijklmnopqrstuvwxyz1234")), "token [redacted] end");
        assert_eq!(redact("Bearer abc"), "Bearer abc", "short values are not tokens");
        assert_eq!(list(&conn, &p.id).unwrap().len(), 3);
        assert!(remove(&conn, &p.id, "missing").unwrap());
    }

    #[test]
    fn suggestions_come_from_detected_scripts() {
        let (conn, _t, mut p) = fixture();
        p.stack.scripts = vec![
            crate::project::ScriptHint { kind: "test".into(), command: "npm test".into() },
            crate::project::ScriptHint { kind: "dev".into(), command: "npm run dev".into() },
            crate::project::ScriptHint { kind: "build".into(), command: "cargo build --release".into() },
        ];
        let s = suggest(&p);
        assert_eq!(s.iter().map(|g| (g.name.as_str(), g.program.as_str())).collect::<Vec<_>>(), [("test", "npm"), ("build", "cargo")]);
        assert_eq!(s[1].args, ["build", "--release"]);
        let _ = conn;
    }
}

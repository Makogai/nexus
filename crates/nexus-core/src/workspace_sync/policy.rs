//! Workspace sync policies (Portable Workspace P7, ADR-0039), on top of M29's Git synchronisation — no second engine.
//!
//! * **Manual** (default): records are saved in the workspace folder; nothing is committed or published until the
//!   person reviews a commit and a push in the Sync Center.
//! * **Automatic with review**: when NEXUS wrote portable records, it asks (a notification) for the reviewed commit and
//!   push — publication still needs the person's confirmation.
//! * **Trusted automatic** (opt-in, per workspace, on this machine only): NEXUS-generated records on a fixed allowlist
//!   (development records, current state, `project.json`, `environment.json`) are committed and pushed to the remote
//!   the person approved — through M29's own review → commit → fetch → review → push with its tokens and inspections.
//!   Any inspection finding, a changed remote or branch, a remote that is ahead, other unpushed commits, or a managed
//!   policy that forbids it stops the automatic path and falls back to asking. Never forced; failures are recorded.
//!
//! * **Autonomous** (opt-in, per workspace, on this machine only; ADR-0042): every change in the workspace repository is
//!   committed and pushed on its own when a Claude session finishes a turn or ends (and on the host's heartbeat) — with
//!   the commit message the session proposed (`message::suggest`). Same route and the same stops as trusted: M29's
//!   inspected commit and push, never forced; a finding, a changed remote or branch, or a remote that is ahead stops it
//!   and asks with one notification.
//!
//! The policy is machine-local (`app_settings`), never read from the workspace: a synced file cannot switch automatic
//! publication on for another computer. Organisations can forbid trusted (or automatic) sync in a managed policy file.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::ops::{self, Sync};
use crate::error::{CoreError, Result};
use crate::workspace::Workspace;
use crate::{git, settings};

const RETRY_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Manual,
    Review,
    Trusted,
    Autonomous,
}

/// After a Claude turn, wait this long before syncing, so a burst of writes becomes one commit.
pub const SETTLE_MS: i64 = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Trust {
    /// The approved remote URL (credentials removed) and branch; a different one pauses automatic publication.
    pub remote: String,
    pub branch: String,
    pub authorized_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub mode: Mode,
    pub trust: Option<Trust>,
}

/// What an organisation's managed policy allows (`<ProgramData>/NEXUS/policy.json` and equivalents):
/// `{ "workspaceSync": { "trustedAutoSync": false, "automaticReview": true } }`. Missing file = allowed.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Managed {
    pub trusted_allowed: bool,
    pub review_allowed: bool,
    /// `workspaceSync.autonomousSync` (default: as `trustedAutoSync`).
    pub autonomous_allowed: bool,
    /// The file that set a restriction, if any.
    pub source: Option<String>,
}

pub fn default_managed_path() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(std::env::var_os("ProgramData").unwrap_or_else(|| "C:\\ProgramData".into())).join("NEXUS").join("policy.json")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/NEXUS/policy.json")
    } else {
        PathBuf::from("/etc/nexus/policy.json")
    }
}

/// Read the managed policy file (strict: an unreadable file is treated as forbidding automatic sync).
pub fn managed(path: &Path) -> Managed {
    let allowed = Managed { trusted_allowed: true, review_allowed: true, autonomous_allowed: true, source: None };
    let Ok(text) = std::fs::read_to_string(path) else { return allowed };
    let source = Some(path.display().to_string());
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(v) => {
            let ws = &v["workspaceSync"];
            let trusted = ws.get("trustedAutoSync").and_then(|x| x.as_bool()).unwrap_or(true);
            let review = ws.get("automaticReview").and_then(|x| x.as_bool()).unwrap_or(true);
            let autonomous = trusted && ws.get("autonomousSync").and_then(|x| x.as_bool()).unwrap_or(true);
            Managed { trusted_allowed: trusted, review_allowed: review, autonomous_allowed: autonomous, source: (!trusted || !review || !autonomous).then_some(source).flatten() }
        }
        Err(_) => Managed { trusted_allowed: false, review_allowed: false, autonomous_allowed: false, source },
    }
}

fn key(ws: &str) -> String {
    format!("sync_policy:{ws}")
}
fn due_key(ws: &str) -> String {
    format!("sync_due:{ws}")
}

pub fn get(conn: &Connection, workspace_id: &str) -> Result<Policy> {
    Ok(settings::get(conn, &key(workspace_id))?.unwrap_or_default())
}

/// Files NEXUS may publish on its own under the trusted policy: the records it generates, nothing the person or
/// Claude writes as knowledge (tasks, decisions, memories need the person's review).
pub fn allowlisted(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    match parts.as_slice() {
        ["project-context", p, "development", f] => !p.is_empty() && f.ends_with(".md") && !f.starts_with('.'),
        ["project-context", p, f] => !p.is_empty() && matches!(*f, "current-state.md" | "project.json" | "environment.json"),
        _ => false,
    }
}

pub const ALLOWLIST: &[&str] = &["project-context/*/development/*.md", "project-context/*/current-state.md", "project-context/*/project.json", "project-context/*/environment.json"];

/// Change the policy. Trusted needs: allowed by the managed policy, the workspace's own repository with a remote and a
/// branch, and the person's explicit confirmation that the remote is private and approved for these records.
pub fn set(conn: &Connection, ws: &Workspace, mode: Mode, remote_is_private: bool, m: &Managed) -> Result<Policy> {
    let policy = match mode {
        Mode::Manual => Policy::default(),
        Mode::Review => {
            if !m.review_allowed {
                return Err(CoreError::Unsupported(format!("automatic sync is turned off by your organisation ({})", m.source.clone().unwrap_or_default())));
            }
            Policy { mode, trust: None }
        }
        Mode::Trusted | Mode::Autonomous => {
            if !m.trusted_allowed || (mode == Mode::Autonomous && !m.autonomous_allowed) {
                return Err(CoreError::Unsupported(format!("automatic publication is turned off by your organisation ({})", m.source.clone().unwrap_or_default())));
            }
            if !super::own_repository(&ws.root_path) {
                return Err(CoreError::invalid("the workspace is not a Git repository of its own"));
            }
            let remote = git::upstream_of_head(&ws.root_path).map(|(r, _)| r).or_else(|| git::remotes(&ws.root_path).into_iter().next()).ok_or_else(|| CoreError::invalid("the workspace has no remote — add a private remote first"))?;
            let url = git::remote_url(&ws.root_path).map(|u| git::redact_url(&u)).ok_or_else(|| CoreError::invalid(format!("remote {remote} has no URL")))?;
            let branch = git::status(&ws.root_path)?.branch.ok_or_else(|| CoreError::invalid("check out a branch first"))?;
            if !remote_is_private {
                return Err(CoreError::invalid("confirm that the remote is private and approved for these records"));
            }
            Policy { mode, trust: Some(Trust { remote: url, branch, authorized_at: crate::now_ms() }) }
        }
    };
    settings::set(conn, &key(&ws.id), &policy)?;
    Ok(policy)
}

/// Note that a workspace has new records to act on (called when a development record is written).
pub fn mark_due(conn: &Connection, workspace_id: &str) -> Result<()> {
    settings::set(conn, &due_key(workspace_id), &crate::now_ms())
}

/// A session ended (its process exited): under the autonomous policy its workspace is due now — the host runs
/// [`run_due`] right after. Covers sessions without hooks (policy-blocked), where no Stop event arrives.
pub fn session_ended(conn: &Connection, session_id: &str) -> Result<()> {
    use rusqlite::OptionalExtension;
    let ws: Option<String> = conn.query_row("SELECT workspace_id FROM sessions WHERE id = ?1", [session_id], |r| r.get(0)).optional()?.flatten();
    if let Some(ws) = ws
        && get(conn, &ws)?.mode == Mode::Autonomous
    {
        mark_due(conn, &ws)?;
    }
    Ok(())
}

/// Note that a workspace may have changes to act on in `delay_ms` — a Claude turn ended (Stop hook) or a session ended.
/// Each call moves the time forward, so a burst of turns becomes one commit. Cheap: one settings write (hooks call it).
pub fn mark_due_in(conn: &Connection, workspace_id: &str, delay_ms: i64) -> Result<()> {
    settings::set(conn, &due_key(workspace_id), &(crate::now_ms() + delay_ms))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub mode: Mode,
    /// `none`, `notified` (review requested), `committed` (published later), `pushed`, `paused` (fell back to review).
    pub action: &'static str,
    pub message: String,
    pub files: usize,
}

fn ask(s: &Sync<'_>, files: usize, why: Option<&str>) -> Result<()> {
    let body = match why {
        Some(w) => format!("{files} workspace file(s) are ready, but automatic publication stopped: {w}. Review them in the Sync Center."),
        None => format!("{files} development record file(s) are ready to commit and push — review them in the Sync Center."),
    };
    let mut e = crate::notify::NewEvent::new(crate::notify::Category::ReviewRequired, "sync", "Workspace records ready to sync").body(body).target("sync", s.ws.id.clone()).dedupe(format!("sync-records:{}", s.ws.id));
    e.workspace_id = Some(s.ws.id.clone());
    crate::notify::publish(s.conn, e)?;
    Ok(())
}

/// Act on the workspace's policy now: nothing (manual), ask (review), or publish the allowlisted records (trusted).
pub fn apply(s: &Sync<'_>, m: &Managed) -> Result<Outcome> {
    let policy = get(s.conn, &s.ws.id)?;
    let out = |action, message: String, files| Ok(Outcome { mode: policy.mode, action, message, files });
    if policy.mode == Mode::Manual || !super::own_repository(&s.ws.root_path) {
        return out("none", "manual sync".into(), 0);
    }
    // Autonomous publishes every change in the workspace repository; the other modes only NEXUS's own records.
    let everything = policy.mode == Mode::Autonomous;
    if everything && !m.autonomous_allowed {
        return out("none", "autonomous sync is turned off by the organisation".into(), 0);
    }
    let pending: Vec<String> = git::changed_paths(&s.ws.root_path)?.into_iter().map(|(_, p)| p).filter(|p| everything || allowlisted(p)).collect();
    let trusted_ok = policy.mode == Mode::Trusted && m.trusted_allowed;
    if policy.mode == Mode::Review || (policy.mode == Mode::Trusted && !m.trusted_allowed) {
        if pending.is_empty() {
            return out("none", "nothing new".into(), 0);
        }
        if !m.review_allowed {
            return out("none", "automatic sync is turned off by the organisation".into(), 0);
        }
        let why = (!trusted_ok && policy.mode == Mode::Trusted).then_some("your organisation turned trusted automatic sync off");
        ask(s, pending.len(), why)?;
        return out("notified", "review requested".into(), pending.len());
    }
    // Trusted or autonomous.
    let mode = policy.mode;
    let trust = policy.trust.clone().ok_or_else(|| CoreError::invalid("the automatic policy has no approved remote"))?;
    // The audit entry is written after M29's own operations (they allow one running operation at a time).
    let audit = |status: &str, kind: Option<&str>, message: &str, commits: usize, files: usize| -> Result<()> {
        let head = git::head(&s.ws.root_path);
        let id = super::history::begin(s.conn, &s.ws.id, "auto-sync", s.host_id, None, head.as_deref())?;
        super::history::finish(s.conn, &id, status, head.as_deref(), commits, files, kind, Some(message))
    };
    let pause = |why: String, files: usize| -> Result<Outcome> {
        audit("refused", Some("paused"), &why, 0, files)?;
        ask(s, files.max(1), Some(&why))?;
        Ok(Outcome { mode, action: "paused", message: why, files })
    };
    let url = git::remote_url(&s.ws.root_path).map(|u| git::redact_url(&u)).unwrap_or_default();
    let branch = git::status(&s.ws.root_path)?.branch.unwrap_or_default();
    if url != trust.remote || branch != trust.branch {
        return pause(format!("the remote or branch changed since you approved it (approved {} on {})", trust.remote, trust.branch), pending.len());
    }
    // 1. Commit — M29's reviewed commit with its inspection; any finding stops it. The message is the one Claude proposed
    //    for this work (or one composed from the paths).
    if !pending.is_empty() {
        let review = ops::review_commit(s, Some(&pending))?;
        if !review.inspection.findings.is_empty() {
            return pause(format!("the security inspection reported {} finding(s) in the changes", review.inspection.findings.len()), pending.len());
        }
        let message = if everything {
            super::message::suggest(s.conn, &s.ws.id, &s.ws.root_path, &pending)?
        } else {
            "NEXUS: development records (trusted automatic sync)".to_string()
        };
        if let Err(e) = ops::commit(s, &message, Some(&pending), &review.token) {
            return pause(format!("committing failed: {e}"), pending.len());
        }
    }
    // 2. Publish — only when the remote is not ahead and every outgoing file is a record.
    if let Err(e) = ops::fetch(s) {
        let _ = settings::set(s.conn, &due_key(&s.ws.id), &(crate::now_ms() + RETRY_MS));
        audit("failed", Some("offline"), &format!("committed locally; publishing later: {e}"), 0, pending.len())?;
        return out("committed", format!("committed locally; the remote could not be reached ({e}) — NEXUS tries again later"), pending.len());
    }
    let pr = ops::review_push(s)?;
    if pr.commits.is_empty() {
        audit("ok", None, "nothing to publish", 0, 0)?;
        return out("none", "nothing to publish".into(), 0);
    }
    if pr.behind > 0 {
        return pause(format!("the remote has {} new commit(s) — pull them in the Sync Center first (NEXUS never force-pushes)", pr.behind), pending.len());
    }
    if !everything && let Some(other) = pr.files.iter().find(|f| !allowlisted(&f.path)) {
        return pause(format!("other unpushed commits change {} — push those yourself after reviewing them", other.path), pending.len());
    }
    if !pr.inspection.findings.is_empty() {
        return pause(format!("the push inspection reported {} finding(s)", pr.inspection.findings.len()), pending.len());
    }
    match ops::push(s, &pr.token) {
        Ok(p) => {
            let what = if everything { "file(s)" } else { "record file(s)" };
            audit("ok", None, &format!("published {} {what} to {}/{}", pr.files.len(), p.remote, p.branch), p.commits, pr.files.len())?;
            out("pushed", format!("published {} {what}", pr.files.len()), pr.files.len())
        }
        Err(e) => {
            let _ = settings::set(s.conn, &due_key(&s.ws.id), &(crate::now_ms() + RETRY_MS));
            audit("failed", Some("push"), &format!("committed locally; push failed: {e}"), 0, pr.files.len())?;
            out("committed", format!("committed locally; pushing failed ({e}) — NEXUS tries again later"), pr.files.len())
        }
    }
}

/// Run the policy for every workspace with records due (hosts call this after writing records and on heartbeats).
pub fn run_due(conn: &Connection, data_dir: &Path, home: &Path, host_id: &str, managed_path: &Path) -> Result<Vec<(String, Outcome)>> {
    let due: Vec<(String, i64)> = conn
        .prepare("SELECT key, value_json FROM app_settings WHERE key LIKE 'sync_due:%'")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .filter_map(|r| r.ok())
        .filter_map(|(k, v)| Some((k.strip_prefix("sync_due:")?.to_string(), v.parse::<i64>().ok()?)))
        .collect();
    let m = managed(managed_path);
    let mut out = Vec::new();
    for (ws_id, at) in due {
        if at > crate::now_ms() {
            continue;
        }
        settings::remove(conn, &due_key(&ws_id))?;
        let Ok(ws) = crate::workspace::get(conn, &ws_id) else { continue };
        if !ws.available {
            continue;
        }
        let s = Sync { conn, ws: &ws, data_dir, home, host_id };
        match apply(&s, &m) {
            Ok(o) => out.push((ws_id, o)),
            Err(e) => tracing::warn!(workspace = %ws_id, error = %e, "sync policy not applied"),
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PolicyView {
    pub policy: Policy,
    pub managed: Managed,
    pub allowlist: &'static [&'static str],
    /// Why trusted mode cannot be chosen now, if it cannot.
    pub trusted_unavailable: Option<String>,
    /// Why autonomous mode cannot be chosen now, if it cannot.
    pub autonomous_unavailable: Option<String>,
    pub remote: Option<String>,
    pub branch: Option<String>,
}

pub fn view(conn: &Connection, ws: &Workspace, managed_path: &Path) -> Result<PolicyView> {
    let m = managed(managed_path);
    let repo = super::own_repository(&ws.root_path);
    let remote = repo.then(|| git::remote_url(&ws.root_path).map(|u| git::redact_url(&u))).flatten();
    let branch = repo.then(|| git::status(&ws.root_path).ok().and_then(|s| s.branch)).flatten();
    let trusted_unavailable = if !m.trusted_allowed {
        Some(format!("turned off by your organisation ({})", m.source.clone().unwrap_or_default()))
    } else if !repo {
        Some("the workspace is not a Git repository yet".into())
    } else if remote.is_none() {
        Some("the workspace has no remote".into())
    } else {
        None
    };
    let autonomous_unavailable = match &trusted_unavailable {
        Some(why) => Some(why.clone()),
        None if !m.autonomous_allowed => Some(format!("turned off by your organisation ({})", m.source.clone().unwrap_or_default())),
        None => None,
    };
    Ok(PolicyView { policy: get(conn, &ws.id)?, managed: m, allowlist: ALLOWLIST, trusted_unavailable, autonomous_unavailable, remote, branch })
}

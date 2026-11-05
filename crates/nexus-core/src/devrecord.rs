//! Development records and history (Portable Workspace P6, ADR-0038). When a NEXUS-managed Claude session that worked
//! on a project ends — normally, or abruptly (found by host reconciliation) — NEXUS writes a concise, structured
//! record into the project's portable context (`project-context/<slug>/development/`), **only if the session did
//! something meaningful** (files changed, commits, checkpoints, a task status change).
//!
//! A record keeps three kinds of statements apart: **Verified** (Git history, gates NEXUS ran), **Observed** (what
//! Claude Code's hooks reported: files edited, command categories, compactions) and **Reported by Claude** (its own
//! checkpoints — not verified). Nothing is invented: missing facts are said to be missing; no model is called.
//! Runtime checkpoints stay machine-local in SQLite; the record is the portable, reviewable result.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::checkpoint::{self, CheckpointKind};
use crate::error::Result;
use crate::knowledge::frontmatter::Value;
use crate::knowledge::{self, Doc, Privacy};
use crate::session::{self, Session};
use crate::{git, project, workspace};

pub const FEATURE: &str = "development-records";
pub const TOPIC: &str = "development-record";
const MAX_FILES: usize = 40;
const MAX_COMMITS: usize = 30;

/// Everything known about a session's work, from its own sources.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Facts {
    pub files: Vec<String>,
    pub commands: BTreeMap<String, usize>,
    pub compactions: usize,
    pub checkpoints: Vec<(String, String)>,
    pub commits: Vec<(String, String)>,
    /// False when the session's starting commit is unknown (commits during it cannot be listed).
    pub commits_known: bool,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub gates: Vec<(String, String)>,
    pub task: Option<(String, String)>,
}

impl Facts {
    /// Worth a record: something changed or was decided. Chat-only sessions leave no record.
    pub fn meaningful(&self) -> bool {
        !self.files.is_empty() || !self.commits.is_empty() || !self.gates.is_empty() || self.checkpoints.iter().any(|(k, _)| k != "note")
    }
}

/// Collect the facts of a session (bounded, read-only).
pub fn gather(conn: &Connection, s: &Session) -> Result<Facts> {
    let mut f = Facts::default();
    let mut stmt = conn.prepare("SELECT kind, summary FROM session_events WHERE session_id = ?1 AND kind IN ('file', 'command', 'compact') ORDER BY created_at")?;
    let rows = stmt.query_map([&s.id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (kind, summary) = row?;
        match kind.as_str() {
            "file" if f.files.len() < MAX_FILES && !f.files.contains(&summary) => f.files.push(summary),
            "file" => {}
            "command" => *f.commands.entry(summary).or_default() += 1,
            _ => f.compactions += 1,
        }
    }
    for c in checkpoint::for_session(conn, &s.id, 50)?.into_iter().rev() {
        f.checkpoints.push((c.kind.as_str().to_string(), c.summary.chars().take(400).collect()));
    }
    let head_start: Option<String> = conn.query_row("SELECT head_start FROM sessions WHERE id = ?1", [&s.id], |r| r.get(0)).optional()?.flatten();
    let local = s.project_id.as_deref().and_then(|p| project::get(conn, p).ok()).and_then(|p| p.local_path).filter(|d| d.is_dir());
    if let Some(dir) = &local {
        if let Ok(st) = git::status(dir) {
            f.branch = st.branch;
            f.head = st.head_oid;
        }
        if let (Some(from), Some(to)) = (&head_start, &f.head) {
            f.commits_known = true;
            if from != to {
                f.commits = git::log_range(dir, &format!("{from}..{to}"), MAX_COMMITS).unwrap_or_default().into_iter().map(|c| (c.oid, c.summary)).collect();
            }
        }
    }
    let end = s.ended_at.unwrap_or_else(crate::now_ms);
    if let Some(pid) = &s.project_id {
        let mut g = conn.prepare("SELECT gate_name, status FROM gate_results WHERE project_id = ?1 AND started_at BETWEEN ?2 AND ?3 AND status <> 'running' ORDER BY started_at LIMIT 30")?;
        f.gates = g.query_map(params![pid, s.started_at, end], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
    }
    if let (Some(tid), Some(ws)) = (&s.task_id, &s.workspace_id)
        && let Ok(item) = knowledge::index::get(conn, ws, tid)
    {
        f.task = Some((item.title, item.status.unwrap_or_default()));
    }
    Ok(f)
}

fn short(oid: &str) -> &str {
    &oid[..oid.len().min(10)]
}

fn untrusted(s: &str, max: usize) -> String {
    knowledge::tasks::escape_untrusted(s, max)
}

/// The record's Markdown body.
pub fn render_body(s: &Session, profile: Option<&str>, f: &Facts) -> String {
    let mut b = String::from(
        "_Development record written by NEXUS from this session's own sources. **Verified**: Git and checks NEXUS ran. **Observed**: what Claude Code's hooks reported. **Reported by Claude**: Claude's checkpoints — not verified._\n\n## Session\n\n",
    );
    let end = s.ended_at.map(knowledge::timestamp).unwrap_or_else(|| "still open".into());
    b.push_str(&format!("- {} → {}", knowledge::timestamp(s.started_at), end));
    if let Some(e) = s.ended_at {
        b.push_str(&format!(" ({} min)", ((e - s.started_at).max(0) + 30_000) / 60_000));
    }
    b.push('\n');
    let how = match s.state {
        session::State::Interrupted => "ended abruptly (the host stopped) — this record has what was saved up to then".to_string(),
        session::State::Failed => format!("failed to run{}", s.error.as_deref().map(|e| format!(": {}", untrusted(e, 160))).unwrap_or_default()),
        _ => s.end_reason.clone().map(|r| format!("ended ({r})")).unwrap_or_else(|| "ended".into()),
    };
    b.push_str(&format!("- {how}\n"));
    if let Some(p) = profile {
        b.push_str(&format!("- Profile: {}{}\n", untrusted(p, 60), s.work_profile.as_deref().map(|w| format!(" · work profile {w}")).unwrap_or_default()));
    }
    if let Some((title, status)) = &f.task {
        b.push_str(&format!("- Task: {} — now *{status}*\n", untrusted(title, 140)));
    }

    b.push_str("\n## Verified\n\n");
    match (&f.branch, &f.head) {
        (Some(br), Some(h)) => b.push_str(&format!("- Branch `{br}` at `{}`\n", short(h))),
        (None, Some(h)) => b.push_str(&format!("- Detached HEAD at `{}`\n", short(h))),
        _ => b.push_str("- Git state not available (the project is not on this computer or is not a repository)\n"),
    }
    if !f.commits_known {
        b.push_str("- Commits during the session: unknown (no starting commit was recorded)\n");
    } else if f.commits.is_empty() {
        b.push_str("- No commits during the session\n");
    } else {
        b.push_str(&format!("- {} commit(s) during the session:\n", f.commits.len()));
        for (oid, summary) in &f.commits {
            b.push_str(&format!("  - `{}` {}\n", short(oid), untrusted(summary, 120)));
        }
    }
    if f.gates.is_empty() {
        b.push_str("- No quality gates were run by NEXUS during the session\n");
    } else {
        let list: Vec<String> = f.gates.iter().map(|(g, st)| format!("{g}: {st}")).collect();
        b.push_str(&format!("- Quality gates run by NEXUS: {}\n", list.join(", ")));
    }

    b.push_str("\n## Observed (Claude Code hooks)\n\n");
    if f.files.is_empty() {
        b.push_str("- No file edits observed\n");
    } else {
        b.push_str(&format!("- Files edited ({}{}): {}\n", f.files.len(), if f.files.len() >= MAX_FILES { "+" } else { "" }, f.files.iter().map(|x| format!("`{}`", untrusted(x, 160))).collect::<Vec<_>>().join(", ")));
    }
    if !f.commands.is_empty() {
        let list: Vec<String> = f.commands.iter().map(|(k, n)| format!("{k} ×{n}")).collect();
        b.push_str(&format!("- Commands by category: {} — results are not observed; only the gates above are verified\n", list.join(", ")));
    }
    if f.compactions > 0 {
        b.push_str(&format!("- Context compactions: {}\n", f.compactions));
    }

    let reported: Vec<&(String, String)> = f.checkpoints.iter().filter(|(k, _)| k != "note" || f.checkpoints.len() < 8).collect();
    b.push_str("\n## Reported by Claude (unverified)\n\n");
    if reported.is_empty() {
        b.push_str("- No checkpoints were recorded\n");
    }
    for (k, text) in &reported {
        b.push_str(&format!("- {k}: {}\n", untrusted(text, 400)));
    }
    let blockers: Vec<&(String, String)> = f.checkpoints.iter().filter(|(k, _)| k == "blocker").collect();
    let last = f.checkpoints.iter().rev().find(|(k, _)| k == "progress" || k == "done" || k == "blocker");
    b.push_str("\n## Next steps\n\n");
    match last {
        Some((k, text)) => b.push_str(&format!("- Last reported state ({k}): {}\n", untrusted(text, 300))),
        None => b.push_str("- Not recorded — check the task and the commits above before continuing\n"),
    }
    if !blockers.is_empty() {
        b.push_str(&format!("- Open blockers reported: {}\n", blockers.len()));
    }
    b
}

/// Write the record of an ended session (once). Returns the file written, `None` when the session was trivial or the
/// workspace does not keep records.
pub fn finalize(conn: &Connection, data_dir: &Path, session_id: &str) -> Result<Option<PathBuf>> {
    let s = session::get(conn, session_id)?;
    let done: Option<i64> = conn.query_row("SELECT record_at FROM sessions WHERE id = ?1", [session_id], |r| r.get(0))?;
    if done.is_some() || s.kind != session::Kind::Claude {
        return Ok(None);
    }
    let mark = |path: Option<&Path>| conn.execute("UPDATE sessions SET record_at = ?2, record_path = ?3 WHERE id = ?1", params![session_id, crate::now_ms(), path.map(|p| p.to_string_lossy().into_owned())]);
    let (Some(pid), Some(wsid)) = (s.project_id.clone(), s.workspace_id.clone()) else {
        mark(None)?;
        return Ok(None);
    };
    let Ok(ws) = workspace::get(conn, &wsid) else {
        mark(None)?;
        return Ok(None);
    };
    let keeps = ws.available
        && workspace::read_manifest(&ws.root_path).map(|m| m.schema_version >= 2 && m.has_feature(FEATURE) && workspace::compat(&m).writable).unwrap_or(false);
    let facts = gather(conn, &s)?;
    if !keeps || !facts.meaningful() {
        mark(None)?;
        return Ok(None);
    }
    if project::context::context_dir(&ws, &pid).is_none() {
        project::context::init(conn, data_dir, &ws, &pid)?;
    }
    let Some(ctx) = project::context::context_dir(&ws, &pid) else {
        mark(None)?;
        return Ok(None);
    };
    let p = project::get(conn, &pid)?;
    let profile = s.profile_id.as_deref().and_then(|id| crate::claude::profile::get(conn, id).ok()).map(|p| p.name);
    let date = knowledge::timestamp(s.started_at);
    let title = match &facts.task {
        Some((t, _)) => format!("{} — {} · {}", p.name, &date[..10], t.chars().take(80).collect::<String>()),
        None => format!("{} — {} development session", p.name, &date[..10]),
    };
    let now = knowledge::timestamp(crate::now_ms());
    let mut fm = knowledge::frontmatter::Frontmatter::default();
    fm.set("nexus", Value::Int(knowledge::SCHEMA));
    fm.set("id", Value::Str(uuid::Uuid::now_v7().to_string()));
    fm.set("kind", Value::Str("note".into()));
    fm.set("title", Value::Str(title.chars().take(knowledge::MAX_TITLE_CHARS).collect()));
    fm.set("project", Value::Str(pid.clone()));
    fm.set("topic", Value::Str(TOPIC.into()));
    fm.set("verification", Value::Str("unverified".into()));
    if let Some(b) = facts.branch.as_ref().filter(|b| knowledge::valid_branch_name(b)) {
        fm.set("branch", Value::Str(b.clone()));
    }
    if let Some(h) = facts.head.as_ref().filter(|h| knowledge::is_commit_id(h)) {
        fm.set("commit", Value::Str(h.clone()));
    }
    fm.set("session", Value::Str(s.id.clone()));
    if let Some(t) = &s.task_id {
        fm.set("task", Value::Str(t.clone()));
    }
    fm.set("created", Value::Str(now.clone()));
    fm.set("updated", Value::Str(now));
    fm.set("source", Value::Str("import".into()));
    let doc = Doc { fm, body: render_body(&s, profile.as_deref(), &facts) };
    doc.validate(Privacy::Shared).map_err(crate::error::CoreError::Invalid)?;
    let dir = ctx.join("development");
    std::fs::create_dir_all(&dir).map_err(|e| crate::error::CoreError::io("cannot create", &dir, e))?;
    let path = dir.join(format!("{}-{}.md", &date[..10], &s.id[..8.min(s.id.len())]));
    workspace::write_atomic(&path, doc.render().as_bytes())?;
    mark(Some(&path))?;
    let _ = project::context::refresh_state(conn, data_dir, &ws, &pid);
    knowledge::service::refresh(conn, data_dir, &ws)?;
    Ok(Some(path))
}

/// Records for every ended Claude session that has none yet (abrupt ends included). Cheap when nothing is pending.
pub fn finalize_pending(conn: &Connection, data_dir: &Path) -> Result<usize> {
    let ids: Vec<String> = conn
        .prepare("SELECT id FROM sessions WHERE record_at IS NULL AND kind = 'claude' AND state IN ('exited', 'interrupted', 'failed') ORDER BY started_at LIMIT 20")?
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut n = 0;
    for id in ids {
        match finalize(conn, data_dir, &id) {
            Ok(Some(_)) => n += 1,
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(session = %id, error = %e, "development record not written");
                let _ = conn.execute("UPDATE sessions SET record_at = ?2 WHERE id = ?1", params![id, crate::now_ms()]);
            }
        }
    }
    Ok(n)
}

/// A durable NEXUS checkpoint at context compaction: what the session had changed so far (no model, no prompt text).
pub fn checkpoint_at_compaction(conn: &Connection, session_id: &str) -> Result<()> {
    let s = session::get(conn, session_id)?;
    let f = gather(conn, &s)?;
    if f.files.is_empty() && f.commands.is_empty() {
        return Ok(());
    }
    let mut text = format!("Context compacted after {} file edit(s)", f.files.len());
    if !f.commands.is_empty() {
        text.push_str(&format!("; commands: {}", f.commands.iter().map(|(k, n)| format!("{k} ×{n}")).collect::<Vec<_>>().join(", ")));
    }
    if let Some(last) = f.files.last() {
        text.push_str(&format!("; last file: {last}"));
    }
    checkpoint::add(conn, session_id, CheckpointKind::Note, &text, checkpoint::Source::Nexus)?;
    Ok(())
}

// ---- history ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecordEntry {
    pub id: Option<String>,
    pub title: String,
    pub path: PathBuf,
    pub created: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
    /// `local` (not committed in the workspace repository), `committed` (not pushed), `synced`, or `no-repository`.
    pub sync: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionEntry {
    pub id: String,
    pub title: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub state: String,
    pub record: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub records: Vec<RecordEntry>,
    pub sessions: Vec<SessionEntry>,
    pub tasks: Vec<(String, String, String)>,
    pub decisions: Vec<(String, String)>,
    pub commits: Vec<(String, String, i64)>,
    pub gates: Vec<(String, String, i64)>,
    pub blockers: Vec<(String, i64)>,
}

fn sync_state(ws_root: &Path, file: &Path, changed: &Option<Vec<String>>, unpushed: &Option<Vec<String>>) -> &'static str {
    let Some(changed) = changed else { return "no-repository" };
    let rel = file.strip_prefix(ws_root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    if changed.contains(&rel) {
        return "local";
    }
    match unpushed {
        Some(u) if u.contains(&rel) => "committed",
        Some(_) => "synced",
        None => "committed",
    }
}

/// A project's development history: records (with their sync state), sessions, tasks, decisions, commits, gate
/// results and blockers — read from the stores, nothing generated.
pub fn history(conn: &Connection, data_dir: &Path, ws: &workspace::Workspace, project_id: &str) -> Result<History> {
    let _ = knowledge::service::refresh(conn, data_dir, ws);
    let items = knowledge::index::list(conn, &ws.id, &knowledge::index::Filter { project_id: Some(project_id.to_string()), limit: Some(500), ..Default::default() })?;
    let mine: Vec<&knowledge::index::Item> = items.iter().filter(|i| i.project_id.as_deref() == Some(project_id)).collect();
    let repo = git::toplevel(&ws.root_path).filter(|t| crate::paths::same_path(t, &ws.root_path));
    let changed = repo.as_ref().and_then(|r| git::changed_paths(r).ok()).map(|v| v.into_iter().map(|(_, p)| p).collect::<Vec<_>>());
    let unpushed = repo.as_ref().and_then(|r| git::upstream_of_head(r).map(|_| r)).and_then(|r| git::diff_raw(r, "@{u}", Some("HEAD")).ok()).map(|v| v.into_iter().map(|d| d.path).collect::<Vec<_>>());
    let mut records: Vec<RecordEntry> = mine
        .iter()
        .filter(|i| i.kind.as_deref() == Some("note") && knowledge::store::read(&i.path).map(|(d, _)| d.fm.str("topic") == Some(TOPIC)).unwrap_or(false))
        .map(|i| {
            let created = knowledge::store::read(&i.path).ok().and_then(|(d, _)| d.fm.str("created").map(str::to_string));
            RecordEntry { id: i.id.clone(), title: i.title.clone(), path: i.path.clone(), created, branch: i.branch.clone(), commit: i.commit.clone(), sync: sync_state(&ws.root_path, &i.path, &changed, &unpushed) }
        })
        .collect();
    records.sort_by(|a, b| b.created.cmp(&a.created));
    let mut stmt = conn.prepare("SELECT id, title, started_at, ended_at, state, record_path FROM sessions WHERE project_id = ?1 AND kind = 'claude' ORDER BY started_at DESC LIMIT 30")?;
    let sessions = stmt
        .query_map([project_id], |r| Ok(SessionEntry { id: r.get(0)?, title: r.get(1)?, started_at: r.get(2)?, ended_at: r.get(3)?, state: r.get(4)?, record: r.get::<_, Option<String>>(5)?.map(PathBuf::from) }))?
        .collect::<std::result::Result<_, _>>()?;
    let tasks = mine.iter().filter(|i| i.kind.as_deref() == Some("task")).map(|i| (i.id.clone().unwrap_or_default(), i.title.clone(), i.status.clone().unwrap_or_default())).collect();
    let decisions = mine.iter().filter(|i| i.kind.as_deref() == Some("decision")).take(15).map(|i| (i.title.clone(), i.status.clone().unwrap_or_default())).collect();
    let commits = project::get(conn, project_id)
        .ok()
        .and_then(|p| p.local_path)
        .filter(|d| d.is_dir())
        .and_then(|d| git::recent_commits(&d, 15))
        .unwrap_or_default()
        .into_iter()
        .map(|c| (c.oid, c.summary, c.timestamp_ms))
        .collect();
    let mut g = conn.prepare("SELECT gate_name, status, started_at FROM gate_results WHERE project_id = ?1 AND status <> 'running' ORDER BY started_at DESC LIMIT 20")?;
    let gates = g.query_map([project_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<std::result::Result<_, _>>()?;
    let blockers = checkpoint::for_project(conn, project_id, 50)?.into_iter().filter(|c| c.kind == "blocker").take(10).map(|c| (c.summary, c.created_at)).collect();
    Ok(History { records, sessions, tasks, decisions, commits, gates, blockers })
}

#[cfg(test)]
mod tests;

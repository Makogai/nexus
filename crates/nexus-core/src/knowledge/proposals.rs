//! Proposed memories (ADR-0022): deterministic extraction of *candidate* notes and decisions from meaningful session
//! activity, held machine-local until a person reviews them. Nothing here is knowledge: only [`approve`] writes an
//! authoritative knowledge file (through the validated store, so secrets are still refused).
//!
//! What counts as meaningful (no model calls, no prompt text — only what Claude chose to record as a checkpoint):
//! * `decision` checkpoints → a proposed **decision**;
//! * `blocker` checkpoints → a proposed **note** ("Known issue: …");
//! * `note` checkpoints → a proposed **note**.
//!
//! Routine `progress`/`done` checkpoints, file lists and command categories are activity, not memory, and are never
//! proposed. Proposals are deduplicated by a normalised fingerprint (a rejected one is not proposed again) and against
//! existing documents with the same title.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::index::Item;
use super::service::{self, Author, CreateInput};
use super::{content_hash, timestamp};
use crate::claude::instructions::normalize;
use crate::error::{CoreError, Result};
use crate::session;
use crate::workspace::Workspace;
use crate::{new_id, now_ms};

const MIN_CHARS: usize = 20;
const MAX_PER_SESSION: usize = 10;
const MAX_TITLE: usize = 100;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub id: String,
    pub workspace_id: String,
    pub project_id: Option<String>,
    pub session_id: String,
    pub session_title: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub source: String,
    pub branch: Option<String>,
    pub status: String,
    pub document_id: Option<String>,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

/// First sentence (or line) of a summary, bounded — a readable title.
fn title_of(summary: &str) -> String {
    let first = summary.lines().next().unwrap_or_default();
    let cut = first.find(". ").map(|i| &first[..i]).unwrap_or(first).trim().trim_end_matches('.');
    let t: String = cut.chars().take(MAX_TITLE).collect();
    if cut.chars().count() > MAX_TITLE { format!("{}…", t.trim_end()) } else { t }
}

/// Extract proposals from one session's checkpoints. Idempotent; returns how many new proposals were created.
pub fn extract_for_session(conn: &Connection, session_id: &str) -> Result<usize> {
    let s = session::get(conn, session_id)?;
    let Some(ws) = s.workspace_id.clone() else { return Ok(0) };
    let mut stmt = conn.prepare(
        "SELECT id, kind, summary FROM session_events WHERE session_id = ?1 AND kind IN ('decision', 'blocker', 'note')
         ORDER BY created_at, rowid LIMIT 200",
    )?;
    let events: Vec<(String, String, String)> = stmt.query_map([session_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<std::result::Result<_, _>>()?;
    let mut created = 0;
    for (event_id, source, summary) in events {
        if created >= MAX_PER_SESSION {
            break;
        }
        let summary = summary.trim();
        if summary.chars().count() < MIN_CHARS {
            continue;
        }
        let (kind, title) = match source.as_str() {
            "decision" => ("decision", title_of(summary)),
            "blocker" => ("note", format!("Known issue: {}", title_of(summary))),
            _ => ("note", title_of(summary)),
        };
        let fingerprint = content_hash(format!("{kind}\n{}", normalize(summary)).as_bytes());
        // Already a document with this title in the workspace → nothing new to propose.
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM knowledge_items WHERE (workspace_id = ?1 OR workspace_id IS NULL) AND valid = 1 AND lower(title) = lower(?2) LIMIT 1",
                params![ws, title],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if exists {
            continue;
        }
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO memory_proposals(id, workspace_id, project_id, session_id, kind, title, body, source,
                source_event_id, branch, fingerprint, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'pending', ?12)",
            params![new_id(), ws, s.project_id, s.id, kind, title, summary, source, event_id, s.branch, fingerprint, now_ms()],
        )?;
        created += inserted;
    }
    if created > 0 {
        let mut e = crate::notify::NewEvent::for_session(
            crate::notify::Category::ReviewRequired,
            "knowledge",
            &s,
            format!("{created} memory proposal(s) from “{}” to review", s.title),
        )
        .body("Nothing becomes project memory until you approve it.")
        .target("proposals", ws.clone())
        .dedupe(format!("proposals:{ws}"));
        e.session_id = None; // about the workspace's review queue, not a live session
        crate::notify::publish(conn, e)?;
    }
    Ok(created)
}

/// Extract from the workspace's recent sessions (for sessions that ended without NEXUS hooks).
pub fn extract_recent(conn: &Connection, workspace_id: &str) -> Result<usize> {
    let since = now_ms() - 30 * 24 * 3600 * 1000;
    let ids: Vec<String> = conn
        .prepare("SELECT id FROM sessions WHERE workspace_id = ?1 AND kind = 'claude' AND started_at >= ?2 ORDER BY started_at DESC LIMIT 50")?
        .query_map(params![workspace_id, since], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut n = 0;
    for id in ids {
        n += extract_for_session(conn, &id)?;
    }
    Ok(n)
}

const SELECT: &str = "SELECT p.id, p.workspace_id, p.project_id, p.session_id, s.title, p.kind, p.title, p.body, p.source, p.branch,
    p.status, p.document_id, p.created_at, p.decided_at FROM memory_proposals p JOIN sessions s ON s.id = p.session_id";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Proposal> {
    Ok(Proposal {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        project_id: r.get(2)?,
        session_id: r.get(3)?,
        session_title: r.get(4)?,
        kind: r.get(5)?,
        title: r.get(6)?,
        body: r.get(7)?,
        source: r.get(8)?,
        branch: r.get(9)?,
        status: r.get(10)?,
        document_id: r.get(11)?,
        created_at: r.get(12)?,
        decided_at: r.get(13)?,
    })
}

pub fn list(conn: &Connection, workspace_id: &str, status: Option<&str>) -> Result<Vec<Proposal>> {
    let mut stmt = conn.prepare(&format!("{SELECT} WHERE p.workspace_id = ?1 AND (?2 IS NULL OR p.status = ?2) ORDER BY p.created_at DESC LIMIT 200"))?;
    let rows = stmt.query_map(params![workspace_id, status], from_row)?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> Result<Proposal> {
    conn.query_row(&format!("{SELECT} WHERE p.id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| CoreError::not_found(format!("no memory proposal {id}")))
}

pub fn pending_count(conn: &Connection, workspace_id: &str, project_id: Option<&str>) -> Result<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_proposals WHERE workspace_id = ?1 AND status = 'pending' AND (?2 IS NULL OR project_id = ?2 OR project_id IS NULL)",
        params![workspace_id, project_id],
        |r| r.get(0),
    )?;
    Ok(n as usize)
}

/// What the reviewer decided to keep (edits are allowed before approving).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Approval {
    pub title: Option<String>,
    pub body: Option<String>,
    /// `workspace` | `project` (default) | `branch` (the session's branch).
    pub scope: Option<String>,
    pub local_only: bool,
}

/// A person approves a proposal: it becomes a knowledge document (decision → `accepted`, note → `verified`) with its
/// provenance in the body. Only now is it authoritative.
pub fn approve(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str, a: &Approval) -> Result<Item> {
    let p = get(conn, id)?;
    if p.workspace_id != ws.id {
        return Err(CoreError::invalid("the proposal belongs to another workspace"));
    }
    if p.status != "pending" {
        return Err(CoreError::Conflict(format!("the proposal was already {}", p.status)));
    }
    let scope = a.scope.as_deref().unwrap_or("project");
    let (project_id, branch) = match scope {
        "workspace" => (None, None),
        "project" => (p.project_id.clone(), None),
        "branch" => (p.project_id.clone(), p.branch.clone().ok_or_else(|| CoreError::invalid("the session recorded no branch"))?.into()),
        _ => return Err(CoreError::invalid("scope must be workspace, project or branch")),
    };
    let title = a.title.as_deref().map(str::trim).filter(|t| !t.is_empty()).unwrap_or(&p.title).to_string();
    let body = a.body.clone().unwrap_or_else(|| p.body.clone());
    let provenance = format!(
        "\n\n_Proposed from the NEXUS session “{}” ({}) and approved for the workspace._",
        crate::knowledge::tasks::escape_untrusted(&p.session_title, 80),
        &timestamp(p.created_at)[..10]
    );
    let input = CreateInput {
        kind: p.kind.clone(),
        title,
        body: format!("{}{provenance}", body.trim()),
        project_id,
        local_only: a.local_only,
        status: (p.kind == "decision").then(|| "accepted".to_string()),
        labels: vec!["from-session".into()],
        branch,
        ..Default::default()
    };
    let item = service::create(conn, data_dir, ws, &input, Author::Human)?;
    conn.execute(
        "UPDATE memory_proposals SET status = 'approved', document_id = ?2, decided_at = ?3 WHERE id = ?1",
        params![id, item.id, now_ms()],
    )?;
    Ok(item)
}

pub fn reject(conn: &Connection, id: &str) -> Result<()> {
    let n = conn.execute("UPDATE memory_proposals SET status = 'rejected', decided_at = ?2 WHERE id = ?1 AND status = 'pending'", params![id, now_ms()])?;
    if n == 0 {
        return Err(CoreError::not_found(format!("no pending memory proposal {id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::{self, CheckpointKind, Source};
    use crate::session::NewSession;

    #[test]
    fn only_meaningful_activity_is_proposed_and_only_approval_makes_knowledge() {
        let t = tempfile::tempdir().unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let data = t.path().join("data");
        let ws = crate::workspace::create(&conn, t.path().join("ws").to_str().unwrap(), "W", "").unwrap();
        let s = session::insert_starting(&conn, NewSession {
            title: "Checkout work".into(), cwd: t.path().to_path_buf(), program: "claude".into(), workspace_id: Some(ws.id.clone()),
            branch: Some("feature/pay".into()), ..Default::default()
        }).unwrap();
        for (kind, text) in [
            (CheckpointKind::Progress, "Cart page renders the line items correctly"),
            (CheckpointKind::Done, "Checkout flow finished and tested end to end"),
            (CheckpointKind::Decision, "Use Stripe Checkout instead of a custom form. It handles SCA for us."),
            (CheckpointKind::Blocker, "Sandbox webhooks fail behind the corporate proxy"),
            (CheckpointKind::Note, "too short"),
            (CheckpointKind::Note, "The payment sandbox resets every night at 02:00 UTC"),
        ] {
            checkpoint::add(&conn, &s.id, kind, text, Source::Claude).unwrap();
        }
        assert_eq!(extract_for_session(&conn, &s.id).unwrap(), 3, "decision, blocker, note — not progress/done/short");
        assert_eq!(extract_for_session(&conn, &s.id).unwrap(), 0, "idempotent");
        let pending = list(&conn, &ws.id, Some("pending")).unwrap();
        let titles: Vec<&str> = pending.iter().map(|p| p.title.as_str()).collect();
        assert!(titles.contains(&"Use Stripe Checkout instead of a custom form"), "{titles:?}");
        assert!(titles.contains(&"Known issue: Sandbox webhooks fail behind the corporate proxy"));
        assert!(service::list(&conn, &ws, &[], false).unwrap().is_empty(), "nothing is knowledge before approval");

        let decision = pending.iter().find(|p| p.kind == "decision").unwrap();
        let item = approve(&conn, &data, &ws, &decision.id, &Approval { scope: Some("branch".into()), ..Default::default() }).unwrap();
        assert_eq!((item.kind.as_deref(), item.status.as_deref(), item.branch.as_deref()), (Some("decision"), Some("accepted"), Some("feature/pay")));
        let text = std::fs::read_to_string(&item.path).unwrap();
        assert!(text.contains("source: human") && text.contains("from-session") && text.contains("Proposed from the NEXUS session “Checkout work”"));
        assert!(approve(&conn, &data, &ws, &decision.id, &Approval::default()).is_err(), "approved once");

        let note = pending.iter().find(|p| p.title.starts_with("Known issue")).unwrap();
        reject(&conn, &note.id).unwrap();
        assert_eq!(extract_for_session(&conn, &s.id).unwrap(), 0, "a rejected proposal is not proposed again");
        assert_eq!(pending_count(&conn, &ws.id, None).unwrap(), 1);

        // Secrets are still refused at approval (the store validates portable knowledge).
        let last = list(&conn, &ws.id, Some("pending")).unwrap().remove(0);
        let leak = Approval { body: Some("token sk-ant-api03-abcdefghijklmnop".into()), ..Default::default() };
        assert!(approve(&conn, &data, &ws, &last.id, &leak).is_err());
        assert_eq!(get(&conn, &last.id).unwrap().status, "pending", "a failed approval leaves the proposal pending");
    }
}

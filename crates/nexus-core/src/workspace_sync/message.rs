//! Commit messages for the workspace repository (ADR-0042; owner feedback 2026-11-06: "Claude instances that do changes
//! always write commit messages, not me").
//!
//! A Claude session whose work changed the workspace proposes one with the MCP tool `propose_commit_message`; NEXUS
//! stores it with the session (bounded, one line of subject plus an optional short body). [`suggest`] takes the
//! proposals of the workspace's sessions since the last commit; without one it composes a message from the changed paths
//! ("umbra: 2 tasks, 1 note, overview"). The Sync Center pre-fills it, automatic sync commits with it, and the person can
//! always edit it before a reviewed commit. NEXUS calls no model.

use std::path::Path;

use rusqlite::{Connection, params};
use serde_json::json;

use crate::error::{CoreError, Result};
use crate::{git, new_id, now_ms};

pub const MAX_SUBJECT: usize = 100;
pub const MAX_BODY: usize = 1200;
/// Proposals kept per session (a long session may propose several; the newest wins).
const MAX_PER_SESSION: i64 = 20;

/// Clean a proposed message: no control characters, a subject line of at most [`MAX_SUBJECT`] characters, a body of at
/// most [`MAX_BODY`]. `None` when nothing usable is left.
pub fn clean(text: &str) -> Option<String> {
    let text = text.replace("\r\n", "\n");
    let mut lines = text.lines().map(|l| l.chars().filter(|c| !c.is_control()).collect::<String>());
    let subject: String = lines.find(|l| !l.trim().is_empty())?.trim().chars().take(MAX_SUBJECT).collect();
    let body: Vec<String> = lines.map(|l| l.trim_end().to_string()).collect();
    let body = body.join("\n").trim_matches('\n').chars().take(MAX_BODY).collect::<String>();
    Some(if body.trim().is_empty() { subject } else { format!("{subject}\n\n{body}") })
}

/// Store a session's proposal (MCP `propose_commit_message`).
pub fn record(conn: &Connection, session_id: &str, text: &str) -> Result<String> {
    let message = clean(text).ok_or_else(|| CoreError::invalid("the commit message is empty"))?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM session_events WHERE session_id = ?1 AND kind = 'commit-message'", [session_id], |r| r.get(0))?;
    if count >= MAX_PER_SESSION {
        return Err(CoreError::invalid("this session already proposed enough commit messages"));
    }
    let subject = message.lines().next().unwrap_or_default().to_string();
    conn.execute(
        "INSERT INTO session_events(id, session_id, kind, summary, data, source, created_at) VALUES (?1, ?2, 'commit-message', ?3, ?4, 'nexus', ?5)",
        params![new_id(), session_id, subject, json!({ "message": message }).to_string(), now_ms()],
    )?;
    Ok(message)
}

/// The newest proposal of each session of the workspace since `since_ms`, newest first.
pub fn proposals_since(conn: &Connection, workspace_id: &str, since_ms: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT e.session_id, json_extract(e.data, '$.message') FROM session_events e JOIN sessions s ON s.id = e.session_id
         WHERE e.kind = 'commit-message' AND s.workspace_id = ?1 AND e.created_at > ?2 ORDER BY e.created_at DESC, e.rowid DESC",
    )?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for row in stmt.query_map(params![workspace_id, since_ms], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))? {
        let (session, message) = row?;
        if let Some(m) = message
            && seen.insert(session)
        {
            out.push(m);
        }
    }
    Ok(out)
}

/// What a workspace path is, for a composed message: (area, kind). The area is a project's folder name or `workspace`.
fn classify(path: &str) -> (String, &'static str) {
    let parts: Vec<&str> = path.split('/').collect();
    let (area, rest) = match parts.as_slice() {
        ["project-context", slug, rest @ ..] if !rest.is_empty() => (slug.to_string(), rest),
        rest => ("workspace".to_string(), rest),
    };
    let kind = match rest {
        ["tasks", ..] => "task",
        ["notes", ..] | ["knowledge", ..] => "note",
        ["decisions", ..] => "decision",
        ["development", ..] => "development record",
        ["rules", ..] => "rule",
        ["constitutions", ..] => "constitution",
        ["workflows", ..] => "workflow",
        ["skills", ..] => "skill",
        ["templates", ..] => "template file",
        ["overview.md"] => "overview",
        ["architecture.md"] => "architecture",
        ["current-state.md"] => "current state",
        ["project.json"] | ["environment.json"] => "project facts",
        ["nexus-workspace.json"] | ["nexus.lock.json"] => "workspace settings",
        [".gitignore"] => "ignore rules",
        _ => "file",
    };
    (area, kind)
}

/// A message composed from the changed paths alone: `NEXUS: umbra — 2 tasks, 1 note, overview; workspace — 1 rule`.
pub fn compose(paths: &[String]) -> String {
    if paths.is_empty() {
        return "NEXUS: update the workspace".into();
    }
    let mut areas: Vec<(String, Vec<(&'static str, usize)>)> = Vec::new();
    for p in paths {
        let (area, kind) = classify(p);
        let i = match areas.iter().position(|(a, _)| *a == area) {
            Some(i) => i,
            None => {
                areas.push((area, Vec::new()));
                areas.len() - 1
            }
        };
        match areas[i].1.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, n)) => *n += 1,
            None => areas[i].1.push((kind, 1)),
        }
    }
    // Projects first (in the order they appear), the workspace's own files last.
    areas.sort_by_key(|(a, _)| a == "workspace");
    let single = |k: &str| matches!(k, "overview" | "architecture" | "current state" | "project facts" | "workspace settings" | "ignore rules");
    let describe = |kinds: &[(&str, usize)]| {
        kinds
            .iter()
            .map(|(k, n)| if single(k) { k.to_string() } else { format!("{n} {k}{}", if *n == 1 { "" } else { "s" }) })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let subject = format!("NEXUS: {}", areas.iter().map(|(a, k)| format!("{a} — {}", describe(k))).collect::<Vec<_>>().join("; "));
    let subject: String = if subject.chars().count() > MAX_SUBJECT { format!("{}…", subject.chars().take(MAX_SUBJECT - 1).collect::<String>()) } else { subject };
    let mut listed: Vec<String> = paths.iter().take(20).map(|p| format!("- {p}")).collect();
    if paths.len() > 20 {
        listed.push(format!("- … and {} more", paths.len() - 20));
    }
    format!("{subject}\n\n{}", listed.join("\n"))
}

fn used_key(workspace_id: &str) -> String {
    format!("commit_messages_used:{workspace_id}")
}

/// A commit was made: the proposals until now were used for it (Git's commit time has only seconds).
pub fn mark_used(conn: &Connection, workspace_id: &str) -> Result<()> {
    crate::settings::set(conn, &used_key(workspace_id), &now_ms())
}

/// The message to commit `paths` of the workspace with: Claude's proposals since the last commit, else a composed one.
pub fn suggest(conn: &Connection, workspace_id: &str, root: &Path, paths: &[String]) -> Result<String> {
    let used: i64 = crate::settings::get(conn, &used_key(workspace_id))?.unwrap_or(0);
    let since = git::last_commit(root).map(|c| c.timestamp_ms).unwrap_or(0).max(used);
    let proposals = proposals_since(conn, workspace_id, since)?;
    Ok(match proposals.len() {
        0 => compose(paths),
        1 => proposals[0].clone(),
        n => {
            let subjects: Vec<String> = proposals.iter().map(|m| format!("- {}", m.lines().next().unwrap_or_default())).collect();
            format!("NEXUS: work from {n} Claude sessions\n\n{}", subjects.join("\n"))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposals_are_cleaned_and_bounded() {
        assert_eq!(clean("  Add login task\u{7}\r\n\r\nWhy: the form\n"), Some("Add login task\n\nWhy: the form".into()));
        assert_eq!(clean("\n\n  \n"), None);
        assert_eq!(clean(&"x".repeat(300)).unwrap().chars().count(), MAX_SUBJECT);
        let long = format!("Subject\n\n{}", "b".repeat(5000));
        assert!(clean(&long).unwrap().chars().count() <= MAX_SUBJECT + 2 + MAX_BODY);
    }

    #[test]
    fn composed_messages_say_what_changed_where() {
        let paths: Vec<String> = [
            "project-context/umbra/tasks/a.md",
            "project-context/umbra/tasks/b.md",
            "project-context/umbra/notes/n.md",
            "project-context/umbra/overview.md",
            "rules/security.md",
            "project-context/shop/development/2026-11-06-x.md",
        ]
        .map(String::from)
        .to_vec();
        let m = compose(&paths);
        let subject = m.lines().next().unwrap();
        assert_eq!(subject, "NEXUS: umbra — 2 tasks, 1 note, overview; shop — 1 development record; workspace — 1 rule");
        assert!(m.contains("- project-context/umbra/overview.md") && m.contains("- rules/security.md"));
        assert_eq!(compose(&[]), "NEXUS: update the workspace");
    }
}

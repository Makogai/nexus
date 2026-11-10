//! "Needs you" (0.5 alpha, ADR-0048): what waits for the person in a workspace — or in one project — built only from
//! data NEXUS already has. Sessions waiting for input or a permission, push requests (pending and failed), workflow
//! steps waiting for sign-off, tasks Claude put into review, learning drafts, and rules, constitutions, workflows and
//! kits whose current version is not approved on this machine. Nothing is stored, nothing is computed by a model; each
//! item names what to open.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::error::Result;
use crate::knowledge::index::{self, Filter};
use crate::knowledge::{Kind, service};
use crate::workspace::Workspace;

/// At most this many items per kind (the inbox is a list of next steps, not an archive).
const MAX_PER_KIND: usize = 20;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// `session`, `push-failed`, `push`, `workflow-review`, `task-review`, `draft`, `approval` or `kit`.
    pub kind: &'static str,
    /// What to open: the session, push request, workflow run, task, draft note, knowledge document or kit id.
    pub id: String,
    /// The thing's own name (a session, task, draft, rule … title, a branch, a kit name).
    pub title: String,
    /// Why it waits — one NEXUS-authored line.
    pub detail: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    /// For approvals: `constitution`, `rule` or `workflow` (each is approved in its own view).
    pub doc_kind: Option<String>,
    /// When it started waiting (Unix ms), when known.
    pub since: Option<i64>,
}

/// Everything that waits for the person, most urgent kind first (then newest first). `project_id` narrows it to one
/// project — workspace-wide approvals and kits stay in, since they reach that project's sessions too.
pub fn needs_you(conn: &Connection, data_dir: &Path, ws: &Workspace, project_id: Option<&str>) -> Result<Vec<Item>> {
    service::refresh(conn, data_dir, ws)?; // incremental: unchanged files are not read
    let projects: HashMap<String, String> = crate::project::list(conn, &ws.id)?.into_iter().map(|p| (p.id, p.name)).collect();
    let in_scope = |pid: Option<&str>| match project_id {
        None => pid.is_none_or(|p| projects.contains_key(p)),
        Some(want) => pid == Some(want),
    };
    let named = |pid: Option<String>| {
        let name = pid.as_ref().and_then(|p| projects.get(p).cloned());
        (pid, name)
    };
    let mut out: Vec<Item> = Vec::new();

    // 1. Sessions waiting for an answer or a permission decision (reported by hooks, ADR-0025).
    let mut stmt = conn.prepare(
        "SELECT id, title, project_id, attention_kind, attention_at FROM sessions
         WHERE workspace_id = ?1 AND state IN ('starting', 'running') AND archived_at IS NULL AND attention_kind IN ('permission', 'input')
         ORDER BY attention_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![ws.id, MAX_PER_KIND as i64], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<i64>>(4)?))
    })?;
    for row in rows {
        let (id, title, pid, kind, at) = row?;
        if project_id.is_some() && !in_scope(pid.as_deref()) {
            continue;
        }
        let (project_id, project_name) = named(pid);
        let detail = if kind == "permission" { "waits for a permission decision" } else { "waits for your answer" };
        out.push(Item { kind: "session", id, title, detail: detail.into(), project_id, project_name, doc_kind: None, since: at });
    }

    // 2. Push requests: failed ones until dismissed or superseded, then pending ones (ADR-0043, ADR-0047).
    let mut failed = Vec::new();
    let mut pending = Vec::new();
    for r in crate::push_request::open(conn, project_id)? {
        if !in_scope(Some(&r.project_id)) {
            continue;
        }
        let (pid, name) = named(Some(r.project_id.clone()));
        if r.status == "failed" {
            let why = r.failure_kind.as_deref().unwrap_or("other");
            failed.push(Item { kind: "push-failed", id: r.id, title: r.branch, detail: format!("push failed ({why})"), project_id: pid, project_name: name, doc_kind: None, since: r.decided_at });
        } else {
            let detail = if r.reason.is_empty() { "Claude asks to push".to_string() } else { format!("Claude asks to push — “{}”", r.reason) };
            pending.push(Item { kind: "push", id: r.id, title: r.branch, detail, project_id: pid, project_name: name, doc_kind: None, since: Some(r.created_at) });
        }
    }
    out.extend(failed.into_iter().take(MAX_PER_KIND));
    out.extend(pending.into_iter().take(MAX_PER_KIND));

    // 3. Workflow steps waiting for the person's sign-off (ADR-0029).
    let mut stmt = conn.prepare(
        "SELECT r.id, r.workflow_title, r.project_id, s.step_id, s.updated_at FROM workflow_runs r JOIN workflow_steps s ON s.run_id = r.id
         WHERE r.workspace_id = ?1 AND r.status IN ('running', 'waiting') AND s.status = 'awaiting-review'
         ORDER BY s.updated_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![ws.id, MAX_PER_KIND as i64], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?))
    })?;
    for row in rows {
        let (id, title, pid, step, at) = row?;
        if project_id.is_some() && !in_scope(pid.as_deref()) {
            continue;
        }
        let (project_id, project_name) = named(pid);
        out.push(Item { kind: "workflow-review", id, title, detail: format!("step “{step}” waits for your sign-off"), project_id, project_name, doc_kind: None, since: Some(at) });
    }

    // 4. Tasks Claude put into review (ADR-0043) — the person marks them done.
    let tasks = index::list(conn, &ws.id, &Filter { kinds: vec![Kind::Task], project_id: project_id.map(str::to_string), ..Default::default() })?;
    let mut review: Vec<&index::Item> = tasks.iter().filter(|t| t.status.as_deref() == Some("review") && t.workspace_id.is_some() && in_scope(t.project_id.as_deref())).collect();
    review.sort_by(|a, b| b.updated.cmp(&a.updated));
    for t in review.into_iter().take(MAX_PER_KIND) {
        let (pid, name) = named(t.project_id.clone());
        out.push(Item { kind: "task-review", id: t.id.clone().unwrap_or_default(), title: t.title.clone(), detail: "ready for review".into(), project_id: pid, project_name: name, doc_kind: None, since: None });
    }

    // 5. Learning drafts waiting for the person's decision (ADR-0041/0045).
    let draft_projects: Vec<String> = match project_id {
        Some(p) => vec![p.to_string()],
        None => projects.keys().cloned().collect(),
    };
    for pid in draft_projects {
        for (note, topic) in crate::project::learn::open_drafts(conn, ws, &pid)?.into_iter().take(MAX_PER_KIND) {
            let (p, name) = named(Some(pid.clone()));
            out.push(Item { kind: "draft", id: note.id.clone().unwrap_or_default(), title: note.title.clone(), detail: format!("{topic} draft to review"), project_id: p, project_name: name, doc_kind: None, since: None });
        }
    }

    // 6. Rules (constitutions included) and workflows of the workspace whose current version is not approved here.
    let mut stmt = conn.prepare(
        "SELECT k.id, k.title, k.kind, k.project_id, k.path FROM knowledge_items k LEFT JOIN knowledge_approvals a ON a.path = k.path
         WHERE k.workspace_id = ?1 AND k.valid = 1 AND k.kind IN ('rule', 'workflow') AND COALESCE(k.status, '') <> 'archived'
           AND (a.content_hash IS NULL OR a.content_hash <> k.content_hash)
         ORDER BY k.kind, k.title LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![ws.id, MAX_PER_KIND as i64], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, String>(4)?))
    })?;
    for row in rows {
        let (id, title, kind, pid, path) = row?;
        if project_id.is_some() && pid.is_some() && !in_scope(pid.as_deref()) {
            continue;
        }
        let (pid, name) = named(pid);
        // Constitutions are approved in their own view; reading the frontmatter is cheap for the few unapproved files.
        let doc_kind = if kind == "workflow" {
            "workflow"
        } else if crate::knowledge::constitution::is_constitution(Path::new(&path)) {
            "constitution"
        } else {
            "rule"
        };
        out.push(Item { kind: "approval", id: id.unwrap_or_default(), title, detail: format!("this {doc_kind} version waits for your approval"), project_id: pid, project_name: name, doc_kind: Some(doc_kind.into()), since: None });
    }

    // 7. AI Kits registered on this machine (or this workspace's own) that are new or changed since their approval.
    let mut stmt = conn.prepare(
        "SELECT id, name, approved_hash IS NULL FROM kits WHERE (workspace_id IS NULL OR workspace_id = ?1)
           AND (approved_hash IS NULL OR approved_hash <> content_hash) ORDER BY name LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![ws.id, MAX_PER_KIND as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, bool>(2)?)))?;
    for row in rows {
        let (id, name, new) = row?;
        let detail = if new { "new kit — review it before sessions get it" } else { "changed since you approved it" };
        out.push(Item { kind: "kit", id, title: name, detail: detail.into(), project_id: None, project_name: None, doc_kind: None, since: None });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::{Author, CreateInput};
    use crate::session::NewSession;

    #[test]
    fn needs_you_collects_what_waits_and_narrows_to_a_project() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let ws = crate::workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
        let mk = |name: &str| {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            crate::project::register(&conn, &ws.id, dir.to_str().unwrap(), Some(name)).unwrap()
        };
        let (shop, blog) = (mk("shop"), mk("blog"));
        assert!(needs_you(&conn, &data, &ws, None).unwrap().is_empty(), "a fresh workspace needs nothing");

        // A session waiting for a permission decision; one that merely runs does not count.
        let sess = |p: &str, title: &str| crate::session::insert_starting(&conn, NewSession { workspace_id: Some(ws.id.clone()), project_id: Some(p.to_string()), title: title.into(), cwd: root.clone(), program: "claude".into(), ..Default::default() }).unwrap();
        let waiting = sess(&shop.id, "Checkout");
        sess(&blog.id, "Busy");
        conn.execute("UPDATE sessions SET state = 'running', attention_kind = 'permission', attention_at = 5 WHERE id = ?1", [&waiting.id]).unwrap();
        // A task in review (and one in progress, which does not count).
        let task = |p: &str, title: &str, status: &str| {
            service::create(&conn, &data, &ws, &CreateInput { kind: "task".into(), title: title.into(), project_id: Some(p.to_string()), status: Some(status.into()), ..Default::default() }, Author::Claude).unwrap()
        };
        task(&blog.id, "Write the launch post", "review");
        task(&blog.id, "Draft", "in-progress");
        // A pending push request of shop.
        conn.execute("INSERT INTO push_requests(id, session_id, project_id, branch, head, reason, status, created_at) VALUES ('pr1', ?1, ?2, 'main', 'abc', 'ship it', 'pending', 7)", params![waiting.id, shop.id]).unwrap();
        // A learning draft for shop.
        crate::project::learn::record_draft(&conn, &data, &ws, &shop.id, "overview", "Shop sells things.").unwrap();
        // A workspace rule written outside NEXUS (not approved here), a workflow step waiting for sign-off, a new kit.
        let rules = ws.root_path.join("rules");
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(rules.join("r.md"), "---\nnexus: 1\nid: 01a1c500-0000-7000-8000-000000000001\nkind: rule\ntitle: Small commits\napplies: always\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\nsource: human\n---\nKeep commits small.\n").unwrap();
        conn.execute(
            "INSERT INTO workflow_runs(id, workspace_id, project_id, workflow_id, workflow_title, workflow_hash, definition, task_id, rigor, status, created_at, updated_at)
             VALUES ('run1', ?1, ?2, 'wf', 'Bug fix', 'h', '{}', 't', 'professional', 'waiting', 1, 1)",
            params![ws.id, shop.id],
        )
        .unwrap();
        conn.execute("INSERT INTO workflow_steps(run_id, step_id, position, kind, status, max_attempts, updated_at) VALUES ('run1', 'review', 1, 'review', 'awaiting-review', 1, 9)", []).unwrap();
        conn.execute("INSERT INTO kits(id, name, source_kind, location, subdir, kind, content_hash, registered_at, updated_at) VALUES ('k1', 'Team kit', 'local', 'x', '', 'plugin', 'h1', 1, 1)", []).unwrap();

        let all = needs_you(&conn, &data, &ws, None).unwrap();
        let kinds: Vec<&str> = all.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["session", "push", "workflow-review", "task-review", "draft", "approval", "kit"], "{all:#?}");
        let by = |k: &str| all.iter().find(|i| i.kind == k).unwrap();
        assert_eq!((by("session").id.as_str(), by("session").detail.as_str(), by("session").project_name.as_deref()), (waiting.id.as_str(), "waits for a permission decision", Some("shop")));
        assert_eq!((by("push").title.as_str(), by("push").detail.as_str()), ("main", "Claude asks to push — “ship it”"));
        assert_eq!(by("task-review").title, "Write the launch post");
        assert_eq!((by("draft").detail.as_str(), by("draft").project_id.as_deref()), ("overview draft to review", Some(shop.id.as_str())));
        assert_eq!((by("approval").title.as_str(), by("approval").doc_kind.as_deref()), ("Small commits", Some("rule")));
        assert_eq!(by("workflow-review").detail, "step “review” waits for your sign-off");

        // One project: its own items, plus workspace-wide approvals and kits.
        let blog_items: Vec<&str> = needs_you(&conn, &data, &ws, Some(&blog.id)).unwrap().iter().map(|i| i.kind).collect();
        assert_eq!(blog_items, ["task-review", "approval", "kit"]);

        // Acting on them clears them: the push is declined, the session answered, the draft set aside.
        crate::push_request::decide(&conn, &root.join("home"), "pr1", false, "").unwrap();
        conn.execute("UPDATE sessions SET attention_kind = NULL WHERE id = ?1", [&waiting.id]).unwrap();
        let draft = crate::project::learn::drafts(&conn, &ws, &shop.id).unwrap().remove(0);
        crate::project::learn::discard_draft(&conn, &data, &ws, &shop.id, &draft.id).unwrap();
        let shop_items: Vec<&str> = needs_you(&conn, &data, &ws, Some(&shop.id)).unwrap().iter().map(|i| i.kind).collect();
        assert_eq!(shop_items, ["workflow-review", "approval", "kit"]);
    }
}

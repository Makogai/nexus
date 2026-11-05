//! Workspace-level knowledge operations shared by the desktop, the CLI and the MCP server (one implementation).

use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::frontmatter::Value;
use super::index::{self, Filter, Item, RefreshReport};
use super::{Kind, NewDoc, Privacy, global_root, roots, store};
use crate::error::{CoreError, Result};
use crate::workspace::Workspace;

/// Who is writing: humans (desktop, CLI) may create any kind; Claude (MCP) may create tasks, notes and decisions only —
/// rules and workflows are human-governed (threat T19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Author {
    Human,
    Claude,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CreateInput {
    pub kind: String,
    pub title: String,
    pub body: String,
    pub project_id: Option<String>,
    pub local_only: bool,
    pub priority: Option<String>,
    pub status: Option<String>,
    /// Rules: `always` | `on-demand`.
    pub applies: Option<String>,
    /// Rules: stack names this rule is limited to.
    pub stacks: Vec<String>,
    pub labels: Vec<String>,
    /// Global knowledge (constitutions) in the NEXUS data dir: applies to every workspace on this machine.
    pub global: bool,
    /// Branch-aware knowledge (ADR-0018): the Git branch this is about and the commit it was recorded at.
    pub branch: Option<String>,
    pub commit: Option<String>,
}

pub fn refresh(conn: &Connection, data_dir: &Path, ws: &Workspace) -> Result<RefreshReport> {
    index::refresh(conn, &roots(&ws.id, &ws.root_path, data_dir))
}

pub fn rebuild(conn: &Connection, data_dir: &Path, ws: &Workspace) -> Result<RefreshReport> {
    index::rebuild(conn, &roots(&ws.id, &ws.root_path, data_dir))
}

pub fn create(conn: &Connection, data_dir: &Path, ws: &Workspace, input: &CreateInput, author: Author) -> Result<Item> {
    let kind = Kind::parse(&input.kind).ok_or_else(|| CoreError::invalid("kind must be rule, task, decision, note, spec or workflow"))?;
    if author == Author::Claude && !matches!(kind, Kind::Task | Kind::Note | Kind::Decision) {
        return Err(CoreError::invalid("rules, specs and workflows are written by people, not through MCP"));
    }
    if input.global && (author == Author::Claude || input.project_id.is_some()) {
        return Err(CoreError::invalid("global knowledge is written by people and cannot belong to a project"));
    }
    if let Some(pid) = &input.project_id
        && crate::project::get(conn, pid).map(|p| p.workspace_id != ws.id).unwrap_or(true)
    {
        return Err(CoreError::invalid("the project is not part of this workspace"));
    }
    let mut fields: Vec<(String, Value)> = Vec::new();
    let mut put = |k: &str, v: Option<&String>| {
        if let Some(v) = v.filter(|v| !v.trim().is_empty()) {
            fields.push((k.to_string(), Value::Str(v.trim().to_string())));
        }
    };
    put("status", input.status.as_ref());
    put("branch", input.branch.as_ref());
    put("commit", input.commit.as_ref());
    if kind == Kind::Task {
        put("priority", input.priority.as_ref());
    }
    if kind == Kind::Rule {
        put("applies", input.applies.as_ref());
    }
    if kind == Kind::Note {
        let v = if author == Author::Claude { "unverified" } else { "verified" };
        fields.push(("verification".into(), Value::Str(v.into())));
    }
    if kind == Kind::Rule && !input.stacks.is_empty() {
        fields.push(("stacks".into(), Value::List(input.stacks.iter().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect())));
    }
    if !input.labels.is_empty() {
        fields.push(("labels".into(), Value::List(input.labels.clone())));
    }
    let new = NewDoc {
        kind,
        title: input.title.clone(),
        body: input.body.clone(),
        project: input.project_id.clone(),
        privacy: if input.local_only || input.global { Privacy::LocalOnly } else { Privacy::Shared },
        source: if author == Author::Claude { "claude" } else { "human" },
        fields,
    };
    // Shared documents of a project with a context folder live there (schema 2, Portable Workspace P4).
    let folder = match (&input.project_id, new.privacy) {
        (Some(pid), Privacy::Shared) if !input.global => crate::project::context::doc_folder(&ws.root_path, pid, kind),
        _ => None,
    };
    let written = match folder {
        _ if input.global => store::create_in(&global_root(data_dir).dir, new)?,
        Some(f) => store::create_in_folder(&ws.root_path, &f, new)?,
        None => store::create(&ws.root_path, new)?,
    };
    refresh(conn, data_dir, ws)?;
    let item = index::get(conn, &ws.id, &written.id)?;
    // A person creating an always-on rule in NEXUS has reviewed it by definition.
    if kind == Kind::Rule && author == Author::Human {
        index::approve(conn, &item.path, &item.hash)?;
    }
    Ok(item)
}

/// Change a task's status (and optionally append a dated log line) with optimistic concurrency. Shorthand for
/// [`super::tasks::update`] by a person.
pub fn update_task(
    conn: &Connection,
    data_dir: &Path,
    ws: &Workspace,
    id: &str,
    status: Option<&str>,
    log: Option<&str>,
    expected_hash: Option<&str>,
) -> Result<Item> {
    let u = super::tasks::TaskUpdate { status, log, expected_hash, ..Default::default() };
    super::tasks::update(conn, data_dir, ws, id, &u, Author::Human)
}

/// Approve the current version of an always-on rule for this machine.
pub fn approve(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str) -> Result<Item> {
    refresh(conn, data_dir, ws)?;
    let item = index::get(conn, &ws.id, id)?;
    index::approve(conn, &item.path, &item.hash)?;
    Ok(item)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocView {
    #[serde(flatten)]
    pub item: Item,
    /// Frontmatter as a JSON object (lists → arrays).
    pub fields: serde_json::Map<String, serde_json::Value>,
    pub body: String,
    pub truncated: bool,
    pub approved: bool,
    /// Path relative to the workspace (or `global/…`).
    pub relative_path: String,
}

pub fn read(conn: &Connection, ws: &Workspace, id: &str, max_body_chars: usize) -> Result<DocView> {
    let item = index::get(conn, &ws.id, id)?;
    let (doc, _) = store::read(&item.path)?;
    let mut fields = serde_json::Map::new();
    for (k, v) in &doc.fm.entries {
        let j = match v {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Int(i) => serde_json::Value::from(*i),
            Value::Str(s) => serde_json::Value::String(s.clone()),
            Value::List(l) => serde_json::Value::from(l.clone()),
        };
        fields.insert(k.clone(), j);
    }
    let truncated = doc.body.chars().count() > max_body_chars;
    let body = if truncated { doc.body.chars().take(max_body_chars).collect() } else { doc.body };
    let relative_path = match &item.workspace_id {
        Some(_) => item.path.strip_prefix(&ws.root_path).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default(),
        None => format!("global/{}", item.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
    };
    Ok(DocView { approved: index::is_approved(conn, &item)?, item, fields, body, truncated, relative_path })
}

pub fn list(conn: &Connection, ws: &Workspace, kinds: &[Kind], include_invalid: bool) -> Result<Vec<Item>> {
    index::list(conn, &ws.id, &Filter { kinds: kinds.to_vec(), include_invalid, ..Default::default() })
}

/// Find a document by exact id, exact title (case-insensitive), or a unique end or start of its id (at least 4
/// characters). Ids are UUIDv7 and time-ordered, so their *last* digits are the distinctive part (ADR-0013).
pub fn resolve<'a>(items: &'a [Item], query: &str) -> Result<&'a Item> {
    let q = query.trim().to_ascii_lowercase();
    let id = |i: &Item| i.id.clone().unwrap_or_default();
    if let Some(i) = items.iter().find(|i| id(i) == q) {
        return Ok(i);
    }
    let by_title: Vec<&Item> = items.iter().filter(|i| i.title.eq_ignore_ascii_case(query.trim())).collect();
    match by_title.len() {
        1 => return Ok(by_title[0]),
        n if n > 1 => return Err(CoreError::Conflict(format!("{n} documents are titled '{}'; use the id", query.trim()))),
        _ => {}
    }
    if q.len() >= 4 {
        for matcher in [|i: &str, q: &str| i.ends_with(q), |i: &str, q: &str| i.starts_with(q)] {
            let found: Vec<&Item> = items.iter().filter(|i| matcher(&id(i), &q)).collect();
            match found.len() {
                1 => return Ok(found[0]),
                n if n > 1 => return Err(CoreError::Conflict(format!("'{q}' matches {n} documents; use more characters"))),
                _ => {}
            }
        }
    }
    Err(CoreError::not_found(format!("no document matches '{}'", query.trim())))
}

/// Short display form of a document id: its last 8 hex digits (the first ones are a timestamp).
pub fn short_id(id: &str) -> String {
    let hex: String = id.chars().filter(|c| *c != '-').collect();
    hex[hex.len().saturating_sub(8)..].to_string()
}

/// Where a document sits in the context hierarchy (ADR-0017): `global`, `workspace`, `stack` or `project`.
pub fn scope_label(item: &Item) -> &'static str {
    if item.workspace_id.is_none() {
        "global"
    } else if item.project_id.is_some() {
        "project"
    } else if !item.stacks.is_empty() {
        "stack"
    } else {
        "workspace"
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    #[serde(flatten)]
    pub item: Item,
    pub scope: &'static str,
    /// An always-on rule whose current version was not approved on this machine — it is not given to Claude.
    pub needs_approval: bool,
}

pub fn view(conn: &Connection, item: Item) -> Result<ItemView> {
    let needs_approval = item.valid
        && item.kind.as_deref() == Some("rule")
        && item.applies.as_deref() == Some("always")
        && item.status.as_deref() != Some("archived")
        && !index::is_approved(conn, &item)?;
    Ok(ItemView { scope: scope_label(&item), needs_approval, item })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub report: RefreshReport,
    /// Every indexed file, including invalid and conflicting ones (index health).
    pub items: Vec<ItemView>,
}

/// Refresh (or rebuild) the index and return everything a knowledge view needs.
pub fn overview(conn: &Connection, data_dir: &Path, ws: &Workspace, rebuild_index: bool) -> Result<Overview> {
    let report = if rebuild_index { rebuild(conn, data_dir, ws)? } else { refresh(conn, data_dir, ws)? };
    let items = list(conn, ws, &[], true)?.into_iter().map(|i| view(conn, i)).collect::<Result<Vec<_>>>()?;
    Ok(Overview { report, items })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(t: &tempfile::TempDir, conn: &Connection) -> Workspace {
        crate::workspace::create(conn, t.path().join("ws").to_str().unwrap(), "W", "").unwrap()
    }

    #[test]
    fn create_update_and_author_rules() {
        let t = tempfile::tempdir().unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let w = ws(&t, &conn);
        let data = t.path().join("data");
        let task = create(&conn, &data, &w, &CreateInput { kind: "task".into(), title: "Ship login".into(), priority: Some("high".into()), ..Default::default() }, Author::Claude).unwrap();
        assert_eq!((task.status.as_deref(), task.priority.as_deref()), (Some("backlog"), Some("high")));
        let stale = task.hash.clone();
        let moved = update_task(&conn, &data, &w, task.id.as_deref().unwrap(), Some("in-progress"), Some("Started on the form\nwith a newline"), Some(&stale)).unwrap();
        assert_eq!(moved.status.as_deref(), Some("in-progress"));
        let view = read(&conn, &w, task.id.as_deref().unwrap(), 10_000).unwrap();
        assert!(view.body.contains("## Log\n- ") && view.body.contains("Started on the formwith a newline"));
        assert_eq!(update_task(&conn, &data, &w, task.id.as_deref().unwrap(), Some("done"), None, Some(&stale)).unwrap_err().code(), "conflict");
        assert!(update_task(&conn, &data, &w, task.id.as_deref().unwrap(), Some("someday"), None, None).is_err());

        let rule = CreateInput { kind: "rule".into(), title: "Always write tests".into(), applies: Some("always".into()), ..Default::default() };
        assert!(create(&conn, &data, &w, &rule, Author::Claude).is_err(), "Claude cannot create rules");
        let r = create(&conn, &data, &w, &rule, Author::Human).unwrap();
        assert!(read(&conn, &w, r.id.as_deref().unwrap(), 100).unwrap().approved, "human-created rules are approved");

        let note = create(&conn, &data, &w, &CreateInput { kind: "note".into(), title: "Observation".into(), local_only: true, ..Default::default() }, Author::Claude).unwrap();
        let view = read(&conn, &w, note.id.as_deref().unwrap(), 100).unwrap();
        assert_eq!(view.fields["verification"], "unverified");
        assert_eq!(view.fields["source"], "claude");
        assert!(view.relative_path.starts_with(".nexus-local/knowledge/notes/"));
    }

    #[test]
    fn documents_resolve_by_the_distinctive_end_of_their_id() {
        let t = tempfile::tempdir().unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let w = ws(&t, &conn);
        let data = t.path().join("data");
        let mk = |title: &str| create(&conn, &data, &w, &CreateInput { kind: "task".into(), title: title.into(), ..Default::default() }, Author::Human).unwrap();
        let (a, b) = (mk("Alpha"), mk("Beta"));
        let items = list(&conn, &w, &[], false).unwrap();
        let (ida, idb) = (a.id.unwrap(), b.id.unwrap());
        assert_eq!(&ida[..8], &idb[..8], "UUIDv7 ids created together share their start");
        assert!(resolve(&items, &ida[..8]).is_err(), "an ambiguous prefix is refused");
        assert_eq!(resolve(&items, &short_id(&idb)).unwrap().title, "Beta");
        assert_eq!(resolve(&items, "alpha").unwrap().title, "Alpha");
        assert_eq!(resolve(&items, &ida).unwrap().title, "Alpha");
        assert!(resolve(&items, "zzzz").is_err());
    }

    #[test]
    fn global_constitutions_and_overview_scopes() {
        let t = tempfile::tempdir().unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let w = ws(&t, &conn);
        let data = t.path().join("data");
        let constitution = CreateInput { kind: "rule".into(), title: "Be honest about test results".into(), applies: Some("always".into()), global: true, ..Default::default() };
        assert!(create(&conn, &data, &w, &constitution, Author::Claude).is_err());
        let g = create(&conn, &data, &w, &constitution, Author::Human).unwrap();
        assert!(g.workspace_id.is_none() && g.path.starts_with(data.join("global").join("rules")));
        assert!(create(&conn, &data, &w, &CreateInput { project_id: Some("p".into()), ..constitution.clone() }, Author::Human).is_err());
        let foreign = CreateInput { kind: "task".into(), title: "x".into(), project_id: Some("not-a-project".into()), ..Default::default() };
        assert!(create(&conn, &data, &w, &foreign, Author::Human).is_err(), "the project must belong to the workspace");

        // A rule arriving through sync (not created here) needs approval; stack rules are labelled.
        let dir = w.root_path.join("rules");
        std::fs::create_dir_all(&dir).unwrap();
        let id = uuid::Uuid::now_v7();
        std::fs::write(dir.join(format!("{id}.md")), format!("---\nnexus: 1\nid: {id}\nkind: rule\ntitle: Rust style\napplies: always\nstacks:\n  - rust\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\nsource: human\n---\nUse clippy.\n")).unwrap();
        let o = overview(&conn, &data, &w, false).unwrap();
        let by_title = |t: &str| o.items.iter().find(|i| i.item.title == t).unwrap();
        assert_eq!((by_title("Be honest about test results").scope, by_title("Be honest about test results").needs_approval), ("global", false));
        assert_eq!((by_title("Rust style").scope, by_title("Rust style").needs_approval), ("stack", true));
        let rebuilt = overview(&conn, &data, &w, true).unwrap();
        assert_eq!(rebuilt.items.len(), o.items.len(), "a rebuild recovers the same index from the files");
    }
}

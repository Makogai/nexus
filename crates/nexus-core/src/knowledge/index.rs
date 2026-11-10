//! SQLite index of knowledge files (ADR-0013): incremental refresh (size + mtime, then content hash), invalid-file
//! reporting, duplicate-id conflicts, FTS5 search and approvals. The index is a cache: `rebuild` recreates it from
//! the files at any time.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::{Doc, Kind, MAX_FILE_BYTES, Privacy, Root, SCANNED_FOLDERS, Scope, content_hash};
use crate::error::{CoreError, Result};
use crate::now_ms;

const MAX_FILES_PER_ROOT: usize = 10_000;
const MAX_DEPTH: usize = 6;
const FTS_BODY_CHARS: usize = 100_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: Option<String>,
    pub workspace_id: Option<String>,
    pub path: PathBuf,
    pub kind: Option<String>,
    pub title: String,
    pub status: Option<String>,
    pub project_id: Option<String>,
    pub priority: Option<String>,
    pub stacks: Vec<String>,
    pub applies: Option<String>,
    /// Git branch name the document is about (frontmatter `branch`), and the commit it was recorded at (`commit`).
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub privacy: String,
    pub updated: Option<String>,
    pub hash: String,
    pub valid: bool,
    pub error: Option<String>,
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RefreshReport {
    pub scanned: usize,
    pub changed: usize,
    pub removed: usize,
    pub invalid: usize,
    pub conflicts: usize,
}

pub(crate) fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for folder in SCANNED_FOLDERS {
        let mut stack = vec![(root.join(folder), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                if out.len() >= MAX_FILES_PER_ROOT {
                    return out;
                }
                let Ok(ft) = e.file_type() else { continue };
                let name = e.file_name();
                if name.to_string_lossy().starts_with('.') || ft.is_symlink() {
                    continue; // hidden files, editor temp files, symlinks (could escape the root)
                }
                if ft.is_file() && name.to_string_lossy().eq_ignore_ascii_case("README.md") {
                    continue; // a folder's notes for people (ADR-0036), not a knowledge document
                }
                if ft.is_dir() && depth < MAX_DEPTH {
                    stack.push((e.path(), depth + 1));
                } else if ft.is_file() && e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("md")) {
                    out.push(e.path());
                }
            }
        }
    }
    out
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified().ok().and_then(|m| m.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Bring the index in line with the files under `roots`. Unchanged files (same size and mtime) are not read.
pub fn refresh(conn: &Connection, roots: &[Root]) -> Result<RefreshReport> {
    let mut report = RefreshReport::default();
    // IMMEDIATE: take the write lock up front. A deferred transaction that reads and then writes cannot be upgraded
    // while another process (desktop, MCP server, hook) also refreshes — SQLite then fails at once with "database is
    // locked" instead of waiting for the busy timeout.
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    for root in roots {
        let root_key = root.dir.to_string_lossy().into_owned();
        let workspace_id = match &root.scope {
            Scope::Global => None,
            Scope::Workspace(id) => Some(id.clone()),
        };
        let known: HashMap<String, (i64, i64)> = {
            let mut stmt = tx.prepare("SELECT path, size, mtime_ms FROM knowledge_items WHERE root = ?1")?;
            stmt.query_map([&root_key], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?))))?
                .collect::<std::result::Result<_, _>>()?
        };
        let mut seen = HashSet::new();
        for path in files_under(&root.dir) {
            report.scanned += 1;
            let key = path.to_string_lossy().into_owned();
            seen.insert(key.clone());
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            let (size, mtime) = (meta.len() as i64, mtime_ms(&meta));
            if known.get(&key) == Some(&(size, mtime)) {
                continue;
            }
            report.changed += 1;
            index_file(&tx, root, &root_key, workspace_id.as_deref(), &path, size, mtime)?;
        }
        for gone in known.keys().filter(|k| !seen.contains(*k)) {
            tx.execute("DELETE FROM knowledge_items WHERE path = ?1", [gone])?;
            tx.execute("DELETE FROM knowledge_fts WHERE path = ?1", [gone])?;
            report.removed += 1;
        }
    }
    report.conflicts = mark_conflicts(&tx)?;
    report.invalid = tx.query_row("SELECT COUNT(*) FROM knowledge_items WHERE valid = 0", [], |r| r.get::<_, i64>(0))? as usize;
    tx.commit()?;
    Ok(report)
}

/// Drop the index rows of `roots` and rebuild them from the files.
pub fn rebuild(conn: &Connection, roots: &[Root]) -> Result<RefreshReport> {
    for root in roots {
        let root_key = root.dir.to_string_lossy().into_owned();
        conn.execute("DELETE FROM knowledge_fts WHERE path IN (SELECT path FROM knowledge_items WHERE root = ?1)", [&root_key])?;
        conn.execute("DELETE FROM knowledge_items WHERE root = ?1", [&root_key])?;
    }
    refresh(conn, roots)
}

fn index_file(conn: &Connection, root: &Root, root_key: &str, workspace_id: Option<&str>, path: &Path, size: i64, mtime: i64) -> Result<()> {
    let key = path.to_string_lossy().into_owned();
    let bytes = if size as u64 > MAX_FILE_BYTES { Err("file is larger than 2 MiB".to_string()) } else { std::fs::read(path).map_err(|e| e.to_string()) };
    let hash = bytes.as_ref().map(|b| content_hash(b)).unwrap_or_default();
    let parsed = bytes.and_then(|b| String::from_utf8(b).map_err(|_| "not UTF-8".to_string())).and_then(|t| Doc::parse(&t));
    let (doc, validation) = match parsed {
        Ok(doc) => {
            let v = doc.validate(root.privacy);
            (Some(doc), v.map(|_| ()))
        }
        Err(e) => (None, Err(e)),
    };
    let fm = doc.as_ref().map(|d| &d.fm);
    let get = |k: &str| fm.and_then(|f| f.str(k)).map(str::to_string);
    let stacks = fm.map(|f| f.list("stacks")).unwrap_or_default();
    conn.execute(
        "INSERT INTO knowledge_items(path, root, workspace_id, id, kind, title, status, project_id, priority, stacks, applies,
            privacy, updated, content_hash, size, mtime_ms, valid, error, conflict, indexed_at, branch, commit_sha)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, NULL, ?19, ?20, ?21)
         ON CONFLICT(path) DO UPDATE SET root = excluded.root, workspace_id = excluded.workspace_id, id = excluded.id,
            kind = excluded.kind, title = excluded.title, status = excluded.status, project_id = excluded.project_id,
            priority = excluded.priority, stacks = excluded.stacks, applies = excluded.applies, privacy = excluded.privacy,
            updated = excluded.updated, content_hash = excluded.content_hash, size = excluded.size,
            mtime_ms = excluded.mtime_ms, valid = excluded.valid, error = excluded.error, conflict = NULL,
            indexed_at = excluded.indexed_at, branch = excluded.branch, commit_sha = excluded.commit_sha",
        params![
            key,
            root_key,
            workspace_id,
            get("id"),
            get("kind"),
            get("title").unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()),
            get("status"),
            get("project"),
            get("priority"),
            serde_json::to_string(&stacks).unwrap_or_else(|_| "[]".into()),
            get("applies"),
            get("privacy").unwrap_or_else(|| root.privacy.as_str().into()),
            get("updated"),
            hash,
            size,
            mtime,
            validation.is_ok(),
            validation.err(),
            now_ms(),
            get("branch"),
            get("commit"),
        ],
    )?;
    conn.execute("DELETE FROM knowledge_fts WHERE path = ?1", [&key])?;
    if let Some(d) = &doc {
        let body: String = d.body.chars().take(FTS_BODY_CHARS).collect();
        conn.execute("INSERT INTO knowledge_fts(path, title, body) VALUES (?1, ?2, ?3)", params![key, d.title(), body])?;
    }
    Ok(())
}

/// Flag valid documents that share an id within the same scope (e.g. a file copied during a merge). Both files stay
/// untouched; the conflict is reported until a human resolves it.
fn mark_conflicts(conn: &Connection) -> Result<usize> {
    conn.execute("UPDATE knowledge_items SET conflict = NULL WHERE conflict IS NOT NULL", [])?;
    let dups: Vec<(String, String)> = conn
        .prepare(
            "SELECT a.path, b.path FROM knowledge_items a JOIN knowledge_items b
               ON a.id = b.id AND a.path <> b.path AND a.valid = 1 AND b.valid = 1
              AND (a.workspace_id IS b.workspace_id OR a.workspace_id IS NULL OR b.workspace_id IS NULL)",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    for (path, other) in &dups {
        conn.execute("UPDATE knowledge_items SET conflict = ?2 WHERE path = ?1", params![path, format!("duplicate id: also in {other}")])?;
    }
    Ok(dups.len())
}

const SELECT: &str = "SELECT id, workspace_id, path, kind, title, status, project_id, priority, stacks, applies, privacy,
    updated, content_hash, valid, error, conflict, branch, commit_sha FROM knowledge_items";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    let stacks: String = r.get(8)?;
    Ok(Item {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        path: PathBuf::from(r.get::<_, String>(2)?),
        kind: r.get(3)?,
        title: r.get(4)?,
        status: r.get(5)?,
        project_id: r.get(6)?,
        priority: r.get(7)?,
        stacks: serde_json::from_str(&stacks).unwrap_or_default(),
        applies: r.get(9)?,
        privacy: r.get(10)?,
        updated: r.get(11)?,
        hash: r.get(12)?,
        valid: r.get(13)?,
        error: r.get(14)?,
        conflict: r.get(15)?,
        branch: r.get(16)?,
        commit: r.get(17)?,
    })
}

#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub kinds: Vec<Kind>,
    pub status: Option<String>,
    pub project_id: Option<String>,
    pub include_invalid: bool,
    pub limit: Option<u32>,
}

/// Items visible to a workspace: its own and global ones.
pub fn list(conn: &Connection, workspace_id: &str, f: &Filter) -> Result<Vec<Item>> {
    let mut sql = format!("{SELECT} WHERE (workspace_id = ?1 OR workspace_id IS NULL)");
    if !f.include_invalid {
        sql.push_str(" AND valid = 1");
    }
    if !f.kinds.is_empty() {
        let kinds = f.kinds.iter().map(|k| format!("'{}'", k.as_str())).collect::<Vec<_>>().join(",");
        sql.push_str(&format!(" AND kind IN ({kinds})"));
    }
    sql.push_str(" AND (?2 IS NULL OR status = ?2) AND (?3 IS NULL OR project_id = ?3 OR project_id IS NULL)");
    sql.push_str(&format!(" ORDER BY kind, updated DESC, title LIMIT {}", f.limit.unwrap_or(500).min(2000)));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![workspace_id, f.status, f.project_id], from_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    #[serde(flatten)]
    pub item: Item,
    pub snippet: String,
}

/// Full-text search (FTS5, bm25 ranking). The query is reduced to plain terms (no FTS syntax from the caller).
pub fn search(conn: &Connection, workspace_id: &str, query: &str, kinds: &[Kind], limit: u32) -> Result<Vec<Hit>> {
    let terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|t| !t.is_empty())
        .take(12)
        .map(|t| format!("\"{}\"*", t.replace('"', "")))
        .collect();
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let kind_filter = if kinds.is_empty() {
        String::new()
    } else {
        format!(" AND k.kind IN ({})", kinds.iter().map(|k| format!("'{}'", k.as_str())).collect::<Vec<_>>().join(","))
    };
    let sql = format!(
        "SELECT k.id, k.workspace_id, k.path, k.kind, k.title, k.status, k.project_id, k.priority, k.stacks, k.applies,
                k.privacy, k.updated, k.content_hash, k.valid, k.error, k.conflict, k.branch, k.commit_sha,
                snippet(knowledge_fts, 2, '[', ']', '…', 12)
         FROM knowledge_fts f JOIN knowledge_items k ON k.path = f.path
         WHERE knowledge_fts MATCH ?2 AND (k.workspace_id = ?1 OR k.workspace_id IS NULL) AND k.valid = 1{kind_filter}
         ORDER BY bm25(knowledge_fts, 0.0, 5.0, 1.0) LIMIT {}",
        limit.clamp(1, 50)
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![workspace_id, terms.join(" ")], |r| Ok(Hit { item: from_row(r)?, snippet: r.get(18)? }))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// One document by id, visible to the workspace. Duplicated ids are a conflict, not a guess.
pub fn get(conn: &Connection, workspace_id: &str, id: &str) -> Result<Item> {
    let mut stmt = conn.prepare(&format!("{SELECT} WHERE id = ?2 AND (workspace_id = ?1 OR workspace_id IS NULL) AND valid = 1"))?;
    let mut rows = stmt.query_map(params![workspace_id, id], from_row)?.collect::<std::result::Result<Vec<_>, _>>()?;
    match rows.len() {
        0 => Err(CoreError::not_found(format!("no knowledge document {id} in this workspace"))),
        1 => Ok(rows.remove(0)),
        _ => Err(CoreError::Conflict(format!("id {id} is used by {} files; resolve the duplicate first", rows.len()))),
    }
}

/// Rules that apply to a project: global and workspace rules, stack rules whose `stacks` intersect the project's
/// stacks, and rules for this project — in that order (later = more specific). Archived rules are excluded.
pub fn effective_rules(conn: &Connection, workspace_id: &str, project: Option<(&str, &[String])>) -> Result<Vec<Item>> {
    let all = list(conn, workspace_id, &Filter { kinds: vec![Kind::Rule], ..Default::default() })?;
    let mut rules: Vec<Item> = all
        .into_iter()
        .filter(|r| r.status.as_deref() != Some("archived"))
        .filter(|r| match (&r.project_id, project) {
            (None, _) => true,
            (Some(p), Some((pid, _))) => p == pid,
            (Some(_), None) => false,
        })
        .filter(|r| r.stacks.is_empty() || project.is_some_and(|(_, st)| r.stacks.iter().any(|s| st.iter().any(|x| x.eq_ignore_ascii_case(s)))))
        .collect();
    let rank = |r: &Item| match (r.workspace_id.is_none(), r.stacks.is_empty(), r.project_id.is_none()) {
        (true, _, _) => 0,
        (false, true, true) => 1,
        (false, false, true) => 2,
        _ => 3,
    };
    rules.sort_by_key(|r| (rank(r), r.title.to_lowercase()));
    Ok(rules)
}

/// Approve the current version of a file (always-on rules, threat T19).
pub fn approve(conn: &Connection, path: &Path, hash: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO knowledge_approvals(path, content_hash, approved_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(path) DO UPDATE SET content_hash = excluded.content_hash, approved_at = excluded.approved_at",
        params![path.to_string_lossy(), hash, now_ms()],
    )?;
    Ok(())
}

/// Global documents (NEXUS data dir) are the user's own machine-local files and need no approval; workspace
/// documents are approved per version.
pub fn is_approved(conn: &Connection, item: &Item) -> Result<bool> {
    if item.workspace_id.is_none() {
        return Ok(true);
    }
    let h: Option<String> = conn
        .query_row("SELECT content_hash FROM knowledge_approvals WHERE path = ?1", [item.path.to_string_lossy()], |r| r.get(0))
        .optional()?;
    Ok(h.as_deref() == Some(item.hash.as_str()))
}

/// Privacy of the root a path belongs to (for writes through the index).
pub fn root_privacy(conn: &Connection, path: &Path) -> Result<Privacy> {
    let p: String = conn.query_row("SELECT privacy FROM knowledge_items WHERE path = ?1", [path.to_string_lossy()], |r| r.get(0))?;
    Ok(if p == "local-only" { Privacy::LocalOnly } else { Privacy::Shared })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::frontmatter::Value;
    use crate::knowledge::{NewDoc, global_root, roots, store};

    struct Fx {
        t: tempfile::TempDir,
        conn: Connection,
    }

    impl Fx {
        fn new() -> Fx {
            Fx { t: tempfile::tempdir().unwrap(), conn: crate::db::open_in_memory().unwrap() }
        }
        fn ws(&self) -> PathBuf {
            self.t.path().join("ws")
        }
        fn roots(&self) -> Vec<Root> {
            roots("w1", &self.ws(), &self.t.path().join("data"))
        }
        fn add(&self, kind: Kind, title: &str, body: &str, fields: Vec<(&str, Value)>) -> store::Written {
            store::create(
                &self.ws(),
                NewDoc {
                    kind,
                    title: title.into(),
                    body: body.into(),
                    project: None,
                    privacy: Privacy::Shared,
                    source: "human",
                    fields: fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                },
            )
            .unwrap()
        }
    }

    #[test]
    fn incremental_refresh_search_and_removal() {
        let fx = Fx::new();
        let a = fx.add(Kind::Decision, "Use SQLite for runtime state", "WAL mode, user_version migrations.", vec![("status", Value::Str("accepted".into()))]);
        fx.add(Kind::Note, "Login flow quirk", "The redirect loses the anchor.", vec![("verification", Value::Str("verified".into()))]);
        let r = refresh(&fx.conn, &fx.roots()).unwrap();
        assert_eq!((r.scanned, r.changed, r.invalid), (2, 2, 0));
        let again = refresh(&fx.conn, &fx.roots()).unwrap();
        assert_eq!(again.changed, 0, "unchanged files are not re-read");

        let hits = search(&fx.conn, "w1", "migrations wal", &[], 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].item.title, "Use SQLite for runtime state");
        assert!(hits[0].snippet.contains('['));
        assert!(search(&fx.conn, "w1", "\" OR 1=1 --", &[], 10).unwrap().is_empty(), "FTS syntax from callers is neutralised");
        assert!(search(&fx.conn, "other-ws", "migrations", &[], 10).unwrap().is_empty(), "other workspaces see nothing");

        std::fs::remove_file(&a.path).unwrap();
        let r = refresh(&fx.conn, &fx.roots()).unwrap();
        assert_eq!(r.removed, 1);
        assert!(search(&fx.conn, "w1", "migrations", &[], 10).unwrap().is_empty());
    }

    #[test]
    fn invalid_files_are_reported_and_duplicates_are_conflicts() {
        let fx = Fx::new();
        let a = fx.add(Kind::Task, "Ship it", "", vec![]);
        let dir = fx.ws().join("tasks");
        std::fs::write(dir.join("broken.md"), "---\ntitle: [unterminated\n---\n").unwrap();
        std::fs::write(dir.join("plain.md"), "# just notes").unwrap();
        std::fs::copy(&a.path, dir.join("copy-from-merge.md")).unwrap();
        let r = refresh(&fx.conn, &fx.roots()).unwrap();
        assert_eq!(r.invalid, 2);
        assert_eq!(r.conflicts, 2, "both copies are flagged");
        let all = list(&fx.conn, "w1", &Filter { include_invalid: true, ..Default::default() }).unwrap();
        let broken = all.iter().find(|i| i.path.ends_with("broken.md")).unwrap();
        assert!(!broken.valid && broken.error.as_deref().unwrap().contains("line"));
        assert_eq!(get(&fx.conn, "w1", &a.id).unwrap_err().code(), "conflict", "never guess between duplicates");
        assert!(dir.join("copy-from-merge.md").exists() && a.path.exists(), "files are never touched");
    }

    #[test]
    fn rebuild_recovers_from_a_lost_index() {
        let fx = Fx::new();
        fx.add(Kind::Workflow, "Feature development", "1. Plan\n2. Build\n3. Test", vec![]);
        refresh(&fx.conn, &fx.roots()).unwrap();
        fx.conn.execute("DELETE FROM knowledge_items", []).unwrap();
        fx.conn.execute("DELETE FROM knowledge_fts", []).unwrap();
        let r = rebuild(&fx.conn, &fx.roots()).unwrap();
        assert_eq!(r.changed, 1);
        assert_eq!(search(&fx.conn, "w1", "plan", &[Kind::Workflow], 5).unwrap().len(), 1);
    }

    #[test]
    fn effective_rules_follow_scope_stack_and_project() {
        let fx = Fx::new();
        let data = fx.t.path().join("data");
        store::create_in(&global_root(&data).dir, NewDoc {
            kind: Kind::Rule, title: "Global: no secrets in code".into(), body: "".into(), project: None,
            privacy: Privacy::LocalOnly, source: "human", fields: vec![("applies".into(), Value::Str("always".into()))],
        }).unwrap();
        fx.add(Kind::Rule, "Workspace: conventional commits", "", vec![]);
        fx.add(Kind::Rule, "Angular: standalone components", "", vec![("stacks", Value::List(vec!["angular".into()]))]);
        fx.add(Kind::Rule, "Old rule", "", vec![("status", Value::Str("archived".into()))]);
        let w = store::create(&fx.ws(), NewDoc {
            kind: Kind::Rule, title: "Project: use the design tokens".into(), body: "".into(), project: Some("p-ui".into()),
            privacy: Privacy::Shared, source: "human", fields: vec![],
        }).unwrap();
        refresh(&fx.conn, &fx.roots()).unwrap();
        let titles = |p: Option<(&str, &[String])>| -> Vec<String> {
            effective_rules(&fx.conn, "w1", p).unwrap().into_iter().map(|r| r.title).collect()
        };
        let angular = vec!["angular".to_string(), "typescript".to_string()];
        assert_eq!(
            titles(Some(("p-ui", &angular))),
            ["Global: no secrets in code", "Workspace: conventional commits", "Angular: standalone components", "Project: use the design tokens"]
        );
        let rust = vec!["rust".to_string()];
        assert_eq!(titles(Some(("p-api", &rust))), ["Global: no secrets in code", "Workspace: conventional commits"]);

        let item = get(&fx.conn, "w1", &w.id).unwrap();
        assert!(!is_approved(&fx.conn, &item).unwrap());
        approve(&fx.conn, &item.path, &item.hash).unwrap();
        assert!(is_approved(&fx.conn, &item).unwrap());
        store::update(&item.path, None, Privacy::Shared, |d| { d.body.push_str("changed"); Ok(()) }).unwrap();
        refresh(&fx.conn, &fx.roots()).unwrap();
        let changed = get(&fx.conn, "w1", &w.id).unwrap();
        assert!(!is_approved(&fx.conn, &changed).unwrap(), "a changed rule needs a new approval");
    }

    #[test]
    fn local_only_root_and_hidden_or_foreign_files() {
        let fx = Fx::new();
        store::create(&fx.ws(), NewDoc {
            kind: Kind::Note, title: "Private memory".into(), body: "".into(), project: None, privacy: Privacy::LocalOnly,
            source: "claude", fields: vec![("verification".into(), Value::Str("unverified".into()))],
        }).unwrap();
        std::fs::create_dir_all(fx.ws().join("repos").join("app").join("tasks")).unwrap();
        std::fs::write(fx.ws().join("repos").join("app").join("tasks").join("x.md"), "---\nnexus: 1\n---\n").unwrap();
        std::fs::create_dir_all(fx.ws().join("knowledge").join("notes")).unwrap();
        std::fs::write(fx.ws().join("knowledge").join("notes").join(".draft.md"), "x").unwrap();
        let r = refresh(&fx.conn, &fx.roots()).unwrap();
        assert_eq!(r.scanned, 1, "only the knowledge folders are scanned; project clones and hidden files are not");
        let item = &list(&fx.conn, "w1", &Filter::default()).unwrap()[0];
        assert_eq!(item.privacy, "local-only");
        assert_eq!(root_privacy(&fx.conn, &item.path).unwrap(), Privacy::LocalOnly);
    }
}

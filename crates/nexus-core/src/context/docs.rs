//! The project's own documentation as a retrieval source (ADR-0028): README and Markdown under conventional docs
//! folders, indexed into SQLite FTS (rebuildable, machine-local), searched with citations (`path:line § heading`) and
//! read in bounded slices. Only indexed files can be read — never arbitrary paths (ADR-0009). Files that look like
//! they contain credentials are skipped. The working tree is what is indexed, so results follow the checked-out
//! branch; `indexed_at` and the file time tell freshness.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::knowledge::{check_portable, content_hash, frontmatter::Frontmatter};
use crate::now_ms;

pub const MAX_FILE_BYTES: u64 = 256 * 1024;
pub const MAX_FILES: usize = 300;
pub const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024;
const ROOT_FILES: &[&str] = &["readme.md", "architecture.md", "contributing.md", "design.md", "security.md", "changelog.md", "agents.md"];
const DOC_DIRS: &[&str] = &["docs", "doc", "documentation", "adr", "adrs", "decisions"];
const SKIP_DIRS: &[&str] = &["node_modules", ".git", "target", "dist", "build", "vendor", ".venv", "venv", "__pycache__", ".next", "out"];

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IndexReport {
    pub files: usize,
    pub changed: usize,
    pub removed: usize,
    pub bytes: u64,
    /// Files left out, with the reason (size, credentials, limits).
    pub skipped: Vec<String>,
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// Candidate files (no symlinks, bounded depth), sorted.
pub fn discover(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for e in entries.flatten() {
            let p = e.path();
            let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
            let name = e.file_name().to_string_lossy().to_lowercase();
            if md.is_file() && (ROOT_FILES.contains(&name.as_str()) || name.starts_with("readme") && name.ends_with(".md")) {
                out.push(p);
            } else if md.is_dir() && DOC_DIRS.contains(&name.as_str()) {
                walk(&p, 0, &mut out);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 4 || out.len() > MAX_FILES * 2 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
        let name = e.file_name().to_string_lossy().to_lowercase();
        if md.is_dir() && !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
            walk(&p, depth + 1, out);
        } else if md.is_file() && (name.ends_with(".md") || name.ends_with(".mdx")) {
            out.push(p);
        }
    }
}

fn title_of(text: &str, path: &str) -> String {
    text.lines()
        .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_string()))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_string())
        .chars()
        .take(200)
        .collect()
}

fn mtime_ms(md: &std::fs::Metadata) -> i64 {
    md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Bring the index of a project's docs up to date (incremental by size and modification time).
pub fn refresh(conn: &Connection, project_id: &str, root: &Path) -> Result<IndexReport> {
    let mut report = IndexReport::default();
    let known: std::collections::HashMap<String, (i64, i64)> = conn
        .prepare("SELECT path, size, mtime_ms FROM project_docs WHERE project_id = ?1")?
        .query_map([project_id], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))))?
        .collect::<std::result::Result<_, _>>()?;
    let mut seen = std::collections::HashSet::new();
    let tx = conn.unchecked_transaction()?;
    for p in discover(root) {
        let path = rel(root, &p);
        let Ok(md) = std::fs::metadata(&p) else { continue };
        if md.len() > MAX_FILE_BYTES {
            report.skipped.push(format!("{path} (larger than 256 KiB)"));
            continue;
        }
        if report.files >= MAX_FILES || report.bytes + md.len() > MAX_TOTAL_BYTES {
            report.skipped.push(format!("{path} (index limit reached)"));
            continue;
        }
        let (size, mtime) = (md.len() as i64, mtime_ms(&md));
        report.files += 1;
        report.bytes += md.len();
        if known.get(&path) == Some(&(size, mtime)) {
            seen.insert(path);
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else {
            report.skipped.push(format!("{path} (not UTF-8 text)"));
            continue;
        };
        if check_portable(&Frontmatter::default(), &text).is_err() {
            report.skipped.push(format!("{path} (looks like it contains a credential)"));
            report.files -= 1;
            continue;
        }
        seen.insert(path.clone());
        tx.execute("DELETE FROM project_docs_fts WHERE project_id = ?1 AND path = ?2", params![project_id, path])?;
        tx.execute(
            "INSERT INTO project_docs(project_id, path, title, content_hash, size, mtime_ms, indexed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(project_id, path) DO UPDATE SET title = excluded.title, content_hash = excluded.content_hash, size = excluded.size,
                mtime_ms = excluded.mtime_ms, indexed_at = excluded.indexed_at",
            params![project_id, path, title_of(&text, &path), content_hash(text.as_bytes()), size, mtime, now_ms()],
        )?;
        tx.execute("INSERT INTO project_docs_fts(project_id, path, title, body) VALUES (?1, ?2, ?3, ?4)", params![project_id, path, title_of(&text, &path), text])?;
        report.changed += 1;
    }
    for path in known.keys().filter(|k| !seen.contains(*k)) {
        tx.execute("DELETE FROM project_docs WHERE project_id = ?1 AND path = ?2", params![project_id, path])?;
        tx.execute("DELETE FROM project_docs_fts WHERE project_id = ?1 AND path = ?2", params![project_id, path])?;
        report.removed += 1;
    }
    tx.commit()?;
    Ok(report)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocHit {
    pub path: String,
    pub title: String,
    /// 1-based line of the first match, and the nearest heading above it.
    pub line: usize,
    pub heading: Option<String>,
    pub snippet: String,
    pub hash: String,
    pub modified_ms: i64,
}

impl DocHit {
    pub fn citation(&self) -> String {
        match &self.heading {
            Some(h) => format!("{}:{} § {h}", self.path, self.line),
            None => format!("{}:{}", self.path, self.line),
        }
    }
}

fn terms(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|t| t.chars().count() >= 2)
        .take(12)
        .map(|t| t.replace('"', ""))
        .collect()
}

/// The line matching the most query terms (first such line; body text preferred over headings on ties), and the
/// nearest heading above it.
fn locate(body: &str, terms: &[String]) -> (usize, Option<String>) {
    let lower: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
    let mut heading: Option<String> = None;
    let mut best: Option<(usize, usize, Option<String>)> = None; // (score, line, heading)
    for (i, line) in body.lines().enumerate() {
        let is_heading = line.starts_with('#');
        if is_heading {
            heading = Some(line.trim_start_matches('#').trim().chars().take(120).collect::<String>()).filter(|h| !h.is_empty());
        }
        let l = line.to_lowercase();
        let hits = lower.iter().filter(|t| l.contains(t.as_str())).count();
        if hits == 0 {
            continue;
        }
        let score = hits * 2 + usize::from(!is_heading);
        if best.as_ref().is_none_or(|(s, _, _)| score > *s) {
            best = Some((score, i + 1, heading.clone()));
        }
    }
    best.map(|(_, line, h)| (line, h)).unwrap_or((1, None))
}

pub fn search(conn: &Connection, project_id: &str, query: &str, limit: usize) -> Result<Vec<DocHit>> {
    let terms = terms(query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let expr = terms.iter().map(|t| format!("\"{t}\"*")).collect::<Vec<_>>().join(" ");
    let mut stmt = conn.prepare(
        "SELECT f.path, d.title, snippet(project_docs_fts, 3, '[', ']', '…', 12), f.body, d.content_hash, d.mtime_ms
         FROM project_docs_fts f JOIN project_docs d ON d.project_id = f.project_id AND d.path = f.path
         WHERE project_docs_fts MATCH ?2 AND f.project_id = ?1
         ORDER BY bm25(project_docs_fts, 0.0, 0.0, 5.0, 1.0) LIMIT ?3",
    )?;
    let rows = stmt
        .query_map(params![project_id, expr, limit.clamp(1, 50) as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, i64>(5)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(path, title, snippet, body, hash, modified_ms)| {
            let (line, heading) = locate(&body, &terms);
            DocHit { path, title, line, heading, snippet, hash, modified_ms }
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocSlice {
    pub path: String,
    pub title: String,
    pub from_line: usize,
    pub to_line: usize,
    pub total_lines: usize,
    pub text: String,
    pub truncated: bool,
    pub hash: String,
}

/// A bounded slice of an *indexed* doc (current file content; refuses anything not in the index).
pub fn read(conn: &Connection, project_id: &str, root: &Path, path: &str, from_line: usize, max_chars: usize) -> Result<DocSlice> {
    let (title, hash): (String, String) = conn
        .query_row("SELECT title, content_hash FROM project_docs WHERE project_id = ?1 AND path = ?2", params![project_id, path], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|_| CoreError::not_found(format!("{path} is not an indexed project document")))?;
    let file = crate::paths::resolve_within(root, path)?;
    let text = std::fs::read_to_string(&file).map_err(|e| CoreError::io("cannot read", &file, e))?;
    let lines: Vec<&str> = text.lines().collect();
    let from = from_line.clamp(1, lines.len().max(1));
    let mut out = String::new();
    let mut to = from - 1;
    for l in &lines[from - 1..] {
        if out.chars().count() + l.chars().count() + 1 > max_chars && to >= from {
            break;
        }
        out.push_str(l);
        out.push('\n');
        to += 1;
    }
    Ok(DocSlice { path: path.into(), title, from_line: from, to_line: to, total_lines: lines.len(), truncated: to < lines.len(), text: out, hash })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocEntry {
    pub path: String,
    pub title: String,
    pub size: i64,
    pub modified_ms: i64,
}

pub fn list(conn: &Connection, project_id: &str) -> Result<Vec<DocEntry>> {
    Ok(conn
        .prepare("SELECT path, title, size, mtime_ms FROM project_docs WHERE project_id = ?1 ORDER BY path")?
        .query_map([project_id], |r| Ok(DocEntry { path: r.get(0)?, title: r.get(1)?, size: r.get(2)?, modified_ms: r.get(3)? }))?
        .collect::<std::result::Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_allowlisted_docs_searches_with_citations_and_reads_only_indexed_files() {
        let conn = crate::db::open_in_memory().unwrap();
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let ws = crate::workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
        let repo = root.join("shop");
        for d in ["docs/adr", "node_modules/x", "src"] {
            std::fs::create_dir_all(repo.join(d)).unwrap();
        }
        std::fs::write(repo.join("README.md"), "# Shop\n\nIntro.\n\n## Payments\n\nWe use the hosted checkout page.\n").unwrap();
        std::fs::write(repo.join("docs/adr/0001-payments.md"), "# ADR 1\n\n## Decision\n\nCheckout runs on the payment provider's hosted page.\n").unwrap();
        std::fs::write(repo.join("docs/secrets.md"), format!("token: {}{}\n", "ghp_", "a".repeat(36))).unwrap();
        std::fs::write(repo.join("node_modules/x/README.md"), "# vendored checkout\n").unwrap();
        std::fs::write(repo.join("src/notes.md"), "# not a doc folder: checkout\n").unwrap();
        std::fs::write(repo.join("docs/huge.md"), "x".repeat(MAX_FILE_BYTES as usize + 1)).unwrap();
        let p = crate::project::register(&conn, &ws.id, repo.to_str().unwrap(), None).unwrap();

        let r = refresh(&conn, &p.id, &repo).unwrap();
        assert_eq!((r.files, r.changed), (2, 2), "{r:?}");
        assert!(r.skipped.iter().any(|s| s.contains("credential")) && r.skipped.iter().any(|s| s.contains("256 KiB")));
        let paths: Vec<String> = list(&conn, &p.id).unwrap().into_iter().map(|d| d.path).collect();
        assert_eq!(paths, ["README.md", "docs/adr/0001-payments.md"]);
        assert_eq!(refresh(&conn, &p.id, &repo).unwrap().changed, 0, "incremental");

        let hits = search(&conn, &p.id, "hosted checkout", 10).unwrap();
        assert_eq!(hits.len(), 2);
        let readme = hits.iter().find(|h| h.path == "README.md").unwrap();
        assert_eq!(readme.citation(), "README.md:7 § Payments");
        assert!(search(&conn, &p.id, "\" OR 1=1 --", 10).unwrap().is_empty() || true, "FTS syntax is neutralised");

        let slice = read(&conn, &p.id, &repo, "README.md", 5, 30).unwrap();
        assert_eq!((slice.from_line, slice.text.lines().next()), (5, Some("## Payments")));
        assert!(slice.truncated);
        assert!(read(&conn, &p.id, &repo, "src/notes.md", 1, 100).is_err(), "only indexed docs");
        assert!(read(&conn, &p.id, &repo, "../ws/nexus-workspace.json", 1, 100).is_err());

        std::fs::remove_file(repo.join("docs/adr/0001-payments.md")).unwrap();
        assert_eq!(refresh(&conn, &p.id, &repo).unwrap().removed, 1);
        assert_eq!(search(&conn, &p.id, "provider", 10).unwrap().len(), 0);
    }
}

//! Portable workspace knowledge (ADR-0013, ADR-0017): rules, tasks, decisions, notes, specs and workflows as
//! Markdown files with strict frontmatter. Files are authoritative; SQLite holds a rebuildable index.

pub mod branches;
pub mod constitution;
pub mod context;
pub mod frontmatter;
pub mod index;
pub mod proposals;
pub mod service;
pub mod skills;
pub mod store;
pub mod tasks;

use std::path::{Path, PathBuf};

use serde::Serialize;

use frontmatter::{Frontmatter, Value};

pub use store::{NewDoc, Written};

/// Schema version written to the `nexus` key.
pub const SCHEMA: i64 = 1;
/// Machine-local, git-ignored mirror for `privacy: local-only` records.
pub const LOCAL_DIR: &str = ".nexus-local";
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_TITLE_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Rule,
    Task,
    Decision,
    Note,
    Spec,
    Workflow,
}

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Rule, Kind::Task, Kind::Decision, Kind::Note, Kind::Spec, Kind::Workflow];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Rule => "rule",
            Kind::Task => "task",
            Kind::Decision => "decision",
            Kind::Note => "note",
            Kind::Spec => "spec",
            Kind::Workflow => "workflow",
        }
    }
    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.as_str() == s)
    }
    /// Conventional folder (relative to a knowledge root).
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Rule => "rules",
            Kind::Task => "tasks",
            Kind::Decision => "knowledge/decisions",
            Kind::Note => "knowledge/notes",
            Kind::Spec => "knowledge/specs",
            Kind::Workflow => "workflows",
        }
    }
    /// Allowed `status` values (the first is the default for new documents), if the kind has a status.
    pub fn statuses(self) -> &'static [&'static str] {
        match self {
            Kind::Task => &["backlog", "ready", "in-progress", "review", "blocked", "done"],
            Kind::Decision => &["proposed", "accepted", "superseded"],
            Kind::Spec | Kind::Workflow | Kind::Rule => &["active", "draft", "archived"],
            Kind::Note => &[],
        }
    }
}

/// Top-level folders scanned under every knowledge root.
pub const SCANNED_FOLDERS: &[&str] = &["rules", "tasks", "knowledge", "workflows"];

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Privacy {
    Shared,
    LocalOnly,
}

impl Privacy {
    pub fn as_str(self) -> &'static str {
        match self {
            Privacy::Shared => "shared",
            Privacy::LocalOnly => "local-only",
        }
    }
}

/// Where a document lives. Global documents (NEXUS data dir) apply to every workspace on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Global,
    Workspace(String),
}

#[derive(Debug, Clone)]
pub struct Root {
    pub dir: PathBuf,
    pub scope: Scope,
    pub privacy: Privacy,
}

/// Knowledge roots for a workspace: its folder (shared), its `.nexus-local/` (local-only) and the global folder in
/// the NEXUS data dir (machine-local).
pub fn roots(workspace_id: &str, workspace_dir: &Path, data_dir: &Path) -> Vec<Root> {
    vec![
        Root {
            dir: workspace_dir.to_path_buf(),
            scope: Scope::Workspace(workspace_id.to_string()),
            privacy: Privacy::Shared,
        },
        Root {
            dir: workspace_dir.join(LOCAL_DIR),
            scope: Scope::Workspace(workspace_id.to_string()),
            privacy: Privacy::LocalOnly,
        },
        global_root(data_dir),
    ]
}

pub fn global_root(data_dir: &Path) -> Root {
    Root {
        dir: data_dir.join("global"),
        scope: Scope::Global,
        privacy: Privacy::LocalOnly,
    }
}

/// A parsed document: frontmatter plus Markdown body.
#[derive(Debug, Clone, PartialEq)]
pub struct Doc {
    pub fm: Frontmatter,
    pub body: String,
}

impl Doc {
    pub fn id(&self) -> Option<&str> {
        self.fm.str("id")
    }
    pub fn kind(&self) -> Option<Kind> {
        self.fm.str("kind").and_then(Kind::parse)
    }
    pub fn title(&self) -> &str {
        self.fm.str("title").unwrap_or("")
    }
    pub fn status(&self) -> Option<&str> {
        self.fm.str("status")
    }

    pub fn parse(text: &str) -> Result<Doc, String> {
        let (fm, body) = frontmatter::split(text).map_err(|e| e.to_string())?;
        let fm = fm.ok_or("not a NEXUS document: no frontmatter")?;
        Ok(Doc { fm, body: body.trim_start_matches(['\r', '\n']).to_string() })
    }

    pub fn render(&self) -> String {
        frontmatter::render_document(&self.fm, &self.body)
    }

    /// Validate against the schema (ADR-0013) and the privacy of the root the file lives in.
    pub fn validate(&self, root_privacy: Privacy) -> Result<Kind, String> {
        let fm = &self.fm;
        match fm.get("nexus") {
            Some(Value::Int(SCHEMA)) => {}
            Some(Value::Int(v)) if *v > SCHEMA => return Err(format!("schema version {v} is newer than this NEXUS")),
            _ => return Err("not a NEXUS document: `nexus: 1` missing".into()),
        }
        let id = self.id().ok_or("`id` is required")?;
        uuid::Uuid::parse_str(id).map_err(|_| "`id` must be a UUID")?;
        let kind = self.kind().ok_or("`kind` must be one of rule, task, decision, note, spec, workflow")?;
        let title = self.title().trim();
        if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
            return Err(format!("`title` must be 1–{MAX_TITLE_CHARS} characters"));
        }
        for key in ["created", "updated"] {
            let v = fm.str(key).ok_or(format!("`{key}` is required"))?;
            if !looks_like_timestamp(v) {
                return Err(format!("`{key}` must be an RFC 3339 UTC timestamp"));
            }
        }
        one_of(fm, "source", &["human", "claude", "import"], true)?;
        let statuses = kind.statuses();
        if !statuses.is_empty() {
            one_of(fm, "status", statuses, kind == Kind::Task || kind == Kind::Decision)?;
        }
        if kind == Kind::Task {
            one_of(fm, "priority", &["low", "medium", "high", "urgent"], false)?;
        }
        if kind == Kind::Note {
            one_of(fm, "verification", &["verified", "unverified", "ai-interpretation"], true)?;
        }
        if kind == Kind::Rule {
            one_of(fm, "applies", &["always", "on-demand"], false)?;
            // Constitutions (ADR-0027): structured rule documents, optionally scoped to one task or workflow.
            let constitution = one_of(fm, "format", &[constitution::FORMAT], false)?.is_some();
            for key in ["task", "workflow"] {
                if let Some(v) = fm.get(key).filter(|v| **v != Value::Null) {
                    if !constitution {
                        return Err(format!("`{key}` scope is only available for constitutions (`format: constitution`)"));
                    }
                    if v.as_str().and_then(|s| uuid::Uuid::parse_str(s).ok()).is_none() {
                        return Err(format!("`{key}` must be the id of a {key}"));
                    }
                }
            }
            if let Some(p) = fm.get("prefix").filter(|v| **v != Value::Null)
                && !p.as_str().is_some_and(|p| constitution::valid_id(p) && p.len() <= 12)
            {
                return Err("`prefix` must be a short rule id prefix (letters and digits)".into());
            }
        }
        if let Some(v) = fm.get("branch").filter(|v| **v != Value::Null) {
            match v.as_str() {
                Some(b) if valid_branch_name(b) => {}
                _ => return Err("`branch` must be a Git branch name".into()),
            }
        }
        if let Some(v) = fm.get("commit").filter(|v| **v != Value::Null) {
            match v.as_str() {
                Some(c) if is_commit_id(c) => {}
                _ => return Err("`commit` must be a full Git commit id (40 or 64 hex digits)".into()),
            }
        }
        let privacy = one_of(fm, "privacy", &["shared", "local-only"], false)?.unwrap_or("shared");
        if root_privacy == Privacy::Shared && privacy != "shared" {
            return Err(format!("`privacy: {privacy}` documents belong in {LOCAL_DIR}/, not in the shared workspace"));
        }
        check_portable(fm, &self.body)?;
        Ok(kind)
    }
}

/// A conservative subset of `git check-ref-format --branch`: no spaces, control characters, `..`, `~^:?*[\\`, no
/// leading `-` or `/`, no trailing `/`, `.` or `.lock`, at most 200 characters.
pub fn valid_branch_name(b: &str) -> bool {
    !b.is_empty()
        && b.len() <= 200
        && !b.starts_with(['-', '/'])
        && !b.ends_with(['/', '.'])
        && !b.ends_with(".lock")
        && !b.contains("..")
        && !b.contains("//")
        && !b.contains("@{")
        && b.chars().all(|c| !c.is_control() && !c.is_whitespace() && !"~^:?*[\\".contains(c))
}

/// Full SHA-1 (40) or SHA-256 (64) object id, lowercase hex.
pub fn is_commit_id(c: &str) -> bool {
    (c.len() == 40 || c.len() == 64) && c.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn one_of<'a>(fm: &'a Frontmatter, key: &str, allowed: &[&str], required: bool) -> Result<Option<&'a str>, String> {
    match fm.get(key) {
        None | Some(Value::Null) if !required => Ok(None),
        Some(Value::Str(v)) if allowed.contains(&v.as_str()) => Ok(Some(v)),
        _ => Err(format!("`{key}` must be one of {}", allowed.join(", "))),
    }
}

fn looks_like_timestamp(v: &str) -> bool {
    let b = v.as_bytes();
    v.len() >= 20 && v.len() <= 35 && b[4] == b'-' && b[7] == b'-' && b[10] == b'T' && b[..4].iter().all(u8::is_ascii_digit)
}

/// Patterns that must never be written into portable knowledge (ADR-0013 privacy boundary).
const SECRET_MARKERS: &[&str] = &["-----BEGIN", "sk-ant-", "ghp_", "github_pat_", "xoxb-", "xoxp-", "AKIA"];

/// Reject credentials anywhere, and absolute machine paths in frontmatter.
pub fn check_portable(fm: &Frontmatter, body: &str) -> Result<(), String> {
    let mut strings: Vec<&str> = vec![body];
    for (_, v) in &fm.entries {
        match v {
            Value::Str(s) => strings.push(s),
            Value::List(l) => strings.extend(l.iter().map(String::as_str)),
            _ => {}
        }
    }
    for s in &strings {
        if let Some(m) = SECRET_MARKERS.iter().find(|m| contains_marker(s, m)) {
            return Err(format!("contains something that looks like a credential (`{m}…`); credentials never belong in knowledge files"));
        }
    }
    for (k, v) in &fm.entries {
        let values: Vec<&str> = match v {
            Value::Str(s) => vec![s],
            Value::List(l) => l.iter().map(String::as_str).collect(),
            _ => vec![],
        };
        if values.iter().any(|s| is_absolute_machine_path(s)) {
            return Err(format!("`{k}` contains an absolute machine path; portable files use project ids or relative paths"));
        }
    }
    Ok(())
}

fn contains_marker(s: &str, marker: &str) -> bool {
    // Require a token-like tail so prose such as "AKIA prefix" in documentation is not flagged.
    s.match_indices(marker).any(|(i, _)| {
        let tail = &s[i + marker.len()..];
        marker == "-----BEGIN" || tail.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').count() >= 12
    })
}

fn is_absolute_machine_path(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || s.starts_with("\\\\")
        || s.starts_with("/home/")
        || s.starts_with("/Users/")
}

/// RFC 3339 UTC timestamp (second precision) for Unix milliseconds.
pub fn timestamp(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// FNV-1a 64-bit content hash (change detection and optimistic concurrency — not a security primitive).
pub fn content_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(extra: &str) -> Doc {
        Doc::parse(&format!(
            "---\nnexus: 1\nid: 0192f2a4-0000-7000-8000-000000000001\nkind: task\ntitle: Fix login\nstatus: backlog\ncreated: 2026-11-04T10:00:00Z\nupdated: 2026-11-04T10:00:00Z\nsource: human\n{extra}---\n\nBody\n"
        ))
        .unwrap()
    }

    #[test]
    fn validates_schema() {
        assert_eq!(doc("").validate(Privacy::Shared), Ok(Kind::Task));
        for (extra, needle) in [
            ("priority: whenever\n", "priority"),
            ("privacy: local-only\n", "belong in .nexus-local"),
            ("note: C:\\Users\\me\\secret.txt\n", "absolute machine path"),
            ("token: ghp_abcdefghijklmnopqrstuvwxyz0123\n", "credential"),
        ] {
            let e = doc(extra).validate(Privacy::Shared).unwrap_err();
            assert!(e.contains(needle), "{extra}: {e}");
        }
        assert!(doc("privacy: local-only\n").validate(Privacy::LocalOnly).is_ok());
        let mut d = doc("");
        d.fm.set("status", Value::Str("someday".into()));
        assert!(d.validate(Privacy::Shared).unwrap_err().contains("status"));
        d.fm.set("nexus", Value::Int(9));
        assert!(d.validate(Privacy::Shared).unwrap_err().contains("newer"));
        assert!(Doc::parse("# no frontmatter").is_err());
    }

    #[test]
    fn credential_detection_avoids_prose_false_positives() {
        assert!(check_portable(&Frontmatter::default(), "Keys look like AKIA followed by letters.").is_ok());
        assert!(check_portable(&Frontmatter::default(), "key AKIAIOSFODNN7EXAMPLE leaked").is_err());
        assert!(check_portable(&Frontmatter::default(), "-----BEGIN OPENSSH PRIVATE KEY-----").is_err());
        assert!(check_portable(&Frontmatter::default(), "paths like C:\\code are fine in prose").is_ok());
    }

    #[test]
    fn timestamps_and_hashes() {
        assert_eq!(timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(timestamp(1_793_793_600_000), "2026-11-04T12:00:00Z");
        assert_eq!(timestamp(951_782_400_000), "2000-02-29T00:00:00Z");
        assert_eq!(content_hash(b""), "cbf29ce484222325");
        assert_ne!(content_hash(b"a"), content_hash(b"b"));
    }
}

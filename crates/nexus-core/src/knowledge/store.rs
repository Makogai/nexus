//! Reading and writing knowledge files: atomic (temp file + fsync + rename), validated before every write, with
//! optimistic concurrency through content hashes (ADR-0013).

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::frontmatter::{Frontmatter, Value};
use super::{Doc, Kind, LOCAL_DIR, MAX_FILE_BYTES, Privacy, SCHEMA, content_hash, timestamp};
use crate::error::{CoreError, Result};
use crate::now_ms;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    pub id: String,
    pub path: PathBuf,
    pub hash: String,
}

/// A new document. `fields` are kind-specific keys (status, priority, verification, applies, stacks, labels, …).
#[derive(Debug, Clone)]
pub struct NewDoc {
    pub kind: Kind,
    pub title: String,
    pub body: String,
    pub project: Option<String>,
    pub privacy: Privacy,
    pub source: &'static str,
    pub fields: Vec<(String, Value)>,
}

pub fn read(path: &Path) -> Result<(Doc, String)> {
    let meta = std::fs::metadata(path).map_err(|e| CoreError::io("cannot read", path, e))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(CoreError::invalid(format!("{} is larger than 2 MiB", path.display())));
    }
    let bytes = std::fs::read(path).map_err(|e| CoreError::io("cannot read", path, e))?;
    let text = String::from_utf8(bytes.clone()).map_err(|_| CoreError::invalid("knowledge files must be UTF-8"))?;
    let doc = Doc::parse(&text).map_err(CoreError::Invalid)?;
    Ok((doc, content_hash(&bytes)))
}

/// Create a document under `workspace_dir` (shared) or `workspace_dir/.nexus-local` (local-only).
pub fn create(workspace_dir: &Path, new: NewDoc) -> Result<Written> {
    let root = match new.privacy {
        Privacy::Shared => workspace_dir.to_path_buf(),
        Privacy::LocalOnly => {
            ensure_local_ignored(workspace_dir)?;
            workspace_dir.join(LOCAL_DIR)
        }
    };
    create_in(&root, new)
}

/// Create a document in an explicit root (global knowledge lives in the NEXUS data dir).
pub fn create_in(root: &Path, new: NewDoc) -> Result<Written> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = timestamp(now_ms());
    let mut fm = Frontmatter::default();
    fm.set("nexus", Value::Int(SCHEMA));
    fm.set("id", Value::Str(id.clone()));
    fm.set("kind", Value::Str(new.kind.as_str().into()));
    fm.set("title", Value::Str(new.title.trim().to_string()));
    if let Some(&default) = new.kind.statuses().first() {
        fm.set("status", Value::Str(default.into()));
    }
    if let Some(p) = &new.project {
        fm.set("project", Value::Str(p.clone()));
    }
    fm.set("created", Value::Str(now.clone()));
    fm.set("updated", Value::Str(now));
    fm.set("source", Value::Str(new.source.into()));
    if new.privacy == Privacy::LocalOnly {
        fm.set("privacy", Value::Str("local-only".into()));
    }
    for (k, v) in new.fields {
        fm.set(&k, v);
    }
    let doc = Doc { fm, body: new.body };
    let root_privacy = if root.ends_with(LOCAL_DIR) || new.privacy == Privacy::LocalOnly { Privacy::LocalOnly } else { Privacy::Shared };
    doc.validate(root_privacy).map_err(CoreError::Invalid)?;
    let dir = root.join(new.kind.folder());
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::io("cannot create", &dir, e))?;
    let path = dir.join(format!("{id}.md"));
    let text = doc.render();
    write_atomic(&path, text.as_bytes())?;
    Ok(Written { id, path, hash: content_hash(text.as_bytes()) })
}

/// Read–modify–write with optimistic concurrency: when `expected_hash` is given and the file changed since the
/// caller read it, nothing is written and `conflict` is returned. `id`, `kind`, `created` and `nexus` cannot change.
pub fn update(path: &Path, expected_hash: Option<&str>, root_privacy: Privacy, edit: impl FnOnce(&mut Doc) -> Result<()>) -> Result<Written> {
    let (mut doc, hash) = read(path)?;
    if let Some(expected) = expected_hash
        && expected != hash
    {
        return Err(CoreError::Conflict(format!(
            "{} changed since it was read (expected {expected}, found {hash}); read it again",
            path.display()
        )));
    }
    let frozen: Vec<(String, Option<Value>)> =
        ["nexus", "id", "kind", "created"].iter().map(|k| (k.to_string(), doc.fm.get(k).cloned())).collect();
    edit(&mut doc)?;
    for (k, v) in frozen {
        if doc.fm.get(&k).cloned() != v {
            return Err(CoreError::invalid(format!("`{k}` cannot be changed")));
        }
    }
    doc.fm.set("updated", Value::Str(timestamp(now_ms())));
    doc.validate(root_privacy).map_err(CoreError::Invalid)?;
    let text = doc.render();
    write_atomic(path, text.as_bytes())?;
    Ok(Written {
        id: doc.id().unwrap_or_default().to_string(),
        path: path.to_path_buf(),
        hash: content_hash(text.as_bytes()),
    })
}

/// Temp file in the same directory, flushed and fsynced, then renamed over the target.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    crate::workspace::write_atomic(path, bytes)
}

/// Make sure the workspace's `.gitignore` excludes `.nexus-local/` before anything local-only is written.
pub fn ensure_local_ignored(workspace_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(workspace_dir).map_err(|e| CoreError::io("cannot create", workspace_dir, e))?;
    let gi = workspace_dir.join(".gitignore");
    let current = std::fs::read_to_string(&gi).unwrap_or_default();
    if current.lines().any(|l| matches!(l.trim(), ".nexus-local/" | ".nexus-local" | "/.nexus-local/" | "/.nexus-local")) {
        return Ok(());
    }
    let mut next = current;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str("\n# Local-only NEXUS knowledge (privacy: local-only) never leaves this machine.\n.nexus-local/\n");
    write_atomic(&gi, next.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(title: &str, privacy: Privacy) -> NewDoc {
        NewDoc {
            kind: Kind::Task,
            title: title.into(),
            body: "Acceptance: it works.\n".into(),
            project: Some("proj-1".into()),
            privacy,
            source: "human",
            fields: vec![("priority".into(), Value::Str("high".into()))],
        }
    }

    #[test]
    fn create_writes_a_valid_document_named_by_id() {
        let t = tempfile::tempdir().unwrap();
        let w = create(t.path(), task("Fix login", Privacy::Shared)).unwrap();
        assert!(w.path.starts_with(t.path().join("tasks")));
        assert_eq!(w.path.file_stem().unwrap().to_string_lossy(), w.id);
        assert_eq!(uuid::Uuid::parse_str(&w.id).unwrap().get_version_num(), 7, "UUIDv7");
        let (doc, hash) = read(&w.path).unwrap();
        assert_eq!(hash, w.hash);
        assert_eq!((doc.title(), doc.status()), ("Fix login", Some("backlog")));
        assert_eq!(doc.validate(Privacy::Shared), Ok(Kind::Task));
    }

    #[test]
    fn local_only_goes_to_the_ignored_folder() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join(".gitignore"), "repos/").unwrap();
        let w = create(t.path(), task("Private", Privacy::LocalOnly)).unwrap();
        assert!(w.path.starts_with(t.path().join(LOCAL_DIR)));
        let gi = std::fs::read_to_string(t.path().join(".gitignore")).unwrap();
        assert!(gi.contains("repos/\n") && gi.lines().any(|l| l == ".nexus-local/"));
        create(t.path(), task("Another", Privacy::LocalOnly)).unwrap();
        let gi2 = std::fs::read_to_string(t.path().join(".gitignore")).unwrap();
        assert_eq!(gi2.matches(".nexus-local/").count(), 1, "added once");
    }

    #[test]
    fn update_is_optimistic_preserves_unknown_keys_and_freezes_identity() {
        let t = tempfile::tempdir().unwrap();
        let w = create(t.path(), task("Fix login", Privacy::Shared)).unwrap();
        // A human adds a field NEXUS does not know.
        let text = std::fs::read_to_string(&w.path).unwrap().replacen("source: human\n", "source: human\nestimate: 3d\n", 1);
        std::fs::write(&w.path, &text).unwrap();
        let stale = update(&w.path, Some(&w.hash), Privacy::Shared, |_| Ok(()));
        assert_eq!(stale.unwrap_err().code(), "conflict", "the file changed after it was read");
        let (_, fresh) = read(&w.path).unwrap();
        let moved = update(&w.path, Some(&fresh), Privacy::Shared, |d| {
            d.fm.set("status", Value::Str("in-progress".into()));
            Ok(())
        })
        .unwrap();
        let (doc, _) = read(&moved.path).unwrap();
        assert_eq!(doc.status(), Some("in-progress"));
        assert_eq!(doc.fm.str("estimate"), Some("3d"), "unknown keys survive");
        let err = update(&w.path, None, Privacy::Shared, |d| {
            d.fm.set("id", Value::Str(uuid::Uuid::now_v7().to_string()));
            Ok(())
        });
        assert!(err.unwrap_err().to_string().contains("cannot be changed"));
        let invalid = update(&w.path, None, Privacy::Shared, |d| {
            d.body.push_str("\nkey: sk-ant-api03-abcdefghijklmnopqrstu\n");
            Ok(())
        });
        assert!(invalid.is_err(), "validation runs before every write");
        assert!(!read(&w.path).unwrap().0.body.contains("sk-ant"), "nothing was written");
    }

    #[test]
    fn rejects_invalid_new_documents() {
        let t = tempfile::tempdir().unwrap();
        assert!(create(t.path(), task("", Privacy::Shared)).is_err());
        let mut bad = task("x", Privacy::Shared);
        bad.fields.push(("priority".into(), Value::Str("whenever".into())));
        bad.fields.retain(|(k, v)| k != "priority" || v == &Value::Str("whenever".into()));
        assert!(create(t.path(), bad).is_err());
        assert!(!t.path().join("tasks").exists() || std::fs::read_dir(t.path().join("tasks")).unwrap().next().is_none());
    }
}

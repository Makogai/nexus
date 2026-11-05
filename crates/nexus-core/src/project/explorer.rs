//! Project explorer (W3): the project's files and Git history for people to look at — strictly read-only.
//!
//! * Paths are resolved inside the project folder only; a symlink is listed but never followed out of it.
//! * `.git` is not listed; files Git ignores are listed and marked.
//! * Files that may hold secrets (`.env`, keys, credential stores — the same check the workspace sync inspection
//!   uses) are never shown: the view says why and points to the person's own editor (security rules: never display
//!   secrets).
//! * Text files are shown up to [`MAX_FILE_BYTES`], binary files not at all.
//! * History reads disable external diff programs, textconv filters and signature checks (`git::commit_files`).
//!
//! Nothing here is reachable through MCP (ADR-0009): it serves the desktop only.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::{git, paths, project};

pub const MAX_ENTRIES: usize = 2000;
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
pub const MAX_DIFF_BYTES: usize = 400 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    /// Project-relative, forward slashes.
    pub path: String,
    pub dir: bool,
    pub symlink: bool,
    pub size: Option<u64>,
    /// Git ignores it (build output, dependencies, local files).
    pub ignored: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub path: String,
    pub entries: Vec<Entry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    pub path: String,
    pub size: u64,
    /// `None` when the file is binary or hidden.
    pub text: Option<String>,
    pub binary: bool,
    pub truncated: bool,
    /// Why the content is not shown (a file that may hold secrets).
    pub hidden: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitDiff {
    pub path: String,
    pub text: Option<String>,
    pub truncated: bool,
    pub hidden: Option<String>,
}

/// The project folder on this computer (canonical).
fn root(conn: &Connection, project_id: &str) -> Result<PathBuf> {
    let p = project::get(conn, project_id)?;
    let dir = p.local_path.filter(|_| p.available).ok_or_else(|| CoreError::invalid(format!("{} is not on this computer", p.name)))?;
    dunce::canonicalize(&dir).map_err(|e| CoreError::io("cannot open", &dir, e))
}

/// `rel` inside `root`, refusing anything that resolves outside it (`..`, absolute paths, symlinks leading out).
fn inside(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel = rel.trim_matches('/');
    if rel.is_empty() {
        return Ok(root.to_path_buf());
    }
    if rel.split('/').any(|s| s == ".git") {
        return Err(CoreError::invalid("the .git folder is not shown"));
    }
    let p = paths::resolve_within(root, rel)?;
    let canon = dunce::canonicalize(&p).map_err(|e| CoreError::io("cannot open", &p, e))?;
    if !paths::is_within(&canon, root) {
        return Err(CoreError::invalid(format!("{rel} leads outside the project")));
    }
    Ok(canon)
}

/// One folder of the project: folders first, then files, by name (case-insensitive).
pub fn list(conn: &Connection, project_id: &str, rel: &str) -> Result<Listing> {
    let root = root(conn, project_id)?;
    let dir = inside(&root, rel)?;
    let rd = std::fs::read_dir(&dir).map_err(|e| CoreError::io("cannot list", &dir, e))?;
    let prefix = rel.trim_matches('/');
    let mut entries = Vec::new();
    let mut truncated = false;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        let meta = std::fs::symlink_metadata(e.path()).ok();
        let symlink = meta.as_ref().is_some_and(|m| m.file_type().is_symlink());
        let dir = !symlink && meta.as_ref().is_some_and(|m| m.is_dir());
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        entries.push(Entry { size: meta.as_ref().filter(|m| m.is_file()).map(|m| m.len()), name, path, dir, symlink, ignored: false });
    }
    let probe: Vec<String> = entries.iter().map(|e| if e.dir { format!("{}/", e.path) } else { e.path.clone() }).collect();
    let ignored = git::ignored_paths(&root, &probe);
    for e in &mut entries {
        e.ignored = ignored.contains(&e.path) || ignored.contains(&format!("{}/", e.path));
    }
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(Listing { path: prefix.to_string(), entries, truncated })
}

/// A project file to look at (text only, at most [`MAX_FILE_BYTES`]; never a file that may hold secrets).
pub fn read(conn: &Connection, project_id: &str, rel: &str) -> Result<FileView> {
    let root = root(conn, project_id)?;
    let path = inside(&root, rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| CoreError::io("cannot read", &path, e))?;
    if !meta.is_file() {
        return Err(CoreError::invalid(format!("{rel} is not a file")));
    }
    let rel = rel.trim_matches('/').to_string();
    if let Some(why) = crate::workspace_sync::sensitive_reason(&rel) {
        return Ok(FileView { path: rel, size: meta.len(), text: None, binary: false, truncated: false, hidden: Some(hidden_text(why)) });
    }
    let mut bytes = Vec::new();
    {
        use std::io::Read;
        let f = std::fs::File::open(&path).map_err(|e| CoreError::io("cannot read", &path, e))?;
        f.take(MAX_FILE_BYTES as u64).read_to_end(&mut bytes).map_err(|e| CoreError::io("cannot read", &path, e))?;
    }
    let binary = bytes.iter().take(8192).any(|b| *b == 0);
    let truncated = meta.len() > MAX_FILE_BYTES as u64;
    let text = (!binary).then(|| String::from_utf8_lossy(&bytes).into_owned());
    Ok(FileView { path: rel, size: meta.len(), text, binary, truncated, hidden: None })
}

fn hidden_text(why: &str) -> String {
    let why = if why.contains("secret") { why.to_string() } else { format!("{why} — such files can hold secrets") };
    format!("Not shown: {why}. Open it in your own editor if you need it.")
}

/// A page of the project's commits (newest first).
pub fn log(conn: &Connection, project_id: &str, skip: usize, limit: usize) -> Result<Vec<git::LogEntry>> {
    git::log_page(&root(conn, project_id)?, skip, limit)
}

/// The files a commit changed.
pub fn commit(conn: &Connection, project_id: &str, oid: &str) -> Result<Vec<git::ChangedFile>> {
    git::commit_files(&root(conn, project_id)?, oid)
}

/// What a commit changed in one file (never for a file that may hold secrets).
pub fn commit_diff(conn: &Connection, project_id: &str, oid: &str, rel: &str) -> Result<CommitDiff> {
    let root = root(conn, project_id)?;
    if let Some(why) = crate::workspace_sync::sensitive_reason(rel) {
        return Ok(CommitDiff { path: rel.to_string(), text: None, truncated: false, hidden: Some(hidden_text(why)) });
    }
    let (text, truncated) = git::commit_file_diff(&root, oid, rel, MAX_DIFF_BYTES)?;
    Ok(CommitDiff { path: rel.to_string(), text: Some(text), truncated, hidden: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(["-c", "user.name=Ann", "-c", "user.email=ann@example.invalid", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
    }

    /// W3: folders and files read-only and inside the project, secrets never shown, history with changed files and
    /// diffs.
    #[test]
    fn files_and_history_are_shown_read_only_inside_the_project() {
        let t = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let ws = crate::workspace::create(&conn, base.join("ws").to_str().unwrap(), "W", "").unwrap();
        let repo = base.join("app");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src").join("main.ts"), "export const a = 1\n").unwrap();
        std::fs::write(repo.join("README.md"), "# App\n").unwrap();
        std::fs::write(repo.join(".gitignore"), "dist/\n").unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "First"]);
        std::fs::write(repo.join("src").join("main.ts"), "export const a = 2\nexport const b = 3\n").unwrap();
        git(&repo, &["mv", "README.md", "ABOUT.md"]);
        git(&repo, &["commit", "-qam", "Second"]);
        std::fs::create_dir_all(repo.join("dist")).unwrap();
        std::fs::write(repo.join("dist").join("out.js"), "x").unwrap();
        std::fs::write(repo.join(".env"), "TOKEN=not-for-display\n").unwrap();
        std::fs::write(repo.join("logo.png"), [0x89u8, b'P', b'N', b'G', 0, 0, 1]).unwrap();
        let p = project::register(&conn, &ws.id, repo.to_str().unwrap(), None).unwrap();

        let top = list(&conn, &p.id, "").unwrap();
        let names: Vec<&str> = top.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["dist", "src", ".env", ".gitignore", "ABOUT.md", "logo.png"], "folders first, no .git");
        assert!(top.entries.iter().find(|e| e.name == "dist").unwrap().ignored, "ignored output is marked");
        assert!(!top.entries.iter().find(|e| e.name == "src").unwrap().ignored);
        assert_eq!(list(&conn, &p.id, "src").unwrap().entries[0].path, "src/main.ts");

        let f = read(&conn, &p.id, "src/main.ts").unwrap();
        assert_eq!(f.text.as_deref(), Some("export const a = 2\nexport const b = 3\n"));
        let env = read(&conn, &p.id, ".env").unwrap();
        assert!(env.text.is_none() && env.hidden.as_deref().is_some_and(|h| h.contains("secrets")), "{env:?}");
        assert!(read(&conn, &p.id, "logo.png").unwrap().binary);
        for bad in ["../ws/nexus-workspace.json", ".git/config", "/etc/passwd", "src/../../ws"] {
            assert!(read(&conn, &p.id, bad).is_err() && list(&conn, &p.id, bad).is_err(), "{bad} is refused");
        }

        let commits = log(&conn, &p.id, 0, 10).unwrap();
        assert_eq!(commits.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(), ["Second", "First"]);
        assert_eq!(commits[0].author, "Ann");
        assert_eq!(log(&conn, &p.id, 1, 10).unwrap().len(), 1, "paging");
        let files = commit(&conn, &p.id, &commits[0].oid).unwrap();
        let main = files.iter().find(|f| f.path == "src/main.ts").unwrap();
        assert_eq!((main.status.as_str(), main.additions, main.deletions), ("M", Some(2), Some(1)));
        let renamed = files.iter().find(|f| f.path == "ABOUT.md").unwrap();
        assert_eq!((renamed.status.as_str(), renamed.old_path.as_deref()), ("R", Some("README.md")));
        let d = commit_diff(&conn, &p.id, &commits[0].oid, "src/main.ts").unwrap();
        assert!(d.text.as_deref().unwrap().contains("+export const b = 3"), "{d:?}");
        assert!(commit(&conn, &p.id, "--output=x").is_err() && commit_diff(&conn, &p.id, &commits[0].oid, "--x").is_err());
        assert!(commit_diff(&conn, &p.id, &commits[0].oid, ".env").unwrap().hidden.is_some());
    }
}

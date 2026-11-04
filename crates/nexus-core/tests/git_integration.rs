//! Integration tests against real `git` in temporary repositories.

use std::path::Path;
use std::process::Command;

use nexus_core::git;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn repo() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    git(t.path(), &["init", "-q", "-b", "main"]);
    t
}

#[test]
fn status_of_fresh_dirty_and_committed_repo() {
    let t = repo();
    let st = git::status(t.path()).unwrap();
    assert!(st.is_repo);
    assert_eq!(st.branch.as_deref(), Some("main"));
    assert_eq!(st.head_oid, None, "no commits yet");

    std::fs::write(t.path().join("a.txt"), "a").unwrap();
    std::fs::write(t.path().join("b.txt"), "b").unwrap();
    git(t.path(), &["add", "a.txt"]);
    let st = git::status(t.path()).unwrap();
    assert_eq!((st.staged, st.untracked), (1, 1));

    git(t.path(), &["commit", "-q", "-m", "first commit"]);
    std::fs::write(t.path().join("a.txt"), "changed").unwrap();
    let st = git::status(t.path()).unwrap();
    assert_eq!((st.staged, st.unstaged, st.untracked), (0, 1, 1));
    let c = st.last_commit.expect("last commit");
    assert_eq!(c.summary, "first commit");
    assert!(c.timestamp_ms > 0);
}

#[test]
fn non_repository_is_not_an_error() {
    let t = tempfile::tempdir().unwrap();
    let st = git::status(t.path()).unwrap();
    assert!(!st.is_repo);
}

#[test]
fn remote_url_is_redacted() {
    let t = repo();
    git(t.path(), &["remote", "add", "origin", "https://user:s3cret@example.invalid/org/repo.git"]);
    assert_eq!(git::remote_url(t.path()).as_deref(), Some("https://example.invalid/org/repo.git"));
}

#[test]
fn toplevel_from_subdirectory() {
    let t = repo();
    let sub = t.path().join("src").join("deep");
    std::fs::create_dir_all(&sub).unwrap();
    let top = git::toplevel(&sub).unwrap();
    assert!(nexus_core::paths::same_path(&top, &dunce::canonicalize(t.path()).unwrap()));
}

/// A repository's own config can name an fsmonitor hook that git runs on every `status`.
/// NEXUS must not execute it (threat T5).
#[test]
fn hostile_fsmonitor_hook_is_not_executed() {
    let t = repo();
    std::fs::write(t.path().join("f.txt"), "x").unwrap();
    git(t.path(), &["add", "f.txt"]);
    git(t.path(), &["commit", "-q", "-m", "c"]);
    git(t.path(), &["config", "core.fsmonitor", "echo pwned > fsmonitor-ran.txt; false"]);
    let marker = t.path().join("fsmonitor-ran.txt");

    // Control: plain git really does run the hook in this environment (otherwise the test proves nothing).
    let _ = Command::new("git").args(["status", "--porcelain"]).current_dir(t.path()).output();
    let attack_possible = marker.exists();
    let _ = std::fs::remove_file(&marker);

    let st = git::status(t.path()).unwrap();
    assert!(st.is_repo);
    assert!(!marker.exists(), "NEXUS git status executed the repository's fsmonitor hook");
    if !attack_possible {
        eprintln!("note: plain git did not run the hook here; assertion is weaker on this platform");
    }
}

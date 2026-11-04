//! Git through the user's `git` CLI (ADR-0006).
//!
//! Read-only calls disable fsmonitor (a repository's `.git/config` could otherwise run an arbitrary
//! command on every status, threat T5), skip optional locks (no index-lock races with the user's own git)
//! and never prompt for credentials.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::proc;

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub is_repo: bool,
    /// Branch name; `None` when detached or not a repository.
    pub branch: Option<String>,
    pub detached: bool,
    pub head_oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub last_commit: Option<CommitInfo>,
}

impl GitStatus {
    pub fn changed(&self) -> u32 {
        self.staged + self.unstaged + self.untracked + self.conflicted
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub oid: String,
    pub summary: String,
    pub timestamp_ms: i64,
}

fn git_exe() -> Result<PathBuf> {
    which::which("git").map_err(|_| CoreError::Unsupported("git was not found on PATH".into()))
}

fn git_cmd(repo: Option<&Path>) -> Result<std::process::Command> {
    let mut cmd = proc::command(git_exe()?);
    cmd.args(["-c", "core.fsmonitor=false", "-c", "core.quotepath=false", "-c", "color.ui=false"]);
    if let Some(repo) = repo {
        cmd.arg("-C").arg(repo);
    }
    cmd.env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("LC_ALL", "C");
    Ok(cmd)
}

fn run_git(repo: Option<&Path>, args: &[&str]) -> Result<proc::Output> {
    let mut cmd = git_cmd(repo)?;
    cmd.args(args);
    proc::run(cmd, TIMEOUT)
}

/// `git --version` → "2.55.0.windows.2".
pub fn version() -> Option<String> {
    let out = run_git(None, &["--version"]).ok()?;
    out.success()
        .then(|| out.stdout_str().trim().trim_start_matches("git version ").to_string())
}

/// Top-level directory of the work tree containing `path`, if any.
pub fn toplevel(path: &Path) -> Option<PathBuf> {
    let out = run_git(Some(path), &["rev-parse", "--show-toplevel"]).ok()?;
    if !out.success() {
        return None;
    }
    let p = PathBuf::from(out.stdout_str().trim());
    dunce::canonicalize(&p).ok().or(Some(p))
}

/// Full status of the repository at `repo`. A non-repository yields `is_repo = false` (not an error).
pub fn status(repo: &Path) -> Result<GitStatus> {
    let out = run_git(Some(repo), &["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=normal"])?;
    if !out.success() {
        let err = out.stderr_str();
        if err.contains("not a git repository") {
            return Ok(GitStatus::default());
        }
        return Err(CoreError::External {
            program: "git status".into(),
            message: first_line(&err),
        });
    }
    let mut st = parse_porcelain_v2(&out.stdout);
    st.is_repo = true;
    if st.head_oid.is_some() {
        st.last_commit = last_commit(repo);
    }
    Ok(st)
}

/// Remote URL of `origin` (or the first remote), with credentials removed.
pub fn remote_url(repo: &Path) -> Option<String> {
    let out = run_git(Some(repo), &["remote"]).ok()?;
    if !out.success() {
        return None;
    }
    let remotes = out.stdout_str();
    let name = remotes
        .lines()
        .find(|l| l.trim() == "origin")
        .or_else(|| remotes.lines().next())?
        .trim()
        .to_string();
    let out = run_git(Some(repo), &["remote", "get-url", &name]).ok()?;
    out.success().then(|| redact_url(out.stdout_str().trim()))
}

/// Git URLs NEXUS clones kits from: `https://`, `ssh://`, `git@host:path` and `file://`. Transports that can run
/// commands (`ext::`, `fd::`) and option-like values are refused.
pub fn clone_url_allowed(url: &str) -> bool {
    let u = url.trim();
    !u.starts_with('-')
        && !u.contains("::")
        && !u.chars().any(|c| c.is_whitespace() || c.is_control())
        && (u.starts_with("https://") || u.starts_with("ssh://") || u.starts_with("file://") || (u.starts_with("git@") && u.contains(':')))
}

fn git_long(repo: Option<&Path>, args: &[&str]) -> Result<proc::Output> {
    let mut cmd = git_cmd(repo)?;
    // Never execute transports that run commands, never prompt.
    cmd.args(["-c", "protocol.ext.allow=never", "-c", "protocol.fd.allow=never"]);
    cmd.args(args);
    proc::run(cmd, Duration::from_secs(180))
}

/// Shallow clone of `url` (one branch, no submodules) into `dest` (must not exist).
pub fn clone_shallow(url: &str, dest: &Path, git_ref: Option<&str>) -> Result<()> {
    if !clone_url_allowed(url) {
        return Err(CoreError::invalid("only https://, ssh://, git@host:path and file:// Git URLs are supported"));
    }
    let dest_s = dest.to_string_lossy().into_owned();
    let mut args = vec!["clone", "--quiet", "--depth", "1", "--single-branch", "--no-recurse-submodules"];
    if let Some(r) = git_ref {
        if r.starts_with('-') || r.contains("..") || r.chars().any(char::is_whitespace) {
            return Err(CoreError::invalid("not a valid branch or tag name"));
        }
        args.extend(["--branch", r]);
    }
    args.extend(["--", url, &dest_s]);
    let out = git_long(None, &args)?;
    if !out.success() {
        return Err(CoreError::External { program: "git clone".into(), message: redact_url(&first_line(&out.stderr_str())) });
    }
    Ok(())
}

/// Fetch the followed ref of a NEXUS-owned checkout and move it there (the checkout is NEXUS's, not the user's).
pub fn fetch_reset(dir: &Path, git_ref: Option<&str>) -> Result<String> {
    let r = git_ref.unwrap_or("HEAD");
    let out = git_long(Some(dir), &["fetch", "--quiet", "--depth", "1", "origin", r])?;
    if !out.success() {
        return Err(CoreError::External { program: "git fetch".into(), message: redact_url(&first_line(&out.stderr_str())) });
    }
    let out = git_long(Some(dir), &["reset", "--quiet", "--hard", "FETCH_HEAD"])?;
    if !out.success() {
        return Err(CoreError::External { program: "git reset".into(), message: first_line(&out.stderr_str()) });
    }
    head(dir).ok_or_else(|| CoreError::External { program: "git".into(), message: "no HEAD after update".into() })
}

pub fn head(dir: &Path) -> Option<String> {
    let out = run_git(Some(dir), &["rev-parse", "HEAD"]).ok()?;
    out.success().then(|| out.stdout_str().trim().to_string())
}

/// Commit ids reachable from HEAD, newest first, at most `max` (for branch-aware knowledge, ADR-0018). `None` when the
/// folder is not a repository or has no commits.
pub fn history(repo: &Path, max: usize) -> Option<Vec<String>> {
    let n = format!("--max-count={}", max.clamp(1, 20_000));
    let out = run_git(Some(repo), &["rev-list", &n, "HEAD"]).ok()?;
    out.success().then(|| out.stdout_str().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
}

/// The last `n` commits on HEAD (newest first).
pub fn recent_commits(repo: &Path, n: usize) -> Option<Vec<CommitInfo>> {
    let count = format!("-{}", n.clamp(1, 50));
    let out = run_git(Some(repo), &["log", &count, "--format=%H%x00%ct%x00%s"]).ok()?;
    if !out.success() {
        return None;
    }
    Some(
        out.stdout_str()
            .lines()
            .filter_map(|l| {
                let mut parts = l.splitn(3, '\0');
                let oid = parts.next()?.to_string();
                let ts: i64 = parts.next()?.parse().ok()?;
                Some(CommitInfo { oid, timestamp_ms: ts * 1000, summary: parts.next().unwrap_or("").to_string() })
            })
            .collect(),
    )
}

pub fn last_commit(repo: &Path) -> Option<CommitInfo> {
    let out = run_git(Some(repo), &["log", "-1", "--format=%H%x00%ct%x00%s"]).ok()?;
    if !out.success() {
        return None;
    }
    let s = out.stdout_str();
    let mut parts = s.trim_end_matches('\n').splitn(3, '\0');
    let oid = parts.next()?.to_string();
    let ts: i64 = parts.next()?.parse().ok()?;
    let summary = parts.next().unwrap_or("").to_string();
    Some(CommitInfo {
        oid,
        summary,
        timestamp_ms: ts * 1000,
    })
}

/// Parse `git status --porcelain=v2 --branch -z` output.
pub fn parse_porcelain_v2(bytes: &[u8]) -> GitStatus {
    let mut st = GitStatus::default();
    let text = String::from_utf8_lossy(bytes);
    let mut records = text.split('\0').peekable();
    while let Some(rec) = records.next() {
        if rec.is_empty() {
            continue;
        }
        if let Some(header) = rec.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" if value != "(initial)" => st.head_oid = Some(value.to_string()),
                "branch.head" => {
                    if value == "(detached)" {
                        st.detached = true;
                    } else {
                        st.branch = Some(value.to_string());
                    }
                }
                "branch.upstream" => st.upstream = Some(value.to_string()),
                "branch.ab" => {
                    for part in value.split_whitespace() {
                        if let Some(a) = part.strip_prefix('+') {
                            st.ahead = a.parse().unwrap_or(0);
                        } else if let Some(b) = part.strip_prefix('-') {
                            st.behind = b.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let kind = rec.as_bytes()[0];
        match kind {
            b'1' | b'2' => {
                // "1 XY ..." — X = index (staged), Y = work tree (unstaged).
                let xy = rec.get(2..4).unwrap_or("..").as_bytes();
                if xy[0] != b'.' {
                    st.staged += 1;
                }
                if xy[1] != b'.' {
                    st.unstaged += 1;
                }
                if kind == b'2' {
                    // Rename/copy records are followed by the original path as a separate NUL field.
                    records.next();
                }
            }
            b'u' => st.conflicted += 1,
            b'?' => st.untracked += 1,
            _ => {}
        }
    }
    st
}

/// Remove `user:password@` (or a bare token) from URLs; scp-style `git@host:path` is left intact.
pub fn redact_url(url: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let (scheme, rest) = url.split_at(scheme_end + 3);
        let authority_end = rest.find('/').unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        if let Some(at) = authority.rfind('@') {
            return format!("{scheme}{}{}", &authority[at + 1..], &rest[authority_end..]);
        }
    }
    url.to_string()
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_headers_and_entries() {
        let raw = "# branch.oid 1234abcd\0# branch.head feature/x\0# branch.upstream origin/feature/x\0\
                   # branch.ab +3 -1\0\
                   1 M. N... 100644 100644 100644 aaa bbb src/a.rs\0\
                   1 .M N... 100644 100644 100644 aaa bbb src/b.rs\0\
                   1 MM N... 100644 100644 100644 aaa bbb src/c.rs\0\
                   2 R. N... 100644 100644 100644 aaa bbb R100 new.rs\0old.rs\0\
                   u UU N... 100644 100644 100644 100644 a b c conflict.rs\0\
                   ? untracked.txt\0? other.txt\0";
        let st = parse_porcelain_v2(raw.as_bytes());
        assert_eq!(st.branch.as_deref(), Some("feature/x"));
        assert_eq!(st.head_oid.as_deref(), Some("1234abcd"));
        assert_eq!(st.upstream.as_deref(), Some("origin/feature/x"));
        assert_eq!((st.ahead, st.behind), (3, 1));
        assert_eq!(st.staged, 3, "M., MM, R.");
        assert_eq!(st.unstaged, 2, ".M, MM");
        assert_eq!(st.conflicted, 1);
        assert_eq!(st.untracked, 2);
        assert_eq!(st.changed(), 8);
    }

    #[test]
    fn parses_initial_and_detached() {
        let st = parse_porcelain_v2(b"# branch.oid (initial)\0# branch.head main\0");
        assert_eq!(st.head_oid, None);
        assert_eq!(st.branch.as_deref(), Some("main"));
        let st = parse_porcelain_v2(b"# branch.oid abc\0# branch.head (detached)\0");
        assert!(st.detached);
        assert_eq!(st.branch, None);
    }

    #[test]
    fn redacts_credentials() {
        assert_eq!(redact_url("https://user:secret@github.com/a/b.git"), "https://github.com/a/b.git");
        assert_eq!(redact_url("https://ghp_token@github.com/a/b"), "https://github.com/a/b");
        assert_eq!(redact_url("https://github.com/a/b.git"), "https://github.com/a/b.git");
        assert_eq!(redact_url("git@github.com:a/b.git"), "git@github.com:a/b.git");
        assert_eq!(redact_url("ssh://git@host:22/x"), "ssh://host:22/x");
    }
}

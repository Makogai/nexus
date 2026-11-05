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

/// Commits that changed `rel_path` (following renames), newest first (version history of a document).
pub fn file_history(repo: &Path, rel_path: &str, n: usize) -> Option<Vec<CommitInfo>> {
    if rel_path.starts_with('-') || rel_path.contains("..") {
        return None;
    }
    let count = format!("-{}", n.clamp(1, 100));
    let out = run_git(Some(repo), &["log", &count, "--follow", "--format=%H%x00%ct%x00%s", "--", rel_path]).ok()?;
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

/// A file's content at a commit (`git show <commit>:<path>`), at most 2 MiB.
pub fn show_file(repo: &Path, commit: &str, rel_path: &str) -> Option<String> {
    if !commit.chars().all(|c| c.is_ascii_hexdigit()) || rel_path.starts_with('-') || rel_path.contains("..") {
        return None;
    }
    let spec = format!("{commit}:{rel_path}");
    let out = run_git(Some(repo), &["show", &spec]).ok()?;
    (out.success() && out.stdout.len() <= 2 * 1024 * 1024).then(|| out.stdout_str())
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

// ---- Workspace synchronisation (ADR-0031) ---------------------------------------------------------------------
// Everything below runs `git` with argument vectors, without prompts, with `ext::`/`fd::` transports disabled for
// commands that talk to a remote, and with external diff/text-conversion tools disabled.

/// Why a command that talks to a remote failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteFailure {
    /// No network or the host cannot be reached.
    Offline,
    /// The remote refused the credentials (or Git would have to ask for them).
    Auth,
    /// The remote repository does not exist (or is not visible with these credentials).
    RemoteMissing,
    /// The remote has commits the local branch does not have (push), or a non-fast-forward update.
    Rejected,
    Timeout,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteError {
    pub kind: RemoteFailure,
    /// The relevant line of Git's message, with credentials removed from URLs.
    pub message: String,
}

impl RemoteError {
    pub fn explain(&self) -> String {
        let what = match self.kind {
            RemoteFailure::Offline => "the remote cannot be reached — check the network (you can keep working offline)",
            RemoteFailure::Auth => "the remote refused the credentials — sign in with your Git tool or credential manager (NEXUS never asks for or stores them)",
            RemoteFailure::RemoteMissing => "the remote repository was not found, or these credentials cannot see it",
            RemoteFailure::Rejected => "the remote has changes you do not have yet — fetch and pull first (NEXUS never force-pushes)",
            RemoteFailure::Timeout => "the remote did not answer in time",
            RemoteFailure::Other => "Git reported an error",
        };
        if self.message.is_empty() { what.to_string() } else { format!("{what}: {}", self.message) }
    }
}

pub fn classify_remote_error(stderr: &str) -> RemoteFailure {
    let s = stderr.to_ascii_lowercase();
    let any = |needles: &[&str]| needles.iter().any(|n| s.contains(n));
    if any(&["[rejected]", "non-fast-forward", "fetch first", "updates were rejected", "[remote rejected]"]) {
        RemoteFailure::Rejected
    } else if any(&[
        "authentication failed",
        "could not read username",
        "could not read password",
        "permission denied (publickey",
        "invalid username or password",
        "terminal prompts disabled",
        "access denied",
        "403",
        "401",
        "host key verification failed",
    ]) {
        RemoteFailure::Auth
    } else if any(&[
        "could not resolve host",
        "failed to connect",
        "connection timed out",
        "connection refused",
        "network is unreachable",
        "no route to host",
        "could not connect",
        "temporary failure in name resolution",
        "unable to access",
    ]) {
        RemoteFailure::Offline
    } else if any(&["does not appear to be a git repository", "repository not found", "not found", "no such file or directory"]) {
        RemoteFailure::RemoteMissing
    } else if any(&["timed out"]) {
        RemoteFailure::Timeout
    } else {
        RemoteFailure::Other
    }
}

fn remote_error(stderr: &str) -> RemoteError {
    let line = stderr
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("fatal:") || l.starts_with("error:") || l.starts_with("! [") || l.contains("[rejected]"))
        .or_else(|| stderr.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or("");
    let message: String = line.split_whitespace().map(redact_url).collect::<Vec<_>>().join(" ").chars().take(300).collect();
    RemoteError { kind: classify_remote_error(stderr), message }
}

fn remote_cmd(repo: Option<&Path>, args: &[&str]) -> std::result::Result<proc::Output, RemoteError> {
    match git_long(repo, args) {
        Ok(out) if out.success() => Ok(out),
        Ok(out) => Err(remote_error(&out.stderr_str())),
        Err(e) => {
            let m = e.to_string();
            Err(RemoteError { kind: if m.contains("timed out") { RemoteFailure::Timeout } else { RemoteFailure::Other }, message: redact_url(&m) })
        }
    }
}

/// `git fetch --prune <remote>` (no submodules, no tags beyond those of fetched commits).
pub fn fetch_remote(repo: &Path, remote: &str) -> std::result::Result<(), RemoteError> {
    if remote.starts_with('-') || remote.is_empty() {
        return Err(RemoteError { kind: RemoteFailure::Other, message: "invalid remote name".into() });
    }
    remote_cmd(Some(repo), &["fetch", "--quiet", "--prune", "--no-recurse-submodules", remote]).map(|_| ())
}

/// Push the current branch to `remote` as `branch` — never forced. `set_upstream` for the first push.
pub fn push_branch(repo: &Path, remote: &str, branch: &str, set_upstream: bool) -> std::result::Result<(), RemoteError> {
    if remote.starts_with('-') || branch.starts_with('-') || branch.contains("..") {
        return Err(RemoteError { kind: RemoteFailure::Other, message: "invalid remote or branch name".into() });
    }
    let refspec = format!("HEAD:refs/heads/{branch}");
    let mut args = vec!["push", "--quiet", "--no-recurse-submodules"];
    if set_upstream {
        args.push("--set-upstream");
    }
    args.extend([remote, refspec.as_str()]);
    remote_cmd(Some(repo), &args).map(|_| ())
}

/// Full clone (history, one checkout, no submodules) of `url` into `dest` (must not exist or be empty).
pub fn clone_full(url: &str, dest: &Path) -> std::result::Result<(), RemoteError> {
    if !clone_url_allowed(url) {
        return Err(RemoteError { kind: RemoteFailure::Other, message: "only https://, ssh://, git@host:path and file:// Git URLs are supported".into() });
    }
    let dest_s = dest.to_string_lossy().into_owned();
    remote_cmd(None, &["clone", "--quiet", "--no-recurse-submodules", "--", url, &dest_s]).map(|_| ())
}

/// A commit for review: id, time, author name (no e-mail) and summary — summaries are untrusted text, cleaned.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub oid: String,
    pub timestamp_ms: i64,
    pub author: String,
    pub summary: String,
}

fn clean(s: &str, max: usize) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).take(max).collect::<String>().trim().to_string()
}

/// Commits in `range` (e.g. `HEAD..@{u}`), newest first, at most `max`.
pub fn log_range(repo: &Path, range: &str, max: usize) -> Result<Vec<LogEntry>> {
    let n = format!("-n{}", max.clamp(1, 500));
    let out = run_git(Some(repo), &["log", "--no-color", &n, "--format=%H%x1f%at%x1f%an%x1f%s%x1e", range, "--"])?;
    if !out.success() {
        return Err(CoreError::External { program: "git log".into(), message: first_line(&out.stderr_str()) });
    }
    Ok(out
        .stdout_str()
        .split('\x1e')
        .filter_map(|rec| {
            let mut f = rec.trim_start_matches(['\n', '\r']).split('\x1f');
            let oid = f.next()?.trim().to_string();
            if oid.len() < 7 {
                return None;
            }
            let ts: i64 = f.next()?.trim().parse().ok()?;
            Some(LogEntry { oid, timestamp_ms: ts * 1000, author: clean(f.next().unwrap_or(""), 80), summary: clean(f.next().unwrap_or(""), 200) })
        })
        .collect())
}

/// A changed path between two trees (or a tree and the working tree): status letter, mode of the new side, path.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
    /// A added, M modified, D deleted, T type changed.
    pub status: String,
    /// Octal mode of the new side (`120000` symlink, `160000` submodule), empty when deleted.
    pub mode: String,
    pub path: String,
}

/// `git diff --raw -z --no-renames from [to]` — `to: None` compares with the working tree.
pub fn diff_raw(repo: &Path, from: &str, to: Option<&str>) -> Result<Vec<DiffEntry>> {
    let mut args = vec!["diff", "--raw", "-z", "--no-renames", "--no-ext-diff", "--no-textconv", from];
    if let Some(t) = to {
        args.push(t);
    }
    args.push("--");
    let out = run_git(Some(repo), &args)?;
    if !out.success() {
        return Err(CoreError::External { program: "git diff".into(), message: first_line(&out.stderr_str()) });
    }
    Ok(parse_diff_raw(&out.stdout))
}

pub fn parse_diff_raw(bytes: &[u8]) -> Vec<DiffEntry> {
    let text = String::from_utf8_lossy(bytes);
    let mut fields = text.split('\0');
    let mut out = Vec::new();
    while let Some(meta) = fields.next() {
        let Some(meta) = meta.strip_prefix(':') else { continue };
        let Some(path) = fields.next() else { break };
        let parts: Vec<&str> = meta.split_whitespace().collect();
        if parts.len() < 5 || out.len() >= MAX_PATHS {
            continue;
        }
        let status: String = parts[4].chars().take(1).collect();
        let mode = if status == "D" { String::new() } else { parts[1].to_string() };
        out.push(DiffEntry { status, mode, path: path.to_string() });
    }
    out
}

/// A bounded text diff (`git diff --no-ext-diff --no-textconv …`); `true` when truncated.
pub fn diff_text(repo: &Path, args: &[&str], max_bytes: usize) -> Result<(String, bool)> {
    let mut all = vec!["diff", "--no-color", "--no-ext-diff", "--no-textconv"];
    all.extend_from_slice(args);
    let out = run_git(Some(repo), &all)?;
    if !out.success() {
        return Err(CoreError::External { program: "git diff".into(), message: first_line(&out.stderr_str()) });
    }
    let truncated = out.stdout.len() > max_bytes;
    let mut s = String::from_utf8_lossy(&out.stdout[..out.stdout.len().min(max_bytes)]).into_owned();
    if truncated {
        s.push_str("\n… (diff truncated)\n");
    }
    Ok((s, truncated))
}

/// A read-only `log`/`diff`/`show` with bounded output (for inspections); `true` when truncated.
pub fn bounded(repo: &Path, args: &[&str], max_bytes: usize) -> Result<(String, bool)> {
    if !matches!(args.first(), Some(&"log") | Some(&"diff") | Some(&"show")) {
        return Err(CoreError::invalid("only git log, diff and show are allowed here"));
    }
    let mut all = vec![args[0], "--no-color"];
    all.extend_from_slice(&args[1..]);
    let out = run_git(Some(repo), &all)?;
    if !out.success() {
        return Err(CoreError::External { program: format!("git {}", args[0]), message: first_line(&out.stderr_str()) });
    }
    let truncated = out.stdout.len() > max_bytes;
    Ok((String::from_utf8_lossy(&out.stdout[..out.stdout.len().min(max_bytes)]).into_owned(), truncated))
}

/// `git rev-parse --verify <rev>^{commit}` → full id.
pub fn rev_parse(repo: &Path, rev: &str) -> Option<String> {
    let spec = format!("{rev}^{{commit}}");
    let out = run_git(Some(repo), &["rev-parse", "--verify", "--quiet", &spec]).ok()?;
    out.success().then(|| out.stdout_str().trim().to_string()).filter(|s| !s.is_empty())
}

/// The upstream of the current branch: (remote name, `remote/branch`).
pub fn upstream_of_head(repo: &Path) -> Option<(String, String)> {
    let out = run_git(Some(repo), &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).ok()?;
    if !out.success() {
        return None;
    }
    let up = out.stdout_str().trim().to_string();
    let branch = run_git(Some(repo), &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok().filter(|o| o.success())?.stdout_str().trim().to_string();
    let key = format!("branch.{branch}.remote");
    let remote = run_git(Some(repo), &["config", "--get", &key]).ok().filter(|o| o.success()).map(|o| o.stdout_str().trim().to_string());
    Some((remote.unwrap_or_else(|| up.split('/').next().unwrap_or("origin").to_string()), up))
}

pub fn remotes(repo: &Path) -> Vec<String> {
    run_git(Some(repo), &["remote"]).ok().filter(|o| o.success()).map(|o| o.stdout_str().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()).unwrap_or_default()
}

/// The repository's `.git` directory (worktrees supported).
pub fn git_dir(repo: &Path) -> Option<PathBuf> {
    let out = run_git(Some(repo), &["rev-parse", "--absolute-git-dir"]).ok()?;
    out.success().then(|| PathBuf::from(out.stdout_str().trim()))
}

/// Fast-forward the current branch to `target` (refuses anything else).
pub fn merge_ff_only(repo: &Path, target: &str) -> Result<proc::Output> {
    run_git(Some(repo), &["merge", "--ff-only", "--no-edit", "--quiet", target])
}

/// Stage new files, then commit exactly `paths` (other staged changes stay staged and are not committed).
/// Returns the paths that were added to the index (so a failed commit can unstage them again).
pub fn commit_only(repo: &Path, message: &str, paths: &[String], new_files: &[String]) -> std::result::Result<String, (String, Vec<String>)> {
    let mut added = Vec::new();
    if !new_files.is_empty() {
        let mut args = vec!["add", "--"];
        args.extend(new_files.iter().map(String::as_str));
        match run_git(Some(repo), &args) {
            Ok(o) if o.success() => added = new_files.to_vec(),
            Ok(o) => return Err((first_line(&o.stderr_str()), Vec::new())),
            Err(e) => return Err((e.to_string(), Vec::new())),
        }
    }
    let mut args = vec!["commit", "--quiet", "-m", message, "--only", "--"];
    args.extend(paths.iter().map(String::as_str));
    match run_git(Some(repo), &args) {
        Ok(o) if o.success() => Ok(head(repo).unwrap_or_default()),
        Ok(o) => {
            let err = o.stderr_str();
            let line = err.lines().map(str::trim).find(|l| l.starts_with("fatal:") || l.starts_with("error:")).unwrap_or_else(|| err.lines().next().unwrap_or("")).to_string();
            Err((if line.is_empty() { first_line(&o.stdout_str()) } else { line }, added))
        }
        Err(e) => Err((e.to_string(), added)),
    }
}

/// Unstage paths (`git reset -q -- paths`) — the rollback of a failed commit.
pub fn unstage(repo: &Path, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut args = vec!["reset", "-q", "--"];
    args.extend(paths.iter().map(String::as_str));
    let out = run_git(Some(repo), &args)?;
    if out.success() { Ok(()) } else { Err(CoreError::External { program: "git reset".into(), message: first_line(&out.stderr_str()) }) }
}

/// Restore paths to `HEAD` in index and working tree — the rollback of a failed fast-forward (only for paths that
/// were clean before).
pub fn restore_to_head(repo: &Path, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut args = vec!["restore", "--source=HEAD", "--staged", "--worktree", "--"];
    args.extend(paths.iter().map(String::as_str));
    let out = run_git(Some(repo), &args)?;
    if out.success() { Ok(()) } else { Err(CoreError::External { program: "git restore".into(), message: first_line(&out.stderr_str()) }) }
}

/// Files of a tree with their modes (`git ls-tree -r -z <rev>`), bounded.
pub fn ls_tree(repo: &Path, rev: &str) -> Result<Vec<(String, String)>> {
    let out = run_git(Some(repo), &["ls-tree", "-r", "-z", "--full-tree", rev])?;
    if !out.success() {
        return Err(CoreError::External { program: "git ls-tree".into(), message: first_line(&out.stderr_str()) });
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter_map(|rec| {
            let (meta, path) = rec.split_once('\t')?;
            Some((meta.split_whitespace().next()?.to_string(), path.to_string()))
        })
        .take(MAX_PATHS)
        .collect())
}

/// Is a Git identity (user.name and user.email) configured for commits in this repository?
pub fn identity_configured(repo: &Path) -> bool {
    ["user.name", "user.email"].iter().all(|k| run_git(Some(repo), &["config", "--get", k]).ok().is_some_and(|o| o.success() && !o.stdout_str().trim().is_empty()))
}

/// Most paths listed by [`changed_paths`] / [`tracked_files`] (bounded output).
pub const MAX_PATHS: usize = 20_000;

/// Changed paths relative to the repository (`git status --porcelain=v1 -z`, untracked files one by one, ignored
/// files not listed): `(XY, path)`. Read-only.
pub fn changed_paths(repo: &Path) -> Result<Vec<(String, String)>> {
    let out = run_git(Some(repo), &["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
    if !out.success() {
        return Err(CoreError::External { program: "git status".into(), message: first_line(&out.stderr_str()) });
    }
    Ok(parse_porcelain_v1(&out.stdout))
}

pub fn parse_porcelain_v1(bytes: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let mut fields = text.split('\0');
    while let Some(rec) = fields.next() {
        if rec.len() < 4 || out.len() >= MAX_PATHS {
            continue;
        }
        let (xy, path) = (&rec[..2], &rec[3..]);
        if xy.contains('R') || xy.contains('C') {
            fields.next(); // the original path of a rename/copy
        }
        out.push((xy.to_string(), path.to_string()));
    }
    out
}

/// Tracked files relative to the repository (`git ls-files -z`). Read-only.
pub fn tracked_files(repo: &Path) -> Result<Vec<String>> {
    let out = run_git(Some(repo), &["ls-files", "-z"])?;
    if !out.success() {
        return Err(CoreError::External { program: "git ls-files".into(), message: first_line(&out.stderr_str()) });
    }
    Ok(String::from_utf8_lossy(&out.stdout).split('\0').filter(|p| !p.is_empty()).take(MAX_PATHS).map(str::to_string).collect())
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
    fn remote_errors_are_classified_and_redacted() {
        assert_eq!(classify_remote_error("fatal: unable to access 'https://x/': Could not resolve host: x"), RemoteFailure::Offline);
        assert_eq!(classify_remote_error("fatal: Authentication failed for 'https://x/'"), RemoteFailure::Auth);
        assert_eq!(classify_remote_error("fatal: could not read Username for 'https://github.com': terminal prompts disabled"), RemoteFailure::Auth);
        assert_eq!(classify_remote_error("git@github.com: Permission denied (publickey)."), RemoteFailure::Auth);
        assert_eq!(classify_remote_error("fatal: '/tmp/none' does not appear to be a git repository"), RemoteFailure::RemoteMissing);
        assert_eq!(classify_remote_error(" ! [rejected]        main -> main (fetch first)"), RemoteFailure::Rejected);
        let e = remote_error(&format!("fatal: Authentication failed for 'https://user:{}@example.invalid/r.git/'", "hunter".to_string() + "2"));
        assert!(!e.message.contains("hunter2") && e.kind == RemoteFailure::Auth, "{e:?}");
    }

    #[test]
    fn diff_raw_parsing() {
        let raw = b":100644 100644 aaa bbb M\0rules/a.md\0:000000 120000 000 ccc A\0link\0:100644 000000 ddd 000 D\0gone.md\0";
        let d = parse_diff_raw(raw);
        assert_eq!(d.iter().map(|e| (e.status.as_str(), e.mode.as_str(), e.path.as_str())).collect::<Vec<_>>(), [("M", "100644", "rules/a.md"), ("A", "120000", "link"), ("D", "", "gone.md")]);
    }

    #[test]
    fn porcelain_v1_paths_and_renames() {
        let raw = b" M rules/a.md\0?? tasks/new.md\0R  knowledge/new.md\0knowledge/old.md\0UU tasks/x.md\0";
        assert_eq!(
            parse_porcelain_v1(raw),
            vec![(" M".into(), "rules/a.md".into()), ("??".into(), "tasks/new.md".into()), ("R ".into(), "knowledge/new.md".into()), ("UU".into(), "tasks/x.md".into())]
        );
    }

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

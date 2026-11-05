//! Security inspection before a commit, before a push, and of incoming changes (ADR-0031, threat T39/T40).
//!
//! Checks file *names* (machine-local databases, environment and credential files, keys, local-only knowledge,
//! machine-local Claude configuration, project repositories inside the workspace), file *modes* (symbolic links,
//! submodules), *paths* that are not portable (Windows reserved names, `..`, absolute or drive paths), and the *added
//! lines* of text files (private keys, access tokens, credentials in URLs, password assignments, machine-specific
//! paths). Findings name the file, line and kind — never the matched value.

use std::path::Path;

use serde::Serialize;

use crate::{CoreError, Result, git};

/// Bytes of content inspected per operation (larger changes are reported as not fully inspected).
pub const MAX_SCAN_BYTES: usize = 16 << 20;
/// Files larger than this are blocked (Git hosts reject them; they rarely belong in a workspace).
pub const MAX_FILE_BYTES: u64 = 50 << 20;
const WARN_FILE_BYTES: u64 = 5 << 20;
const MAX_FINDINGS_PER_FILE: usize = 10;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// The operation is refused while this is part of it.
    Block,
    /// Shown for a decision; does not refuse.
    Warn,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub path: String,
    pub line: Option<usize>,
    pub kind: &'static str,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub findings: Vec<Finding>,
    pub blocked: bool,
    pub files: usize,
    pub scanned_bytes: usize,
    /// Not everything could be inspected (size limit) — a warning is part of the findings.
    pub truncated: bool,
}

impl Inspection {
    fn push(&mut self, f: Finding) {
        if f.severity == Severity::Block {
            self.blocked = true;
        }
        self.findings.push(f);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Outgoing,
    Incoming,
}

/// Machine-specific context: the home folder whose paths must not travel.
pub struct Context<'a> {
    pub home: &'a Path,
}

fn finding(path: &str, line: Option<usize>, kind: &'static str, severity: Severity, message: impl Into<String>) -> Finding {
    Finding { path: path.to_string(), line, kind, severity, message: message.into() }
}

/// Names that must never travel (checked by name; contents are never read for this).
pub fn sensitive_reason(rel: &str) -> Option<&'static str> {
    let name = rel.rsplit(['/', '\\']).next().unwrap_or(rel).to_ascii_lowercase();
    if name == ".env" || (name.starts_with(".env.") && !matches!(name.as_str(), ".env.example" | ".env.sample" | ".env.template")) {
        return Some("environment file (may contain secrets)");
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if matches!(ext, "db" | "db-wal" | "db-shm" | "sqlite" | "sqlite3") || name.ends_with(".db-journal") {
        return Some("machine-local database");
    }
    if matches!(ext, "pem" | "key" | "p12" | "pfx" | "jks" | "keystore" | "kdbx") || name.starts_with("id_rsa") || name.starts_with("id_ed25519") || name.starts_with("id_ecdsa") {
        return Some("private key or keystore");
    }
    if matches!(name.as_str(), ".credentials.json" | "credentials.json" | ".git-credentials" | ".netrc" | "_netrc" | ".npmrc" | ".pypirc" | ".pgpass") {
        return Some("credential file");
    }
    None
}

/// A path that cannot be checked out safely on every machine (or could escape the folder).
pub fn unsafe_path_reason(rel: &str) -> Option<&'static str> {
    if rel.is_empty() || rel.starts_with('/') || rel.starts_with('\\') || rel.contains('\\') {
        return Some("absolute or backslash path");
    }
    if rel.chars().any(|c| c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*')) {
        return Some("characters that are not valid in file names on Windows");
    }
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Some("`.`/`..` or empty path component");
        }
        if part.eq_ignore_ascii_case(".git") {
            return Some("a `.git` component");
        }
        if part.ends_with('.') || part.ends_with(' ') {
            return Some("a name ending in a dot or space (not valid on Windows)");
        }
        let stem = part.split('.').next().unwrap_or(part).to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ((stem.starts_with("COM") || stem.starts_with("LPT")) && stem.len() == 4 && stem.as_bytes()[3].is_ascii_digit());
        if reserved {
            return Some("a reserved device name on Windows");
        }
    }
    None
}

/// Rules on the path and Git mode of a file in a change.
pub fn path_findings(rel: &str, mode: &str, dir: Direction) -> Vec<Finding> {
    let mut out = Vec::new();
    let out_block = if dir == Direction::Outgoing { Severity::Block } else { Severity::Warn };
    if let Some(why) = unsafe_path_reason(rel) {
        out.push(finding(rel, None, "unsafe-path", Severity::Block, format!("not a portable path: {why}")));
    }
    match mode {
        "120000" => out.push(finding(rel, None, "symlink", Severity::Block, "symbolic link — it could point outside the workspace; NEXUS does not sync links")),
        "160000" => out.push(finding(rel, None, "submodule", Severity::Warn, "a submodule — NEXUS does not sync submodules")),
        _ => {}
    }
    if let Some(why) = sensitive_reason(rel) {
        out.push(finding(rel, None, "sensitive-file", out_block, format!("{why} — never synced")));
    }
    let lower = rel.to_ascii_lowercase();
    if lower.starts_with(".nexus-local/") || lower == ".nexus-local" {
        out.push(finding(rel, None, "local-only-knowledge", out_block, "local-only knowledge stays on this machine"));
    }
    if lower.starts_with("repos/") {
        out.push(finding(rel, None, "project-repository", out_block, "project repositories cloned into repos/ are separate repositories"));
    }
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    if name == "claude.local.md" || (lower.contains(".claude/") && name == "settings.local.json") {
        out.push(finding(rel, None, "machine-local-config", out_block, "machine-local Claude configuration (`*.local.*`) stays on this machine"));
    }
    out
}

/// Rules on the added lines of a text file.
pub fn content_findings(rel: &str, lines: &[(usize, &str)], ctx: &Context<'_>) -> Vec<Finding> {
    let mut out = Vec::new();
    let home = ctx.home.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    let home_ok = home.len() > 3;
    for (n, line) in lines {
        if out.len() >= MAX_FINDINGS_PER_FILE {
            break;
        }
        if let Some(kind) = secret_in(line) {
            out.push(finding(rel, Some(*n), "secret", Severity::Block, format!("looks like {kind} — credentials never belong in a workspace")));
            continue;
        }
        if let Some(kind) = assignment_in(line) {
            out.push(finding(rel, Some(*n), "possible-secret", Severity::Warn, format!("a {kind} value is assigned here — make sure it is not a real credential")));
            continue;
        }
        let norm = line.replace('\\', "/").to_ascii_lowercase();
        if (home_ok && norm.contains(&home)) || machine_path_in(&norm) {
            out.push(finding(rel, Some(*n), "machine-path", Severity::Warn, "an absolute path of this machine — portable files use relative paths or project ids"));
        }
    }
    out
}

const TOKEN_PREFIXES: &[(&str, usize, &str)] = &[
    ("sk-ant-", 20, "an Anthropic API key"),
    ("ghp_", 30, "a GitHub token"),
    ("gho_", 30, "a GitHub token"),
    ("ghu_", 30, "a GitHub token"),
    ("ghs_", 30, "a GitHub token"),
    ("ghr_", 30, "a GitHub token"),
    ("github_pat_", 40, "a GitHub token"),
    ("glpat-", 20, "a GitLab token"),
    ("xoxb-", 20, "a Slack token"),
    ("xoxp-", 20, "a Slack token"),
    ("xoxa-", 20, "a Slack token"),
    ("sk_live_", 20, "a Stripe secret key"),
    ("AIza", 35, "a Google API key"),
    ("npm_", 36, "an npm token"),
];

fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn secret_in(line: &str) -> Option<&'static str> {
    if line.contains("-----BEGIN") && line.contains("PRIVATE KEY") {
        return Some("a private key");
    }
    for (prefix, min, kind) in TOKEN_PREFIXES {
        for (i, _) in line.match_indices(prefix) {
            if i > 0 && line[..i].chars().next_back().is_some_and(token_char) {
                continue; // part of a longer word
            }
            if line[i + prefix.len()..].chars().take_while(|c| token_char(*c)).count() >= *min {
                return Some(kind);
            }
        }
    }
    for (i, _) in line.match_indices("AKIA") {
        let tail: String = line[i + 4..].chars().take(16).collect();
        if tail.len() == 16 && tail.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
            return Some("an AWS access key");
        }
    }
    // JWT: three base64url parts, the first two starting with eyJ.
    for (i, _) in line.match_indices("eyJ") {
        let rest = &line[i..];
        let parts: Vec<&str> = rest.split(|c: char| !(token_char(c) || c == '.')).next().unwrap_or("").split('.').collect();
        if parts.len() >= 3 && parts[1].starts_with("eyJ") && parts.iter().take(3).all(|p| p.len() >= 10) {
            return Some("a JSON Web Token");
        }
    }
    let lower = line.to_ascii_lowercase();
    if let Some(i) = lower.find("bearer ") {
        let tail = line[i + 7..].trim_start();
        if tail.chars().take_while(|c| token_char(*c) || *c == '.').count() >= 20 {
            return Some("a bearer token");
        }
    }
    // user:password@ in a URL.
    for (i, _) in line.match_indices("://") {
        let rest = &line[i + 3..];
        let authority = rest.split(['/', ' ', '"', '\'', '`']).next().unwrap_or("");
        if let Some((cred, host)) = authority.rsplit_once('@')
            && let Some((user, pass)) = cred.split_once(':')
            && !user.is_empty()
            && pass.len() >= 3
            && !host.is_empty()
            && !pass.starts_with('$')
            && !pass.starts_with('{')
            && !pass.starts_with('<')
        {
            return Some("a password in a URL");
        }
    }
    None
}

fn assignment_in(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    const KEYS: &[(&str, &str)] = &[
        ("password", "password"),
        ("passwd", "password"),
        ("client_secret", "secret"),
        ("secret_key", "secret"),
        ("api_key", "API key"),
        ("apikey", "API key"),
        ("access_token", "token"),
        ("auth_token", "token"),
        ("private_key", "private key"),
    ];
    for (k, label) in KEYS {
        let Some(i) = lower.find(k) else { continue };
        let rest = lower[i + k.len()..].trim_start_matches(['"', '\'', ' ', '_']);
        let Some(rest) = rest.strip_prefix([':', '=']) else { continue };
        let value = rest.trim().trim_start_matches(['"', '\'']).trim_end_matches([',', ';', '"', '\'']);
        let value = value.split_whitespace().next().unwrap_or("");
        let placeholder = value.is_empty()
            || value.starts_with('<')
            || value.starts_with("${")
            || value.starts_with('{')
            || value.starts_with('$')
            || value.chars().all(|c| c == 'x' || c == '*' || c == '.')
            || matches!(value, "null" | "none" | "true" | "false" | "changeme" | "example" | "redacted" | "[redacted]");
        if !placeholder && value.len() >= 8 {
            return Some(label);
        }
    }
    None
}

fn machine_path_in(norm: &str) -> bool {
    for marker in ["c:/users/", "/users/", "/home/"] {
        if let Some(i) = norm.find(marker) {
            let name: String = norm[i + marker.len()..].chars().take_while(|c| !matches!(c, '/' | '"' | '\'' | ' ' | ')')).collect();
            if name.len() >= 2 && !matches!(name.as_str(), "shared" | "public" | "runner" | "<you>" | "you" | "me" | "name" | "user" | "username" | "example") {
                return true;
            }
        }
    }
    false
}

/// Parse a unified diff (`git diff -U0`, `git log -p`) into the added lines per file (`b/` side).
pub fn added_lines(diff: &str) -> Vec<(String, Vec<(usize, String)>)> {
    let mut out: Vec<(String, Vec<(usize, String)>)> = Vec::new();
    let mut line_no = 0usize;
    for l in diff.lines() {
        if let Some(path) = l.strip_prefix("+++ ") {
            let p = path.strip_prefix("b/").unwrap_or(path).trim();
            out.push((if p == "/dev/null" { String::new() } else { p.to_string() }, Vec::new()));
            continue;
        }
        if l.starts_with("@@") {
            // @@ -a,b +c,d @@
            line_no = l.split('+').nth(1).and_then(|s| s.split([',', ' ']).next()).and_then(|n| n.parse().ok()).unwrap_or(1);
            continue;
        }
        if l.starts_with("diff --git") || l.starts_with("--- ") || l.starts_with("index ") {
            continue;
        }
        if let Some(added) = l.strip_prefix('+')
            && let Some((_, lines)) = out.last_mut()
        {
            lines.push((line_no, added.to_string()));
            line_no += 1;
        } else if !l.starts_with('-') && !l.starts_with('\\') {
            line_no += 1;
        }
    }
    out.retain(|(p, _)| !p.is_empty());
    out
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0)
}

/// What a commit of these working-tree changes would contain: `(status, path)` with status `A`/`M`/`D`.
pub fn inspect_worktree(repo: &Path, ctx: &Context<'_>, changes: &[(String, String)]) -> Result<Inspection> {
    let mut insp = Inspection { files: changes.len(), ..Default::default() };
    for (status, rel) in changes {
        let full = repo.join(rel);
        let meta = std::fs::symlink_metadata(&full).ok();
        let mode = match &meta {
            Some(m) if m.file_type().is_symlink() => "120000",
            _ => "100644",
        };
        for f in path_findings(rel, mode, Direction::Outgoing) {
            insp.push(f);
        }
        if status == "D" || mode == "120000" {
            continue;
        }
        let Some(meta) = meta.filter(|m| m.is_file()) else { continue };
        if meta.len() > MAX_FILE_BYTES {
            insp.push(finding(rel, None, "large-file", Severity::Block, format!("{} MB — too large to sync", meta.len() >> 20)));
            continue;
        }
        if meta.len() > WARN_FILE_BYTES {
            insp.push(finding(rel, None, "large-file", Severity::Warn, format!("{} MB — large files make the workspace slow to sync", meta.len() >> 20)));
        }
        if insp.scanned_bytes >= MAX_SCAN_BYTES {
            insp.truncated = true;
            continue;
        }
        let lines: Vec<(usize, String)> = if status == "A" {
            let bytes = std::fs::read(&full).map_err(|e| CoreError::io("cannot read", &full, e))?;
            insp.scanned_bytes += bytes.len();
            if is_binary(&bytes) {
                continue;
            }
            String::from_utf8_lossy(&bytes).lines().enumerate().map(|(i, l)| (i + 1, l.to_string())).collect()
        } else {
            let (diff, truncated) = git::diff_text(repo, &["-U0", "HEAD", "--", rel], MAX_SCAN_BYTES - insp.scanned_bytes)?;
            insp.scanned_bytes += diff.len();
            insp.truncated |= truncated;
            added_lines(&diff).into_iter().flat_map(|(_, l)| l).collect()
        };
        let refs: Vec<(usize, &str)> = lines.iter().map(|(n, l)| (*n, l.as_str())).collect();
        for f in content_findings(rel, &refs, ctx) {
            insp.push(f);
        }
    }
    if insp.truncated {
        insp.push(finding("", None, "not-fully-inspected", Severity::Warn, "the change is larger than NEXUS inspects — review it in your Git tool too"));
    }
    Ok(insp)
}

/// What a push of `range` (e.g. `origin/main..HEAD`, or `HEAD` for a first push = the whole history) would send.
pub fn inspect_history(repo: &Path, ctx: &Context<'_>, range: &str) -> Result<Inspection> {
    let mut insp = Inspection::default();
    // Every path touched by any commit in the range (a file added and removed again still travels in history).
    let (names, _) = log_text(repo, &["log", "--format=", "--name-only", range, "--"], 4 << 20)?;
    let mut paths: Vec<&str> = names.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    paths.sort();
    paths.dedup();
    insp.files = paths.len();
    let modes: std::collections::HashMap<String, String> = git::ls_tree(repo, "HEAD")?.into_iter().map(|(m, p)| (p, m)).collect();
    for p in &paths {
        for f in path_findings(p, modes.get(*p).map(String::as_str).unwrap_or("100644"), Direction::Outgoing) {
            insp.push(f);
        }
    }
    let (patches, truncated) = log_text(repo, &["log", "-p", "--format=", "--no-ext-diff", "--no-textconv", "-U0", range, "--"], MAX_SCAN_BYTES)?;
    insp.scanned_bytes = patches.len();
    insp.truncated = truncated;
    for (path, lines) in added_lines(&patches) {
        let refs: Vec<(usize, &str)> = lines.iter().map(|(n, l)| (*n, l.as_str())).collect();
        for f in content_findings(&path, &refs, ctx) {
            if !insp.findings.iter().any(|x| x.path == f.path && x.line == f.line && x.kind == f.kind) {
                insp.push(f);
            }
        }
    }
    if truncated {
        insp.push(finding("", None, "not-fully-inspected", Severity::Warn, "the history is larger than NEXUS inspects — review it in your Git tool too"));
    }
    Ok(insp)
}

/// Incoming changes `from..to`: paths and modes that must not be checked out here.
pub fn inspect_incoming(repo: &Path, from: &str, to: &str) -> Result<Inspection> {
    let mut insp = Inspection::default();
    let entries = git::diff_raw(repo, from, Some(to))?;
    insp.files = entries.len();
    for e in &entries {
        if e.status == "D" {
            continue;
        }
        for f in path_findings(&e.path, &e.mode, Direction::Incoming) {
            insp.push(f);
        }
    }
    Ok(insp)
}

fn log_text(repo: &Path, args: &[&str], max: usize) -> Result<(String, bool)> {
    // `git log` through the bounded diff runner (same safety flags).
    let mut cmd_args: Vec<&str> = Vec::new();
    cmd_args.extend_from_slice(args);
    git::bounded(repo, &cmd_args, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_modes_and_paths() {
        assert!(path_findings("config/.env", "100644", Direction::Outgoing).iter().any(|f| f.severity == Severity::Block));
        assert!(path_findings(".env.example", "100644", Direction::Outgoing).is_empty());
        assert!(path_findings(".nexus-local/tasks/a.md", "100644", Direction::Outgoing).iter().any(|f| f.kind == "local-only-knowledge"));
        assert!(path_findings("tools/run", "120000", Direction::Incoming).iter().any(|f| f.kind == "symlink" && f.severity == Severity::Block));
        assert!(path_findings("notes/aux.md", "100644", Direction::Incoming).iter().any(|f| f.kind == "unsafe-path"));
        assert!(path_findings("a/../b.md", "100644", Direction::Incoming).iter().any(|f| f.kind == "unsafe-path"));
        assert!(path_findings(".claude/settings.local.json", "100644", Direction::Outgoing).iter().any(|f| f.kind == "machine-local-config"));
        assert!(path_findings("rules/keys.md", "100644", Direction::Outgoing).is_empty());
        assert_eq!(path_findings("data/nexus.db", "100644", Direction::Incoming)[0].severity, Severity::Warn, "incoming: reported, not blocking");
    }

    #[test]
    fn secrets_are_found_without_reporting_values() {
        let ctx = Context { home: Path::new("C:\\Users\\alex") };
        let gh = format!("token = {}{}", "ghp_", "a".repeat(36));
        let aws = format!("key {}{}", "AKIA", "ABCDEFGHIJKLMNOP");
        let url = format!("url = https://deploy:{}@git.example.invalid/x.git", "hunter".to_string() + "22");
        let pem = format!("-----BEGIN {} PRIVATE KEY-----", "OPENSSH");
        let lines = vec![
            (1, gh.as_str()),
            (2, url.as_str()),
            (3, "password: \"correct-horse-battery\""),
            (4, "password: <your password>"),
            (5, "log at C:\\Users\\alex\\AppData\\x.log"),
            (6, aws.as_str()),
            (7, pem.as_str()),
            (8, "The ghp_ prefix marks GitHub tokens."),
        ];
        let f = content_findings("notes/n.md", &lines, &ctx);
        let kinds: Vec<(usize, &str)> = f.iter().map(|f| (f.line.unwrap(), f.kind)).collect();
        assert_eq!(kinds, [(1, "secret"), (2, "secret"), (3, "possible-secret"), (5, "machine-path"), (6, "secret"), (7, "secret")]);
        assert!(f.iter().all(|f| !f.message.contains("hunter22") && !f.message.contains("aaaa") && !f.message.contains("correct-horse")));
    }

    #[test]
    fn unified_diffs_yield_added_lines_with_numbers() {
        let d = "diff --git a/x.md b/x.md\nindex 1..2 100644\n--- a/x.md\n+++ b/x.md\n@@ -3,0 +4,2 @@\n+one\n+two\ndiff --git a/y b/y\n--- /dev/null\n+++ b/y\n@@ -0,0 +1 @@\n+new\n";
        let a = added_lines(d);
        assert_eq!(a, vec![("x.md".into(), vec![(4, "one".into()), (5, "two".into())]), ("y".into(), vec![(1, "new".into())])]);
    }
}

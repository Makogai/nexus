//! Effective-instructions inspector (ADR-0019): which instruction files Claude Code itself loads for a session, in
//! Claude's native order, plus what NEXUS appends. Read-only: NEXUS never edits, reorders or overrides these files.
//!
//! Model (Claude Code docs, verified with the `InstructionsLoaded` hook on 2.1.311 — see docs/RESEARCH.md):
//! 1. managed policy `CLAUDE.md` (machine folder) and managed `claudeMd` settings — cannot be excluded;
//! 2. user memory `<config dir>/CLAUDE.md` and user rules `<config dir>/rules/**/*.md` (follows `CLAUDE_CONFIG_DIR`);
//! 3. for every directory from the filesystem root down to the working directory: `CLAUDE.md`, `.claude/CLAUDE.md`,
//!    `.claude/rules/**/*.md` (rules with `paths:` frontmatter load on demand), `CLAUDE.local.md`;
//!    the traversal does not stop at the Git root or the home folder;
//! 4. `@path` imports inside those files (relative to the importing file, up to four hops, not inside code);
//! 5. `claudeMdExcludes` (managed, user and project settings, glob patterns on absolute paths) removes files except
//!    the managed one; `CLAUDE_CODE_DISABLE_CLAUDE_MDS` disables all of them;
//! 6. AGENTS.md is read by Claude's AGENTS.md support only when no CLAUDE.md applies (documented, not observed);
//! 7. subdirectory `CLAUDE.md` files below the working directory load when Claude works there.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::policy::{self, PolicySummary};

/// Claude skips CLAUDE.md files larger than this.
pub const CLAUDE_SKIP_BYTES: u64 = 4 * 1024 * 1024;
/// NEXUS reads at most this much of a file (for size, line counts and duplicate detection).
const READ_LIMIT: u64 = 1024 * 1024;
const MAX_IMPORT_HOPS: usize = 4;
const MAX_RULE_FILES: usize = 200;
const ADVISORY_LINES: usize = 200;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    Managed,
    User,
    Project,
    Local,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Load {
    /// Loaded when the session starts.
    AtStart,
    /// Loaded later, when Claude touches matching files.
    OnDemand,
    /// Removed by a `claudeMdExcludes` pattern.
    Excluded,
    /// Present but not loaded (e.g. AGENTS.md while a CLAUDE.md applies, files over 4 MiB, missing imports).
    NotLoaded,
    /// Documented fallback whose loading could not be observed (AGENTS.md).
    Fallback,
    /// An import from outside the project: Claude loads it only after the user approves external imports once per
    /// project (verified: not loaded in a session without that approval).
    NeedsApproval,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub scope: Scope,
    /// `claude-md`, `rule`, `import`, `agents-md` or `managed-setting`.
    pub kind: &'static str,
    pub path: Option<PathBuf>,
    pub load: Load,
    pub reason: Option<String>,
    pub chars: usize,
    pub lines: usize,
    pub imported_from: Option<PathBuf>,
    /// Conditional rules: the `paths:` globs that trigger loading.
    pub paths: Vec<String>,
    #[serde(skip)]
    text: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub layers: Vec<Layer>,
    pub files_at_start: usize,
    pub chars_at_start: usize,
    pub excludes: Vec<String>,
    pub agents_mode: String,
    pub disabled: bool,
    pub warnings: Vec<String>,
}

pub struct Input<'a> {
    /// Folder of the machine-wide managed files (e.g. `C:\Program Files\ClaudeCode`).
    pub managed_dir: Option<&'a Path>,
    pub policy: &'a PolicySummary,
    /// Effective config dir of the profile (`CLAUDE_CONFIG_DIR`, or `~/.claude`).
    pub config_dir: &'a Path,
    pub home: &'a Path,
    pub cwd: &'a Path,
    /// `CLAUDE_CODE_DISABLE_CLAUDE_MDS` is set in the environment Claude receives.
    pub env_disabled: bool,
    /// `claudeMdExcludes` NEXUS adds to the session settings (profile instruction isolation, ADR-0020).
    pub session_excludes: &'a [String],
}

pub fn inspect(i: &Input<'_>) -> Inspection {
    let mut excludes = i.policy.claude_md_excludes.clone();
    let mut agents_mode = i.policy.instruction_files.clone();
    let (user_ex, user_mode) = settings_file(&i.config_dir.join("settings.json"));
    excludes.extend(user_ex);
    agents_mode = agents_mode.or(user_mode);
    for f in [".claude/settings.json", ".claude/settings.local.json"] {
        excludes.extend(settings_file(&i.cwd.join(f)).0);
    }
    let agents_mode = agents_mode.unwrap_or_else(|| policy::INSTRUCTION_FILE_MODES[0].to_string());
    let mut sourced: Vec<(String, &'static str)> = excludes.iter().map(|p| (p.clone(), "claudeMdExcludes")).collect();
    sourced.extend(i.session_excludes.iter().map(|p| (p.clone(), "NEXUS profile isolation")));
    excludes.extend(i.session_excludes.iter().cloned());
    let mut b = Builder { layers: Vec::new(), seen: HashSet::new(), excludes: &sourced, cwd: i.cwd };

    if let Some(dir) = i.managed_dir {
        b.file(Scope::Managed, "claude-md", &dir.join("CLAUDE.md"), false);
    }
    if i.policy.managed_claude_md {
        b.layers.push(Layer {
            scope: Scope::Managed,
            kind: "managed-setting",
            path: None,
            load: Load::AtStart,
            reason: Some("organisation instructions in managed settings (`claudeMd`; content not shown)".into()),
            chars: 0,
            lines: 0,
            imported_from: None,
            paths: vec![],
            text: None,
        });
    }
    if agents_mode != "managed-only" {
        b.file(Scope::User, "claude-md", &i.config_dir.join("CLAUDE.md"), true);
        b.rules(Scope::User, &i.config_dir.join("rules"));
        let default_cfg = i.home.join(".claude");
        let mut dirs: Vec<&Path> = i.cwd.ancestors().collect();
        dirs.reverse();
        for dir in dirs {
            let before = b.layers.len();
            b.file(Scope::Project, "claude-md", &dir.join("CLAUDE.md"), true);
            b.file(Scope::Project, "claude-md", &dir.join(".claude").join("CLAUDE.md"), true);
            b.rules(Scope::Project, &dir.join(".claude").join("rules"));
            b.file(Scope::Local, "claude-md", &dir.join("CLAUDE.local.md"), true);
            if !paths_eq(&default_cfg, i.config_dir) {
                for l in &mut b.layers[before..] {
                    if l.load != Load::Excluded && l.path.as_ref().is_some_and(|p| p.starts_with(&default_cfg)) {
                        l.reason = Some(format!(
                            "the default profile's {} — loaded as project memory because the folder is inside your home folder",
                            l.path.as_ref().and_then(|p| p.strip_prefix(&default_cfg).ok()).map(|p| p.display().to_string()).unwrap_or_default()
                        ));
                    }
                }
            }
        }
        let has_claude_md = b.layers.iter().any(|l| l.kind == "claude-md" && l.scope != Scope::Managed && l.scope != Scope::User && l.load != Load::Excluded);
        for dir in i.cwd.ancestors() {
            let p = dir.join("AGENTS.md");
            if p.is_file() {
                let (load, reason) = match agents_mode.as_str() {
                    "claude-md" => (Load::NotLoaded, "AGENTS.md support is turned off (Project instructions: claude-md)"),
                    "claude-md-and-agents-md" => (Load::Fallback, "read alongside CLAUDE.md (Project instructions setting)"),
                    _ if has_claude_md => (Load::NotLoaded, "a CLAUDE.md applies, so Claude does not read AGENTS.md"),
                    _ => (Load::Fallback, "documented fallback when no CLAUDE.md applies; loading could not be observed"),
                };
                b.push_file(Scope::Project, "agents-md", &p, None, load, Some(reason.into()));
            }
        }
    }

    let disabled = i.env_disabled;
    if disabled {
        for l in &mut b.layers {
            if l.load == Load::AtStart || l.load == Load::OnDemand {
                l.load = Load::NotLoaded;
                l.reason = Some("CLAUDE_CODE_DISABLE_CLAUDE_MDS is set".into());
            }
        }
    }
    let mut warnings = Vec::new();
    for l in &b.layers {
        if l.load == Load::AtStart && l.lines > ADVISORY_LINES {
            warnings.push(format!("{} has {} lines (Claude recommends under {ADVISORY_LINES})", short(l), l.lines));
        }
        if l.reason.as_deref().is_some_and(|r| r.starts_with("the default profile's")) && l.load == Load::AtStart {
            warnings.push(format!(
                "{} belongs to the default Claude profile but also reaches this profile's sessions — turn on instruction isolation for this profile to keep it out",
                short(l)
            ));
        }
    }
    let at_start: Vec<&Layer> = b.layers.iter().filter(|l| l.load == Load::AtStart).collect();
    Inspection {
        files_at_start: at_start.iter().filter(|l| l.path.is_some()).count(),
        chars_at_start: at_start.iter().map(|l| l.chars).sum(),
        layers: b.layers,
        excludes,
        agents_mode,
        disabled,
        warnings,
    }
}

impl Inspection {
    /// Normalised text of everything loaded at start (for "already in CLAUDE.md" checks).
    pub fn loaded_text(&self) -> String {
        self.layers
            .iter()
            .filter(|l| l.load == Load::AtStart)
            .filter_map(|l| l.text.as_deref())
            .map(normalize)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The loaded layer whose text contains `needle` (normalised), if any.
    pub fn containing(&self, needle: &str) -> Option<&Layer> {
        let n = normalize(needle);
        if n.chars().count() < 12 {
            return None; // too short to call a duplicate
        }
        self.layers.iter().filter(|l| l.load == Load::AtStart).find(|l| l.text.as_deref().is_some_and(|t| normalize(t).contains(&n)))
    }
}

/// Lowercase, whitespace collapsed, Markdown list/heading markers removed.
pub fn normalize(s: &str) -> String {
    s.lines()
        .map(|l| l.trim().trim_start_matches(['#', '-', '*', '>']).trim())
        .filter(|l| !l.is_empty())
        .flat_map(|l| l.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn short(l: &Layer) -> String {
    l.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| l.kind.to_string())
}

fn paths_eq(a: &Path, b: &Path) -> bool {
    crate::paths::same_path(a, b)
}

fn settings_file(path: &Path) -> (Vec<String>, Option<String>) {
    let Ok(meta) = std::fs::metadata(path) else { return (vec![], None) };
    if meta.len() > READ_LIMIT {
        return (vec![], None);
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map(|v| policy::instruction_settings(&v))
        .unwrap_or_default()
}

struct Builder<'a> {
    layers: Vec<Layer>,
    seen: HashSet<String>,
    /// (pattern, where it comes from)
    excludes: &'a [(String, &'static str)],
    cwd: &'a Path,
}

impl Builder<'_> {
    fn excluded(&self, path: &Path) -> Option<String> {
        let p = path.to_string_lossy().replace('\\', "/");
        self.excludes.iter().find(|(pat, _)| glob_match(&pat.replace('\\', "/"), &p)).map(|(pat, src)| format!("{src}: {pat}"))
    }

    fn file(&mut self, scope: Scope, kind: &'static str, path: &Path, excludable: bool) {
        if !path.is_file() {
            return;
        }
        let ex = if excludable { self.excluded(path) } else { None };
        let (load, reason) = match ex {
            Some(why) => (Load::Excluded, Some(why)),
            None => (Load::AtStart, None),
        };
        if let Some(idx) = self.push_file(scope, kind, path, None, load, reason)
            && load == Load::AtStart
        {
            self.imports(scope, idx, 1);
        }
    }

    /// Returns the index of the new layer (None if the path was already listed).
    fn push_file(&mut self, scope: Scope, kind: &'static str, path: &Path, from: Option<&Path>, load: Load, reason: Option<String>) -> Option<usize> {
        let key = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().to_lowercase();
        if !self.seen.insert(key) {
            return None;
        }
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let (load, reason, text) = if size > CLAUDE_SKIP_BYTES {
            (Load::NotLoaded, Some("larger than 4 MiB — Claude skips it".into()), None)
        } else {
            let text = read_bounded(path);
            (load, reason, text)
        };
        let text = text.map(|t| strip_html_comments(&t));
        let (chars, lines) = text.as_deref().map(|t| (t.chars().count(), t.lines().count())).unwrap_or((0, 0));
        self.layers.push(Layer {
            scope,
            kind,
            path: Some(path.to_path_buf()),
            load,
            reason,
            chars,
            lines,
            imported_from: from.map(Path::to_path_buf),
            paths: vec![],
            text,
        });
        Some(self.layers.len() - 1)
    }

    fn rules(&mut self, scope: Scope, dir: &Path) {
        let mut files = Vec::new();
        collect_md(dir, 0, &mut files);
        files.sort();
        for f in files.into_iter().take(MAX_RULE_FILES) {
            let text = read_bounded(&f).unwrap_or_default();
            let globs = rule_paths(&text);
            let ex = self.excluded(&f);
            let (load, reason) = match (ex, globs.is_empty()) {
                (Some(why), _) => (Load::Excluded, Some(why)),
                (None, true) => (Load::AtStart, None),
                (None, false) => (Load::OnDemand, Some("conditional rule: loads when Claude touches matching files".into())),
            };
            if let Some(idx) = self.push_file(scope, "rule", &f, None, load, reason) {
                self.layers[idx].paths = globs;
                if load == Load::AtStart {
                    self.imports(scope, idx, 1);
                }
            }
        }
    }

    fn imports(&mut self, scope: Scope, idx: usize, hop: usize) {
        if hop > MAX_IMPORT_HOPS {
            return;
        }
        let Some(parent) = self.layers[idx].path.clone() else { return };
        let Some(text) = self.layers[idx].text.clone() else { return };
        let base = parent.parent().map(Path::to_path_buf).unwrap_or_default();
        for raw in import_refs(&text) {
            let target = resolve_import(&raw, &base);
            let Some(target) = target else { continue };
            if !target.is_file() {
                continue;
            }
            // Project-level imports from outside the working directory need a one-time approval in Claude.
            let outside = scope != Scope::User && !target.starts_with(self.cwd);
            let (load, reason) = if outside {
                (Load::NeedsApproval, Some("outside the project: Claude loads it only after you approve external imports for this project".to_string()))
            } else {
                (Load::AtStart, None)
            };
            if let Some(child) = self.push_file(scope, "import", &target, Some(&parent), load, reason)
                && load == Load::AtStart
            {
                self.imports(scope, child, hop + 1);
            }
        }
    }
}

fn read_bounded(path: &Path) -> Option<String> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    f.take(READ_LIMIT).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn collect_md(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() || out.len() >= MAX_RULE_FILES {
            continue;
        }
        let p = e.path();
        if ft.is_dir() && depth < 4 {
            collect_md(&p, depth + 1, out);
        } else if ft.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md")) {
            out.push(p);
        }
    }
}

/// `paths:` globs from a rule's frontmatter (the only key Claude reads there).
fn rule_paths(text: &str) -> Vec<String> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return vec![];
    }
    let mut out = Vec::new();
    let mut in_paths = false;
    for line in lines {
        let t = line.trim();
        if t == "---" {
            break;
        }
        if let Some(rest) = t.strip_prefix("paths:") {
            in_paths = true;
            let rest = rest.trim();
            if rest.starts_with('[') {
                out.extend(rest.trim_matches(['[', ']']).split(',').map(|s| s.trim().trim_matches(['"', '\'']).to_string()).filter(|s| !s.is_empty()));
                in_paths = false;
            } else if !rest.is_empty() {
                out.push(rest.trim_matches(['"', '\'']).to_string());
                in_paths = false;
            }
        } else if in_paths && t.starts_with("- ") {
            out.push(t[2..].trim().trim_matches(['"', '\'']).to_string());
        } else if !line.starts_with([' ', '\t']) {
            in_paths = false;
        }
    }
    out
}

/// `@path` references outside fenced code blocks and code spans.
pub fn import_refs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut in_code = false;
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '`' {
                in_code = !in_code;
            } else if c == '@' && !in_code && (i == 0 || chars[i - 1].is_whitespace()) {
                let mut j = i + 1;
                let mut path = String::new();
                while j < chars.len() {
                    if chars[j] == '\\' && j + 1 < chars.len() && chars[j + 1] == ' ' {
                        path.push(' ');
                        j += 2;
                        continue;
                    }
                    if chars[j].is_whitespace() {
                        break;
                    }
                    path.push(chars[j]);
                    j += 1;
                }
                let looks_like_path = path.contains('/') || path.contains('\\') || path.contains('.') || path.starts_with('~');
                if !path.is_empty() && looks_like_path && !path.contains('@') {
                    out.push(path.trim_end_matches([',', ';', ')', ':', '.']).to_string());
                }
                i = j;
                continue;
            }
            i += 1;
        }
    }
    out
}

fn resolve_import(raw: &str, base: &Path) -> Option<PathBuf> {
    let p = if let Some(rest) = raw.strip_prefix("~/") {
        crate::paths::home_dir().ok()?.join(rest)
    } else {
        let p = PathBuf::from(raw);
        if p.is_absolute() { p } else { base.join(p) }
    };
    Some(dunce::canonicalize(&p).unwrap_or(p))
}

fn strip_html_comments(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Glob match on `/`-separated paths, **case-sensitive like Claude** (verified: a lower-case drive letter does not
/// match): `**` spans directories, `*` and `?` stay within one segment, `[...]` is a character class.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = path.chars().collect();
    fn class(p: &[char]) -> Option<(Vec<char>, usize)> {
        // p[0] == '['; returns the members and the index after ']'.
        let close = p.iter().skip(2).position(|c| *c == ']').map(|i| i + 2)?;
        Some((p[1..close].to_vec(), close + 1))
    }
    fn m(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('*') if p.get(1) == Some(&'*') => {
                let rest = if p.get(2) == Some(&'/') { &p[3..] } else { &p[2..] };
                (0..=s.len()).any(|i| m(rest, &s[i..])) || (p.get(2) == Some(&'/') && m(&p[3..], s))
            }
            Some('*') => (0..=s.len()).take_while(|i| *i == 0 || s[i - 1] != '/').any(|i| m(&p[1..], &s[i..])),
            Some('?') => s.first().is_some_and(|c| *c != '/') && m(&p[1..], &s[1..]),
            Some('[') => match class(p) {
                Some((members, next)) => s.first().is_some_and(|c| members.contains(c)) && m(&p[next..], &s[1..]),
                None => s.first() == Some(&'[') && m(&p[1..], &s[1..]),
            },
            Some(c) => s.first() == Some(c) && m(&p[1..], &s[1..]),
        }
    }
    m(&p, &s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(p: &Path, t: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, t).unwrap();
    }

    #[test]
    fn globs() {
        assert!(glob_match("**/parent/CLAUDE.local.md", "C:/t/parent/CLAUDE.local.md"));
        assert!(glob_match("/home/u/.claude/rules/**", "/home/u/.claude/rules/a/b.md"));
        assert!(glob_match("**/*.md", "/x/y.md"));
        assert!(!glob_match("/x/*.md", "/x/y/z.md"));
        assert!(glob_match("/x/?.md", "/x/a.md") && !glob_match("/x/?.md", "/x/ab.md"));
        assert!(glob_match("C:/my home [(]x[)]/.claude/rules/**", "C:/my home (x)/.claude/rules/a/b.md"), "bracket escapes");
        assert!(!glob_match("c:/home/.claude/CLAUDE.md", "C:/home/.claude/CLAUDE.md"), "case-sensitive, like Claude");
    }

    #[test]
    fn imports_are_found_outside_code_only() {
        let text = "See @docs/a.md and @~/x.md.\n`@not/this.md`\n```\n@nor/this.md\n```\nmail me@example.com @Design\\ Docs/b.md";
        assert_eq!(import_refs(text), ["docs/a.md", "~/x.md", "Design Docs/b.md"]);
    }

    #[test]
    fn layers_follow_claudes_order_excludes_and_home_bleed() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let home = root.join("home");
        let cfg = root.join("cfg");
        let managed = root.join("managed");
        let proj = home.join("work").join("proj");
        w(&managed.join("CLAUDE.md"), "Org rule");
        w(&cfg.join("CLAUDE.md"), "User memory\n@extra.md\n");
        w(&cfg.join("extra.md"), "User extra (imported)\n@deeper.md\n");
        w(&cfg.join("deeper.md"), "Deeper import");
        w(&cfg.join("rules").join("style.md"), "Prefer small functions.");
        w(&home.join(".claude").join("CLAUDE.md"), "Default profile memory");
        w(&home.join("work").join("CLAUDE.md"), "Parent folder memory");
        w(&proj.join("CLAUDE.md"), "# Project\nAlways run the linter before committing.\n<!-- hidden note -->\n");
        w(&proj.join(".claude").join("CLAUDE.md"), "Dot-claude memory");
        w(&proj.join(".claude").join("rules").join("rust.md"), "---\npaths:\n  - \"src/**/*.rs\"\n---\nUse clippy.");
        w(&proj.join(".claude").join("rules").join("always.md"), "Unconditional project rule");
        w(&proj.join("CLAUDE.local.md"), "Local notes");
        w(&proj.join("AGENTS.md"), "Agents");
        w(&proj.join(".claude").join("settings.json"), r#"{"claudeMdExcludes": ["**/work/CLAUDE.md"], "env": {"SECRET": "x"}}"#);

        let pol = PolicySummary::default();
        let i = inspect(&Input { managed_dir: Some(&managed), policy: &pol, config_dir: &cfg, home: &home, cwd: &proj, env_disabled: false, session_excludes: &[] });
        // Only the synthetic tree: real ancestors of the temp dir (outside `root`) are machine-specific.
        let mine: Vec<&Layer> = i.layers.iter().filter(|l| l.path.as_ref().is_some_and(|p| p.starts_with(&root))).collect();
        let order: Vec<(Scope, String, Load)> = mine
            .iter()
            .map(|l| (l.scope, l.path.as_ref().unwrap().strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"), l.load))
            .collect();
        use Load::*;
        use Scope::*;
        assert_eq!(order, [
            (Managed, "managed/CLAUDE.md".into(), AtStart),
            (User, "cfg/CLAUDE.md".into(), AtStart),
            (User, "cfg/extra.md".into(), AtStart),
            (User, "cfg/deeper.md".into(), AtStart),
            (User, "cfg/rules/style.md".into(), AtStart),
            (Project, "home/.claude/CLAUDE.md".into(), AtStart),
            (Project, "home/work/CLAUDE.md".into(), Excluded),
            (Project, "home/work/proj/CLAUDE.md".into(), AtStart),
            (Project, "home/work/proj/.claude/CLAUDE.md".into(), AtStart),
            (Project, "home/work/proj/.claude/rules/always.md".into(), AtStart),
            (Project, "home/work/proj/.claude/rules/rust.md".into(), OnDemand),
            (Local, "home/work/proj/CLAUDE.local.md".into(), AtStart),
            (Project, "home/work/proj/AGENTS.md".into(), NotLoaded),
        ]);
        assert_eq!(mine[10].paths, ["src/**/*.rs"]);
        assert!(mine[5].reason.as_deref().unwrap().starts_with("the default profile's"));
        assert!(i.warnings.iter().any(|w| w.contains("belongs to the default Claude profile")));
        assert!(i.containing("always run the linter before committing").is_some(), "duplicate detection");
        assert!(i.containing("hidden note").is_none(), "HTML comments are stripped like Claude does");
        assert!(i.containing("Parent folder memory").is_none(), "excluded files do not count");
        assert!(!serde_json::to_string(&i).unwrap().contains("Always run the linter"), "content is never serialised");

        let off = inspect(&Input { managed_dir: Some(&managed), policy: &pol, config_dir: &cfg, home: &home, cwd: &proj, env_disabled: true, session_excludes: &[] });
        assert!(off.disabled && off.layers.iter().all(|l| l.load != AtStart));
    }
}

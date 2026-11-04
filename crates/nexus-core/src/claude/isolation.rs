//! Opt-in profile instruction isolation (ADR-0020).
//!
//! Claude Code loads `CLAUDE.md`, `.claude/CLAUDE.md` and `.claude/rules/**` from *every* ancestor of the working
//! directory. For a project inside the home folder this includes `~/.claude` — the **default profile's** personal
//! memory — even when the session runs another profile through `CLAUDE_CONFIG_DIR` (verified on 2.1.311).
//!
//! The officially supported control is the `claudeMdExcludes` setting. NEXUS adds it to the session-scoped
//! `--settings` file only (never to a Claude config dir — ADR-0004). Arrays merge with managed/user/project values,
//! so organisation exclusions stay in force, and Claude never lets it exclude the managed `CLAUDE.md`.
//!
//! Verified pattern behaviour (2.1.311, Windows): matching is case-sensitive; absolute paths with `/` or `\` work;
//! a glob over a path containing `(`, `)` silently fails unless those characters are bracket-escaped (`[(]`);
//! backslash escaping does not work. NEXUS therefore emits the exact literal path of every existing file (built from
//! the session's own working-directory path, so the case matches what Claude sees) plus a bracket-escaped
//! `rules/**` glob as a safety net for rule files created later.

use std::path::{Path, PathBuf};

use crate::paths;

const MAX_RULE_FILES: usize = 200;

/// `claudeMdExcludes` patterns that keep the default profile's personal instructions out of a session of
/// another profile. Empty when the profile *is* the default profile or the folder is not inside the home folder.
pub fn excludes(home: &Path, profile_config_dir: &Path, cwd: &Path) -> Vec<String> {
    let default_cfg = home.join(".claude");
    if paths::same_path(profile_config_dir, &default_cfg) {
        return Vec::new();
    }
    // The home folder as it appears in the session's working directory (Claude derives ancestors from the cwd).
    let Some(base) = cwd.ancestors().find(|a| paths::same_path(a, home)).map(Path::to_path_buf) else {
        return Vec::new();
    };
    let dir = base.join(".claude");
    if profile_config_dir.starts_with(&dir) || paths::same_path(profile_config_dir, &dir) {
        return Vec::new();
    }
    let slash = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let mut out = vec![slash(&dir.join("CLAUDE.md"))];
    let mut rules = Vec::new();
    collect(&dir.join("rules"), 0, &mut rules);
    rules.sort();
    out.extend(rules.iter().take(MAX_RULE_FILES).map(|p| slash(p)));
    if let Some(glob) = escape_glob(&slash(&dir.join("rules"))) {
        out.push(format!("{glob}/**"));
    }
    out.dedup();
    out
}

/// Bracket-escape glob metacharacters in a literal path prefix (`(` → `[(]`). `None` for prefixes that cannot be
/// escaped reliably (`!`, `]`, `\`), where only the literal file paths are used.
pub fn escape_glob(prefix: &str) -> Option<String> {
    if prefix.contains(['!', ']', '\\']) {
        return None;
    }
    Some(
        prefix
            .chars()
            .map(|c| if "()[{}*?+@".contains(c) { format!("[{c}]") } else { c.to_string() })
            .collect(),
    )
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if out.len() >= MAX_RULE_FILES || ft.is_symlink() {
            continue;
        }
        let p = e.path();
        if ft.is_dir() && depth < 4 {
            collect(&p, depth + 1, out);
        } else if ft.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md")) {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_only_the_default_profile_files_for_projects_inside_home() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let home = root.join("my home (x)");
        let cfg = root.join("cfg");
        let proj = home.join("work").join("proj");
        for d in [home.join(".claude").join("rules").join("sub"), proj.clone(), cfg.clone()] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(home.join(".claude").join("rules").join("a.md"), "x").unwrap();
        std::fs::write(home.join(".claude").join("rules").join("sub").join("b.md"), "x").unwrap();
        let ex = excludes(&home, &cfg, &proj);
        let h = home.to_string_lossy().replace('\\', "/");
        assert_eq!(ex[0], format!("{h}/.claude/CLAUDE.md"));
        assert!(ex.contains(&format!("{h}/.claude/rules/a.md")) && ex.contains(&format!("{h}/.claude/rules/sub/b.md")));
        assert!(ex.last().unwrap().ends_with("my home [(]x[)]/.claude/rules/**"), "{ex:?}");
        assert!(ex.iter().all(|p| !p.contains("work/proj")), "project instructions are never excluded");

        assert!(excludes(&home, &home.join(".claude"), &proj).is_empty(), "the default profile keeps its own memory");
        assert!(excludes(&home, &cfg, &root.join("elsewhere")).is_empty(), "outside home nothing reaches the session");
        assert_eq!(escape_glob("C:/Users/a!b"), None);
    }
}

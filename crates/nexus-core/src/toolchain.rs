//! Development tools NEXUS knows by name (Workspace Doctor, integration manifests, environment reconstruction).
//!
//! Workspace files (integration manifests, environment manifests) refer to tools only by these **ids**; program names,
//! version arguments and package-manager ids come from this table, never from a workspace file — a synced file can
//! therefore never make NEXUS look up or run an arbitrary program (threat T22). `find` is a PATH search; `probe` runs
//! the tool's own version command (argv, timeout) and nothing else. Installing is never done here: `install_command`
//! only *describes* an official package-manager command the person can run in a terminal.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

/// A tool NEXUS can look for.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub id: &'static str,
    pub name: &'static str,
    /// Program names tried in order (Windows finds `.exe`/`.cmd` through the PATH search).
    #[serde(skip)]
    pub programs: &'static [&'static str],
    #[serde(skip)]
    pub version_args: &'static [&'static str],
    /// Official installation page.
    pub install_url: &'static str,
    /// WinGet package id (Windows) and Homebrew formula/cask (macOS), when the vendor publishes one.
    #[serde(skip)]
    pub winget: Option<&'static str>,
    #[serde(skip)]
    pub brew: Option<&'static str>,
}

const V: &[&str] = &["--version"];

pub const TOOLS: &[Tool] = &[
    Tool { id: "git", name: "Git", programs: &["git"], version_args: V, install_url: "https://git-scm.com/downloads", winget: Some("Git.Git"), brew: Some("git") },
    Tool { id: "node", name: "Node.js", programs: &["node"], version_args: V, install_url: "https://nodejs.org/en/download", winget: Some("OpenJS.NodeJS.LTS"), brew: Some("node") },
    Tool { id: "npm", name: "npm", programs: &["npm"], version_args: V, install_url: "https://nodejs.org/en/download", winget: None, brew: None },
    Tool { id: "npx", name: "npx", programs: &["npx"], version_args: V, install_url: "https://nodejs.org/en/download", winget: None, brew: None },
    Tool { id: "pnpm", name: "pnpm", programs: &["pnpm"], version_args: V, install_url: "https://pnpm.io/installation", winget: Some("pnpm.pnpm"), brew: Some("pnpm") },
    Tool { id: "yarn", name: "Yarn", programs: &["yarn"], version_args: V, install_url: "https://yarnpkg.com/getting-started/install", winget: None, brew: None },
    Tool { id: "bun", name: "Bun", programs: &["bun"], version_args: V, install_url: "https://bun.sh/docs/installation", winget: Some("Oven-sh.Bun"), brew: None },
    Tool { id: "python", name: "Python", programs: &["python3", "python", "py"], version_args: V, install_url: "https://www.python.org/downloads/", winget: Some("Python.Python.3.13"), brew: Some("python") },
    Tool { id: "uv", name: "uv", programs: &["uv"], version_args: V, install_url: "https://docs.astral.sh/uv/getting-started/installation/", winget: Some("astral-sh.uv"), brew: Some("uv") },
    Tool { id: "pipx", name: "pipx", programs: &["pipx"], version_args: V, install_url: "https://pipx.pypa.io/stable/installation/", winget: None, brew: Some("pipx") },
    Tool { id: "poetry", name: "Poetry", programs: &["poetry"], version_args: V, install_url: "https://python-poetry.org/docs/#installation", winget: None, brew: None },
    Tool { id: "cargo", name: "Cargo", programs: &["cargo"], version_args: V, install_url: "https://www.rust-lang.org/tools/install", winget: Some("Rustlang.Rustup"), brew: Some("rustup") },
    Tool { id: "rustc", name: "Rust", programs: &["rustc"], version_args: V, install_url: "https://www.rust-lang.org/tools/install", winget: Some("Rustlang.Rustup"), brew: Some("rustup") },
    Tool { id: "php", name: "PHP", programs: &["php"], version_args: V, install_url: "https://www.php.net/downloads.php", winget: None, brew: Some("php") },
    Tool { id: "composer", name: "Composer", programs: &["composer"], version_args: V, install_url: "https://getcomposer.org/download/", winget: None, brew: Some("composer") },
    Tool { id: "dart", name: "Dart", programs: &["dart"], version_args: V, install_url: "https://dart.dev/get-dart", winget: None, brew: None },
    Tool { id: "flutter", name: "Flutter", programs: &["flutter"], version_args: V, install_url: "https://docs.flutter.dev/get-started/install", winget: None, brew: None },
    Tool { id: "go", name: "Go", programs: &["go"], version_args: &["version"], install_url: "https://go.dev/doc/install", winget: Some("GoLang.Go"), brew: Some("go") },
    Tool { id: "docker", name: "Docker", programs: &["docker"], version_args: V, install_url: "https://docs.docker.com/get-started/get-docker/", winget: Some("Docker.DockerDesktop"), brew: None },
    Tool { id: "gh", name: "GitHub CLI", programs: &["gh"], version_args: V, install_url: "https://cli.github.com", winget: Some("GitHub.cli"), brew: Some("gh") },
    Tool { id: "codex", name: "OpenAI Codex CLI", programs: &["codex"], version_args: V, install_url: "https://github.com/openai/codex", winget: None, brew: None },
    Tool { id: "graphify", name: "Graphify", programs: &["graphify"], version_args: V, install_url: "https://github.com/safishamsi/graphify", winget: None, brew: None },
    Tool { id: "claude", name: "Claude Code", programs: &["claude"], version_args: V, install_url: "https://code.claude.com/docs/en/setup", winget: Some("Anthropic.ClaudeCode"), brew: None },
];

pub fn tool(id: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.id == id)
}

/// Where the tool is on this machine's PATH, if anywhere. Unknown ids are never looked up.
pub fn find(id: &str) -> Option<PathBuf> {
    tool(id)?.programs.iter().find_map(|p| which::which(p).ok())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub path: PathBuf,
    pub version: Option<String>,
}

/// Find the tool and ask it for its version (its own version command, argv, 20 s timeout).
pub fn probe(id: &str) -> Option<Probe> {
    let t = tool(id)?;
    let path = find(id)?;
    let mut cmd = crate::proc::command(&path);
    cmd.args(t.version_args);
    let version = crate::proc::run(cmd, Duration::from_secs(20)).ok().filter(|o| o.success()).and_then(|o| first_version(&format!("{} {}", o.stdout_str(), o.stderr_str())));
    Some(Probe { path, version })
}

/// The first dotted version number in a tool's output (`v20.11.1`, `Python 3.12.4`, `go1.22.3`).
pub fn first_version(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            let v = s[start..i].trim_end_matches('.');
            if v.contains('.') {
                return Some(v.to_string());
            }
        }
        i += 1;
    }
    None
}

fn nums(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V', '=']).split(['.', '-', '+']).map_while(|p| p.parse::<u64>().ok()).collect()
}

fn cmp(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    let n = a.len().max(b.len());
    (0..n).map(|i| a.get(i).copied().unwrap_or(0).cmp(&b.get(i).copied().unwrap_or(0))).find(|o| o.is_ne()).unwrap_or(std::cmp::Ordering::Equal)
}

/// Whether an installed version meets a constraint as written in a project file. `None` when the constraint cannot be
/// evaluated (a channel such as `stable`, `lts/*`, an unusual syntax) — reported as "check by hand", never guessed.
/// Supports `20`, `20.x`, `20.*`, `>=18`, `>18 <21`, `>=3.11,<3.14`, `^8.2`, `~1.4`, `~=3.11`, `^8.2 || ^8.3`, `=1.2.3`.
pub fn satisfies(installed: &str, constraint: &str) -> Option<bool> {
    let c = constraint.trim();
    if c.is_empty() || c == "*" {
        return Some(true);
    }
    let have = nums(installed);
    if have.is_empty() {
        return None;
    }
    let mut any_known = false;
    for alt in c.split("||").flat_map(|a| a.split('|')) {
        let mut all = Some(true);
        for comp in alt.split([',', ' ']).map(str::trim).filter(|s| !s.is_empty()) {
            let r = comparator(&have, comp);
            all = match (all, r) {
                (Some(true), Some(x)) => Some(x),
                (Some(false), _) => Some(false),
                _ => None,
            };
        }
        match all {
            Some(true) => return Some(true),
            Some(false) => any_known = true,
            None => {}
        }
    }
    any_known.then_some(false)
}

fn comparator(have: &[u64], comp: &str) -> Option<bool> {
    use std::cmp::Ordering::*;
    let (op, v) = ["~=", ">=", "<=", "==", ">", "<", "=", "^", "~"].iter().find_map(|op| comp.strip_prefix(op).map(|v| (*op, v.trim()))).unwrap_or(("", comp));
    let v = v.trim_end_matches(".x").trim_end_matches(".*");
    let want = nums(v);
    if want.is_empty() {
        return None; // `stable`, `lts/*`, `latest`, …
    }
    let o = cmp(have, &want);
    Some(match op {
        ">=" => o != Less,
        ">" => o == Greater,
        "<=" => o != Greater,
        "<" => o == Less,
        "=" | "==" => have.starts_with(&want) || o == Equal,
        "^" => {
            let mut upper = want.clone();
            let i = upper.iter().position(|x| *x != 0).unwrap_or(upper.len().saturating_sub(1));
            upper.truncate(i + 1);
            upper[i] += 1;
            o != Less && cmp(have, &upper) == Less
        }
        "~" | "~=" => {
            let mut upper = want.clone();
            let keep = if op == "~=" { want.len().saturating_sub(1).max(1) } else if want.len() > 1 { 2 } else { 1 };
            upper.truncate(keep);
            if let Some(last) = upper.last_mut() {
                *last += 1;
            }
            o != Less && cmp(have, &upper) == Less
        }
        // A bare version is a prefix: `20` = any 20.x, `3.12` = any 3.12.x.
        _ => have.starts_with(&want),
    })
}

/// An official package-manager command for installing the tool on this platform (`winget install --id …` on Windows,
/// `brew install …` on macOS), for the person to run in a terminal — NEXUS never runs it.
pub fn install_command(id: &str) -> Option<Vec<String>> {
    let t = tool(id)?;
    if cfg!(windows) {
        t.winget.map(|w| vec!["winget".into(), "install".into(), "--id".into(), w.into(), "-e".into()])
    } else if cfg!(target_os = "macos") {
        t.brew.map(|b| vec!["brew".into(), "install".into(), b.into()])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_tools_are_looked_up() {
        assert!(tool("node").is_some());
        assert!(find("rm -rf /").is_none() && tool("cmd").is_none(), "ids, not programs");
        let ids: Vec<&str> = TOOLS.iter().map(|t| t.id).collect();
        let unique: std::collections::HashSet<&&str> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn versions_are_read_from_tool_output() {
        assert_eq!(first_version("v20.11.1\n").as_deref(), Some("20.11.1"));
        assert_eq!(first_version("Python 3.12.4").as_deref(), Some("3.12.4"));
        assert_eq!(first_version("go version go1.22.3 windows/amd64").as_deref(), Some("1.22.3"));
        assert_eq!(first_version("rustc 1.90.0 (1159e78c4 2025-09-14)").as_deref(), Some("1.90.0"));
        assert_eq!(first_version("no numbers"), None);
    }

    #[test]
    fn constraints_follow_the_ecosystems_rules() {
        let yes = |v: &str, c: &str| assert_eq!(satisfies(v, c), Some(true), "{v} satisfies {c}");
        let no = |v: &str, c: &str| assert_eq!(satisfies(v, c), Some(false), "{v} does not satisfy {c}");
        yes("20.11.1", "20");
        yes("20.11.1", "20.x");
        no("18.19.0", "20");
        yes("20.11.1", ">=18");
        yes("3.12.4", ">=3.11,<3.14");
        no("3.14.0", ">=3.11,<3.14");
        yes("8.3.2", "^8.2");
        no("9.0.0", "^8.2");
        yes("0.3.9", "^0.3.1");
        no("0.4.0", "^0.3.1");
        yes("1.4.9", "~1.4.2");
        no("1.5.0", "~1.4.2");
        yes("3.12.1", "~=3.11");
        yes("8.3.0", "^8.2 || ^9.0");
        yes("1.90.0", ">=1.85");
        no("1.80.0", ">=1.85");
        yes("20.11.1", "v20.11.1");
        assert_eq!(satisfies("1.90.0", "stable"), None, "channels are not guessed");
        assert_eq!(satisfies("20.1.0", "lts/*"), None);
        yes("1.0.0", "");
    }
}

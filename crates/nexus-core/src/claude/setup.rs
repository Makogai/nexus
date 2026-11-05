//! Guided Claude Code setup (Portable Workspace P5): is Claude Code installed, which version, can NEXUS start it,
//! which profiles exist, and — when it is missing — the **official** installation methods for this platform
//! (code.claude.com/docs/en/setup, verified 2026-11-05).
//!
//! NEXUS never installs anything on its own and never touches credentials: a method is shown as the exact command; the
//! desktop can type it into a terminal for the person, who presses Enter (explicit consent, visible output). Sign-in
//! happens in Claude Code's own browser flow; NEXUS only reads `claude auth status` per profile.

use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use super::install::{self, Installation};
use crate::error::Result;

pub const DOCS_URL: &str = "https://code.claude.com/docs/en/setup";
pub const VERIFIED: &str = "2026-11-05";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Method {
    pub id: &'static str,
    pub title: &'static str,
    /// The command exactly as the official documentation gives it.
    pub command: &'static str,
    /// Shell it is typed into: `powershell`, `cmd`, `bash` (macOS, Linux, WSL) or `any`.
    pub shell: &'static str,
    pub recommended: bool,
    pub auto_updates: bool,
    pub notes: &'static str,
}

const WINDOWS: &[Method] = &[
    Method {
        id: "native-powershell",
        title: "Native installer (PowerShell)",
        command: "irm https://claude.ai/install.ps1 | iex",
        shell: "powershell",
        recommended: true,
        auto_updates: true,
        notes: "No administrator rights needed. Open a new terminal afterwards and run `claude --version`.",
    },
    Method {
        id: "winget",
        title: "WinGet",
        command: "winget install Anthropic.ClaudeCode",
        shell: "any",
        recommended: false,
        auto_updates: false,
        notes: "Does not update itself: run `winget upgrade Anthropic.ClaudeCode` from time to time. Keep only one installation.",
    },
    Method {
        id: "native-cmd",
        title: "Native installer (Command Prompt)",
        command: "curl -fsSL https://claude.ai/install.cmd -o install.cmd && install.cmd && del install.cmd",
        shell: "cmd",
        recommended: false,
        auto_updates: true,
        notes: "The same installer for CMD.",
    },
];

const UNIX: &[Method] = &[
    Method {
        id: "native-sh",
        title: "Native installer",
        command: "curl -fsSL https://claude.ai/install.sh | bash",
        shell: "bash",
        recommended: true,
        auto_updates: true,
        notes: "macOS 13+, Ubuntu 20.04+, Debian 10+, Alpine 3.19+ (Alpine also needs bash, curl, libgcc, libstdc++, ripgrep).",
    },
    Method {
        id: "homebrew",
        title: "Homebrew (macOS)",
        command: "brew install --cask claude-code",
        shell: "bash",
        recommended: false,
        auto_updates: false,
        notes: "Stable channel; update with `brew upgrade claude-code`.",
    },
];

const NPM: Method = Method {
    id: "npm",
    title: "npm (global package)",
    command: "npm install -g @anthropic-ai/claude-code",
    shell: "any",
    recommended: false,
    auto_updates: true,
    notes: "Needs Node.js 22 or later. Never use sudo for it.",
};

/// The official methods for this platform, recommended first.
pub fn methods() -> Vec<Method> {
    let mut v: Vec<Method> = if cfg!(windows) { WINDOWS.to_vec() } else { UNIX.iter().filter(|m| cfg!(target_os = "macos") || m.id != "homebrew").cloned().collect() };
    v.push(NPM);
    v
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub installations: Vec<Installation>,
    /// A Claude Code NEXUS can start was found.
    pub ready: bool,
    pub version: Option<String>,
    pub profiles: usize,
    /// Windows: Git for Windows gives Claude Code its Bash tool (without it, Claude uses PowerShell).
    pub git_for_windows: Option<bool>,
    /// Node.js version, relevant only for the npm method.
    pub node: Option<String>,
    pub methods: Vec<Method>,
    pub docs_url: &'static str,
    pub verified: &'static str,
    /// What to do next, in order.
    pub next: Vec<String>,
}

pub fn status(conn: &Connection, home: &Path, probe: bool) -> Result<Status> {
    let installations = if probe { install::discover(home) } else { Vec::new() };
    let ready_install = installations.iter().find(|i| i.launchable);
    let profiles = super::profile::list(conn, false)?.len();
    let git_for_windows = cfg!(windows).then(|| crate::toolchain::find("git").is_some());
    let node = if probe { crate::toolchain::probe("node").and_then(|p| p.version) } else { None };
    let mut next = Vec::new();
    match ready_install {
        None => {
            next.push("Install Claude Code with one of the official methods below (the recommended one updates itself).".into());
            next.push("Open a new terminal afterwards so the `claude` command is found, then check again.".into());
        }
        Some(_) if profiles == 0 => next.push("Add a Claude profile (Claude profiles → Add): your existing configuration or a new, separate one.".into()),
        Some(_) => {}
    }
    if ready_install.is_some() {
        next.push("Sign in by starting a session with the profile: Claude Code opens its own browser sign-in; NEXUS never sees passwords or tokens.".into());
    }
    if git_for_windows == Some(false) {
        next.push("Optional on Windows: install Git for Windows so Claude Code can use its Bash tool (NEXUS also needs Git for projects and sync).".into());
    }
    Ok(Status {
        ready: ready_install.is_some(),
        version: ready_install.and_then(|i| i.version.clone()),
        installations,
        profiles,
        git_for_windows,
        node,
        methods: methods(),
        docs_url: DOCS_URL,
        verified: VERIFIED,
        next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_are_the_official_ones_for_this_platform() {
        let m = methods();
        assert!(m[0].recommended && m[0].auto_updates);
        assert!(m.iter().all(|x| x.command.contains("claude") || x.command.contains("Anthropic.ClaudeCode")));
        if cfg!(windows) {
            assert_eq!(m[0].command, "irm https://claude.ai/install.ps1 | iex");
            assert!(m.iter().any(|x| x.id == "winget" && !x.auto_updates));
        }
        assert!(m.iter().any(|x| x.id == "npm" && x.notes.contains("Node.js 22")));
    }

    #[test]
    fn status_without_claude_explains_the_next_steps() {
        let conn = crate::db::open_in_memory().unwrap();
        let t = tempfile::tempdir().unwrap();
        let s = status(&conn, t.path(), false).unwrap();
        assert!(!s.ready && s.profiles == 0);
        assert!(s.next[0].contains("official methods"));
        assert_eq!(s.docs_url, DOCS_URL);
    }
}

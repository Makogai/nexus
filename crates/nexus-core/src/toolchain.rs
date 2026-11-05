//! Development tools NEXUS knows by name (Workspace Doctor, integration manifests, environment reconstruction).
//!
//! Workspace files (integration manifests, environment manifests) refer to tools only by these **ids**; the program
//! names come from this table, never from a workspace file — a synced file can therefore never make NEXUS look up or
//! run an arbitrary program (threat T22). Lookup is a PATH search; nothing is executed here.

use std::path::PathBuf;

use serde::Serialize;

/// A tool NEXUS can look for.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub id: &'static str,
    pub name: &'static str,
    /// Program names tried in order (Windows adds `.exe`/`.cmd` through the PATH search).
    #[serde(skip)]
    pub programs: &'static [&'static str],
    /// Official installation page.
    pub install_url: &'static str,
}

pub const TOOLS: &[Tool] = &[
    Tool { id: "git", name: "Git", programs: &["git"], install_url: "https://git-scm.com/downloads" },
    Tool { id: "node", name: "Node.js", programs: &["node"], install_url: "https://nodejs.org/en/download" },
    Tool { id: "npm", name: "npm", programs: &["npm"], install_url: "https://nodejs.org/en/download" },
    Tool { id: "npx", name: "npx", programs: &["npx"], install_url: "https://nodejs.org/en/download" },
    Tool { id: "python", name: "Python", programs: &["python3", "python", "py"], install_url: "https://www.python.org/downloads/" },
    Tool { id: "uv", name: "uv", programs: &["uv"], install_url: "https://docs.astral.sh/uv/getting-started/installation/" },
    Tool { id: "pipx", name: "pipx", programs: &["pipx"], install_url: "https://pipx.pypa.io/stable/installation/" },
    Tool { id: "docker", name: "Docker", programs: &["docker"], install_url: "https://docs.docker.com/get-started/get-docker/" },
    Tool { id: "gh", name: "GitHub CLI", programs: &["gh"], install_url: "https://cli.github.com" },
    Tool { id: "codex", name: "OpenAI Codex CLI", programs: &["codex"], install_url: "https://github.com/openai/codex" },
    Tool { id: "graphify", name: "Graphify", programs: &["graphify"], install_url: "https://github.com/safishamsi/graphify" },
    Tool { id: "claude", name: "Claude Code", programs: &["claude"], install_url: "https://code.claude.com/docs/en/setup" },
];

pub fn tool(id: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.id == id)
}

/// Where the tool is on this machine's PATH, if anywhere. Unknown ids are never looked up.
pub fn find(id: &str) -> Option<PathBuf> {
    tool(id)?.programs.iter().find_map(|p| which::which(p).ok())
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
}

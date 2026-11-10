//! AI agents NEXUS can start in its terminal (ADR-0052). Claude Code is the reference agent with its own profiles and
//! subsystems (`claude`); other agents are registered here with what NEXUS can give their sessions, so the UI can say
//! so instead of guessing. Adding an agent: a [`Kind`] variant with its facts below, an adapter module
//! (`<agent>::launch` — a pure argv/env builder — and `<agent>::prepare`), and a branch in [`launch::prepare`].

pub mod launch;
pub mod profile;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Claude,
    Codex,
}

impl Kind {
    pub const ALL: &'static [Kind] = &[Kind::Claude, Kind::Codex];

    pub fn id(self) -> &'static str {
        match self {
            Kind::Claude => "claude",
            Kind::Codex => "codex",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.iter().copied().find(|k| k.id() == s)
    }

    /// Product name for headings.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Claude => "Claude Code",
            Kind::Codex => "OpenAI Codex CLI",
        }
    }

    /// Short name for messages ("Codex finished in …").
    pub fn short(self) -> &'static str {
        match self {
            Kind::Claude => "Claude",
            Kind::Codex => "Codex",
        }
    }

    /// Its id in the development-tool catalog (`toolchain::TOOLS`).
    pub fn tool(self) -> &'static str {
        self.id()
    }

    /// The environment variable that selects the agent's home (configuration and login).
    pub fn home_var(self) -> &'static str {
        match self {
            Kind::Claude => "CLAUDE_CONFIG_DIR",
            Kind::Codex => "CODEX_HOME",
        }
    }

    /// The agent's own default home, used when a profile names none.
    pub fn default_home(self, home: &Path) -> PathBuf {
        match self {
            Kind::Claude => home.join(".claude"),
            Kind::Codex => home.join(".codex"),
        }
    }

    /// Credentials in the environment that would override the profile's own login (removed unless the profile opts in).
    pub fn provider_credentials(self) -> &'static [&'static str] {
        match self {
            Kind::Claude => crate::env::PROVIDER_CREDENTIALS,
            Kind::Codex => &["OPENAI_API_KEY", "OPENAI_BASE_URL", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN"],
        }
    }

    /// Variables the agent sets for the commands it runs; inherited by a new session they would make it misbehave.
    /// (Claude's are removed from every child, `env::SESSION_MARKERS`.)
    pub fn session_markers(self) -> &'static [&'static str] {
        match self {
            Kind::Claude => &[],
            Kind::Codex => &["CODEX_SANDBOX", "CODEX_SANDBOX_NETWORK_DISABLED"],
        }
    }

    /// What NEXUS gives this agent's sessions.
    pub fn capabilities(self) -> Vec<Capability> {
        use Support::*;
        let c = |id, label, support, detail| Capability { id, label, support, detail };
        match self {
            Kind::Claude => vec![
                c("launch", "Launch in a NEXUS terminal", Full, "Profiles, clean environment, session records."),
                c("tools", "NEXUS tools (MCP)", Full, "Session-scoped --mcp-config."),
                c("context", "Context", Full, "Work profile, approved rules and the task brief at session start."),
                c("resume", "Resume", Full, "NEXUS assigns the conversation id."),
                c("attention", "Attention", Full, "Permission, question and turn-finished signals from hooks."),
                c("activity", "Activity", Full, "Turns, prompts, file edits and command categories from hooks."),
                c("usage", "Usage", Full, "From Claude Code's local transcripts."),
                c("kits", "AI Kits & skills", Full, "Per-profile --plugin-dir overlays."),
                c("inspector", "Instructions inspector", Full, "CLAUDE.md layers as Claude loads them."),
            ],
            Kind::Codex => vec![
                c("launch", "Launch in a NEXUS terminal", Full, "Clean environment, CODEX_HOME per profile, session records."),
                c("tools", "NEXUS tools (MCP)", Full, "Given per session with -c mcp_servers.nexus.*; nothing is written into your Codex configuration."),
                c("context", "Context", Partial, "Work profile, approved rules and the task pointer as developer instructions; the task itself through get_task. No session-start brief (no hooks)."),
                c("resume", "Resume", Partial, "After the first finished turn, when Codex has told NEXUS its conversation id."),
                c("attention", "Attention", Partial, "Turn finished (Codex's notify program) and request_attention. Permission prompts and questions are not reported — Codex hooks would need trust written into your Codex configuration."),
                c("activity", "Activity", Partial, "Finished turns only."),
                c("usage", "Usage", Unavailable, "Codex's transcript format is not a stable interface."),
                c("kits", "AI Kits & skills", Unavailable, "AI Kits are Claude Code plugins."),
                c("inspector", "Instructions inspector", Unavailable, "Codex reads AGENTS.md itself; NEXUS does not inspect it yet."),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Support {
    Full,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub id: &'static str,
    pub label: &'static str,
    pub support: Support,
    pub detail: &'static str,
}

/// An agent as the UI lists it: what it is, whether it is installed, what its sessions get.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub id: Kind,
    pub name: &'static str,
    pub install_url: &'static str,
    /// Where it is on PATH and its version (`probe` only).
    pub installed: Option<crate::toolchain::Probe>,
    pub capabilities: Vec<Capability>,
}

pub fn info(kind: Kind, probe: bool) -> Info {
    let installed = if probe { crate::toolchain::probe(kind.tool()) } else { None };
    Info {
        id: kind,
        name: kind.name(),
        install_url: crate::toolchain::tool(kind.tool()).map(|t| t.install_url).unwrap_or_default(),
        installed,
        capabilities: kind.capabilities(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agent_is_known_to_the_catalog_and_round_trips() {
        for k in Kind::ALL {
            assert_eq!(Kind::parse(k.id()), Some(*k));
            assert!(crate::toolchain::tool(k.tool()).is_some(), "{k:?}");
            assert_eq!(serde_json::to_value(k).unwrap(), k.id());
            let caps = k.capabilities();
            let ids: Vec<&str> = caps.iter().map(|c| c.id).collect();
            assert_eq!(ids, ["launch", "tools", "context", "resume", "attention", "activity", "usage", "kits", "inspector"], "{k:?}: same rows for every agent");
            assert!(caps.iter().all(|c| !c.detail.is_empty()), "every capability says why");
        }
        assert_eq!(Kind::parse("gemini"), None);
    }

    #[test]
    fn codex_says_what_it_cannot_do() {
        let caps = Kind::Codex.capabilities();
        let get = |id: &str| caps.iter().find(|c| c.id == id).unwrap().support;
        assert_eq!((get("tools"), get("attention"), get("usage")), (Support::Full, Support::Partial, Support::Unavailable));
        assert_eq!(Kind::Codex.home_var(), "CODEX_HOME");
        assert!(Kind::Codex.provider_credentials().contains(&"OPENAI_API_KEY"));
    }
}

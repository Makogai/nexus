//! Per-session NEXUS MCP binding (ADR-0004, ADR-0009). NEXUS writes a session-scoped `--mcp-config` file that starts
//! `nexus mcp --session <id>`; the session id in the file is the server's authorization scope. Nothing is written
//! into the profile's Claude configuration.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::claude::policy::PolicySummary;
use crate::error::{CoreError, Result};

pub const SERVER_NAME: &str = "nexus";

/// What happened to one NEXUS integration (MCP server, lifecycle hooks) for one session (stored with the session,
/// shown in the UI).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum IntegrationStatus {
    Enabled { config: PathBuf },
    /// Organisation policy does not allow it; NEXUS omits the flag (Claude would otherwise refuse to start).
    Blocked { reason: String },
    /// The `nexus` executable could not be found.
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Integrations {
    pub mcp: IntegrationStatus,
    pub hooks: IntegrationStatus,
    /// Always-on rules included in the system prompt (and those pending approval or over budget).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<crate::knowledge::context::LaunchRules>,
}

impl IntegrationStatus {
    pub fn config_path(&self) -> Option<&Path> {
        match self {
            IntegrationStatus::Enabled { config } => Some(config),
            _ => None,
        }
    }
}

/// Why policy forbids a session-scoped NEXUS MCP server, if it does.
pub fn blocked_by(policy: &PolicySummary) -> Option<&'static str> {
    if policy.disable_sideload_flags {
        Some("organisation policy disables --mcp-config (disableSideloadFlags)")
    } else if policy.allow_managed_mcp_servers_only {
        Some("organisation policy only allows managed MCP servers (allowManagedMcpServersOnly)")
    } else if policy.strict_plugin_only_customization {
        Some("organisation policy restricts MCP servers to plugins (strictPluginOnlyCustomization)")
    } else {
        None
    }
}

pub fn session_dir(data_dir: &Path, nexus_session_id: &str) -> PathBuf {
    data_dir.join("sessions").join(nexus_session_id)
}

/// The `--mcp-config` document for one session.
pub fn config_json(nexus_exe: &Path, data_dir: &Path, nexus_session_id: &str) -> serde_json::Value {
    serde_json::json!({
        "mcpServers": {
            SERVER_NAME: {
                "type": "stdio",
                "command": nexus_exe.display().to_string(),
                "args": ["--data-dir", data_dir.display().to_string(), "mcp", "--session", nexus_session_id],
            }
        }
    })
}

/// Decide and, when allowed, write the session's MCP config.
pub fn prepare(policy: &PolicySummary, nexus_exe: Option<&Path>, data_dir: &Path, nexus_session_id: &str) -> Result<IntegrationStatus> {
    if let Some(reason) = blocked_by(policy) {
        return Ok(IntegrationStatus::Blocked { reason: reason.into() });
    }
    let Some(exe) = nexus_exe.filter(|e| e.is_file()) else {
        return Ok(IntegrationStatus::Unavailable {
            reason: "the nexus command-line tool was not found next to NEXUS".into(),
        });
    };
    uuid::Uuid::parse_str(nexus_session_id).map_err(|_| CoreError::invalid("session id must be a UUID"))?;
    let dir = session_dir(data_dir, nexus_session_id);
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::io("cannot create", &dir, e))?;
    let config = dir.join("mcp.json");
    let body = serde_json::to_vec_pretty(&config_json(exe, data_dir, nexus_session_id)).map_err(|source| CoreError::Json {
        what: "mcp.json".into(),
        source,
    })?;
    crate::workspace::write_atomic(&config, &body)?;
    Ok(IntegrationStatus::Enabled { config })
}

/// The `nexus` CLI that ships next to the running executable (desktop) or is the running executable (CLI).
pub fn nexus_next_to_current_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "nexus.exe" } else { "nexus" };
    if exe.file_name().is_some_and(|n| n == name) {
        return Some(exe);
    }
    Some(exe.parent()?.join(name)).filter(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "6f1d2c3b-4a5e-4f60-8172-93a4b5c6d7e8";

    #[test]
    fn writes_a_session_scoped_config() {
        let t = tempfile::tempdir().unwrap();
        let exe = t.path().join(if cfg!(windows) { "nexus.exe" } else { "nexus" });
        std::fs::write(&exe, "").unwrap();
        let status = prepare(&PolicySummary::default(), Some(&exe), t.path(), SID).unwrap();
        let path = status.config_path().unwrap().to_path_buf();
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let server = &v["mcpServers"]["nexus"];
        assert_eq!(server["type"], "stdio");
        assert_eq!(server["command"], exe.display().to_string());
        let args: Vec<&str> = server["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
        assert_eq!(args[2..], ["mcp", "--session", SID]);
        assert!(path.starts_with(t.path().join("sessions").join(SID)));
    }

    #[test]
    fn policy_and_missing_binary_are_explained_not_fatal() {
        let t = tempfile::tempdir().unwrap();
        let policy = PolicySummary {
            disable_sideload_flags: true,
            ..Default::default()
        };
        assert!(matches!(prepare(&policy, None, t.path(), SID).unwrap(), IntegrationStatus::Blocked { reason } if reason.contains("disableSideloadFlags")));
        let mcp_only = PolicySummary {
            allow_managed_mcp_servers_only: true,
            ..Default::default()
        };
        assert!(matches!(prepare(&mcp_only, None, t.path(), SID).unwrap(), IntegrationStatus::Blocked { .. }));
        assert!(matches!(
            prepare(&PolicySummary::default(), Some(&t.path().join("missing")), t.path(), SID).unwrap(),
            IntegrationStatus::Unavailable { .. }
        ));
        assert!(!t.path().join("sessions").exists(), "nothing written when not enabled");
    }
}

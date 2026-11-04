//! Organisation policy detection (read-only, ADR-0003 §6, ADR-0004).
//!
//! Managed settings can contain sensitive values (e.g. telemetry headers). This module extracts a fixed
//! allowlist of policy keys and discards everything else immediately; nothing else is stored or logged.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

const MAX_POLICY_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PolicySummary {
    pub sources: Vec<PolicySource>,
    /// At least one managed source applies to this profile.
    pub organization_managed: bool,
    /// `disableSideloadFlags`: `--plugin-dir`, `--plugin-url`, `--agents`, `--mcp-config` make Claude exit.
    pub disable_sideload_flags: bool,
    pub allow_managed_hooks_only: bool,
    pub disable_all_hooks: bool,
    pub allow_managed_mcp_servers_only: bool,
    pub strict_plugin_only_customization: bool,
    pub allow_managed_permission_rules_only: bool,
    pub bypass_permissions_disabled: bool,
    pub managed_telemetry: bool,
    /// Managed settings carry organisation instructions (`claudeMd`). Only the presence is recorded, never the text.
    pub managed_claude_md: bool,
    /// Instruction-loading settings from managed sources (ADR-0019): `claudeMdExcludes` glob patterns and the
    /// AGENTS.md mode (`pluginConfigs["cc-plugin-agents-md@builtin"].options.instructionFiles`).
    pub claude_md_excludes: Vec<String>,
    pub instruction_files: Option<String>,
    /// Sources that exist but could not be parsed (names only).
    pub unreadable: Vec<String>,
}

impl PolicySummary {
    /// Can NEXUS pass session-scoped `--mcp-config` / `--plugin-dir`?
    pub fn sideload_allowed(&self) -> bool {
        !self.disable_sideload_flags
    }
    /// Can NEXUS add its own hooks via `--settings`?
    pub fn session_hooks_allowed(&self) -> bool {
        !(self.allow_managed_hooks_only || self.disable_all_hooks)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PolicySource {
    pub kind: PolicySourceKind,
    pub location: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PolicySourceKind {
    /// Machine-wide file — applies to every profile on this machine.
    MachineFile,
    /// Machine-wide registry policy (Windows).
    Registry,
    /// Server-managed settings cached in the profile's config dir — applies to this account/profile.
    ServerManaged,
}

/// Where machine-wide managed settings live. Injectable for tests.
#[derive(Debug, Clone, Default)]
pub struct MachinePolicyLocations {
    pub files: Vec<PathBuf>,
    pub dropin_dirs: Vec<PathBuf>,
    pub registry: bool,
}

impl MachinePolicyLocations {
    pub fn system() -> Self {
        let base: PathBuf = if cfg!(windows) {
            std::env::var_os("ProgramFiles")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"))
                .join("ClaudeCode")
        } else if cfg!(target_os = "macos") {
            PathBuf::from("/Library/Application Support/ClaudeCode")
        } else {
            PathBuf::from("/etc/claude-code")
        };
        MachinePolicyLocations {
            files: vec![base.join("managed-settings.json")],
            dropin_dirs: vec![base.join("managed-settings.d")],
            registry: cfg!(windows),
        }
    }
}

/// Evaluate the policy that applies to a profile whose effective config dir is `config_dir`.
/// Conservative: a restriction set by *any* source is reported as active.
pub fn evaluate(config_dir: &Path, machine: &MachinePolicyLocations) -> PolicySummary {
    let mut s = PolicySummary::default();
    for f in &machine.files {
        absorb_file(&mut s, f, PolicySourceKind::MachineFile);
    }
    for d in &machine.dropin_dirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            let mut files: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect();
            files.sort();
            for f in files {
                absorb_file(&mut s, &f, PolicySourceKind::MachineFile);
            }
        }
    }
    if machine.registry {
        for (label, value) in read_registry_policies() {
            s.sources.push(PolicySource {
                kind: PolicySourceKind::Registry,
                location: label.clone(),
            });
            match serde_json::from_str::<Value>(&value) {
                Ok(v) => absorb_value(&mut s, &v),
                Err(_) => s.unreadable.push(label),
            }
        }
    }
    absorb_file(&mut s, &config_dir.join("remote-settings.json"), PolicySourceKind::ServerManaged);
    s.organization_managed = !s.sources.is_empty();
    s
}

fn absorb_file(s: &mut PolicySummary, path: &Path, kind: PolicySourceKind) {
    let Ok(meta) = std::fs::metadata(path) else { return };
    if !meta.is_file() {
        return;
    }
    s.sources.push(PolicySource {
        kind,
        location: path.display().to_string(),
    });
    if meta.len() > MAX_POLICY_BYTES {
        s.unreadable.push(path.display().to_string());
        return;
    }
    match std::fs::read_to_string(path).ok().and_then(|raw| serde_json::from_str::<Value>(&raw).ok()) {
        Some(v) => absorb_value(s, &v),
        None => s.unreadable.push(path.display().to_string()),
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::String(s)) => !s.is_empty() && s != "false",
        _ => false,
    }
}

/// Extract only the allowlisted keys from a managed settings document.
pub fn absorb_value(s: &mut PolicySummary, v: &Value) {
    s.disable_sideload_flags |= truthy(v.get("disableSideloadFlags"));
    s.allow_managed_hooks_only |= truthy(v.get("allowManagedHooksOnly"));
    s.disable_all_hooks |= truthy(v.get("disableAllHooks"));
    s.allow_managed_mcp_servers_only |= truthy(v.get("allowManagedMcpServersOnly"));
    s.strict_plugin_only_customization |= truthy(v.get("strictPluginOnlyCustomization"));
    s.allow_managed_permission_rules_only |= truthy(v.get("allowManagedPermissionRulesOnly"));
    if let Some(perms) = v.get("permissions") {
        s.bypass_permissions_disabled |= perms.get("disableBypassPermissionsMode").and_then(Value::as_str) == Some("disable");
    }
    if let Some(env) = v.get("env").and_then(Value::as_object) {
        s.managed_telemetry |= env.keys().any(|k| k == "CLAUDE_CODE_ENABLE_TELEMETRY" || k.starts_with("OTEL_"));
    }
    s.managed_claude_md |= v.get("claudeMd").and_then(Value::as_str).is_some_and(|t| !t.trim().is_empty());
    let (excludes, mode) = instruction_settings(v);
    s.claude_md_excludes.extend(excludes);
    if mode.is_some() {
        s.instruction_files = mode;
    }
}

/// Instruction-loading keys of any settings document (managed, user, project, local): `claudeMdExcludes` (bounded
/// list of glob patterns) and the AGENTS.md mode, if it is one of the documented values. Nothing else is read.
pub fn instruction_settings(v: &Value) -> (Vec<String>, Option<String>) {
    let excludes = v
        .get("claudeMdExcludes")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).filter(|p| p.len() <= 400).take(100).map(str::to_string).collect())
        .unwrap_or_default();
    let mode = v
        .pointer("/pluginConfigs/cc-plugin-agents-md@builtin/options/instructionFiles")
        .and_then(Value::as_str)
        .filter(|m| INSTRUCTION_FILE_MODES.contains(m))
        .map(str::to_string);
    (excludes, mode)
}

/// Documented values of the "Project instructions" setting (default first).
pub const INSTRUCTION_FILE_MODES: &[&str] = &["claude-md-or-agents-md", "claude-md-and-agents-md", "claude-md", "managed-only"];

impl MachinePolicyLocations {
    /// Folder of the machine-wide managed files (`managed-settings.json`, `CLAUDE.md`).
    pub fn managed_dir(&self) -> Option<&Path> {
        self.files.first().and_then(|f| f.parent())
    }
}

#[cfg(windows)]
fn read_registry_policies() -> Vec<(String, String)> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let mut out = Vec::new();
    for (hive, label) in [(HKEY_LOCAL_MACHINE, "HKLM"), (HKEY_CURRENT_USER, "HKCU")] {
        if let Ok(key) = RegKey::predef(hive).open_subkey(r"SOFTWARE\Policies\ClaudeCode")
            && let Ok(value) = key.get_value::<String, _>("Settings")
        {
            out.push((format!(r"{label}\SOFTWARE\Policies\ClaudeCode"), value));
        }
    }
    out
}

#[cfg(not(windows))]
fn read_registry_policies() -> Vec<(String, String)> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_machine() -> MachinePolicyLocations {
        MachinePolicyLocations::default()
    }

    #[test]
    fn unmanaged_profile() {
        let t = tempfile::tempdir().unwrap();
        let s = evaluate(t.path(), &no_machine());
        assert!(!s.organization_managed);
        assert!(s.sideload_allowed() && s.session_hooks_allowed());
    }

    #[test]
    fn server_managed_profile_extracts_only_allowlisted_keys() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join("remote-settings.json"),
            r#"{"allowManagedHooksOnly": true, "disableSideloadFlags": false,
                "permissions": {"deny": ["Bash(rm:*)"], "disableBypassPermissionsMode": "disable"},
                "env": {"OTEL_EXPORTER_OTLP_HEADERS": "Authorization=Bearer SECRET", "CLAUDE_CODE_ENABLE_TELEMETRY": "1"},
                "claudeMd": "corporate rules"}"#,
        )
        .unwrap();
        let s = evaluate(t.path(), &no_machine());
        assert!(s.organization_managed);
        assert_eq!(s.sources[0].kind, PolicySourceKind::ServerManaged);
        assert!(s.allow_managed_hooks_only && !s.session_hooks_allowed());
        assert!(s.sideload_allowed());
        assert!(s.bypass_permissions_disabled && s.managed_telemetry);
        let dump = serde_json::to_string(&s).unwrap();
        assert!(!dump.contains("SECRET") && !dump.contains("corporate"), "values must not survive");
        assert!(s.managed_claude_md, "only the presence of managed instructions is recorded");
    }

    #[test]
    fn machine_files_and_dropins_apply_and_bad_json_is_reported() {
        let t = tempfile::tempdir().unwrap();
        let profile = t.path().join("profile");
        std::fs::create_dir(&profile).unwrap();
        let file = t.path().join("managed-settings.json");
        std::fs::write(&file, r#"{"disableSideloadFlags": true}"#).unwrap();
        let dropins = t.path().join("managed-settings.d");
        std::fs::create_dir(&dropins).unwrap();
        std::fs::write(dropins.join("10-mcp.json"), r#"{"allowManagedMcpServersOnly": true}"#).unwrap();
        std::fs::write(dropins.join("20-broken.json"), "{oops").unwrap();
        let s = evaluate(
            &profile,
            &MachinePolicyLocations {
                files: vec![file],
                dropin_dirs: vec![dropins],
                registry: false,
            },
        );
        assert!(!s.sideload_allowed());
        assert!(s.allow_managed_mcp_servers_only);
        assert_eq!(s.unreadable.len(), 1);
        assert_eq!(s.sources.len(), 3);
    }
}

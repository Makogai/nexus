//! Per-session lifecycle hooks (ADR-0016): NEXUS writes a session-scoped `--settings` file whose hooks run
//! `nexus hook --session <id> <event>` in exec form (a real executable plus an argument vector — no shell).
//!
//! Verified on Claude Code 2.1.311 (synthetic profile): hooks from `--settings` are *added* to the user's own and
//! plugins' hooks (nothing is replaced), exec-form hooks work on Windows, the hook payload's `session_id` equals the
//! `--session-id` NEXUS assigned, and hooks inherit the environment NEXUS gives Claude.

use std::path::Path;

use serde_json::{Value, json};

use crate::claude::mcp::{IntegrationStatus, session_dir};
use crate::claude::policy::PolicySummary;
use crate::error::{CoreError, Result};

/// Hook events NEXUS subscribes to, with the `nexus hook` event argument and an optional tool matcher.
pub const SUBSCRIPTIONS: &[(&str, &str, Option<&str>)] = &[
    ("SessionStart", "session-start", None),
    ("SessionEnd", "session-end", None),
    ("PreCompact", "pre-compact", None),
    ("Stop", "stop", None),
    ("UserPromptSubmit", "user-prompt", None),
    // Only tools whose effects matter for continuity; reads and searches would only add process spawns.
    ("PostToolUse", "post-tool-use", Some("Write|Edit|MultiEdit|NotebookEdit|Bash")),
    ("Notification", "notification", None),
    ("SubagentStop", "subagent-stop", None),
];

/// Seconds Claude waits for one hook; NEXUS handlers finish in milliseconds.
const TIMEOUT_SECS: u64 = 5;

pub fn blocked_by(policy: &PolicySummary) -> Option<&'static str> {
    if policy.disable_all_hooks {
        Some("organisation policy disables all hooks (disableAllHooks)")
    } else if policy.allow_managed_hooks_only {
        Some("organisation policy allows only managed hooks (allowManagedHooksOnly)")
    } else {
        None
    }
}

/// The `--settings` document: hooks only, nothing else.
pub fn settings_json(nexus_exe: &Path, data_dir: &Path, nexus_session_id: &str) -> Value {
    let mut hooks = serde_json::Map::new();
    for (event, arg, matcher) in SUBSCRIPTIONS {
        let handler = json!({
            "type": "command",
            "command": nexus_exe.display().to_string(),
            "args": ["--data-dir", data_dir.display().to_string(), "hook", "--session", nexus_session_id, arg],
            "timeout": TIMEOUT_SECS,
        });
        let mut group = json!({ "hooks": [handler] });
        if let Some(m) = matcher {
            group["matcher"] = json!(m);
        }
        hooks.insert((*event).to_string(), json!([group]));
    }
    json!({ "hooks": hooks })
}

/// Decide and, when allowed, write the session's hook settings.
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
    let config = dir.join("settings.json");
    let body = serde_json::to_vec_pretty(&settings_json(exe, data_dir, nexus_session_id)).map_err(|source| CoreError::Json {
        what: "hook settings".into(),
        source,
    })?;
    crate::workspace::write_atomic(&config, &body)?;
    Ok(IntegrationStatus::Enabled { config })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "6f1d2c3b-4a5e-4f60-8172-93a4b5c6d7e8";

    #[test]
    fn settings_contain_only_exec_form_hooks_with_matchers() {
        let v = settings_json(Path::new("/opt/nexus"), Path::new("/data"), SID);
        assert_eq!(v.as_object().unwrap().keys().collect::<Vec<_>>(), ["hooks"], "nothing but hooks is overridden");
        let hooks = v["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), SUBSCRIPTIONS.len());
        let start = &hooks["SessionStart"][0]["hooks"][0];
        assert_eq!(start["type"], "command");
        assert_eq!(start["command"], "/opt/nexus");
        let args: Vec<&str> = start["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
        assert_eq!(args, ["--data-dir", "/data", "hook", "--session", SID, "session-start"]);
        assert_eq!(hooks["PostToolUse"][0]["matcher"], "Write|Edit|MultiEdit|NotebookEdit|Bash");
        assert!(hooks["Stop"][0].get("matcher").is_none());
    }

    #[test]
    fn policy_blocks_are_reported_and_nothing_is_written() {
        let t = tempfile::tempdir().unwrap();
        for policy in [
            PolicySummary { allow_managed_hooks_only: true, ..Default::default() },
            PolicySummary { disable_all_hooks: true, ..Default::default() },
        ] {
            assert!(matches!(prepare(&policy, None, t.path(), SID).unwrap(), IntegrationStatus::Blocked { .. }));
        }
        assert!(!t.path().join("sessions").exists());
        let exe = t.path().join("nexus-bin");
        std::fs::write(&exe, "").unwrap();
        let ok = prepare(&PolicySummary::default(), Some(&exe), t.path(), SID).unwrap();
        assert!(ok.config_path().unwrap().ends_with("settings.json"));
    }
}

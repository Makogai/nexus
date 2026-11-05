//! NEXUS's built-in Claude Code plugin `nexus` (ADR-0041): skills that ship inside NEXUS itself — read-only, versioned
//! with NEXUS, no hooks, no MCP servers, no code. NEXUS writes them from the binary into its data folder (one folder
//! per content version) and passes that folder with `--plugin-dir` only to the sessions that need it — today the
//! learning sessions of `project::learn`. Other sessions never see these skills, so they cost nothing there.

use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};

/// The plugin's name; Claude namespaces its skills as `nexus:<skill>`.
pub const PLUGIN: &str = "nexus";
/// The learning procedure (`project::learn`).
pub const LEARN_SKILL: &str = "nexus:learn-project";

const FILES: &[(&str, &str)] = &[
    (".claude-plugin/plugin.json", include_str!("../../builtin/nexus/.claude-plugin/plugin.json")),
    ("skills/learn-project/SKILL.md", include_str!("../../builtin/nexus/skills/learn-project/SKILL.md")),
];

/// The plugin folder for `--plugin-dir`, written when missing or different (a crash half-way, an edit by hand).
/// Older versions stay: a session started by an earlier NEXUS may still read its skill.
pub fn plugin_dir(data_dir: &Path) -> Result<PathBuf> {
    let mut all = Vec::new();
    for (rel, text) in FILES {
        all.extend_from_slice(rel.as_bytes());
        all.extend_from_slice(text.as_bytes());
    }
    let hash = crate::knowledge::content_hash(&all);
    let parent = data_dir.join("builtin").join(&hash[..hash.len().min(12)]);
    let dir = parent.join(PLUGIN);
    if FILES.iter().all(|(rel, text)| std::fs::read_to_string(dir.join(rel)).is_ok_and(|t| t == *text)) {
        return Ok(dir);
    }
    // Write next to it and swap in, so a session never sees a half-written plugin.
    let tmp = parent.join(format!(".{PLUGIN}-{}", crate::new_id()));
    for (rel, text) in FILES {
        let path = tmp.join(rel);
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| CoreError::io("cannot create", p, e))?;
        }
        std::fs::write(&path, text).map_err(|e| CoreError::io("cannot write", &path, e))?;
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| CoreError::io("cannot replace", &dir, e))?;
    }
    std::fs::rename(&tmp, &dir).map_err(|e| CoreError::io("cannot write", &dir, e))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plugin_is_written_once_per_version_and_repaired_when_changed() {
        let t = tempfile::tempdir().unwrap();
        let dir = plugin_dir(t.path()).unwrap();
        assert_eq!(dir.file_name().unwrap(), PLUGIN);
        let inv = crate::claude::plugin::inventory(&dir);
        assert_eq!(inv.name, PLUGIN);
        assert_eq!(inv.skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["learn-project"]);
        assert!(inv.hooks.is_empty() && inv.mcp_servers.is_empty() && inv.agents.is_empty(), "skills only, no code");
        assert_eq!(LEARN_SKILL, format!("{PLUGIN}:learn-project"));

        let skill = dir.join("skills/learn-project/SKILL.md");
        let before = std::fs::metadata(&skill).unwrap().modified().unwrap();
        assert_eq!(plugin_dir(t.path()).unwrap(), dir, "same version, same folder");
        assert_eq!(std::fs::metadata(&skill).unwrap().modified().unwrap(), before, "not rewritten");
        std::fs::write(&skill, "changed by hand").unwrap();
        plugin_dir(t.path()).unwrap();
        assert!(std::fs::read_to_string(&skill).unwrap().contains("name: learn-project"), "repaired");
        let leftovers: Vec<_> = std::fs::read_dir(dir.parent().unwrap()).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name()).collect();
        assert_eq!(leftovers, [std::ffi::OsString::from(PLUGIN)], "no temporary folders left");
    }
}

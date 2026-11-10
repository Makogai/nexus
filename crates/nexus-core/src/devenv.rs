//! The Dev Environment page (ADR-0051): this computer's development tools next to what the active workspace's projects
//! need. Project needs come from the projects' own files (or the workspace's environment record when the clone is on
//! another computer — P5); the computer's side comes from [`toolchain::inventory`], which only runs version commands.
//! Nothing here installs anything: install commands and version-manager hints are for the person to run.

use rusqlite::Connection;
use serde::Serialize;

use crate::error::Result;
use crate::project::{self, environment};
use crate::toolchain::{self, Hint, Installed};
use crate::workspace;

/// One project that needs a tool.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Need {
    pub project_id: String,
    pub project: String,
    /// As written in the project file; empty = any version.
    pub constraint: String,
    pub source: String,
    /// `ok`, `mismatch`, `missing` or `unknown` (`environment::fit`).
    pub status: &'static str,
    /// An installed version manager's command for the exact version the project asks for (mismatch or missing only).
    pub hint: Option<Hint>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(flatten)]
    pub installed: Installed,
    pub needed_by: Vec<Need>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub workspace_id: Option<String>,
    /// `windows`, `macos` or `linux` (install commands exist for the first two).
    pub platform: &'static str,
    /// When `inventory` was taken (ms).
    pub probed_at: i64,
    pub tools: Vec<Entry>,
    /// Projects whose needs are unknown (not on this computer and no environment record yet).
    pub projects_without_environment: Vec<String>,
}

pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// The inventory joined with the needs of the active workspace's projects.
pub fn view(conn: &Connection, inventory: &[Installed], probed_at: i64) -> Result<View> {
    let ws = workspace::active(conn)?.filter(|w| w.available);
    let mut tools: Vec<Entry> = inventory.iter().map(|i| Entry { installed: i.clone(), needed_by: Vec::new() }).collect();
    let mut without = Vec::new();
    let has = |id: &str| inventory.iter().any(|i| i.tool.id == id && i.path.is_some());
    if let Some(ws) = &ws {
        for p in project::list(conn, &ws.id)? {
            let Some(report) = environment::report(ws, &p, false) else {
                without.push(p.name.clone());
                continue;
            };
            for r in &report.environment.requirements {
                let Some(entry) = tools.iter_mut().find(|e| e.installed.tool.id == r.tool) else { continue };
                if entry.needed_by.iter().any(|n| n.project_id == p.id && n.constraint == r.constraint) {
                    continue; // the same need from two files of one project (`.nvmrc` and `.node-version`)
                }
                let found = entry.installed.path.as_ref().map(|_| entry.installed.version.as_deref());
                let status = environment::fit(found, &r.constraint);
                let hint = matches!(status, "mismatch" | "missing").then(|| toolchain::version_hint(&r.tool, &r.constraint, &has)).flatten();
                entry.needed_by.push(Need { project_id: p.id.clone(), project: p.name.clone(), constraint: r.constraint.clone(), source: r.source.clone(), status, hint });
            }
        }
    }
    Ok(View { workspace_id: ws.map(|w| w.id), platform: platform(), probed_at, tools, projects_without_environment: without })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn inv(found: &[(&str, &str)]) -> Vec<Installed> {
        toolchain::TOOLS
            .iter()
            .filter(|t| t.on_this_platform())
            .map(|t| {
                let v = found.iter().find(|(id, _)| *id == t.id).map(|(_, v)| v.to_string());
                Installed { tool: *t, path: v.as_ref().map(|_| PathBuf::from(format!("/bin/{}", t.id))), version: v.filter(|v| !v.is_empty()), copies: vec![], install_command: None, note: None }
            })
            .collect()
    }

    #[test]
    fn projects_needs_are_joined_with_this_computer() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let ws = workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
        workspace::activate(&conn, &ws.id).unwrap();
        let repo = root.join("shop");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join(".nvmrc"), "20.11.1\n").unwrap();
        std::fs::write(repo.join(".node-version"), "20.11.1\n").unwrap();
        std::fs::write(repo.join("package.json"), r#"{"engines":{"node":">=18"}}"#).unwrap();
        std::fs::write(repo.join("go.mod"), "module x\n\ngo 1.22\n").unwrap();
        let p = project::register(&conn, &ws.id, repo.to_str().unwrap(), Some("shop")).unwrap();

        let view = view(&conn, &inv(&[("node", "18.19.0"), ("fnm", "1.37.2"), ("git", "2.47.0")]), 7).unwrap();
        assert_eq!(view.workspace_id.as_deref(), Some(ws.id.as_str()));
        assert_eq!(view.probed_at, 7);
        let node = view.tools.iter().find(|e| e.installed.tool.id == "node").unwrap();
        let needs: Vec<(&str, &str, &str)> = node.needed_by.iter().map(|n| (n.constraint.as_str(), n.source.as_str(), n.status)).collect();
        assert_eq!(needs, [("20.11.1", ".nvmrc", "mismatch"), (">=18", "package.json engines.node", "ok")], "one need per constraint and project");
        assert_eq!(node.needed_by[0].project_id, p.id);
        assert_eq!(node.needed_by[0].hint.as_ref().map(|h| h.command.clone()), Some(vec!["fnm".to_string(), "install".into(), "20.11.1".into()]));
        assert!(node.needed_by[1].hint.is_none(), "fits: no hint");
        let go = view.tools.iter().find(|e| e.installed.tool.id == "go").unwrap();
        assert_eq!(go.needed_by[0].status, "missing");
        assert!(go.needed_by[0].hint.is_none(), "a range gets no version hint");
        assert!(view.tools.iter().find(|e| e.installed.tool.id == "php").unwrap().needed_by.is_empty());
    }

    #[test]
    fn without_a_workspace_the_computer_is_shown_alone() {
        let conn = crate::db::open_in_memory().unwrap();
        let view = view(&conn, &inv(&[("git", "2.47.0")]), 1).unwrap();
        assert!(view.workspace_id.is_none() && view.projects_without_environment.is_empty());
        assert!(view.tools.iter().all(|e| e.needed_by.is_empty()));
        assert_eq!(view.tools.len(), toolchain::TOOLS.iter().filter(|t| t.on_this_platform()).count());
    }
}

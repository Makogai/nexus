//! New-computer restoration (Portable Workspace P8): one ordered plan over the existing pieces — Claude Code, the
//! workspace's schema, the knowledge index, approvals of synced rules/constitutions/workflows, the workspace skills,
//! the projects (missing clones, links), their toolchains, integrations, Claude profiles and quality gates — each step
//! with its state and where it is done. It also says plainly what came back with the workspace and what lived only on
//! the other computer and cannot be recovered from workspace metadata (sessions, transcripts, uncommitted source …).
//! Read-only except for refreshing this machine's knowledge index.

use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::error::Result;
use crate::knowledge::index::{self, Filter};
use crate::workspace::{self, Workspace};
use crate::{kits, project};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: &'static str,
    pub title: &'static str,
    /// `done`, `todo`, `optional` or `blocked`.
    pub status: &'static str,
    pub summary: String,
    pub detail: Vec<String>,
    /// What resolves it: a Workspace Doctor repair id, or a desktop view.
    pub repair: Option<String>,
    pub navigate: Option<&'static str>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Restored {
    pub constitutions: usize,
    pub rules: usize,
    pub workflows: usize,
    pub skills: usize,
    pub projects: usize,
    pub tasks: usize,
    pub decisions: usize,
    pub notes: usize,
    pub development_records: usize,
    pub template: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Restoration {
    pub workspace_id: String,
    pub name: String,
    pub steps: Vec<Step>,
    /// Portable knowledge and configuration that came with the workspace repository.
    pub restored: Restored,
    /// What only lived on the other computer.
    pub not_recoverable: Vec<&'static str>,
    pub done: usize,
    pub total: usize,
}

const NOT_RECOVERABLE: &[&str] = &[
    "Claude conversations and transcripts — they stay in the other computer's Claude profiles (development records keep what was decided and verified)",
    "Uncommitted or unpushed source code in the project repositories",
    "Local-only knowledge (.nexus-local/) and global knowledge in the other computer's NEXUS data folder",
    "Machine-local state: sessions, checkpoints, usage, notifications, sync history",
    "Approvals and quality-gate commands — approve and define them again here",
    "Claude profiles, sign-ins and credentials — add and sign in on this computer",
];

fn step(id: &'static str, title: &'static str, status: &'static str, summary: impl Into<String>) -> Step {
    Step { id, title, status, summary: summary.into(), detail: Vec::new(), repair: None, navigate: None }
}

/// The restoration plan for `ws` on this computer. `probe` looks for Claude Code and the projects' tools.
pub fn plan(conn: &Connection, data_dir: &Path, home: &Path, ws: &Workspace, probe: bool) -> Result<Restoration> {
    let root = &ws.root_path;
    let mut steps = Vec::new();

    // 1. Claude Code.
    if probe {
        let installs = crate::claude::install::discover(home);
        let mut s = match installs.iter().find(|i| i.launchable) {
            Some(i) => step("claude", "Claude Code", "done", format!("Claude Code {} is installed", i.version.as_deref().unwrap_or(""))),
            None => step("claude", "Claude Code", "todo", "Install Claude Code with an official method"),
        };
        s.navigate = Some("claude-setup");
        steps.push(s);
    }

    // 2. Workspace schema.
    let manifest = workspace::read_manifest(root)?;
    let c = workspace::compat(&manifest);
    let mut s = if !c.writable {
        step("schema", "Workspace schema", "blocked", c.reason.clone().unwrap_or_default())
    } else if c.needs_migration {
        let mut s = step("schema", "Workspace schema", "todo", format!("Schema {} — migrate to {} (nothing is moved or deleted)", manifest.schema_version, workspace::MANIFEST_SCHEMA));
        s.repair = Some("migrate-schema".into());
        s
    } else {
        step("schema", "Workspace schema", "done", format!("Schema {}, compatible with this NEXUS", manifest.schema_version))
    };
    s.navigate = Some("workspace");
    steps.push(s);

    // 3. Knowledge index (rebuilt on this machine from the files).
    let report = crate::knowledge::service::refresh(conn, data_dir, ws)?;
    let items = index::list(conn, &ws.id, &Filter { include_invalid: true, limit: Some(2000), ..Default::default() })?;
    let mine: Vec<&index::Item> = items.iter().filter(|i| i.workspace_id.as_deref() == Some(ws.id.as_str())).collect();
    let mut s = step("index", "Knowledge index", if report.invalid == 0 { "done" } else { "todo" }, format!("{} document(s) indexed on this computer", mine.iter().filter(|i| i.valid).count()));
    if report.invalid > 0 {
        s.detail = mine.iter().filter(|i| !i.valid).take(8).map(|i| format!("{} — {}", i.path.strip_prefix(root).unwrap_or(&i.path).display(), i.error.clone().unwrap_or_default())).collect();
        s.navigate = Some("knowledge");
    }
    steps.push(s);

    // 4. Approvals of synced rules, constitutions and workflows.
    let pending: Vec<String> = mine
        .iter()
        .filter(|i| i.valid && matches!(i.kind.as_deref(), Some("rule") | Some("workflow")))
        .filter(|i| !index::is_approved(conn, i).unwrap_or(false))
        .map(|i| i.title.clone())
        .collect();
    let mut s = if pending.is_empty() {
        step("approvals", "Constitutions, rules and workflows", "done", "Everything synced is approved on this computer")
    } else {
        let mut s = step("approvals", "Constitutions, rules and workflows", "todo", format!("Review and approve {} synced document(s) — they reach Claude only once approved here", pending.len()));
        s.detail = pending.into_iter().take(12).collect();
        s
    };
    s.navigate = Some("constitutions");
    steps.push(s);

    // 5. Workspace skills.
    let skills = crate::claude::plugin::inventory(&root.join("skills")).skills.len();
    if skills > 0 {
        let s = match kits::workspace_kit(conn, &ws.id)? {
            None => {
                let mut s = step("skills", "Workspace skills", "todo", format!("{skills} skill(s) — register them for review, then approve them in AI Kits"));
                s.repair = Some("register-workspace-skills".into());
                s.navigate = Some("kits");
                s
            }
            Some(k) => match kits::view(conn, data_dir, &k)?.status {
                "approved" => step("skills", "Workspace skills", "done", format!("{skills} skill(s) approved for this workspace's sessions")),
                _ => {
                    let mut s = step("skills", "Workspace skills", "todo", format!("{skills} skill(s) wait for your review in AI Kits"));
                    s.navigate = Some("kits");
                    s
                }
            },
        };
        steps.push(s);
    }

    // 6. Projects.
    let projects = project::list(conn, &ws.id)?;
    let scan = project::discovery::scan(conn, ws)?;
    let cloneable = scan.missing.iter().filter(|m| m.can_clone).count();
    let known = scan.found.iter().filter(|f| f.status == "known").count();
    let mut s = if scan.missing.is_empty() && known == 0 {
        step("projects", "Projects", "done", format!("{} project(s) available on this computer", projects.len()))
    } else {
        let mut s = step("projects", "Projects", "todo", format!("{} project(s) missing here; {cloneable} can be cloned from their remotes{}", scan.missing.len(), if known > 0 { format!(", {known} clone(s) to link") } else { String::new() }));
        s.detail = scan.missing.iter().map(|m| format!("{} — {}", m.name, m.clone_to.clone().or(m.reason.clone()).unwrap_or_default())).collect();
        s.repair = if known > 0 { Some("link-known".into()) } else if cloneable > 0 { Some("clone-missing".into()) } else { None };
        s
    };
    s.navigate = Some("projects");
    steps.push(s);

    // 7. Toolchains of the projects that are here.
    if probe {
        let mut unmet = Vec::new();
        for p in projects.iter().filter(|p| p.available) {
            if let Some(r) = project::environment::report(ws, p, true) {
                unmet.extend(r.checks.iter().filter(|c| c.status == "missing" || c.status == "mismatch").map(|c| format!("{}: {} {}", p.name, c.name, c.constraint)));
            }
        }
        let mut s = if unmet.is_empty() { step("toolchains", "Development tools", "done", "The projects' required tools are installed") } else {
            let mut s = step("toolchains", "Development tools", "todo", format!("{} requirement(s) not met — install them from the projects' Environment cards", unmet.len()));
            s.detail = unmet;
            s
        };
        s.navigate = Some("projects");
        steps.push(s);
    }

    // 8. Claude profiles (machine-local).
    let profiles = crate::claude::profile::list(conn, false)?.len();
    let mut s = if profiles > 0 { step("profiles", "Claude profiles", "done", format!("{profiles} profile(s) on this computer")) } else { step("profiles", "Claude profiles", "todo", "Add your Claude profiles and sign in — they never travel with the workspace") };
    s.navigate = Some("profiles");
    steps.push(s);

    // 9. Quality gates the workflows use (machine-local commands).
    let restore = crate::workspace_sync::ops::restore(conn, data_dir, ws)?;
    if !restore.gates_needed.is_empty() {
        let mut s = step("gates", "Quality gates", "optional", format!("Workflows use the gates {} — define their commands per project when you run them", restore.gates_needed.join(", ")));
        s.navigate = Some("workflows");
        steps.push(s);
    }

    // 10. Continue.
    let mut s = step("continue", "Continue where you left off", "optional", "Open a project: its overview, current state, development history and open tasks came with the workspace");
    s.navigate = Some("projects");
    steps.push(s);

    let count = |kind: &str| mine.iter().filter(|i| i.valid && i.kind.as_deref() == Some(kind)).count();
    let constitutions = mine.iter().filter(|i| i.valid && i.kind.as_deref() == Some("rule") && crate::knowledge::constitution::is_constitution(&i.path)).count();
    let records = mine
        .iter()
        .filter(|i| i.valid && i.kind.as_deref() == Some("note") && i.path.components().any(|c| c.as_os_str() == "development"))
        .count();
    let restored = Restored {
        constitutions,
        rules: count("rule") - constitutions,
        workflows: count("workflow"),
        skills,
        projects: projects.len(),
        tasks: count("task"),
        decisions: count("decision"),
        notes: count("note") - records,
        development_records: records,
        template: manifest.template.map(|t| format!("{} {}", t.id, t.version)),
    };
    let total = steps.iter().filter(|s| s.status != "optional").count();
    let done = steps.iter().filter(|s| s.status == "done").count();
    Ok(Restoration { workspace_id: ws.id.clone(), name: ws.name.clone(), steps, restored, not_recoverable: NOT_RECOVERABLE.to_vec(), done, total })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cloned_workspace_lists_what_this_computer_still_needs_and_what_cannot_come_back() {
        let t = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let data = base.join("data");
        let ws = workspace::create(&conn, base.join("ws").to_str().unwrap(), "W", "").unwrap();
        crate::templates::apply(&conn, &data, &ws, "developer").unwrap();

        // Another computer: a fresh database, the same folder — approvals and the kit are machine-local.
        let conn_b = crate::db::open_in_memory().unwrap();
        let ws_b = workspace::import(&conn_b, ws.root_path.to_str().unwrap()).unwrap();
        let r = plan(&conn_b, &base.join("data-b"), &base, &ws_b, false).unwrap();
        let st = |id: &str| r.steps.iter().find(|s| s.id == id).map(|s| s.status).unwrap_or("absent");
        assert_eq!(st("schema"), "done");
        assert_eq!(st("index"), "done");
        assert_eq!(st("approvals"), "todo", "synced constitutions and workflows need approval here");
        assert_eq!(st("skills"), "todo");
        assert_eq!(r.steps.iter().find(|s| s.id == "skills").unwrap().repair.as_deref(), Some("register-workspace-skills"));
        assert_eq!(st("profiles"), "todo");
        assert_eq!(st("projects"), "done");
        assert_eq!((r.restored.constitutions, r.restored.workflows, r.restored.skills), (6, 4, 5));
        assert_eq!(r.restored.template.as_deref(), Some("developer 1.0.0"));
        assert!(r.not_recoverable.iter().any(|n| n.contains("transcripts")) && r.not_recoverable.iter().any(|n| n.contains("Uncommitted")));
        assert!(r.done < r.total);
    }
}

//! Health checks shared by `nexus doctor` and (later) the desktop diagnostics view. Read-only: checks report
//! problems and suggest fixes; they never change Claude configurations or repositories.

use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::claude::policy::{self, MachinePolicyLocations};
use crate::claude::{install, profile};
use crate::error::Result;
use crate::{db, git, project, workspace};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub area: &'static str,
    pub status: Status,
    pub summary: String,
    pub detail: Option<String>,
    pub fix: Option<String>,
}

impl Check {
    fn new(area: &'static str, status: Status, summary: impl Into<String>) -> Self {
        Check {
            area,
            status,
            summary: summary.into(),
            detail: None,
            fix: None,
        }
    }
    fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
    fn fix(mut self, f: impl Into<String>) -> Self {
        self.fix = Some(f.into());
        self
    }
}

/// Inputs that differ between production and tests.
pub struct Context<'a> {
    pub db_path: &'a Path,
    pub home: &'a Path,
    pub machine_policy: &'a MachinePolicyLocations,
    /// Skip spawning `claude --version` (tests with no Claude installed).
    pub probe_installations: bool,
}

const MIN_GIT: (u32, u32) = (2, 30);

pub fn run(conn: &Connection, ctx: &Context<'_>) -> Result<Vec<Check>> {
    let mut checks = Vec::new();

    // Database
    let version = db::schema_version(conn)?;
    let quick: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    checks.push(if quick == "ok" {
        Check::new("database", Status::Ok, format!("database healthy (schema v{version})")).detail(ctx.db_path.display().to_string())
    } else {
        Check::new("database", Status::Error, "database integrity check failed")
            .detail(quick)
            .fix("Restart NEXUS: an unreadable database is moved aside automatically and a fresh one is created.")
    });

    // Git
    checks.push(match git::version() {
        Some(v) if git_at_least(&v, MIN_GIT) => Check::new("git", Status::Ok, format!("git {v}")),
        Some(v) => Check::new("git", Status::Warn, format!("git {v} is older than {}.{}", MIN_GIT.0, MIN_GIT.1))
            .fix("Update Git; NEXUS relies on `status --porcelain=v2 -z`."),
        None => Check::new("git", Status::Error, "git was not found on PATH").fix("Install Git (https://git-scm.com)."),
    });

    // Claude installations
    if ctx.probe_installations {
        let installs = install::discover(ctx.home);
        if installs.iter().any(|i| i.launchable) {
            for i in &installs {
                let c = if i.launchable {
                    Check::new("claude", Status::Ok, format!("Claude Code {}", i.version.as_deref().unwrap_or("(version unknown)")))
                } else {
                    Check::new("claude", Status::Warn, "Claude Code launcher that NEXUS cannot start")
                        .fix(i.issue.clone().unwrap_or_default())
                };
                checks.push(c.detail(i.path.display().to_string()));
            }
        } else {
            checks.push(
                Check::new("claude", Status::Error, "no launchable Claude Code executable found")
                    .fix("Install Claude Code (https://code.claude.com) or set an executable on the profile."),
            );
        }
    }

    // Profiles
    let profiles = profile::list(conn, false)?;
    if profiles.is_empty() {
        checks.push(
            Check::new("profiles", Status::Info, "no Claude profile registered")
                .fix("Register one: `nexus profiles add --name Personal --config-dir ~/.claude-personal`."),
        );
    }
    for p in &profiles {
        let dir = p.effective_config_dir(ctx.home);
        let label = format!("profile “{}”", p.name);
        if !dir.is_dir() && !p.managed_by_nexus {
            checks.push(
                Check::new("profiles", Status::Error, format!("{label}: configuration folder is missing"))
                    .detail(dir.display().to_string())
                    .fix("Restore the folder or forget the profile."),
            );
            continue;
        }
        if let Some(exe) = &p.executable {
            let (ok, issue) = install::launchability(exe);
            if !ok {
                checks.push(
                    Check::new("profiles", Status::Error, format!("{label}: executable cannot be launched"))
                        .detail(exe.display().to_string())
                        .fix(issue.unwrap_or_default()),
                );
                continue;
            }
        }
        let pol = policy::evaluate(&dir, ctx.machine_policy);
        let mut blocked = Vec::new();
        if !pol.sideload_allowed() {
            blocked.push("session plugins/MCP (disableSideloadFlags)");
        }
        if !pol.session_hooks_allowed() {
            blocked.push("session hooks (managed hooks only)");
        }
        if pol.allow_managed_mcp_servers_only {
            blocked.push("non-managed MCP servers");
        }
        let c = if pol.organization_managed {
            let summary = if blocked.is_empty() {
                format!("{label}: organisation-managed; no restriction affects NEXUS")
            } else {
                format!("{label}: organisation-managed; NEXUS will omit {}", blocked.join(", "))
            };
            Check::new("profiles", Status::Info, summary)
        } else {
            Check::new("profiles", Status::Ok, format!("{label}: ready"))
        };
        checks.push(c.detail(dir.display().to_string()));
        if !pol.unreadable.is_empty() {
            checks.push(
                Check::new("profiles", Status::Warn, format!("{label}: a managed settings file could not be parsed"))
                    .detail(pol.unreadable.join(", ")),
            );
        }
    }

    // Workspace and projects
    match workspace::active(conn)? {
        None => checks.push(
            Check::new("workspace", Status::Info, "no active workspace")
                .fix("Create one: `nexus workspaces create <folder> --name Personal`."),
        ),
        Some(ws) if !ws.available => checks.push(
            Check::new("workspace", Status::Error, format!("workspace “{}” folder is missing", ws.name))
                .detail(ws.root_path.display().to_string())
                .fix("Restore the folder or open the workspace from its new location."),
        ),
        Some(ws) => {
            checks.push(Check::new("workspace", Status::Ok, format!("workspace “{}”", ws.name)).detail(ws.root_path.display().to_string()));
            for p in project::list(conn, &ws.id)? {
                if !p.available {
                    checks.push(
                        Check::new("projects", Status::Warn, format!("project “{}” is not on this machine", p.name))
                            .fix("Clone it and run `nexus projects locate <name> <folder>`."),
                    );
                }
            }
        }
    }
    Ok(checks)
}

fn git_at_least(version: &str, min: (u32, u32)) -> bool {
    let mut parts = version.split(['.', ' ']).filter_map(|p| p.parse::<u32>().ok());
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => (a, b) >= min,
        _ => true, // unknown format: do not cry wolf
    }
}

pub fn worst(checks: &[Check]) -> Status {
    checks.iter().map(|c| c.status).max().unwrap_or(Status::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_missing_profile_dirs_and_policy_restrictions() {
        let t = tempfile::tempdir().unwrap();
        let conn = db::open_in_memory().unwrap();
        let managed = t.path().join("managed");
        std::fs::create_dir(&managed).unwrap();
        std::fs::write(managed.join("remote-settings.json"), r#"{"disableSideloadFlags": true}"#).unwrap();
        let gone = t.path().join("gone");
        std::fs::create_dir(&gone).unwrap();
        for (name, dir) in [("Work", &managed), ("Old", &gone)] {
            profile::register(
                &conn,
                &profile::ProfileInput {
                    name: name.into(),
                    config_dir: Some(dir.display().to_string()),
                    ..Default::default()
                },
                t.path(),
            )
            .unwrap();
        }
        std::fs::remove_dir(&gone).unwrap();
        let machine = MachinePolicyLocations::default();
        let checks = run(
            &conn,
            &Context {
                db_path: Path::new(":memory:"),
                home: t.path(),
                machine_policy: &machine,
                probe_installations: false,
            },
        )
        .unwrap();
        let find = |needle: &str| checks.iter().find(|c| c.summary.contains(needle)).unwrap_or_else(|| panic!("{needle}: {checks:#?}"));
        assert_eq!(find("database healthy").status, Status::Ok);
        assert_eq!(find("“Old”: configuration folder is missing").status, Status::Error);
        let work = find("“Work”: organisation-managed");
        assert!(work.summary.contains("disableSideloadFlags"));
        assert_eq!(find("no active workspace").status, Status::Info);
        assert_eq!(worst(&checks), Status::Error);
    }

    #[test]
    fn git_version_comparison() {
        assert!(git_at_least("2.55.0.windows.2", (2, 30)));
        assert!(!git_at_least("2.17.1", (2, 30)));
        assert!(git_at_least("weird", (2, 30)));
    }
}

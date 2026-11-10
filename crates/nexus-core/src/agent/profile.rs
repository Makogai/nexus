//! Profiles of agents other than Claude Code (ADR-0052): which home the agent uses (its default, or an existing folder
//! the person registered), which executable, extra arguments and defaults. NEXUS never writes into an agent's home.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::Kind;
use crate::error::{CoreError, Result};
use crate::paths::{self, Expect};
use crate::{new_id, now_ms, workprofile};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub id: String,
    pub agent: Kind,
    pub name: String,
    /// `None` = the agent's default home (`~/.codex`).
    pub home_dir: Option<PathBuf>,
    pub executable: Option<PathBuf>,
    pub extra_args: Vec<String>,
    /// Keep the agent's API credentials from the environment (`OPENAI_API_KEY` …); off = removed.
    pub inherit_provider_env: bool,
    /// Let the agent report finished turns to NEXUS (Codex: its `notify` program, replacing your own for NEXUS sessions).
    pub turn_notifications: bool,
    pub default_work_profile: Option<String>,
    pub default_model: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_used_at: Option<i64>,
    pub archived_at: Option<i64>,
}

impl AgentProfile {
    /// The home the agent actually uses for this profile.
    pub fn effective_home(&self, home: &Path) -> PathBuf {
        self.home_dir.clone().unwrap_or_else(|| self.agent.default_home(home))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfileInput {
    pub agent: Kind,
    pub name: String,
    /// Absolute path of an existing folder; empty/None = the agent's default home.
    #[serde(default)]
    pub home_dir: Option<String>,
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub extra_args: Vec<String>,
    #[serde(default)]
    pub inherit_provider_env: bool,
    #[serde(default = "yes")]
    pub turn_notifications: bool,
    #[serde(default)]
    pub default_work_profile: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
}

fn yes() -> bool {
    true
}

impl AgentProfileInput {
    /// The default profile for an agent: its own home, everything else default.
    pub fn default_for(agent: Kind) -> AgentProfileInput {
        AgentProfileInput {
            agent,
            name: agent.short().to_string(),
            home_dir: None,
            executable: None,
            extra_args: Vec::new(),
            inherit_provider_env: false,
            turn_notifications: true,
            default_work_profile: None,
            default_model: None,
        }
    }
}

const SELECT: &str = "SELECT id, agent, name, home_dir, executable, extra_args_json, inherit_provider_env, turn_notifications,
    default_work_profile, default_model, created_at, updated_at, last_used_at, archived_at FROM agent_profiles";

/// A row of an agent this build does not know (written by a newer NEXUS) is `None`: never launched as another agent.
fn from_row(r: &Row<'_>) -> rusqlite::Result<Option<AgentProfile>> {
    let agent: String = r.get(1)?;
    let Some(agent) = Kind::parse(&agent) else { return Ok(None) };
    Ok(Some(AgentProfile {
        id: r.get(0)?,
        agent,
        name: r.get(2)?,
        home_dir: r.get::<_, Option<String>>(3)?.map(PathBuf::from),
        executable: r.get::<_, Option<String>>(4)?.map(PathBuf::from),
        extra_args: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default(),
        inherit_provider_env: r.get(6)?,
        turn_notifications: r.get(7)?,
        default_work_profile: r.get(8)?,
        default_model: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
        last_used_at: r.get(12)?,
        archived_at: r.get(13)?,
    }))
}

struct Valid {
    name: String,
    home_dir: Option<PathBuf>,
    executable: Option<PathBuf>,
    extra_args: Vec<String>,
    default_work_profile: Option<String>,
    default_model: Option<String>,
}

fn opt(v: &Option<String>) -> Option<String> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

fn validate(input: &AgentProfileInput, home: &Path) -> Result<Valid> {
    if input.agent == Kind::Claude {
        return Err(CoreError::invalid("Claude Code profiles are managed under Claude profiles"));
    }
    let name = input.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
        return Err(CoreError::invalid("name must be 1–60 characters"));
    }
    let home_dir = match input.home_dir.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        None => None,
        Some(raw) => {
            let p = paths::validate_user_path(raw, Expect::ExistingDir)?;
            let default = input.agent.default_home(home);
            if paths::same_path(&p, &default) {
                None
            } else if paths::same_path(&p, home) {
                return Err(CoreError::invalid(format!("the home folder itself cannot be a {} home", input.agent.short())));
            } else {
                Some(p)
            }
        }
    };
    let executable = match input.executable.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
        None => None,
        Some(raw) => {
            let p = paths::validate_user_path(raw, Expect::ExistingFile)?;
            let (ok, issue) = crate::claude::install::native_launchability(&p, super::launch::shim_fix(input.agent));
            if !ok {
                return Err(CoreError::invalid(issue.unwrap_or_else(|| "executable cannot be launched".into())));
            }
            Some(p)
        }
    };
    match input.agent {
        Kind::Codex => crate::codex::launch::validate_extra_args(&input.extra_args)?,
        Kind::Claude => unreachable!("refused above"),
    }
    let default_work_profile = opt(&input.default_work_profile);
    if let Some(w) = &default_work_profile
        && workprofile::get(w).is_none()
    {
        return Err(CoreError::invalid(format!("unknown work profile '{w}'")));
    }
    let default_model = opt(&input.default_model);
    if let Some(m) = &default_model {
        crate::claude::profile::validate_model(m)?;
    }
    Ok(Valid { name, home_dir, executable, extra_args: input.extra_args.clone(), default_work_profile, default_model })
}

fn ensure_home_unique(conn: &Connection, agent: Kind, dir: &Option<PathBuf>, except: Option<&str>) -> Result<()> {
    for p in list(conn, false)?.into_iter().filter(|p| p.agent == agent && Some(p.id.as_str()) != except) {
        let clash = match (&p.home_dir, dir) {
            (None, None) => true,
            (Some(a), Some(b)) => paths::same_path(a, b),
            _ => false,
        };
        if clash {
            return Err(CoreError::Conflict(format!("profile '{}' already uses this {} home", p.name, agent.short())));
        }
    }
    Ok(())
}

fn map_unique(res: rusqlite::Result<usize>, name: &str) -> Result<()> {
    match res {
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => {
            Err(CoreError::Conflict(format!("a profile named '{name}' already exists")))
        }
        Err(e) => Err(e.into()),
        Ok(_) => Ok(()),
    }
}

/// Register an agent home (its default, or an existing folder). Read-only: nothing is written into the folder.
pub fn register(conn: &Connection, input: &AgentProfileInput, home: &Path) -> Result<AgentProfile> {
    let v = validate(input, home)?;
    ensure_home_unique(conn, input.agent, &v.home_dir, None)?;
    let id = new_id();
    let now = now_ms();
    let res = conn.execute(
        "INSERT INTO agent_profiles(id, agent, name, home_dir, executable, extra_args_json, inherit_provider_env,
            turn_notifications, default_work_profile, default_model, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
        params![
            id,
            input.agent.id(),
            v.name,
            v.home_dir.as_ref().map(|d| d.to_string_lossy()),
            v.executable.as_ref().map(|e| e.to_string_lossy()),
            serde_json::to_string(&v.extra_args).unwrap_or_else(|_| "[]".into()),
            input.inherit_provider_env,
            input.turn_notifications,
            v.default_work_profile,
            v.default_model,
            now,
        ],
    );
    map_unique(res, &v.name)?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, input: &AgentProfileInput, home: &Path) -> Result<AgentProfile> {
    let current = get(conn, id)?;
    if current.agent != input.agent {
        return Err(CoreError::invalid("a profile's agent cannot change"));
    }
    let v = validate(input, home)?;
    ensure_home_unique(conn, input.agent, &v.home_dir, Some(id))?;
    let res = conn.execute(
        "UPDATE agent_profiles SET name = ?2, home_dir = ?3, executable = ?4, extra_args_json = ?5, inherit_provider_env = ?6,
            turn_notifications = ?7, default_work_profile = ?8, default_model = ?9, updated_at = ?10 WHERE id = ?1",
        params![
            id,
            v.name,
            v.home_dir.as_ref().map(|d| d.to_string_lossy()),
            v.executable.as_ref().map(|e| e.to_string_lossy()),
            serde_json::to_string(&v.extra_args).unwrap_or_else(|_| "[]".into()),
            input.inherit_provider_env,
            input.turn_notifications,
            v.default_work_profile,
            v.default_model,
            now_ms(),
        ],
    );
    map_unique(res, &v.name)?;
    get(conn, id)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<AgentProfile>> {
    let filter = if include_archived { "" } else { " WHERE archived_at IS NULL" };
    let mut stmt = conn.prepare(&format!("{SELECT}{filter} ORDER BY COALESCE(last_used_at, 0) DESC, name COLLATE NOCASE"))?;
    let rows = stmt.query_map([], from_row)?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().flatten().collect())
}

pub fn get(conn: &Connection, id: &str) -> Result<AgentProfile> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row)
        .optional()?
        .flatten()
        .ok_or_else(|| CoreError::not_found(format!("agent profile {id} not found")))
}

/// A profile by id or (case-insensitive) name, for the CLI.
pub fn find(conn: &Connection, agent: Kind, query: &str) -> Result<AgentProfile> {
    let all = list(conn, false)?;
    all.iter()
        .find(|p| p.id == query)
        .or_else(|| all.iter().find(|p| p.agent == agent && p.name.eq_ignore_ascii_case(query)))
        .cloned()
        .ok_or_else(|| CoreError::not_found(format!("no {} profile '{query}'", agent.short())))
}

pub fn set_archived(conn: &Connection, id: &str, archived: bool) -> Result<AgentProfile> {
    let p = get(conn, id)?;
    if !archived {
        ensure_home_unique(conn, p.agent, &p.home_dir, Some(id))?;
    }
    conn.execute("UPDATE agent_profiles SET archived_at = ?2, updated_at = ?3 WHERE id = ?1", params![id, archived.then(now_ms), now_ms()])?;
    get(conn, id)
}

/// Remove the registration. Never touches the agent's home (its login and history live there).
pub fn forget(conn: &Connection, id: &str) -> Result<()> {
    if conn.execute("DELETE FROM agent_profiles WHERE id = ?1", [id])? == 0 {
        return Err(CoreError::not_found(format!("agent profile {id} not found")));
    }
    Ok(())
}

pub fn touch_used(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("UPDATE agent_profiles SET last_used_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
    Ok(())
}

/// An agent installed on this computer and whether its default home is registered (existence checks only — nothing in
/// the home is read).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub agent: Kind,
    pub executable: Option<PathBuf>,
    pub default_home: PathBuf,
    pub default_home_exists: bool,
    pub registered_profile_id: Option<String>,
}

pub fn detect(conn: &Connection, home: &Path) -> Result<Vec<Detected>> {
    let profiles = list(conn, true)?;
    Ok(Kind::ALL
        .iter()
        .filter(|k| **k != Kind::Claude)
        .map(|k| {
            let default_home = k.default_home(home);
            Detected {
                agent: *k,
                executable: crate::toolchain::find(k.tool()),
                default_home_exists: default_home.is_dir(),
                default_home,
                registered_profile_id: profiles.iter().find(|p| p.agent == *k && p.home_dir.is_none()).map(|p| p.id.clone()),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, home_dir: Option<&str>) -> AgentProfileInput {
        AgentProfileInput { name: name.into(), home_dir: home_dir.map(str::to_string), ..AgentProfileInput::default_for(Kind::Codex) }
    }

    #[test]
    fn profiles_register_homes_without_touching_them() {
        let t = tempfile::tempdir().unwrap();
        let home = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let default = register(&conn, &input("Codex", None), &home).unwrap();
        assert_eq!((default.agent, default.home_dir.clone(), default.turn_notifications), (Kind::Codex, None, true));
        assert_eq!(default.effective_home(&home), home.join(".codex"));
        // The default home given by path is still the default; twice is refused.
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        let again = register(&conn, &input("Again", Some(home.join(".codex").to_str().unwrap())), &home).unwrap_err();
        assert!(matches!(again, CoreError::Conflict(_)), "{again}");
        let work = home.join("codex-work");
        std::fs::create_dir_all(&work).unwrap();
        let w = register(&conn, &input("Work", Some(work.to_str().unwrap())), &home).unwrap();
        assert_eq!(w.home_dir.as_deref(), Some(work.as_path()));
        assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0, "nothing written into the home");
        assert!(register(&conn, &input("Missing", Some(home.join("nope").to_str().unwrap())), &home).is_err(), "an existing folder only");
        assert!(register(&conn, &input("Home", Some(home.to_str().unwrap())), &home).is_err());
        assert!(matches!(register(&conn, &input("work", None), &home).unwrap_err(), CoreError::Conflict(_)), "names are unique per agent");
        let mut claude = input("C", None);
        claude.agent = Kind::Claude;
        assert!(register(&conn, &claude, &home).is_err(), "Claude has its own profiles");
        assert_eq!(find(&conn, Kind::Codex, "WORK").unwrap().id, w.id);

        let mut edit = input("Work 2", Some(work.to_str().unwrap()));
        edit.extra_args = vec!["--search".into()];
        edit.turn_notifications = false;
        let w2 = update(&conn, &w.id, &edit, &home).unwrap();
        assert_eq!((w2.name.as_str(), w2.extra_args.clone(), w2.turn_notifications), ("Work 2", vec!["--search".to_string()], false));
        edit.extra_args = vec!["--dangerously-bypass-approvals-and-sandbox".into()];
        assert!(update(&conn, &w.id, &edit, &home).is_err());

        let d = detect(&conn, &home).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].agent, d[0].default_home_exists, d[0].registered_profile_id.as_deref()), (Kind::Codex, true, Some(default.id.as_str())));
        set_archived(&conn, &w.id, true).unwrap();
        assert_eq!(list(&conn, false).unwrap().len(), 1);
        forget(&conn, &w.id).unwrap();
        assert!(work.is_dir(), "forgetting never deletes the home");
        // A profile of an agent a newer NEXUS knows is never read as Codex.
        conn.execute("INSERT INTO agent_profiles(id, agent, name, created_at, updated_at) VALUES ('future', 'gemini', 'G', 0, 0)", []).unwrap();
        assert!(list(&conn, true).unwrap().iter().all(|p| p.id != "future"));
        assert!(matches!(get(&conn, "future"), Err(CoreError::NotFound(_))));
    }
}

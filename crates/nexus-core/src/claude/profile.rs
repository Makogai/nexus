//! Claude profiles: registered Claude Code environments (ADR-0003).
//!
//! A profile either uses the **default location** (`config_dir = None` → `~/.claude` + `~/.claude.json`,
//! launched with `CLAUDE_CONFIG_DIR` removed) or an explicit config directory. NEXUS never writes into a
//! profile directory it did not create.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::claude::EFFORT_LEVELS;
use crate::error::{CoreError, Result};
use crate::paths::{self, Expect};
use crate::{new_id, now_ms, workprofile};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub color: String,
    /// `None` = default location.
    pub config_dir: Option<PathBuf>,
    pub executable: Option<PathBuf>,
    pub extra_args: Vec<String>,
    pub managed_by_nexus: bool,
    pub inherit_provider_env: bool,
    pub default_work_profile: Option<String>,
    pub default_effort: Option<String>,
    pub default_model: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_used_at: Option<i64>,
    pub archived_at: Option<i64>,
}

impl ClaudeProfile {
    /// The directory Claude actually uses for this profile.
    pub fn effective_config_dir(&self, home: &Path) -> PathBuf {
        self.config_dir.clone().unwrap_or_else(|| default_config_dir(home))
    }
}

pub fn default_config_dir(home: &Path) -> PathBuf {
    home.join(".claude")
}

/// Editable profile fields (create, register and update all use the same shape).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfileInput {
    pub name: String,
    pub description: String,
    pub color: Option<String>,
    /// Absolute path; empty/None = default location.
    pub config_dir: Option<String>,
    pub executable: Option<String>,
    pub extra_args: Vec<String>,
    pub inherit_provider_env: bool,
    pub default_work_profile: Option<String>,
    pub default_effort: Option<String>,
    pub default_model: Option<String>,
}

/// Flags NEXUS manages itself, or that would bypass Claude's permission system.
const RESERVED_ARGS: &[&str] = &[
    "--session-id",
    "--resume",
    "-r",
    "--continue",
    "-c",
    "--fork-session",
    "--print",
    "-p",
    "--bg",
    "--background",
    "--output-format",
    "--input-format",
    "--dangerously-skip-permissions",
    "--allow-dangerously-skip-permissions",
];

fn validate_fields(input: &ProfileInput) -> Result<ValidFields> {
    let name = input.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        return Err(CoreError::invalid("profile name must be 1–40 characters without control characters"));
    }
    let description = input.description.trim().to_string();
    if description.chars().count() > 300 {
        return Err(CoreError::invalid("description must be at most 300 characters"));
    }
    let color = match input.color.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        None => "#7c6cf2".to_string(),
        Some(c) if c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit()) => c.to_lowercase(),
        Some(_) => return Err(CoreError::invalid("color must look like #7c6cf2")),
    };
    let executable = match input.executable.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
        None => None,
        Some(raw) => {
            let p = paths::validate_user_path(raw, Expect::ExistingFile)?;
            let (ok, issue) = crate::claude::install::launchability(&p);
            if !ok {
                return Err(CoreError::invalid(issue.unwrap_or_else(|| "executable cannot be launched".into())));
            }
            Some(p)
        }
    };
    let mut prev_permission_mode = false;
    for a in &input.extra_args {
        if a.is_empty() || a.len() > 512 || a.contains('\0') {
            return Err(CoreError::invalid("launcher arguments must be 1–512 characters without NUL"));
        }
        let flag = a.split('=').next().unwrap_or(a);
        if RESERVED_ARGS.contains(&flag) {
            return Err(CoreError::invalid(format!("'{flag}' is managed by NEXUS or disables permission checks")));
        }
        if (prev_permission_mode && a == "bypassPermissions") || a == "--permission-mode=bypassPermissions" {
            return Err(CoreError::invalid("NEXUS does not launch sessions with permission checks bypassed"));
        }
        prev_permission_mode = a == "--permission-mode";
    }
    let default_work_profile = opt(&input.default_work_profile);
    if let Some(w) = &default_work_profile
        && workprofile::get(w).is_none()
    {
        return Err(CoreError::invalid(format!("unknown work profile '{w}'")));
    }
    let default_effort = opt(&input.default_effort);
    if let Some(e) = &default_effort {
        validate_effort(e)?;
    }
    let default_model = opt(&input.default_model);
    if let Some(m) = &default_model {
        validate_model(m)?;
    }
    Ok(ValidFields {
        name,
        description,
        color,
        executable,
        extra_args: input.extra_args.clone(),
        inherit_provider_env: input.inherit_provider_env,
        default_work_profile,
        default_effort,
        default_model,
    })
}

struct ValidFields {
    name: String,
    description: String,
    color: String,
    executable: Option<PathBuf>,
    extra_args: Vec<String>,
    inherit_provider_env: bool,
    default_work_profile: Option<String>,
    default_effort: Option<String>,
    default_model: Option<String>,
}

fn opt(v: &Option<String>) -> Option<String> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

pub fn validate_effort(e: &str) -> Result<()> {
    if EFFORT_LEVELS.contains(&e) {
        Ok(())
    } else {
        Err(CoreError::invalid(format!("effort must be one of {}", EFFORT_LEVELS.join(", "))))
    }
}

pub fn validate_model(m: &str) -> Result<()> {
    let ok = !m.is_empty()
        && m.len() <= 100
        && m.chars().all(|c| c.is_ascii_alphanumeric() || "._-[]:/@".contains(c));
    if ok {
        Ok(())
    } else {
        Err(CoreError::invalid("model must be an alias or model id (letters, digits, . _ - [ ] : / @)"))
    }
}

/// Normalize a user-supplied config dir: `None`/empty or `~/.claude` → default location.
fn normalize_config_dir(raw: Option<&str>, home: &Path, expect: Expect) -> Result<Option<PathBuf>> {
    let Some(raw) = raw.map(str::trim).filter(|r| !r.is_empty()) else {
        return Ok(None);
    };
    let p = paths::validate_user_path(raw, expect)?;
    let default = dunce::canonicalize(default_config_dir(home)).unwrap_or_else(|_| default_config_dir(home));
    if paths::same_path(&p, &default) {
        return Ok(None);
    }
    if paths::same_path(&p, home) {
        return Err(CoreError::invalid("the home directory itself cannot be a Claude config directory"));
    }
    Ok(Some(p))
}

fn ensure_dir_unique(conn: &Connection, dir: &Option<PathBuf>, except: Option<&str>) -> Result<()> {
    for p in list(conn, false)? {
        if Some(p.id.as_str()) == except {
            continue;
        }
        let clash = match (&p.config_dir, dir) {
            (None, None) => true,
            (Some(a), Some(b)) => paths::same_path(a, b),
            _ => false,
        };
        if clash {
            return Err(CoreError::Conflict(format!("profile '{}' already uses this configuration directory", p.name)));
        }
    }
    Ok(())
}

/// Register an existing Claude configuration (default location or an existing directory). Read-only:
/// nothing is written into the directory.
pub fn register(conn: &Connection, input: &ProfileInput, home: &Path) -> Result<ClaudeProfile> {
    let f = validate_fields(input)?;
    let dir = normalize_config_dir(input.config_dir.as_deref(), home, Expect::ExistingDir)?;
    ensure_dir_unique(conn, &dir, None)?;
    insert(conn, f, dir, false)
}

/// Create a new, empty configuration directory managed by NEXUS. Claude initialises it on first login.
pub fn create_managed(conn: &Connection, input: &ProfileInput, home: &Path) -> Result<ClaudeProfile> {
    let f = validate_fields(input)?;
    let raw = input
        .config_dir
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .ok_or_else(|| CoreError::invalid("choose a folder for the new profile"))?;
    let dir = normalize_config_dir(Some(raw), home, Expect::DirOrCreatable)?
        .ok_or_else(|| CoreError::invalid("the default Claude directory cannot be created as a new profile"))?;
    if dir.exists() && std::fs::read_dir(&dir).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(CoreError::Conflict(format!(
            "{} is not empty — register it as an existing profile instead",
            dir.display()
        )));
    }
    ensure_dir_unique(conn, &Some(dir.clone()), None)?;
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::io("cannot create", &dir, e))?;
    let dir = dunce::canonicalize(&dir).unwrap_or(dir);
    insert(conn, f, Some(dir), true)
}

fn insert(conn: &Connection, f: ValidFields, dir: Option<PathBuf>, managed: bool) -> Result<ClaudeProfile> {
    let id = new_id();
    let now = now_ms();
    let res = conn.execute(
        "INSERT INTO claude_profiles(id, name, description, color, config_dir, executable, extra_args_json,
            managed_by_nexus, inherit_provider_env, default_work_profile, default_effort, default_model,
            created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
        params![
            id,
            f.name,
            f.description,
            f.color,
            dir.as_ref().map(|d| d.to_string_lossy()),
            f.executable.as_ref().map(|e| e.to_string_lossy()),
            serde_json::to_string(&f.extra_args).unwrap_or_else(|_| "[]".into()),
            managed,
            f.inherit_provider_env,
            f.default_work_profile,
            f.default_effort,
            f.default_model,
            now
        ],
    );
    map_unique(res, &f.name)?;
    get(conn, &id)
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

/// Update editable fields. The configuration directory of a profile cannot be changed (register a new one).
pub fn update(conn: &Connection, id: &str, input: &ProfileInput) -> Result<ClaudeProfile> {
    let _existing = get(conn, id)?;
    let f = validate_fields(input)?;
    let res = conn.execute(
        "UPDATE claude_profiles SET name = ?2, description = ?3, color = ?4, executable = ?5, extra_args_json = ?6,
            inherit_provider_env = ?7, default_work_profile = ?8, default_effort = ?9, default_model = ?10,
            updated_at = ?11 WHERE id = ?1",
        params![
            id,
            f.name,
            f.description,
            f.color,
            f.executable.as_ref().map(|e| e.to_string_lossy()),
            serde_json::to_string(&f.extra_args).unwrap_or_else(|_| "[]".into()),
            f.inherit_provider_env,
            f.default_work_profile,
            f.default_effort,
            f.default_model,
            now_ms()
        ],
    );
    map_unique(res, &f.name)?;
    get(conn, id)
}

const SELECT: &str = "SELECT id, name, description, color, config_dir, executable, extra_args_json, managed_by_nexus,
    inherit_provider_env, default_work_profile, default_effort, default_model, created_at, updated_at, last_used_at,
    archived_at FROM claude_profiles";

fn from_row(r: &Row<'_>) -> rusqlite::Result<ClaudeProfile> {
    let extra: String = r.get(6)?;
    Ok(ClaudeProfile {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        color: r.get(3)?,
        config_dir: r.get::<_, Option<String>>(4)?.map(PathBuf::from),
        executable: r.get::<_, Option<String>>(5)?.map(PathBuf::from),
        extra_args: serde_json::from_str(&extra).unwrap_or_default(),
        managed_by_nexus: r.get(7)?,
        inherit_provider_env: r.get(8)?,
        default_work_profile: r.get(9)?,
        default_effort: r.get(10)?,
        default_model: r.get(11)?,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
        last_used_at: r.get(14)?,
        archived_at: r.get(15)?,
    })
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<ClaudeProfile>> {
    let filter = if include_archived { "" } else { " WHERE archived_at IS NULL" };
    let mut stmt = conn.prepare(&format!(
        "{SELECT}{filter} ORDER BY COALESCE(last_used_at, 0) DESC, name COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([], from_row)?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> Result<ClaudeProfile> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| CoreError::not_found(format!("Claude profile {id} not found")))
}

pub fn set_archived(conn: &Connection, id: &str, archived: bool) -> Result<ClaudeProfile> {
    let p = get(conn, id)?;
    if !archived {
        ensure_dir_unique(conn, &p.config_dir, Some(id))?;
    }
    conn.execute(
        "UPDATE claude_profiles SET archived_at = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, archived.then(now_ms), now_ms()],
    )?;
    get(conn, id)
}

/// Remove the registration. Never deletes the configuration directory (credentials and history live there).
pub fn forget(conn: &Connection, id: &str) -> Result<()> {
    if conn.execute("DELETE FROM claude_profiles WHERE id = ?1", [id])? == 0 {
        return Err(CoreError::not_found(format!("Claude profile {id} not found")));
    }
    Ok(())
}

pub fn touch_used(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("UPDATE claude_profiles SET last_used_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
    Ok(())
}

/// A Claude configuration found on disk.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DetectedProfile {
    /// `None` = default location.
    pub config_dir: Option<PathBuf>,
    /// Directory shown to the user.
    pub path: PathBuf,
    pub suggested_name: String,
    /// Which marker files exist (existence only — contents are never read here).
    pub markers: Vec<String>,
    pub source: String,
    pub registered_profile_id: Option<String>,
}

fn markers(dir: &Path, global_state: &Path) -> Vec<String> {
    let mut m = Vec::new();
    for (rel, label) in [
        ("settings.json", "settings"),
        (".credentials.json", "credentials"),
        ("projects", "transcripts"),
        ("plugins", "plugins"),
        ("skills", "skills"),
        ("remote-settings.json", "server-managed settings"),
    ] {
        if dir.join(rel).exists() {
            m.push(label.to_string());
        }
    }
    if global_state.is_file() {
        m.push("global state".to_string());
    }
    m
}

fn title_case(s: &str) -> String {
    s.split(['-', '_', '.'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Find Claude configurations: the default location, `~/.claude-*` directories and the
/// `CLAUDE_CONFIG_DIR` NEXUS itself was started with. Read-only.
pub fn detect(conn: &Connection, home: &Path, env_config_dir: Option<&Path>) -> Result<Vec<DetectedProfile>> {
    let registered = list(conn, true)?;
    let registered_id = |dir: &Option<PathBuf>| {
        registered
            .iter()
            .find(|p| match (&p.config_dir, dir) {
                (None, None) => true,
                (Some(a), Some(b)) => paths::same_path(a, b),
                _ => false,
            })
            .map(|p| p.id.clone())
    };
    let mut out: Vec<DetectedProfile> = Vec::new();

    let default_dir = default_config_dir(home);
    let m = markers(&default_dir, &home.join(".claude.json"));
    if default_dir.is_dir() || !m.is_empty() {
        out.push(DetectedProfile {
            registered_profile_id: registered_id(&None),
            config_dir: None,
            path: default_dir.clone(),
            suggested_name: "Default".into(),
            markers: m,
            source: "default location".into(),
        });
    }

    let push_dir = |dir: PathBuf, source: &str, out: &mut Vec<DetectedProfile>| {
        let dir = dunce::canonicalize(&dir).unwrap_or(dir);
        if paths::same_path(&dir, &dunce::canonicalize(&default_dir).unwrap_or(default_dir.clone())) {
            return;
        }
        if out.iter().any(|d| d.config_dir.as_deref().is_some_and(|x| paths::same_path(x, &dir))) {
            return;
        }
        let m = markers(&dir, &dir.join(".claude.json"));
        if m.is_empty() {
            return;
        }
        let base = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let suggested = title_case(base.trim_start_matches(".claude-").trim_start_matches('.'));
        let config_dir = Some(dir.clone());
        out.push(DetectedProfile {
            registered_profile_id: registered_id(&config_dir),
            config_dir,
            path: dir,
            suggested_name: if suggested.is_empty() { "Claude".into() } else { suggested },
            markers: m,
            source: source.into(),
        });
    };

    if let Ok(rd) = std::fs::read_dir(home) {
        let mut dirs: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".claude-"))
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .collect();
        dirs.sort();
        for d in dirs {
            push_dir(d, "home directory", &mut out);
        }
    }
    if let Some(env_dir) = env_config_dir.filter(|d| d.is_dir()) {
        push_dir(env_dir.to_path_buf(), "CLAUDE_CONFIG_DIR", &mut out);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_home() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let h = t.path();
        std::fs::create_dir_all(h.join(".claude").join("projects")).unwrap();
        std::fs::write(h.join(".claude.json"), "{}").unwrap();
        std::fs::create_dir_all(h.join(".claude-personal")).unwrap();
        std::fs::write(h.join(".claude-personal").join("settings.json"), "{}").unwrap();
        std::fs::write(h.join(".claude-personal").join(".credentials.json"), "do-not-read").unwrap();
        std::fs::create_dir_all(h.join(".claude-empty")).unwrap(); // no markers → ignored
        std::fs::create_dir_all(h.join(".claude-work-kit")).unwrap();
        std::fs::write(h.join(".claude-work-kit").join(".claude.json"), "{}").unwrap();
        t
    }

    fn input(name: &str, dir: Option<&Path>) -> ProfileInput {
        ProfileInput {
            name: name.into(),
            config_dir: dir.map(|d| d.to_string_lossy().into_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn detects_default_and_custom_profiles() {
        let home = fake_home();
        let conn = crate::db::open_in_memory().unwrap();
        let found = detect(&conn, home.path(), None).unwrap();
        let names: Vec<_> = found.iter().map(|d| d.suggested_name.as_str()).collect();
        assert_eq!(names, ["Default", "Personal", "Work Kit"]);
        assert_eq!(found[0].config_dir, None);
        assert!(found[0].markers.contains(&"global state".to_string()));
        assert!(found[1].markers.contains(&"credentials".to_string()));
    }

    #[test]
    fn registering_dot_claude_means_default_location() {
        let home = fake_home();
        let conn = crate::db::open_in_memory().unwrap();
        let p = register(&conn, &input("Work", Some(&home.path().join(".claude"))), home.path()).unwrap();
        assert_eq!(p.config_dir, None, "~/.claude must be stored as the default location");
        // A second registration of the default location conflicts.
        let err = register(&conn, &input("Again", None), home.path()).unwrap_err();
        assert_eq!(err.code(), "conflict");
        let found = detect(&conn, home.path(), None).unwrap();
        assert_eq!(found[0].registered_profile_id.as_deref(), Some(p.id.as_str()));
    }

    #[test]
    fn register_is_read_only() {
        let home = fake_home();
        let conn = crate::db::open_in_memory().unwrap();
        let dir = home.path().join(".claude-personal");
        let before: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        register(&conn, &input("Personal", Some(&dir)), home.path()).unwrap();
        let after: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(before.len(), after.len());
    }

    #[test]
    fn create_managed_requires_empty_dir() {
        let home = fake_home();
        let conn = crate::db::open_in_memory().unwrap();
        let fresh = home.path().join(".claude-experimental");
        let p = create_managed(&conn, &input("Experimental", Some(&fresh)), home.path()).unwrap();
        assert!(p.managed_by_nexus && fresh.is_dir());
        let err = create_managed(&conn, &input("X", Some(&home.path().join(".claude-personal"))), home.path());
        assert_eq!(err.unwrap_err().code(), "conflict");
        assert!(create_managed(&conn, &input("Y", None), home.path()).is_err());
    }

    #[test]
    fn validation_rejects_bad_fields() {
        let home = fake_home();
        let conn = crate::db::open_in_memory().unwrap();
        let mut i = input("", None);
        assert!(register(&conn, &i, home.path()).is_err());
        i.name = "Ok".into();
        i.color = Some("red".into());
        assert!(register(&conn, &i, home.path()).is_err());
        i.color = None;
        for bad in [
            vec!["--dangerously-skip-permissions".to_string()],
            vec!["--resume".into(), "x".into()],
            vec!["--permission-mode".into(), "bypassPermissions".into()],
            vec!["--session-id=abc".into()],
        ] {
            i.extra_args = bad.clone();
            assert!(register(&conn, &i, home.path()).is_err(), "accepted {bad:?}");
        }
        i.extra_args = vec!["--permission-mode".into(), "acceptEdits".into()];
        i.default_effort = Some("ultra".into());
        assert!(register(&conn, &i, home.path()).is_err());
        i.default_effort = Some("xhigh".into());
        i.default_work_profile = Some("nope".into());
        assert!(register(&conn, &i, home.path()).is_err());
        i.default_work_profile = Some("prototype".into());
        assert!(register(&conn, &i, home.path()).is_ok());
    }

    #[test]
    fn update_archive_forget() {
        let home = fake_home();
        let conn = crate::db::open_in_memory().unwrap();
        let dir = home.path().join(".claude-personal");
        let p = register(&conn, &input("Personal", Some(&dir)), home.path()).unwrap();
        let mut i = input("Personal 2", None);
        i.default_model = Some("opus".into());
        let u = update(&conn, &p.id, &i).unwrap();
        assert_eq!(u.name, "Personal 2");
        assert_eq!(u.config_dir.as_deref().map(|d| d.ends_with(".claude-personal")), Some(true));
        set_archived(&conn, &p.id, true).unwrap();
        assert!(list(&conn, false).unwrap().is_empty());
        forget(&conn, &p.id).unwrap();
        assert!(dir.join(".credentials.json").exists(), "forget must not touch the directory");
    }
}

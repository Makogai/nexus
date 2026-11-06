//! Skill manager (ADR-0046): every skill a NEXUS session can get, in one list — NEXUS's built-in skills, the
//! workspace's own skills (`<workspace>/skills/<name>/SKILL.md`, synced, `workspace:<name>`), each project's native
//! Claude skills (`<project>/.claude/skills/<name>/SKILL.md`, in the project's repository) and the skills of AI Kits —
//! and the person's editor for the first two kinds.
//!
//! * Only the person writes, through the desktop: never Claude, never the MCP server. Names, descriptions and sizes are
//!   validated; files are written atomically, inside the expected folder, never through a symbolic link; a save names the
//!   version it replaces (optimistic, like knowledge documents).
//! * A workspace skill the person saves is approved on this machine with it — but only when the workspace skills were
//!   approved before the save (or there were none): changes that arrived by sync or from Claude still wait for review in
//!   AI Kits (ADR-0037).
//! * Project skills are the project's own files: Claude Code loads them itself; NEXUS writes only the skill folder the
//!   person edits — the change shows up in the project's Git status for the person to commit.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::workspace::Workspace;
use crate::{kits, project};

/// Claude Code's limits for skill metadata.
pub const MAX_NAME: usize = 64;
pub const MAX_DESCRIPTION: usize = 1024;
/// The largest SKILL.md NEXUS writes (and the catalog reads).
pub const MAX_FILE: usize = 256 * 1024;

/// Where a skill lives.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Builtin,
    Workspace,
    Project,
    Kit,
}

/// One skill as the Skills page lists it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntry {
    pub scope: Scope,
    /// The skill's folder name (for built-in and kit skills: the name Claude knows).
    pub name: String,
    pub description: String,
    /// How Claude names it in a session: `workspace:<name>`, `<name>` (project), `<kit>:<name>`, `nexus:<name>`.
    pub invoke_as: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub kit_id: Option<String>,
    pub kit_name: Option<String>,
    /// The SKILL.md on this computer (built-in skills: none).
    pub path: Option<String>,
    pub editable: bool,
    /// `built-in`, `approved`, `needs-review`, `changed`, `not-registered` (workspace skills NEXUS has not reviewed yet),
    /// `native` (a project skill Claude Code loads itself), or a kit's status.
    pub status: String,
    /// Which sessions get it, in words.
    pub reaches: String,
}

/// A skill's file, read for viewing or editing.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillFile {
    pub entry: SkillEntry,
    pub text: String,
    /// The content hash a save must name (empty for read-only skills).
    pub hash: String,
}

/// Which skill: built-in by id, workspace by name, project by id + name, kit by id + name.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillRef {
    pub scope: Scope,
    pub name: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub kit_id: Option<String>,
}

/// What a save did to the workspace skills' approval.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub file: SkillFile,
    /// `approved` (sessions get it now), `needs-review` (other workspace skill changes wait for review in AI Kits),
    /// `native` (a project skill — Claude Code loads it; commit it in the project).
    pub outcome: &'static str,
    pub message: String,
}

// ---- listing ----------------------------------------------------------------------------------------------------------

/// Every skill a session of this workspace can get, in a stable order: built-in, workspace, projects, AI Kits.
pub fn list(conn: &Connection, data_dir: &Path, ws: &Workspace) -> Result<Vec<SkillEntry>> {
    let mut out: Vec<SkillEntry> = crate::claude::builtin::skills()
        .into_iter()
        .map(|s| SkillEntry {
            scope: Scope::Builtin,
            name: s.id.clone(),
            description: s.description,
            invoke_as: s.id,
            project_id: None,
            project_name: None,
            kit_id: None,
            kit_name: None,
            path: None,
            editable: false,
            status: "built-in".into(),
            reaches: s.given_to.into(),
        })
        .collect();

    let status = workspace_status(conn, data_dir, ws)?;
    for (name, description, file) in folder_skills(&ws.root_path.join("skills")) {
        out.push(SkillEntry {
            scope: Scope::Workspace,
            invoke_as: format!("{}:{name}", kits::WORKSPACE_KIT),
            name,
            description,
            project_id: None,
            project_name: None,
            kit_id: None,
            kit_name: None,
            path: Some(file.display().to_string()),
            editable: true,
            status: status.into(),
            reaches: "every session of this workspace (once approved on this computer)".into(),
        });
    }

    for p in project::list(conn, &ws.id)?.into_iter().filter(|p| p.available) {
        let Some(root) = p.local_path.clone() else { continue };
        for (name, description, file) in folder_skills(&root.join(".claude").join("skills")) {
            out.push(SkillEntry {
                scope: Scope::Project,
                invoke_as: name.clone(),
                name,
                description,
                project_id: Some(p.id.clone()),
                project_name: Some(p.name.clone()),
                kit_id: None,
                kit_name: None,
                path: Some(file.display().to_string()),
                editable: true,
                status: "native".into(),
                reaches: format!("every Claude session in {} (Claude Code loads it from the repository)", p.name),
            });
        }
    }

    for k in kits::list(conn, data_dir)?.into_iter().filter(|k| k.kit.workspace_id.is_none()) {
        for s in &k.inventory.skills {
            out.push(SkillEntry {
                scope: Scope::Kit,
                name: s.name.clone(),
                description: s.description.clone(),
                invoke_as: format!("{}:{}", k.inventory.name, s.name),
                project_id: None,
                project_name: None,
                kit_id: Some(k.kit.id.clone()),
                kit_name: Some(k.kit.name.clone()),
                path: Some(s.src.join("SKILL.md").display().to_string()),
                editable: false,
                status: k.status.into(),
                reaches: if k.profiles.is_empty() { "no profile yet — choose profiles in AI Kits".into() } else { format!("the profiles it is enabled for ({})", k.profiles.len()) },
            });
        }
    }
    Ok(out)
}

/// `(folder name, description, SKILL.md path)` of each skill folder in `dir`, sorted — no symbolic links followed.
fn folder_skills(dir: &Path) -> Vec<(String, String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect();
    dirs.sort();
    dirs.into_iter()
        .filter_map(|d| {
            let file = d.join("SKILL.md");
            let meta = std::fs::symlink_metadata(&file).ok()?;
            if !meta.is_file() || meta.len() as usize > MAX_FILE {
                return None;
            }
            let text = std::fs::read_to_string(&file).ok()?;
            let name = d.file_name()?.to_string_lossy().into_owned();
            Some((name, crate::knowledge::skills::header(&text).1.unwrap_or_default(), file))
        })
        .collect()
}

/// The approval state of the workspace's skills on this machine.
fn workspace_status(conn: &Connection, data_dir: &Path, ws: &Workspace) -> Result<&'static str> {
    Ok(match kits::workspace_kit(conn, &ws.id)? {
        Some(k) => kits::view(conn, data_dir, &k)?.status,
        None => "not-registered",
    })
}

// ---- reading ----------------------------------------------------------------------------------------------------------

pub fn read(conn: &Connection, data_dir: &Path, ws: &Workspace, r: &SkillRef) -> Result<SkillFile> {
    if r.scope == Scope::Builtin {
        let s = crate::claude::builtin::skills().into_iter().find(|s| s.id == r.name).ok_or_else(|| CoreError::NotFound(format!("built-in skill {}", r.name)))?;
        let entry = list(conn, data_dir, ws)?.into_iter().find(|e| e.scope == Scope::Builtin && e.name == r.name).ok_or_else(|| CoreError::NotFound(r.name.clone()))?;
        return Ok(SkillFile { entry, text: s.body, hash: String::new() });
    }
    let entry = find(conn, data_dir, ws, r)?;
    let path = PathBuf::from(entry.path.clone().unwrap_or_default());
    let bytes = std::fs::read(&path).map_err(|e| CoreError::io("cannot read", &path, e))?;
    if bytes.len() > MAX_FILE {
        return Err(CoreError::invalid("the skill file is larger than 256 KB"));
    }
    let hash = if entry.editable { crate::knowledge::content_hash(&bytes) } else { String::new() };
    Ok(SkillFile { entry, text: String::from_utf8_lossy(&bytes).into_owned(), hash })
}

fn find(conn: &Connection, data_dir: &Path, ws: &Workspace, r: &SkillRef) -> Result<SkillEntry> {
    list(conn, data_dir, ws)?
        .into_iter()
        .find(|e| e.scope == r.scope && e.name == r.name && (r.scope != Scope::Project || e.project_id == r.project_id) && (r.scope != Scope::Kit || e.kit_id == r.kit_id))
        .ok_or_else(|| CoreError::NotFound(format!("skill {}", r.name)))
}

// ---- writing ----------------------------------------------------------------------------------------------------------

/// A new skill from a short template: the person names it, says when Claude should use it, and where it lives.
pub fn create(conn: &Connection, data_dir: &Path, ws: &Workspace, scope: Scope, project_id: Option<&str>, name: &str, description: &str) -> Result<Saved> {
    check_name(name)?;
    let description = check_description(description)?;
    let (_, dir) = skill_dir(conn, ws, scope, project_id, name)?;
    if dir.exists() {
        return Err(CoreError::Conflict(format!("a skill named “{name}” already exists there")));
    }
    let title = name.split('-').filter(|w| !w.is_empty()).map(|w| { let mut c = w.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }).collect::<Vec<_>>().join(" ");
    let text = format!(
        "---\nname: {name}\ndescription: {}\n---\n\n# {title}\n\n## When to use\n\n{description}\n\n## Steps\n\n1. \n\n## Notes\n\n- \n",
        yaml_quoted(&description)
    );
    write(conn, data_dir, ws, scope, project_id, name, &text, None)
}

/// Save the person's edit. `expected_hash` is the version the editor showed; a file changed since is not overwritten.
#[allow(clippy::too_many_arguments)]
pub fn save(conn: &Connection, data_dir: &Path, ws: &Workspace, scope: Scope, project_id: Option<&str>, name: &str, text: &str, expected_hash: &str) -> Result<Saved> {
    check_name(name)?;
    check_text(name, text)?;
    write(conn, data_dir, ws, scope, project_id, name, text, Some(expected_hash))
}

/// Delete a skill the person made (its folder holds only SKILL.md — anything else is theirs to remove).
pub fn delete(conn: &Connection, data_dir: &Path, ws: &Workspace, scope: Scope, project_id: Option<&str>, name: &str, expected_hash: &str) -> Result<&'static str> {
    check_name(name)?;
    let (root, dir) = skill_dir(conn, ws, scope, project_id, name)?;
    no_links(&root, &dir)?;
    let file = dir.join("SKILL.md");
    let current = std::fs::read(&file).map_err(|e| CoreError::io("cannot read", &file, e))?;
    if crate::knowledge::content_hash(&current) != expected_hash {
        return Err(CoreError::Conflict("the skill changed on disk since you opened it — open it again".into()));
    }
    let others = std::fs::read_dir(&dir).map_err(|e| CoreError::io("cannot read", &dir, e))?.flatten().filter(|e| e.file_name() != "SKILL.md").count();
    if others > 0 {
        return Err(CoreError::invalid(format!("the skill folder holds {others} other file(s) — remove them yourself, or delete the folder in your file manager")));
    }
    let approved_before = scope == Scope::Workspace && workspace_status(conn, data_dir, ws)? == "approved";
    if std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(CoreError::invalid("the skill file is a symbolic link — remove it yourself"));
    }
    std::fs::remove_file(&file).map_err(|e| CoreError::io("cannot delete", &file, e))?;
    std::fs::remove_dir(&dir).map_err(|e| CoreError::io("cannot delete", &dir, e))?;
    if approved_before {
        kits::register_workspace(conn, data_dir, ws, true)?;
    }
    Ok(if scope == Scope::Workspace { "workspace" } else { "project" })
}

#[allow(clippy::too_many_arguments)]
fn write(conn: &Connection, data_dir: &Path, ws: &Workspace, scope: Scope, project_id: Option<&str>, name: &str, text: &str, expected_hash: Option<&str>) -> Result<Saved> {
    let (root, dir) = skill_dir(conn, ws, scope, project_id, name)?;
    no_links(&root, &dir)?;
    let file = dir.join("SKILL.md");
    if let Some(expected) = expected_hash {
        let current = std::fs::read(&file).map_err(|e| CoreError::io("cannot read", &file, e))?;
        if crate::knowledge::content_hash(&current) != expected {
            return Err(CoreError::Conflict("the skill changed on disk since you opened it — copy your text, open it again and apply your change".into()));
        }
    }
    // Whether the person's save may carry the approval (ADR-0046): only when nothing else waits for review.
    let before = if scope == Scope::Workspace {
        crate::workspace::ensure_writable_at(&file)?;
        Some((workspace_status(conn, data_dir, ws)?, !folder_skills(&ws.root_path.join("skills")).is_empty()))
    } else {
        None
    };
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::io("cannot create", &dir, e))?;
    no_links(&root, &dir)?;
    crate::workspace::write_atomic(&file, text.as_bytes())?;

    let (outcome, message) = match before {
        Some((status, had_skills)) => {
            let approve = status == "approved" || (matches!(status, "not-registered" | "missing") && !had_skills);
            kits::register_workspace(conn, data_dir, ws, approve)?;
            if approve {
                ("approved", format!("Saved — sessions of this workspace get workspace:{name} from now on."))
            } else {
                ("needs-review", "Saved. Other workspace skill changes wait for your review, so sessions get the workspace skills once you approve them in AI Kits.".to_string())
            }
        }
        None => ("native", "Saved in the project's .claude/skills — Claude Code loads it in new sessions there. Commit it with the project.".to_string()),
    };
    let r = SkillRef { scope, name: name.to_string(), project_id: project_id.map(str::to_string), kit_id: None };
    Ok(Saved { file: read(conn, data_dir, ws, &r)?, outcome, message })
}

/// `<workspace>/skills/<name>` or `<project>/.claude/skills/<name>` — only those two places, inside their root.
fn skill_dir(conn: &Connection, ws: &Workspace, scope: Scope, project_id: Option<&str>, name: &str) -> Result<(PathBuf, PathBuf)> {
    let (root, rel) = match scope {
        Scope::Workspace => (ws.root_path.clone(), format!("skills/{name}")),
        Scope::Project => {
            let id = project_id.ok_or_else(|| CoreError::invalid("choose the project"))?;
            let p = project::get(conn, id)?;
            if p.workspace_id != ws.id {
                return Err(CoreError::invalid("the project belongs to another workspace"));
            }
            let root = p.local_path.filter(|_| p.available).ok_or_else(|| CoreError::invalid(format!("{} is not on this computer", p.name)))?;
            (root, format!(".claude/skills/{name}"))
        }
        Scope::Builtin | Scope::Kit => return Err(CoreError::invalid("built-in and AI Kit skills are not edited here")),
    };
    let dir = crate::paths::resolve_within(&root, &rel)?;
    Ok((root, dir))
}

/// No folder between the root and the skill's folder may be a symbolic link (a write must not land elsewhere).
fn no_links(root: &Path, dir: &Path) -> Result<()> {
    let mut d = dir;
    while d != root {
        if std::fs::symlink_metadata(d).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(CoreError::invalid(format!("{} is a symbolic link — NEXUS does not write through links", d.display())));
        }
        match d.parent() {
            Some(p) => d = p,
            None => break,
        }
    }
    Ok(())
}

/// A YAML double-quoted scalar: a description with `: ` or `#` stays one value.
fn yaml_quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

// ---- validation -------------------------------------------------------------------------------------------------------

/// Claude Code's rules for a skill name: lowercase letters, digits and hyphens, at most 64, not "anthropic" or
/// "claude".
pub fn check_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= MAX_NAME
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    if !ok {
        return Err(CoreError::invalid(format!("a skill name uses lowercase letters, digits and hyphens (at most {MAX_NAME}), e.g. release-notes")));
    }
    if name.contains("anthropic") || name.contains("claude") {
        return Err(CoreError::invalid("Claude Code reserves names with “anthropic” or “claude”"));
    }
    Ok(())
}

/// One line, 1–1024 characters, no angle brackets (Claude Code's limits) — what it does and when to use it.
pub fn check_description(description: &str) -> Result<String> {
    let d = description.split_whitespace().collect::<Vec<_>>().join(" ");
    if d.is_empty() || d.chars().count() > MAX_DESCRIPTION {
        return Err(CoreError::invalid(format!("the description needs 1–{MAX_DESCRIPTION} characters: what the skill does and when Claude should use it")));
    }
    if d.contains(['<', '>']) {
        return Err(CoreError::invalid("the description may not contain < or >"));
    }
    Ok(d)
}

/// A SKILL.md the person saves: bounded, with frontmatter whose `name` is the folder's and a valid `description`.
pub fn check_text(name: &str, text: &str) -> Result<()> {
    if text.len() > MAX_FILE {
        return Err(CoreError::invalid("a SKILL.md may be at most 256 KB — move long material into files next to it"));
    }
    if text.lines().next().map(str::trim) != Some("---") {
        return Err(CoreError::invalid("a SKILL.md starts with a frontmatter block (---, name, description, ---)"));
    }
    let (n, d) = crate::knowledge::skills::header(text);
    if n.as_deref() != Some(name) {
        return Err(CoreError::invalid(format!("the frontmatter's name must be “{name}” (the skill's folder)")));
    }
    check_description(d.as_deref().unwrap_or(""))?;
    Ok(())
}

#[cfg(test)]
mod tests;

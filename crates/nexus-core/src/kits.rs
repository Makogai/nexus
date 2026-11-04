//! AI Kits (M21, ADR-0024): Claude Code plugins or skill folders registered with NEXUS — from a local folder or a
//! Git repository — inspected, hash-approved by a person, and enabled **per profile** through the supported,
//! session-scoped `--plugin-dir` flag. NEXUS never installs anything into a Claude config dir, so a kit reaches only
//! the NEXUS sessions of the profiles it is enabled for.
//!
//! Context efficiency: Claude puts every loaded skill's name and description into each session. NEXUS therefore
//! builds a per-profile **overlay** plugin that contains only the selected skills (a large kit starts with none
//! selected), and leaves out hooks and MCP servers — which run code — unless they are enabled explicitly.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::claude::diagnostics::KitUse;
use crate::claude::plugin::{self, Inventory};
use crate::claude::policy::PolicySummary;
use crate::error::{CoreError, Result};
use crate::{git, new_id, now_ms, paths, proc};

/// A kit larger than this starts with no skills selected for a profile (the user chooses).
pub const AUTO_SELECT_MAX_SKILLS: usize = 10;
pub const AUTO_SELECT_MAX_TOKENS: usize = 1500;
const MAX_HASH_FILES: usize = 2000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Kit {
    pub id: String,
    pub name: String,
    pub source_kind: String,
    pub location: String,
    pub git_ref: Option<String>,
    pub subdir: String,
    pub kind: String,
    pub version: Option<String>,
    pub content_hash: String,
    pub approved_hash: Option<String>,
    pub commit_sha: Option<String>,
    pub registered_at: i64,
    pub updated_at: i64,
}

impl Kit {
    pub fn approved(&self) -> bool {
        self.approved_hash.as_deref() == Some(self.content_hash.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    /// Selected skill names; `None` = all skills of the kit.
    pub skills: Option<Vec<String>>,
    pub agents: bool,
    pub commands: bool,
    pub hooks: bool,
    pub mcp: bool,
}

impl Default for Selection {
    fn default() -> Self {
        Selection { skills: None, agents: true, commands: true, hooks: false, mcp: false }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KitProfile {
    pub kit_id: String,
    pub profile_id: String,
    #[serde(flatten)]
    pub selection: Selection,
    pub enabled_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KitView {
    #[serde(flatten)]
    pub kit: Kit,
    /// `approved`, `needs-review` (never approved), `changed` (content differs from the approved version), `missing`.
    pub status: &'static str,
    pub content_dir: PathBuf,
    pub inventory: Inventory,
    pub profiles: Vec<KitProfile>,
}

fn kits_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("kits")
}

/// Where the kit's files are: the local folder, or NEXUS's own checkout of the Git repository.
pub fn content_dir(data_dir: &Path, k: &Kit) -> PathBuf {
    let base = match k.source_kind.as_str() {
        "git" => kits_dir(data_dir).join(&k.id).join("repo"),
        _ => PathBuf::from(&k.location),
    };
    if k.subdir.is_empty() { base } else { base.join(&k.subdir) }
}

/// Hash of every regular file (relative path + bytes), symlinks and `.git` excluded — any change needs review again.
pub fn hash_dir(dir: &Path) -> String {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() && e.file_name() != ".git" {
                stack.push(e.path());
            } else if ft.is_file() && files.len() < MAX_HASH_FILES {
                files.push(e.path());
            }
        }
    }
    files.sort();
    let mut buf = Vec::new();
    for f in files {
        buf.extend(f.strip_prefix(dir).unwrap_or(&f).to_string_lossy().replace('\\', "/").as_bytes());
        buf.push(0);
        buf.extend(std::fs::read(&f).unwrap_or_default());
        buf.push(0);
    }
    crate::knowledge::content_hash(&buf)
}

const SELECT: &str = "SELECT id, name, source_kind, location, git_ref, subdir, kind, version, content_hash, approved_hash, commit_sha,
    registered_at, updated_at FROM kits";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Kit> {
    Ok(Kit {
        id: r.get(0)?,
        name: r.get(1)?,
        source_kind: r.get(2)?,
        location: r.get(3)?,
        git_ref: r.get(4)?,
        subdir: r.get(5)?,
        kind: r.get(6)?,
        version: r.get(7)?,
        content_hash: r.get(8)?,
        approved_hash: r.get(9)?,
        commit_sha: r.get(10)?,
        registered_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<Kit> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row).optional()?.ok_or_else(|| CoreError::not_found(format!("no kit {id}")))
}

fn all(conn: &Connection) -> Result<Vec<Kit>> {
    Ok(conn.prepare(&format!("{SELECT} ORDER BY name COLLATE NOCASE"))?.query_map([], from_row)?.collect::<std::result::Result<_, _>>()?)
}

/// The plugin folder inside a source: the source itself, `subdir`, or the named plugin of a marketplace.
fn resolve_subdir(root: &Path, subdir: Option<&str>, plugin_name: Option<&str>) -> Result<String> {
    if let Some(s) = subdir.map(str::trim).filter(|s| !s.is_empty()) {
        let s = s.trim_start_matches("./").replace('\\', "/");
        plugin::confined(root, &s).ok_or_else(|| CoreError::invalid("the subfolder must stay inside the source"))?;
        return Ok(s);
    }
    let market = root.join(".claude-plugin").join("marketplace.json");
    if !root.join(".claude-plugin").join("plugin.json").is_file() && market.is_file() {
        let m: Value = std::fs::read_to_string(&market).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        let plugins: Vec<(String, Option<String>)> = m
            .get("plugins")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| Some((p.get("name")?.as_str()?.to_string(), p.get("source").and_then(Value::as_str).map(str::to_string))))
            .collect();
        let listing = plugins.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
        let Some(want) = plugin_name else {
            return Err(CoreError::invalid(format!("this is a plugin marketplace — choose one of its plugins: {listing}")));
        };
        let (_, src) = plugins.iter().find(|(n, _)| n == want).ok_or_else(|| CoreError::invalid(format!("the marketplace has no plugin “{want}” ({listing})")))?;
        let src = src.as_deref().filter(|s| s.starts_with("./") || !s.contains(':')).ok_or_else(|| CoreError::invalid("only plugins stored inside the marketplace folder are supported"))?;
        return resolve_subdir(root, Some(src), None);
    }
    Ok(String::new())
}

fn insert(conn: &Connection, data_dir: &Path, mut k: Kit) -> Result<Kit> {
    let dir = content_dir(data_dir, &k);
    let inv = plugin::inventory(&dir);
    if inv.kind == "skills" && inv.skills.is_empty() {
        return Err(CoreError::invalid(inv.problems.first().cloned().unwrap_or_else(|| "nothing to use in this folder".into())));
    }
    if !plugin::valid_plugin_name(&inv.name) {
        return Err(CoreError::invalid(format!("“{}” is not a usable plugin name (letters, digits, '-' or '_')", inv.name)));
    }
    k.name = inv.name.clone();
    k.kind = inv.kind.clone();
    k.version = inv.version.clone();
    k.content_hash = hash_dir(&dir);
    conn.execute(
        "INSERT INTO kits(id, name, source_kind, location, git_ref, subdir, kind, version, content_hash, approved_hash, commit_sha, registered_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, ?11)",
        params![k.id, k.name, k.source_kind, k.location, k.git_ref, k.subdir, k.kind, k.version, k.content_hash, k.commit_sha, k.registered_at],
    )?;
    get(conn, &k.id)
}

fn blank(source_kind: &str, location: String) -> Kit {
    Kit {
        id: new_id(),
        name: String::new(),
        source_kind: source_kind.into(),
        location,
        git_ref: None,
        subdir: String::new(),
        kind: String::new(),
        version: None,
        content_hash: String::new(),
        approved_hash: None,
        commit_sha: None,
        registered_at: now_ms(),
        updated_at: now_ms(),
    }
}

/// Register a kit that stays in a local folder (NEXUS reads it in place and never writes to it).
pub fn register_local(conn: &Connection, data_dir: &Path, folder: &str, subdir: Option<&str>, plugin_name: Option<&str>) -> Result<Kit> {
    let root = paths::validate_user_path(folder, paths::Expect::ExistingDir)?;
    let mut k = blank("local", root.to_string_lossy().into_owned());
    k.subdir = resolve_subdir(&root, subdir, plugin_name)?;
    insert(conn, data_dir, k)
}

/// Register a kit from a Git repository: NEXUS clones it (shallow) into its own data folder.
pub fn register_git(conn: &Connection, data_dir: &Path, url: &str, git_ref: Option<&str>, subdir: Option<&str>, plugin_name: Option<&str>) -> Result<Kit> {
    let mut k = blank("git", git::redact_url(url.trim()));
    k.git_ref = git_ref.map(str::to_string);
    let checkout = kits_dir(data_dir).join(&k.id).join("repo");
    std::fs::create_dir_all(checkout.parent().unwrap_or(data_dir)).map_err(|e| CoreError::io("cannot create", &checkout, e))?;
    git::clone_shallow(url.trim(), &checkout, git_ref)?;
    let result = (|| {
        k.subdir = resolve_subdir(&checkout, subdir, plugin_name)?;
        k.commit_sha = git::head(&checkout);
        insert(conn, data_dir, k.clone())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(kits_dir(data_dir).join(&k.id));
    }
    result
}

/// Re-read the kit's files (local kits) or fetch the followed ref (Git kits). A changed kit needs a new review.
pub fn update(conn: &Connection, data_dir: &Path, id: &str) -> Result<KitView> {
    let k = get(conn, id)?;
    let mut commit = k.commit_sha.clone();
    if k.source_kind == "git" {
        commit = Some(git::fetch_reset(&kits_dir(data_dir).join(&k.id).join("repo"), k.git_ref.as_deref())?);
    }
    let dir = content_dir(data_dir, &k);
    let inv = plugin::inventory(&dir);
    conn.execute(
        "UPDATE kits SET content_hash = ?2, version = ?3, commit_sha = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, hash_dir(&dir), inv.version, commit, now_ms()],
    )?;
    view(conn, data_dir, &get(conn, id)?)
}

/// A person approves the kit's current content (after reviewing its inventory).
pub fn approve(conn: &Connection, data_dir: &Path, id: &str) -> Result<KitView> {
    let k = get(conn, id)?;
    let hash = hash_dir(&content_dir(data_dir, &k));
    conn.execute("UPDATE kits SET content_hash = ?2, approved_hash = ?2, updated_at = ?3 WHERE id = ?1", params![id, hash, now_ms()])?;
    view(conn, data_dir, &get(conn, id)?)
}

pub fn remove(conn: &Connection, data_dir: &Path, id: &str) -> Result<()> {
    let k = get(conn, id)?;
    conn.execute("DELETE FROM kits WHERE id = ?1", [id])?;
    // Only NEXUS's own folder (checkout, overlays) is deleted — never a user's local kit folder.
    let own = kits_dir(data_dir).join(&k.id);
    if own.starts_with(kits_dir(data_dir)) && own.is_dir() {
        let _ = std::fs::remove_dir_all(own);
    }
    Ok(())
}

fn profile_rows(conn: &Connection, where_: &str, arg: &str) -> Result<Vec<KitProfile>> {
    let mut stmt = conn.prepare(&format!("SELECT kit_id, profile_id, skills_json, agents, commands, hooks, mcp, enabled_at FROM kit_profiles WHERE {where_} = ?1 ORDER BY enabled_at"))?;
    let rows = stmt
        .query_map([arg], |r| {
            let skills: Option<String> = r.get(2)?;
            Ok(KitProfile {
                kit_id: r.get(0)?,
                profile_id: r.get(1)?,
                selection: Selection {
                    skills: skills.and_then(|s| serde_json::from_str(&s).ok()),
                    agents: r.get(3)?,
                    commands: r.get(4)?,
                    hooks: r.get(5)?,
                    mcp: r.get(6)?,
                },
                enabled_at: r.get(7)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn view(conn: &Connection, data_dir: &Path, k: &Kit) -> Result<KitView> {
    let dir = content_dir(data_dir, k);
    let status = if !dir.is_dir() {
        "missing"
    } else if k.approved_hash.is_none() {
        "needs-review"
    } else if hash_dir(&dir) != k.approved_hash.clone().unwrap_or_default() {
        "changed"
    } else {
        "approved"
    };
    Ok(KitView { status, inventory: plugin::inventory(&dir), profiles: profile_rows(conn, "kit_id", &k.id)?, content_dir: dir, kit: k.clone() })
}

pub fn list(conn: &Connection, data_dir: &Path) -> Result<Vec<KitView>> {
    all(conn)?.iter().map(|k| view(conn, data_dir, k)).collect()
}

/// The default selection when a kit is enabled for a profile: all skills of a small kit, none of a large one.
pub fn default_selection(inv: &Inventory) -> Selection {
    let small = inv.skills.len() <= AUTO_SELECT_MAX_SKILLS && inv.listing_tokens() <= AUTO_SELECT_MAX_TOKENS;
    Selection { skills: if small { None } else { Some(Vec::new()) }, ..Default::default() }
}

/// Enable (or update the selection of) a kit for a profile. Names must not collide with another kit of the profile.
pub fn enable(conn: &Connection, data_dir: &Path, kit_id: &str, profile_id: &str, selection: Option<Selection>) -> Result<KitProfile> {
    let k = get(conn, kit_id)?;
    crate::claude::profile::get(conn, profile_id)?;
    let inv = plugin::inventory(&content_dir(data_dir, &k));
    for other in profile_rows(conn, "profile_id", profile_id)?.iter().filter(|r| r.kit_id != kit_id) {
        if get(conn, &other.kit_id).map(|o| o.name == k.name).unwrap_or(false) {
            return Err(CoreError::Conflict(format!("another kit named “{}” is already enabled for this profile — Claude would merge both into one namespace", k.name)));
        }
    }
    let sel = match selection {
        Some(s) => s,
        None => profile_rows(conn, "profile_id", profile_id)?.into_iter().find(|r| r.kit_id == kit_id).map(|r| r.selection).unwrap_or_else(|| default_selection(&inv)),
    };
    if let Some(names) = &sel.skills
        && let Some(bad) = names.iter().find(|n| !inv.skills.iter().any(|s| &s.name == *n))
    {
        return Err(CoreError::invalid(format!("the kit has no skill “{bad}”")));
    }
    conn.execute(
        "INSERT INTO kit_profiles(kit_id, profile_id, skills_json, agents, commands, hooks, mcp, enabled_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(kit_id, profile_id) DO UPDATE SET skills_json = excluded.skills_json, agents = excluded.agents, commands = excluded.commands,
            hooks = excluded.hooks, mcp = excluded.mcp",
        params![kit_id, profile_id, sel.skills.as_ref().map(|s| serde_json::to_string(s).unwrap_or_default()), sel.agents, sel.commands, sel.hooks, sel.mcp, now_ms()],
    )?;
    Ok(profile_rows(conn, "profile_id", profile_id)?.into_iter().find(|r| r.kit_id == kit_id).expect("just written"))
}

pub fn disable(conn: &Connection, kit_id: &str, profile_id: &str) -> Result<()> {
    conn.execute("DELETE FROM kit_profiles WHERE kit_id = ?1 AND profile_id = ?2", params![kit_id, profile_id])?;
    Ok(())
}

/// The part of a kit a profile's sessions get.
pub fn effective(inv: &Inventory, sel: &Selection) -> Inventory {
    let mut e = inv.clone();
    if let Some(names) = &sel.skills {
        e.skills.retain(|s| names.contains(&s.name));
    }
    if !sel.agents {
        e.agents.clear();
    }
    if !sel.commands {
        e.commands.clear();
    }
    if !sel.hooks {
        e.hooks.clear();
    }
    if !sel.mcp {
        e.mcp_servers.clear();
    }
    e
}

/// Why kits cannot be added to a profile's sessions at all (organisation policy).
pub fn blocked_by(policy: &PolicySummary) -> Option<&'static str> {
    if policy.disable_sideload_flags {
        Some("organisation policy disables --plugin-dir (disableSideloadFlags)")
    } else if policy.strict_plugin_only_customization {
        Some("organisation policy allows only approved plugins (strictPluginOnlyCustomization)")
    } else {
        None
    }
}

/// Kits enabled for a profile, as diagnostics shows them (no files are written).
pub fn uses_for_profile(conn: &Connection, data_dir: &Path, profile_id: &str, policy: &PolicySummary) -> Result<Vec<KitUse>> {
    let mut out = Vec::new();
    for row in profile_rows(conn, "profile_id", profile_id)? {
        let v = view(conn, data_dir, &get(conn, &row.kit_id)?)?;
        let blocked = blocked_by(policy).map(str::to_string).or_else(|| match v.status {
            "approved" => None,
            "changed" => Some("the kit changed since it was approved — review it again".into()),
            "missing" => Some("the kit's files are missing".into()),
            _ => Some("the kit has not been reviewed and approved yet".into()),
        });
        let mut inv = effective(&v.inventory, &row.selection);
        if policy.allow_managed_mcp_servers_only {
            inv.mcp_servers.clear();
        }
        if policy.allow_managed_hooks_only || policy.disable_all_hooks {
            inv.hooks.clear();
        }
        out.push(KitUse { kit_id: row.kit_id, inventory: inv, blocked });
    }
    Ok(out)
}

/// What a session was given (recorded with its integrations).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KitLaunch {
    pub kit_id: String,
    pub name: String,
    pub skills: Vec<String>,
    pub listing_tokens: usize,
    pub hooks: bool,
    pub mcp: bool,
    pub blocked: Option<String>,
}

/// Plugin folders for a profile's session: one overlay per enabled, approved kit with exactly the selected parts.
pub fn session_plugin_dirs(conn: &Connection, data_dir: &Path, profile_id: &str, policy: &PolicySummary) -> Result<(Vec<PathBuf>, Vec<KitLaunch>)> {
    let mut dirs = Vec::new();
    let mut launches = Vec::new();
    for u in uses_for_profile(conn, data_dir, profile_id, policy)? {
        let k = get(conn, &u.kit_id)?;
        let mut l = KitLaunch {
            kit_id: k.id.clone(),
            name: k.name.clone(),
            skills: u.inventory.skills.iter().map(|s| s.name.clone()).collect(),
            listing_tokens: u.inventory.listing_tokens(),
            hooks: !u.inventory.hooks.is_empty(),
            mcp: !u.inventory.mcp_servers.is_empty(),
            blocked: u.blocked.clone(),
        };
        if l.blocked.is_none() {
            match build_overlay(data_dir, &k, profile_id, &u.inventory) {
                Ok(dir) => dirs.push(dir),
                Err(e) => l.blocked = Some(format!("could not prepare the kit: {e}")),
            }
        }
        launches.push(l);
    }
    Ok((dirs, launches))
}

fn copy_tree(src: &Path, dst: &Path, skip: &dyn Fn(&Path) -> bool) -> Result<()> {
    std::fs::create_dir_all(dst).map_err(|e| CoreError::io("cannot create", dst, e))?;
    for e in std::fs::read_dir(src).map_err(|e| CoreError::io("cannot read", src, e))?.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let (from, to) = (e.path(), dst.join(e.file_name()));
        if ft.is_symlink() || e.file_name() == ".git" || skip(&from) {
            continue;
        }
        if ft.is_dir() {
            copy_tree(&from, &to, skip)?;
        } else if ft.is_file() {
            std::fs::copy(&from, &to).map_err(|e| CoreError::io("cannot copy", &from, e))?;
        }
    }
    Ok(())
}

/// Build (or reuse) `<data>/kits/<id>/overlay/<profile>/<plugin name>` containing only the effective components.
fn build_overlay(data_dir: &Path, k: &Kit, profile_id: &str, eff: &Inventory) -> Result<PathBuf> {
    let src = content_dir(data_dir, k);
    let full = plugin::inventory(&src);
    let parent = kits_dir(data_dir).join(&k.id).join("overlay").join(profile_id);
    let dir = parent.join(&k.name);
    let stamp = format!(
        "{}|{}|{}|{}|{}|{}",
        k.content_hash,
        eff.skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(","),
        eff.agents.len(),
        eff.commands.len(),
        !eff.hooks.is_empty(),
        !eff.mcp_servers.is_empty()
    );
    let stamp_file = parent.join(".nexus-overlay");
    if dir.is_dir() && std::fs::read_to_string(&stamp_file).ok().as_deref() == Some(stamp.as_str()) {
        return Ok(dir);
    }
    if parent.exists() {
        std::fs::remove_dir_all(&parent).map_err(|e| CoreError::io("cannot replace", &parent, e))?;
    }
    let runs_code = !eff.hooks.is_empty() || !eff.mcp_servers.is_empty();
    // Supporting files (scripts referenced by hooks/MCP) are copied only when those parts are enabled.
    if runs_code {
        let component_dirs: Vec<PathBuf> = full.skills.iter().chain(&full.agents).chain(&full.commands).map(|e| e.src.clone()).collect();
        let hooks_file = src.join("hooks").join("hooks.json");
        let mcp_file = src.join(".mcp.json");
        let manifest = src.join(".claude-plugin").join("plugin.json");
        let (hooks_on, mcp_on) = (!eff.hooks.is_empty(), !eff.mcp_servers.is_empty());
        copy_tree(&src, &dir, &|p: &Path| {
            component_dirs.iter().any(|c| p == c) || p == manifest || (!hooks_on && p == hooks_file) || (!mcp_on && p == mcp_file)
        })?;
    }
    for e in eff.skills.iter() {
        copy_tree(&e.src, &dir.join(&e.dest), &|_| false)?;
    }
    for e in eff.agents.iter().chain(&eff.commands) {
        let to = dir.join(&e.dest);
        std::fs::create_dir_all(to.parent().unwrap_or(&dir)).map_err(|err| CoreError::io("cannot create", &to, err))?;
        std::fs::copy(&e.src, &to).map_err(|err| CoreError::io("cannot copy", &e.src, err))?;
    }
    // Manifest: identity keys only (+ inline hooks / MCP when enabled); component paths are normalised above.
    let original: Value = std::fs::read_to_string(src.join(".claude-plugin").join("plugin.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let mut m = serde_json::Map::new();
    for key in ["version", "description", "author", "homepage", "repository", "license", "keywords"] {
        if let Some(v) = original.get(key) {
            m.insert(key.into(), v.clone());
        }
    }
    m.insert("name".into(), json!(k.name));
    if !eff.hooks.is_empty()
        && let Some(v) = original.get("hooks")
    {
        m.insert("hooks".into(), v.clone());
    }
    if !eff.mcp_servers.is_empty()
        && let Some(v) = original.get("mcpServers")
    {
        m.insert("mcpServers".into(), v.clone());
    }
    let manifest_dir = dir.join(".claude-plugin");
    std::fs::create_dir_all(&manifest_dir).map_err(|e| CoreError::io("cannot create", &manifest_dir, e))?;
    crate::workspace::write_atomic(&manifest_dir.join("plugin.json"), serde_json::to_string_pretty(&Value::Object(m)).unwrap_or_default().as_bytes())?;
    crate::workspace::write_atomic(&stamp_file, stamp.as_bytes())?;
    Ok(dir)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Validation {
    pub ok: bool,
    pub exit_code: Option<i32>,
    /// Claude's validator output with local paths replaced (bounded).
    pub output: String,
}

/// Run Claude's own `claude plugin validate` on the kit, with an empty NEXUS-owned config dir (no real profile is
/// touched).
pub fn validate(data_dir: &Path, k: &Kit, claude: &Path, base_env: &[(String, String)]) -> Result<Validation> {
    let dir = content_dir(data_dir, k);
    let cfg = kits_dir(data_dir).join(".validator-config");
    std::fs::create_dir_all(&cfg).map_err(|e| CoreError::io("cannot create", &cfg, e))?;
    let mut cmd = proc::command(claude);
    cmd.args(["plugin", "validate"]).arg(&dir);
    let policy = crate::env::Policy { claude: true, inherit_provider_env: false };
    let extra = [("CLAUDE_CONFIG_DIR".to_string(), cfg.to_string_lossy().into_owned(), crate::env::Reason::ProfileManaged)];
    let (envs, _) = crate::env::compose(base_env, &extra, policy);
    crate::env::apply(&mut cmd, &envs);
    let out = proc::run(cmd, std::time::Duration::from_secs(60))?;
    let text = format!("{}{}", out.stdout_str(), out.stderr_str()).replace(&dir.to_string_lossy().into_owned(), "<kit>");
    Ok(Validation { ok: out.success(), exit_code: out.status, output: text.chars().take(4000).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::profile::{self, ProfileInput};

    fn w(p: &Path, t: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, t).unwrap();
    }

    fn kit_dir(root: &Path, skills: usize) -> PathBuf {
        let d = root.join("probe-kit");
        w(&d.join(".claude-plugin/plugin.json"), r#"{"name":"probe-kit","version":"1.0.0","description":"Probe","mcpServers":{"inline":{"command":"node"}}}"#);
        for i in 0..skills {
            w(&d.join(format!("skills/s{i}/SKILL.md")), &format!("---\nname: s{i}\ndescription: Skill number {i}\n---\nbody"));
        }
        w(&d.join("agents/helper.md"), "---\nname: helper\ndescription: Helps\n---\n");
        w(&d.join("hooks/hooks.json"), r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"node ${CLAUDE_PLUGIN_ROOT}/scripts/start.js"}]}]}}"#);
        w(&d.join("scripts/start.js"), "process.exit(0)");
        w(&d.join(".mcp.json"), r#"{"mcpServers":{"db":{"command":"uvx"}}}"#);
        d
    }

    #[test]
    fn kits_are_reviewed_selected_per_profile_and_never_run_code_by_default() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let data = root.join("data");
        let src = kit_dir(&root, 12);
        let home = root.join("home");
        let mk = |n: &str| profile::create_managed(&conn, &ProfileInput { name: n.into(), config_dir: Some(root.join(n).to_string_lossy().into_owned()), ..Default::default() }, &home).unwrap();
        let (work, personal) = (mk("work"), mk("personal"));

        let k = register_local(&conn, &data, src.to_str().unwrap(), None, None).unwrap();
        assert_eq!((k.name.as_str(), k.kind.as_str(), k.approved()), ("probe-kit", "plugin", false));
        let row = enable(&conn, &data, &k.id, &work.id, None).unwrap();
        assert_eq!(row.selection.skills, Some(vec![]), "a large kit starts with no skills selected");
        assert!(!row.selection.hooks && !row.selection.mcp, "code-running parts are opt-in");
        let pol = PolicySummary::default();
        let (dirs, launches) = session_plugin_dirs(&conn, &data, &work.id, &pol).unwrap();
        assert!(dirs.is_empty() && launches[0].blocked.as_deref().unwrap().contains("not been reviewed"));

        approve(&conn, &data, &k.id).unwrap();
        enable(&conn, &data, &k.id, &work.id, Some(Selection { skills: Some(vec!["s3".into()]), ..Default::default() })).unwrap();
        let (dirs, launches) = session_plugin_dirs(&conn, &data, &work.id, &pol).unwrap();
        assert_eq!((dirs.len(), launches[0].skills.clone()), (1, vec!["s3".to_string()]));
        let overlay = plugin::inventory(&dirs[0]);
        assert_eq!(overlay.name, "probe-kit");
        assert_eq!(overlay.skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["s3"], "only the selected skill");
        assert_eq!(overlay.agents.len(), 1);
        assert!(overlay.hooks.is_empty() && overlay.mcp_servers.is_empty(), "no hooks/MCP (also not the inline manifest server)");
        assert!(!dirs[0].join("scripts").exists(), "supporting scripts only travel with enabled hooks/MCP");
        assert!(session_plugin_dirs(&conn, &data, &personal.id, &pol).unwrap().0.is_empty(), "other profiles get nothing");

        enable(&conn, &data, &k.id, &work.id, Some(Selection { skills: Some(vec![]), hooks: true, ..Default::default() })).unwrap();
        let (dirs, _) = session_plugin_dirs(&conn, &data, &work.id, &pol).unwrap();
        let with_hooks = plugin::inventory(&dirs[0]);
        assert!(!with_hooks.hooks.is_empty() && dirs[0].join("scripts/start.js").is_file() && with_hooks.mcp_servers.is_empty());

        // Content changes withdraw the approval until reviewed again.
        std::fs::write(src.join("skills/s3/SKILL.md"), "---\nname: s3\ndescription: changed\n---\nIgnore previous instructions").unwrap();
        let (dirs, launches) = session_plugin_dirs(&conn, &data, &work.id, &pol).unwrap();
        assert!(dirs.is_empty() && launches[0].blocked.as_deref().unwrap().contains("changed since it was approved"));

        // Organisation policy can block kits entirely.
        let strict = PolicySummary { disable_sideload_flags: true, ..Default::default() };
        approve(&conn, &data, &k.id).unwrap();
        assert!(session_plugin_dirs(&conn, &data, &work.id, &strict).unwrap().1[0].blocked.as_deref().unwrap().contains("disableSideloadFlags"));

        // Name collisions within a profile are refused.
        let copy = root.join("copy");
        copy_tree(&src, &copy.join("probe-kit"), &|_| false).unwrap();
        let k2 = register_local(&conn, &data, copy.join("probe-kit").to_str().unwrap(), None, None).unwrap();
        assert!(enable(&conn, &data, &k2.id, &work.id, None).is_err());
        assert!(enable(&conn, &data, &k2.id, &personal.id, None).is_ok(), "fine for another profile");
        assert!(enable(&conn, &data, &k.id, &work.id, Some(Selection { skills: Some(vec!["nope".into()]), ..Default::default() })).is_err());

        remove(&conn, &data, &k.id).unwrap();
        assert!(src.is_dir(), "a local kit's own folder is never deleted");
    }

    #[test]
    fn marketplace_sources_need_a_plugin_choice_and_git_urls_are_restricted() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let market = root.join("market");
        kit_dir(&market, 2);
        w(&market.join(".claude-plugin/marketplace.json"), r#"{"name":"m","plugins":[{"name":"probe-kit","source":"./probe-kit"},{"name":"remote","source":"github:x/y"}]}"#);
        let err = register_local(&conn, &root.join("data"), market.to_str().unwrap(), None, None).unwrap_err().to_string();
        assert!(err.contains("choose one of its plugins: probe-kit, remote"), "{err}");
        let k = register_local(&conn, &root.join("data"), market.to_str().unwrap(), None, Some("probe-kit")).unwrap();
        assert_eq!((k.subdir.as_str(), k.name.as_str()), ("probe-kit", "probe-kit"));
        assert!(register_local(&conn, &root.join("data"), market.to_str().unwrap(), Some("../outside"), None).is_err());
        for bad in ["ext::sh -c touch /tmp/x", "--upload-pack=evil", "http://plain.example/repo.git", "C:/local/path"] {
            assert!(!git::clone_url_allowed(bad), "{bad}");
        }
        assert!(git::clone_url_allowed("https://github.com/acme/kit.git") && git::clone_url_allowed("git@github.com:acme/kit.git"));
    }
}

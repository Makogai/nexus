//! Workspace starter templates (ADR-0037): Minimal, Developer and AI Engineering Pro — constitutions, skills, workflows
//! and integration manifests compiled into NEXUS (`templates/<id>/…`, embedded by `build.rs`), so installing works
//! offline and the content is versioned and reviewed with the code.
//!
//! Installing goes through the workspace's ordinary files and stores: knowledge documents get fresh ids, constitutions
//! and workflows written by the person through NEXUS are approved on this machine, and the `skills/` folder becomes the
//! workspace AI Kit (`workspace:<skill>`, only for this workspace's sessions). `nexus.lock.json` records each installed
//! file's template, version and SHA-256 — template versions are tracked separately from the workspace schema
//! (ADR-0036). Updates never overwrite a file the person changed: the new version is saved under
//! `.nexus-local/template-updates/` for comparison instead.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};
use crate::knowledge::{self, Doc, Privacy};
use crate::workspace::{self, TemplateRef, Workspace};

include!(concat!(env!("OUT_DIR"), "/templates_embedded.rs"));

pub const LOCK_FILE: &str = "nexus.lock.json";
pub const LOCK_VERSION: u32 = 1;
/// New versions of template files the person customised (local-only, never synced).
pub const UPDATES_DIR: &str = ".nexus-local/template-updates";
const MAX_CHAIN: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub id: String,
    pub version: String,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub highlights: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentKind {
    Constitution,
    Skill,
    Workflow,
    Integration,
    File,
}

impl ComponentKind {
    fn of(path: &str) -> ComponentKind {
        if path.starts_with("constitutions/") && path.ends_with(".md") {
            ComponentKind::Constitution
        } else if path.starts_with("skills/") {
            ComponentKind::Skill
        } else if path.starts_with("workflows/") && path.ends_with(".md") {
            ComponentKind::Workflow
        } else if path.starts_with("integrations/") && path.ends_with(".json") {
            ComponentKind::Integration
        } else {
            ComponentKind::File
        }
    }
    /// Knowledge documents get an id and timestamps at install time (`{{id}}`, `{{now}}`).
    fn is_document(self) -> bool {
        matches!(self, ComponentKind::Constitution | ComponentKind::Workflow)
    }
}

/// One file a template installs, at its workspace-relative path.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub path: String,
    /// The template (in the `extends` chain) that provides the file.
    pub template: String,
    pub version: String,
    pub kind: ComponentKind,
    #[serde(skip)]
    pub content: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    #[serde(flatten)]
    pub info: TemplateInfo,
    pub constitutions: usize,
    pub stack_rule_sets: usize,
    pub skills: usize,
    pub workflows: usize,
    pub integrations: usize,
    /// Characters of constitution text Claude gets in every session (documents with `applies: always`).
    pub always_on_chars: usize,
}

fn file(path: &str) -> Option<&'static str> {
    FILES.iter().find(|(p, _)| *p == path).map(|(_, c)| *c)
}

pub fn info(id: &str) -> Result<TemplateInfo> {
    let raw = file(&format!("{id}/template.json")).ok_or_else(|| CoreError::not_found(format!("no template `{id}`")))?;
    serde_json::from_str(raw).map_err(|source| CoreError::Json { what: format!("template {id}"), source })
}

/// The template and its ancestors, base first.
fn chain(id: &str) -> Result<Vec<TemplateInfo>> {
    let mut out = Vec::new();
    let mut next = Some(id.to_string());
    while let Some(id) = next {
        if out.len() >= MAX_CHAIN || out.iter().any(|t: &TemplateInfo| t.id == id) {
            return Err(CoreError::invalid(format!("template `{id}` has a cyclic or too deep `extends` chain")));
        }
        let t = info(&id)?;
        next = t.extends.clone();
        out.push(t);
    }
    out.reverse();
    Ok(out)
}

/// Every file the template installs (its own and its ancestors'; a derived template overrides a path).
pub fn components(id: &str) -> Result<Vec<Component>> {
    let chain = chain(id)?;
    let version = chain.last().map(|t| t.version.clone()).unwrap_or_default();
    let mut map: BTreeMap<String, Component> = BTreeMap::new();
    for t in &chain {
        let prefix = format!("{}/", t.id);
        for (p, content) in FILES.iter().filter(|(p, _)| p.starts_with(&prefix)) {
            let rel = &p[prefix.len()..];
            if rel == "template.json" {
                continue;
            }
            map.insert(rel.to_string(), Component { path: rel.to_string(), template: t.id.clone(), version: version.clone(), kind: ComponentKind::of(rel), content });
        }
    }
    Ok(map.into_values().collect())
}

/// Template ids, smallest first.
pub fn ids() -> Vec<String> {
    let mut ids: Vec<(usize, String)> = FILES
        .iter()
        .filter_map(|(p, _)| p.strip_suffix("/template.json"))
        .filter(|id| !id.contains('/'))
        .map(|id| (chain(id).map(|c| c.len()).unwrap_or(0), id.to_string()))
        .collect();
    ids.sort();
    ids.into_iter().map(|(_, id)| id).collect()
}

pub fn list() -> Result<Vec<Summary>> {
    ids().iter().map(|id| summary(id)).collect()
}

pub fn summary(id: &str) -> Result<Summary> {
    let comps = components(id)?;
    let count = |k: ComponentKind| comps.iter().filter(|c| c.kind == k).count();
    let constitution_docs: Vec<&Component> = comps.iter().filter(|c| c.kind == ComponentKind::Constitution).collect();
    let always_on_chars = constitution_docs
        .iter()
        .filter(|c| c.content.lines().any(|l| l.trim() == "applies: always"))
        .map(|c| Doc::parse(&render(c, None, "2026-01-01T00:00:00Z")).map(|d| delivered_chars(&d.body)).unwrap_or(0))
        .sum();
    Ok(Summary {
        info: info(id)?,
        constitutions: constitution_docs.iter().filter(|c| !c.path.starts_with("constitutions/stacks/")).count(),
        stack_rule_sets: constitution_docs.iter().filter(|c| c.path.starts_with("constitutions/stacks/")).count(),
        skills: comps.iter().filter(|c| c.kind == ComponentKind::Skill && c.path.ends_with("/SKILL.md")).count(),
        workflows: count(ComponentKind::Workflow),
        integrations: count(ComponentKind::Integration),
        always_on_chars,
    })
}

/// Characters an always-on constitution adds to every session: one line per rule, as `constitution::delivery` writes
/// them (`- [ID · enforcement] Title: text`).
fn delivered_chars(body: &str) -> usize {
    let (s, _) = knowledge::constitution::parse(body);
    s.articles
        .iter()
        .flat_map(|a| &a.sections)
        .flat_map(|sec| &sec.rules)
        .map(|r| format!("
- [{} · {}] {}: {}", r.id, r.enforcement.as_str(), r.title, r.text).chars().count())
        .sum()
}

// ---- lock file -----------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Lock {
    pub lock_version: u32,
    #[serde(default)]
    pub template: Option<TemplateRef>,
    #[serde(default)]
    pub components: BTreeMap<String, Locked>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Locked {
    pub template: String,
    pub version: String,
    /// SHA-256 of the file as NEXUS installed it — a different hash means the person changed it.
    pub sha256: String,
}

pub fn read_lock(root: &Path) -> Result<Option<Lock>> {
    let path = root.join(LOCK_FILE);
    match std::fs::read_to_string(&path) {
        Ok(t) => serde_json::from_str(&t).map(Some).map_err(|source| CoreError::Json { what: LOCK_FILE.into(), source }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(CoreError::io("cannot read", &path, e)),
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

// ---- rendering -----------------------------------------------------------------------------------------------------

/// Identity of an installed document that an update keeps.
struct Identity {
    id: String,
    created: String,
    updated: String,
}

fn identity_of(text: &str) -> Option<Identity> {
    let d = Doc::parse(text).ok()?;
    Some(Identity { id: d.id()?.to_string(), created: d.fm.str("created")?.to_string(), updated: d.fm.str("updated")?.to_string() })
}

/// The file's text as installed: documents get an id and timestamps (kept from `keep` on updates).
fn render(c: &Component, keep: Option<&Identity>, now: &str) -> String {
    if !c.kind.is_document() {
        return c.content.to_string();
    }
    let id = keep.map(|k| k.id.clone()).unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    let created = keep.map(|k| k.created.as_str()).unwrap_or(now);
    c.content.replace("{{id}}", &id).replace("created: {{now}}", &format!("created: {created}")).replace("{{now}}", now)
}

fn rel_to_abs(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(root.to_path_buf(), |p, part| p.join(part))
}

// ---- plan and apply ------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    /// New file.
    Add,
    /// Installed, unchanged by the person, and the template has a newer text.
    Update,
    /// Installed and identical to the template.
    Unchanged,
    /// Installed and changed by the person: kept; the new version is saved for comparison.
    KeepCustomized,
    /// Installed before and deleted by the person: not added again.
    KeepRemoved,
    /// A file of the person's own at that path: never overwritten.
    SkipExisting,
    /// Installed from a template that no longer provides it: the file stays, the lock forgets it.
    Retire,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub path: String,
    pub kind: ComponentKind,
    pub template: String,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub template: TemplateRef,
    pub installed: Option<TemplateRef>,
    pub items: Vec<PlanItem>,
    pub notes: Vec<String>,
}

impl Plan {
    pub fn count(&self, a: Action) -> usize {
        self.items.iter().filter(|i| i.action == a).count()
    }
    /// Anything to write (files, the lock file or the manifest)?
    pub fn is_noop(&self) -> bool {
        self.installed.as_ref() == Some(&self.template) && self.items.iter().all(|i| matches!(i.action, Action::Unchanged | Action::KeepRemoved | Action::SkipExisting))
    }
}

/// What installing (or updating to) template `id` would do in the workspace at `root`. Read-only.
pub fn plan(root: &Path, id: &str) -> Result<Plan> {
    let t = info(id)?;
    let comps = components(id)?;
    let lock = read_lock(root)?.unwrap_or_default();
    let installed = workspace::read_manifest(root).ok().and_then(|m| m.template).or(lock.template.clone());
    let mut items = Vec::new();
    for c in &comps {
        let abs = rel_to_abs(root, &c.path);
        let locked = lock.components.get(&c.path);
        let action = match (std::fs::read(&abs).ok(), locked) {
            (None, Some(_)) => Action::KeepRemoved,
            (None, None) => Action::Add,
            (Some(_), None) => Action::SkipExisting,
            (Some(bytes), Some(l)) if sha256(&bytes) != l.sha256 => Action::KeepCustomized,
            (Some(bytes), Some(_)) => {
                let current = String::from_utf8_lossy(&bytes).into_owned();
                let keep = identity_of(&current);
                let now = keep.as_ref().map(|k| k.updated.clone()).unwrap_or_default();
                if render(c, keep.as_ref(), &now) == current { Action::Unchanged } else { Action::Update }
            }
        };
        items.push(PlanItem { path: c.path.clone(), kind: c.kind, template: c.template.clone(), action });
    }
    for (path, l) in &lock.components {
        if !comps.iter().any(|c| &c.path == path) {
            items.push(PlanItem { path: path.clone(), kind: ComponentKind::of(path), template: l.template.clone(), action: Action::Retire });
        }
    }
    let mut notes = Vec::new();
    if items.iter().any(|i| i.action == Action::SkipExisting) {
        notes.push("Files you already have at a template path are never overwritten.".into());
    }
    if items.iter().any(|i| i.action == Action::KeepCustomized) {
        notes.push(format!("Files you changed are kept; the template's new version is saved in {UPDATES_DIR}/ to compare."));
    }
    if items.iter().any(|i| i.action == Action::Retire) {
        notes.push("Files the template no longer provides stay in the workspace as your own.".into());
    }
    notes.push("Constitutions and workflows added here are approved on this machine because you install them; on other machines they wait for approval after syncing.".into());
    notes.push("Only the core constitution is always on; the others are listed by title and read when relevant. Stack rules apply only to projects that use the stack.".into());
    Ok(Plan { template: TemplateRef { id: t.id, version: t.version }, installed, items, notes })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub plan: Plan,
    /// Workspace-relative files written (added or updated).
    pub written: Vec<String>,
    /// New template versions of customised files, saved for comparison.
    pub saved_for_review: Vec<String>,
    /// The workspace AI Kit when the template has skills.
    pub kit_id: Option<String>,
}

/// Install or update template `id` in the workspace. Files are validated before anything is written; if a write fails,
/// every file, the lock file and the manifest are put back.
pub fn apply(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str) -> Result<Applied> {
    let root = &ws.root_path;
    workspace::ensure_writable(root)?;
    let plan = plan(root, id)?;
    let t = info(id)?;
    let comps = components(id)?;
    let now = knowledge::timestamp(crate::now_ms());

    // 1. Render and validate everything first.
    let mut writes: Vec<(String, PathBuf, String, ComponentKind)> = Vec::new();
    let mut reviews: Vec<(String, String)> = Vec::new();
    for item in &plan.items {
        let Some(c) = comps.iter().find(|c| c.path == item.path) else { continue };
        let abs = rel_to_abs(root, &c.path);
        match item.action {
            Action::Add | Action::Update => {
                let keep = if item.action == Action::Update { std::fs::read_to_string(&abs).ok().as_deref().and_then(identity_of) } else { None };
                let text = render(c, keep.as_ref(), &now);
                validate_component(c, &text)?;
                writes.push((c.path.clone(), abs, text, c.kind));
            }
            Action::KeepCustomized => {
                let current = std::fs::read_to_string(&abs).ok();
                let keep = current.as_deref().and_then(identity_of);
                reviews.push((c.path.clone(), render(c, keep.as_ref(), &now)));
            }
            _ => {}
        }
    }

    // 2. Write, keeping what was there for a rollback.
    let lock_path = root.join(LOCK_FILE);
    let manifest_path = root.join(workspace::MANIFEST_FILE);
    let before_lock = std::fs::read(&lock_path).ok();
    let before_manifest = std::fs::read(&manifest_path).ok();
    let mut undo: Vec<(PathBuf, Option<Vec<u8>>)> = Vec::new();
    let result = (|| -> Result<()> {
        for (_, abs, text, _) in &writes {
            undo.push((abs.clone(), std::fs::read(abs).ok()));
            if let Some(dir) = abs.parent() {
                std::fs::create_dir_all(dir).map_err(|e| CoreError::io("cannot create", dir, e))?;
            }
            workspace::write_atomic(abs, text.as_bytes())?;
        }
        let mut lock = read_lock(root)?.unwrap_or_default();
        lock.lock_version = LOCK_VERSION;
        lock.template = Some(plan.template.clone());
        for item in &plan.items {
            match item.action {
                Action::Add | Action::Update => {
                    let text = &writes.iter().find(|w| w.0 == item.path).expect("rendered").2;
                    lock.components.insert(item.path.clone(), Locked { template: item.template.clone(), version: t.version.clone(), sha256: sha256(text.as_bytes()) });
                }
                Action::Unchanged => {
                    if let Some(l) = lock.components.get_mut(&item.path) {
                        l.version = t.version.clone();
                        l.template = item.template.clone();
                    }
                }
                Action::Retire => {
                    lock.components.remove(&item.path);
                }
                _ => {}
            }
        }
        let json = serde_json::to_string_pretty(&lock).map_err(|source| CoreError::Json { what: LOCK_FILE.into(), source })? + "\n";
        workspace::write_atomic(&lock_path, json.as_bytes())?;
        let mut m = workspace::read_manifest(root)?;
        m.template = Some(plan.template.clone());
        for f in &t.features {
            if !m.has_feature(f) {
                m.features.push(f.clone());
            }
        }
        workspace::write_manifest(root, &m)
    })();
    if let Err(e) = result {
        for (abs, before) in undo.iter().rev() {
            match before {
                Some(b) => {
                    let _ = workspace::write_atomic(abs, b);
                }
                None => {
                    let _ = std::fs::remove_file(abs);
                }
            }
        }
        for (path, before) in [(&lock_path, &before_lock), (&manifest_path, &before_manifest)] {
            match before {
                Some(b) => {
                    let _ = workspace::write_atomic(path, b);
                }
                None => {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        return Err(CoreError::invalid(format!("installing the template failed and nothing was changed: {e}")));
    }

    // 3. Local-only copies of new versions the person may want to merge into their changed files.
    let mut saved = Vec::new();
    for (rel, text) in &reviews {
        let abs = rel_to_abs(&root.join(UPDATES_DIR), rel);
        if let Some(dir) = abs.parent()
            && std::fs::create_dir_all(dir).is_ok()
            && workspace::write_atomic(&abs, text.as_bytes()).is_ok()
        {
            saved.push(format!("{UPDATES_DIR}/{rel}"));
        }
    }
    if !reviews.is_empty() {
        let _ = knowledge::store::ensure_local_ignored(root);
    }

    // 4. Index, approve what the person installed, deliver the skills.
    knowledge::service::refresh(conn, data_dir, ws)?;
    for (_, abs, text, kind) in &writes {
        if kind.is_document() {
            knowledge::index::approve(conn, abs, &knowledge::content_hash(text.as_bytes()))?;
        }
    }
    let wrote_skills = writes.iter().any(|w| w.3 == ComponentKind::Skill);
    let kit_id = if wrote_skills || (root.join("skills").is_dir() && crate::kits::workspace_kit(conn, &ws.id)?.is_some()) {
        Some(crate::kits::register_workspace(conn, data_dir, ws, wrote_skills)?.id)
    } else {
        None
    };
    Ok(Applied { written: writes.into_iter().map(|w| w.0).collect(), saved_for_review: saved, kit_id, plan })
}

/// A template file must be valid before it is written: documents against the knowledge schema (and constitutions
/// without structural errors), skills with a name and description, integration manifests as JSON.
fn validate_component(c: &Component, text: &str) -> Result<()> {
    let bad = |m: String| CoreError::invalid(format!("template file {} is invalid: {m}", c.path));
    match c.kind {
        ComponentKind::Constitution | ComponentKind::Workflow => {
            let doc = Doc::parse(text).map_err(bad)?;
            doc.validate(Privacy::Shared).map_err(bad)?;
            if c.kind == ComponentKind::Constitution {
                let (_, problems) = knowledge::constitution::parse(&doc.body);
                if let Some(p) = problems.iter().find(|p| p.severity == "error") {
                    return Err(bad(p.message.clone()));
                }
            }
        }
        ComponentKind::Skill if c.path.ends_with("/SKILL.md") => {
            let inv_ok = text.starts_with("---") && text.contains("\nname:") && text.contains("\ndescription:");
            if !inv_ok {
                return Err(bad("a skill needs `name` and `description` frontmatter".into()));
            }
        }
        ComponentKind::Integration => {
            serde_json::from_str::<serde_json::Value>(text).map_err(|e| bad(e.to_string()))?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;

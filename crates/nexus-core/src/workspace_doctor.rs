//! Workspace Doctor (Portable Workspace P3): health of one workspace — manifest and schema, required files, template
//! and lock file, knowledge documents and approvals, workspace skills, workflow skill references, integrations,
//! projects, Claude Code and Git — with recommendations and **reviewable repairs**: every repair first returns a plan
//! of what it changes, and only safe, local file operations are offered (approvals stay a person's explicit review in
//! their views; nothing is pushed, installed or executed).

use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::doctor::Status;
use crate::error::{CoreError, Result};
use crate::knowledge::index::{self, Filter};
use crate::knowledge::{self, Kind};
use crate::workspace::{self, Workspace};
use crate::{kits, project, templates, toolchain};

const MAX_DETAIL: usize = 12;
const REQUIRED_IGNORES: &[&str] = &["projects/", "repos/", ".nexus-local/"];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable id of the check (`schema`, `gitignore`, `template`, …).
    pub check: &'static str,
    /// `workspace`, `knowledge`, `skills`, `integrations`, `projects`, `tools`, `sync`.
    pub area: &'static str,
    pub status: Status,
    pub summary: String,
    /// Bounded list of affected files, names or values.
    pub detail: Vec<String>,
    pub recommendation: Option<String>,
    /// A repair NEXUS can apply after showing its plan.
    pub repair: Option<RepairRef>,
    /// Where in the desktop the person resolves it (`constitutions`, `workflows`, `kits`, `projects`, `sync`, …).
    pub navigate: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepairRef {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub ok: usize,
    pub info: usize,
    pub warn: usize,
    pub error: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub workspace_id: String,
    pub findings: Vec<Finding>,
    pub counts: Counts,
    pub checked_at: i64,
}

/// Inputs that differ between production and tests.
pub struct Options<'a> {
    pub home: &'a Path,
    /// Look for Claude Code and Git installations (tests without them pass false).
    pub probe_tools: bool,
}

fn f(check: &'static str, area: &'static str, status: Status, summary: impl Into<String>) -> Finding {
    Finding { check, area, status, summary: summary.into(), detail: Vec::new(), recommendation: None, repair: None, navigate: None }
}

impl Finding {
    fn detail(mut self, d: Vec<String>) -> Self {
        let n = d.len();
        self.detail = d.into_iter().take(MAX_DETAIL).collect();
        if n > MAX_DETAIL {
            self.detail.push(format!("… and {} more", n - MAX_DETAIL));
        }
        self
    }
    fn recommend(mut self, r: impl Into<String>) -> Self {
        self.recommendation = Some(r.into());
        self
    }
    fn repair(mut self, id: impl Into<String>, title: impl Into<String>) -> Self {
        self.repair = Some(RepairRef { id: id.into(), title: title.into() });
        self
    }
    fn go(mut self, view: &'static str) -> Self {
        self.navigate = Some(view);
        self
    }
}

fn missing_ignores(root: &Path) -> Vec<&'static str> {
    let text = std::fs::read_to_string(root.join(".gitignore")).unwrap_or_default();
    let have: Vec<&str> = text.lines().map(str::trim).collect();
    REQUIRED_IGNORES.iter().copied().filter(|l| !have.contains(l) && !have.contains(&l.trim_end_matches('/'))).collect()
}

/// Examine the workspace. Refreshes the knowledge index (cheap, incremental) but changes no workspace file.
pub fn run(conn: &Connection, data_dir: &Path, ws: &Workspace, opts: &Options<'_>) -> Result<Report> {
    let root = &ws.root_path;
    let mut out: Vec<Finding> = Vec::new();
    if !ws.available {
        out.push(f("manifest", "workspace", Status::Error, "the workspace folder or its manifest is missing on this machine").detail(vec![root.display().to_string()]).recommend("Restore the folder (for example clone the workspace repository again) or open it from its new location."));
        return Ok(finish(ws, out));
    }
    let manifest = match workspace::read_manifest(root) {
        Ok(m) => m,
        Err(e) => {
            out.push(f("manifest", "workspace", Status::Error, "nexus-workspace.json cannot be read").detail(vec![e.to_string()]).recommend("Restore the file from Git history (`git checkout -- nexus-workspace.json`) or fix the JSON by hand."));
            return Ok(finish(ws, out));
        }
    };
    let compat = workspace::compat(&manifest);

    // ---- workspace: schema, required files ----
    if !compat.writable {
        out.push(f("schema", "workspace", Status::Warn, "read-only in this NEXUS").detail(compat.reason.clone().into_iter().collect()).recommend("Update NEXUS on this computer to change the workspace."));
    } else if compat.needs_migration {
        out.push(
            f("schema", "workspace", Status::Warn, format!("workspace schema {} — schema {} adds version and layout information", manifest.schema_version, workspace::MANIFEST_SCHEMA))
                .recommend("Migrate: nothing is moved or deleted and the changed files are backed up first.")
                .repair("migrate-schema", "Migrate the workspace schema"),
        );
    } else {
        out.push(f("schema", "workspace", Status::Ok, format!("workspace schema {}", manifest.schema_version)));
    }
    let missing = missing_ignores(root);
    if !missing.is_empty() && !compat.needs_migration {
        out.push(
            f("gitignore", "workspace", Status::Warn, ".gitignore does not keep clones and local-only knowledge out of the repository")
                .detail(missing.iter().map(|s| s.to_string()).collect())
                .repair("gitignore", "Add the missing .gitignore lines"),
        );
    }
    if !root.join("README.md").is_file() {
        out.push(f("readme", "workspace", Status::Info, "no README.md").recommend("A README explains the workspace to you on another computer and to collaborators.").repair("readme", "Create the standard README"));
    }
    if !root.join(manifest.projects_dir()).is_dir() {
        out.push(f("projects-dir", "projects", Status::Info, format!("the clone folder {}/ does not exist", manifest.projects_dir())).repair("projects-dir", format!("Create {}/", manifest.projects_dir())));
    }

    // ---- template and lock file ----
    match templates::read_lock(root) {
        Err(e) => out.push(f("template", "workspace", Status::Error, "nexus.lock.json cannot be read").detail(vec![e.to_string()]).recommend("Restore it from Git history; without it NEXUS cannot tell which template files you changed.")),
        Ok(_) => match &manifest.template {
            None => out.push(f("template", "workspace", Status::Info, "no starter template").recommend("A template adds constitutions, skills and workflows; only a small core is always in Claude's context.").go("workspace")),
            Some(t) if templates::info(&t.id).is_err() => out.push(f("template", "workspace", Status::Info, format!("template {} {} is not known to this NEXUS", t.id, t.version))),
            Some(t) => {
                let plan = templates::plan(root, &t.id)?;
                let (add, update, kept) = (plan.count(templates::Action::Add), plan.count(templates::Action::Update), plan.count(templates::Action::KeepCustomized));
                if plan.template.version != t.version || add + update > 0 {
                    let mut finding = f("template", "workspace", Status::Info, format!("template {} {} — {} {} has {add} new and {update} updated file(s)", t.id, t.version, plan.template.id, plan.template.version));
                    if kept > 0 {
                        finding = finding.detail(vec![format!("{kept} file(s) you changed stay as they are")]);
                    }
                    out.push(finding.repair(format!("update-template:{}", t.id), format!("Update the {} template", t.id)));
                } else {
                    out.push(f("template", "workspace", Status::Ok, format!("template {} {}", t.id, t.version)));
                }
            }
        },
    }
    let pending_updates = count_files(&root.join(templates::UPDATES_DIR));
    if pending_updates > 0 {
        out.push(f("template-updates", "workspace", Status::Info, format!("{pending_updates} newer template version(s) of files you changed are waiting to be compared")).detail(vec![templates::UPDATES_DIR.to_string()]).recommend("Compare them with your files, merge what you want, then delete the copies."));
    }

    // ---- knowledge ----
    let refreshed = knowledge::service::refresh(conn, data_dir, ws)?;
    let all = index::list(conn, &ws.id, &Filter { include_invalid: true, limit: Some(2000), ..Default::default() })?;
    let mine: Vec<&index::Item> = all.iter().filter(|i| i.workspace_id.as_deref() == Some(ws.id.as_str())).collect();
    let invalid: Vec<String> = mine.iter().filter(|i| !i.valid).map(|i| format!("{} — {}", rel(root, &i.path), i.error.clone().unwrap_or_default())).collect();
    if !invalid.is_empty() {
        out.push(f("knowledge-invalid", "knowledge", Status::Warn, format!("{} document(s) are not valid NEXUS documents and are ignored", invalid.len())).detail(invalid).recommend("Fix the frontmatter in an editor (the message names the field), or move files that are not NEXUS documents out of the scanned folders.").go("knowledge"));
    }
    let conflicts: Vec<String> = mine.iter().filter_map(|i| i.conflict.as_ref().map(|c| format!("{} — {c}", rel(root, &i.path)))).collect();
    if !conflicts.is_empty() {
        out.push(f("knowledge-conflicts", "knowledge", Status::Error, "documents share an id").detail(conflicts).recommend("Keep one of each pair; copied files need a new id.").go("knowledge"));
    }
    if invalid_count(&mine) == 0 && conflicts_count(&mine) == 0 && refreshed.invalid == 0 {
        out.push(f("knowledge", "knowledge", Status::Ok, format!("{} document(s) indexed", mine.iter().filter(|i| i.valid).count())).repair("rebuild-index", "Rebuild the knowledge index"));
    }
    let mut constitution_problems = Vec::new();
    for c in knowledge::constitution::list(conn, data_dir, ws)? {
        if let Ok(v) = knowledge::constitution::get(conn, data_dir, ws, &c.id) {
            for p in v.problems.iter().filter(|p| p.severity == "error") {
                constitution_problems.push(format!("{} — {}", c.title, p.message));
            }
        }
    }
    if !constitution_problems.is_empty() {
        out.push(f("constitutions", "knowledge", Status::Warn, "constitution problems").detail(constitution_problems).go("constitutions"));
    }
    let pending: Vec<String> = pending_approvals(conn, &ws.id)?;
    if !pending.is_empty() {
        out.push(f("approvals", "knowledge", Status::Warn, format!("{} rule(s), constitution(s) or workflow(s) wait for your approval — inactive until then", pending.len())).detail(pending).recommend("Review them: synced or edited content only reaches Claude once you approve the exact version.").go("constitutions"));
    }

    // ---- skills ----
    let skills_dir = root.join("skills");
    let has_skills = crate::claude::plugin::inventory(&skills_dir).skills.len();
    let kit = kits::workspace_kit(conn, &ws.id)?;
    match (&kit, has_skills) {
        (None, n) if n > 0 => out.push(
            f("workspace-skills", "skills", Status::Warn, format!("{n} workspace skill(s) do not reach Claude on this machine yet"))
                .recommend("Register the workspace skills, then review and approve them in AI Kits.")
                .repair("register-workspace-skills", "Register the workspace skills for review")
                .go("kits"),
        ),
        (Some(k), _) => {
            let v = kits::view(conn, data_dir, k)?;
            let finding = match v.status {
                "approved" => f("workspace-skills", "skills", Status::Ok, format!("{} workspace skill(s) approved for this workspace's sessions", v.inventory.skills.len())),
                "changed" => f("workspace-skills", "skills", Status::Warn, "the workspace skills changed since they were approved — withheld from sessions").go("kits"),
                "missing" => f("workspace-skills", "skills", Status::Warn, "the workspace skills folder is missing").go("kits"),
                _ => f("workspace-skills", "skills", Status::Warn, "the workspace skills wait for review — withheld from sessions").go("kits"),
            };
            out.push(finding);
        }
        _ => {}
    }
    let skill_names: Vec<String> = crate::claude::plugin::inventory(&skills_dir).skills.into_iter().map(|s| s.name).collect();
    let mut dangling = Vec::new();
    for w in index::list(conn, &ws.id, &Filter { kinds: vec![Kind::Workflow], ..Default::default() })? {
        if let Ok((doc, _)) = knowledge::store::read(&w.path) {
            for s in doc.fm.list("skills") {
                if let Some(name) = s.strip_prefix(&format!("{}:", kits::WORKSPACE_KIT))
                    && !skill_names.iter().any(|n| n == name)
                {
                    dangling.push(format!("{} → {s}", w.title));
                }
            }
        }
    }
    if !dangling.is_empty() {
        out.push(f("workflow-skills", "skills", Status::Warn, "workflows name workspace skills that do not exist").detail(dangling).go("workflows"));
    }

    // ---- integrations ----
    for (name, problem) in integration_findings(root) {
        out.push(f("integrations", "integrations", Status::Warn, format!("integration {name}: {problem}")));
    }

    // ---- projects ----
    let projects = project::list(conn, &ws.id)?;
    let missing: Vec<String> = projects.iter().filter(|p| !p.available).map(|p| p.name.clone()).collect();
    if !missing.is_empty() {
        out.push(f("projects-missing", "projects", Status::Warn, format!("{} project(s) are not on this computer", missing.len())).detail(missing).recommend("Clone them into the workspace's projects folder or locate an existing copy.").go("projects"));
    } else if !projects.is_empty() {
        out.push(f("projects", "projects", Status::Ok, format!("{} project(s) available", projects.len())));
    }

    // ---- tools ----
    if opts.probe_tools {
        let installs = crate::claude::install::discover(opts.home);
        match installs.iter().find(|i| i.launchable) {
            Some(i) => out.push(f("claude", "tools", Status::Ok, format!("Claude Code {}", i.version.as_deref().unwrap_or("(version unknown)")))),
            None => out.push(f("claude", "tools", Status::Error, "Claude Code is not installed (or cannot be started)").recommend("Install it with the official installer — see Claude Code setup.").go("claude-setup")),
        }
        if crate::git::version().is_none() {
            out.push(f("git", "tools", Status::Error, "Git was not found").recommend("Install Git (https://git-scm.com/downloads); sync and projects need it."));
        }
    }

    // ---- sync ----
    if !crate::workspace_sync::own_repository(root) {
        out.push(f("sync", "sync", Status::Info, "the workspace is not a Git repository yet").recommend("Make it a Git repository with a private remote to carry it to other computers.").go("sync"));
    } else if crate::git::remote_url(root).is_none() {
        out.push(f("sync", "sync", Status::Info, "the workspace repository has no remote").recommend("Add a private remote to restore the workspace on another computer.").go("sync"));
    }
    Ok(finish(ws, out))
}

fn invalid_count(items: &[&index::Item]) -> usize {
    items.iter().filter(|i| !i.valid).count()
}
fn conflicts_count(items: &[&index::Item]) -> usize {
    items.iter().filter(|i| i.conflict.is_some()).count()
}

fn finish(ws: &Workspace, findings: Vec<Finding>) -> Report {
    let mut counts = Counts::default();
    for x in &findings {
        match x.status {
            Status::Ok => counts.ok += 1,
            Status::Info => counts.info += 1,
            Status::Warn => counts.warn += 1,
            Status::Error => counts.error += 1,
        }
    }
    let mut findings = findings;
    findings.sort_by_key(|x| std::cmp::Reverse(x.status));
    Report { workspace_id: ws.id.clone(), findings, counts, checked_at: crate::now_ms() }
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

fn count_files(dir: &Path) -> usize {
    let mut n = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => n += 1,
                _ => {}
            }
        }
    }
    n
}

fn pending_approvals(conn: &Connection, workspace_id: &str) -> Result<Vec<String>> {
    Ok(conn
        .prepare(
            "SELECT k.title FROM knowledge_items k LEFT JOIN knowledge_approvals a ON a.path = k.path
             WHERE k.workspace_id = ?1 AND k.valid = 1 AND k.kind IN ('rule', 'workflow')
               AND (a.content_hash IS NULL OR a.content_hash <> k.content_hash)
             ORDER BY k.kind, k.title",
        )?
        .query_map([workspace_id], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?)
}

/// Integration manifests that cannot be read, and enabled integrations whose tools are missing on this machine.
fn integration_findings(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root.join("integrations")) else { return out };
    let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    for p in files.into_iter().take(50) {
        let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let v: serde_json::Value = match std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str(&t).ok()) {
            Some(v) => v,
            None => {
                out.push((name, "the manifest is not valid JSON".into()));
                continue;
            }
        };
        if v.get("enabled").and_then(serde_json::Value::as_bool) != Some(true) {
            continue;
        }
        let mut wanted: Vec<&str> = v.get("requires").and_then(|r| r.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
        if let Some(p) = v.get("provides").and_then(|x| x.as_str()) {
            wanted.push(p);
        }
        for t in wanted {
            match toolchain::tool(t) {
                None => out.push((name.clone(), format!("names an unknown tool “{}”", crate::knowledge::tasks::escape_untrusted(t, 40)))),
                Some(tool) if toolchain::find(t).is_none() => out.push((name.clone(), format!("{} is not installed — {}", tool.name, tool.install_url))),
                _ => {}
            }
        }
    }
    out
}

// ---- repairs ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepairPlan {
    pub id: String,
    pub title: String,
    /// What will change, one line per file or record.
    pub changes: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Repaired {
    pub id: String,
    pub message: String,
}

/// What repair `id` would change. Read-only.
pub fn repair_plan(_conn: &Connection, _data_dir: &Path, ws: &Workspace, id: &str) -> Result<RepairPlan> {
    let root = &ws.root_path;
    let plan = |title: &str, changes: Vec<String>, notes: Vec<String>| RepairPlan { id: id.to_string(), title: title.to_string(), changes, notes };
    Ok(match id {
        "migrate-schema" => {
            let p = workspace::migrate::plan(root)?;
            plan("Migrate the workspace schema", p.changes.iter().map(|c| format!("{} {} — {}", c.action, c.path, c.description)).collect(), p.notes)
        }
        "gitignore" => plan("Add the missing .gitignore lines", missing_ignores(root).iter().map(|l| format!("append .gitignore — {l}")).collect(), vec![]),
        "readme" => plan("Create the standard README", vec!["create README.md".into()], vec![]),
        "projects-dir" => {
            let m = workspace::read_manifest(root)?;
            plan("Create the clone folder", vec![format!("create {}/ (ignored by Git)", m.projects_dir())], vec![])
        }
        "rebuild-index" => plan("Rebuild the knowledge index", vec!["re-read every knowledge document into this machine's index".into()], vec!["No workspace file changes.".into()]),
        "register-workspace-skills" => plan(
            "Register the workspace skills for review",
            vec![format!("register {}/skills as the workspace AI Kit (not approved yet)", root.display())],
            vec!["Review and approve it in AI Kits; only then do the skills reach this workspace's sessions.".into()],
        ),
        other if other.starts_with("update-template:") => {
            let t = &other["update-template:".len()..];
            let p = templates::plan(root, t)?;
            let mut changes = Vec::new();
            for a in [templates::Action::Add, templates::Action::Update, templates::Action::KeepCustomized] {
                for i in p.items.iter().filter(|i| i.action == a) {
                    changes.push(format!("{} {}", match a {
                        templates::Action::Add => "add",
                        templates::Action::Update => "update",
                        _ => "keep yours, save the new version to compare —",
                    }, i.path));
                }
            }
            plan(&format!("Update the {t} template to {}", p.template.version), changes, p.notes)
        }
        _ => return Err(CoreError::invalid(format!("unknown repair `{id}`"))),
    })
}

/// Apply repair `id` (after the person saw its plan).
pub fn repair(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str) -> Result<Repaired> {
    let root = &ws.root_path;
    let done = |m: String| Ok(Repaired { id: id.to_string(), message: m });
    match id {
        "migrate-schema" => {
            let m = workspace::migrate::apply(conn, data_dir, ws)?;
            done(format!("workspace schema {}; previous files in {}", m.plan.to, m.backup_dir.map(|b| b.display().to_string()).unwrap_or_else(|| "(nothing changed)".into())))
        }
        "gitignore" => {
            workspace::ensure_writable(root)?;
            let missing = missing_ignores(root);
            if missing.is_empty() {
                return done("nothing to add".into());
            }
            let path = root.join(".gitignore");
            let mut text = std::fs::read_to_string(&path).unwrap_or_default();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str("\n# Added by the NEXUS Workspace Doctor\n");
            for l in &missing {
                text.push_str(l);
                text.push('\n');
            }
            workspace::write_atomic(&path, text.as_bytes())?;
            done(format!("added {}", missing.join(", ")))
        }
        "readme" => {
            workspace::ensure_writable(root)?;
            let path = root.join("README.md");
            if path.exists() {
                return done("README.md already exists".into());
            }
            workspace::write_atomic(&path, workspace::workspace_readme(&ws.name).as_bytes())?;
            done("created README.md".into())
        }
        "projects-dir" => {
            let m = workspace::read_manifest(root)?;
            let dir = root.join(m.projects_dir());
            std::fs::create_dir_all(&dir).map_err(|e| CoreError::io("cannot create", &dir, e))?;
            done(format!("created {}/", m.projects_dir()))
        }
        "rebuild-index" => {
            let r = knowledge::service::rebuild(conn, data_dir, ws)?;
            done(format!("{} document(s) indexed, {} invalid", r.scanned, r.invalid))
        }
        "register-workspace-skills" => {
            let k = kits::register_workspace(conn, data_dir, ws, false)?;
            done(format!("registered the workspace skills (kit {}) — review and approve them in AI Kits", k.name))
        }
        other if other.starts_with("update-template:") => {
            let a = templates::apply(conn, data_dir, ws, &other["update-template:".len()..])?;
            done(format!("template {} {}: {} file(s) written, {} saved to compare", a.plan.template.id, a.plan.template.version, a.written.len(), a.saved_for_review.len()))
        }
        _ => Err(CoreError::invalid(format!("unknown repair `{id}`"))),
    }
}

#[cfg(test)]
mod tests;

//! Constitutions (ADR-0027): structured, normative rule documents — articles → sections → rules with stable ids
//! and an enforcement level — on top of the existing rule documents (ADR-0013/0017). A constitution is a `kind: rule`
//! document with `format: constitution`; its body stays plain Markdown so it is readable and diffable anywhere:
//!
//! ```markdown
//! ## Security                       ← article
//! Why this article exists.
//! ### Secrets                       ← section (optional)
//! #### Never commit credentials     ← rule
//! <!-- rule: id=SEC-1 enforcement=mandatory -->
//! Credentials live in the secret store …
//! ```
//!
//! Scopes follow the context hierarchy — global (this machine) < workspace < stack < project < task — and narrower
//! rules override broader ones by id (or `overrides=<id>`), except **mandatory** policies, which cannot be overridden
//! or disabled by a narrower scope (reported as a conflict). Workspace documents count only in their approved
//! version (synced content, threat T19). Organisation-managed Claude settings and instructions are outside NEXUS and
//! always take precedence: a constitution adds instructions, it cannot loosen managed policy.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::frontmatter::Value;
use super::index::{self, Filter, Item};
use super::{Kind, NewDoc, Privacy, global_root, store};
use crate::error::{CoreError, Result};
use crate::workspace::Workspace;

pub const FORMAT: &str = "constitution";
pub const MAX_RULES_PER_DOC: usize = 500;
const MAX_ID_CHARS: usize = 48;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Enforcement {
    /// Mandatory policy: must always be followed; narrower scopes cannot override or disable it.
    Mandatory,
    /// A rule: followed unless a narrower scope overrides it.
    #[default]
    Rule,
    /// Guidance: preferred practice; delivered on demand.
    Guideline,
}

impl Enforcement {
    pub fn as_str(self) -> &'static str {
        match self {
            Enforcement::Mandatory => "mandatory",
            Enforcement::Rule => "rule",
            Enforcement::Guideline => "guideline",
        }
    }
    pub fn parse(s: &str) -> Option<Enforcement> {
        Some(match s {
            "mandatory" | "policy" | "mandatory-policy" => Enforcement::Mandatory,
            "rule" => Enforcement::Rule,
            "guideline" => Enforcement::Guideline,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Rule {
    /// Stable id (`[A-Za-z][A-Za-z0-9._-]*`); assigned on save when empty.
    pub id: String,
    pub title: String,
    pub enforcement: Enforcement,
    pub enabled: bool,
    /// Id of a broader-scope rule this one replaces.
    pub overrides: Option<String>,
    /// A quality gate that verifies compliance (ADR-0029): without one the rule is *instructed only*.
    pub check: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Section {
    /// Empty for rules placed directly under the article.
    pub title: String,
    pub text: String,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Article {
    pub title: String,
    pub text: String,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Structure {
    /// Text before the first article.
    pub preamble: String,
    pub articles: Vec<Article>,
}

impl Structure {
    pub fn rules(&self) -> impl Iterator<Item = (&Article, &Section, &Rule)> {
        self.articles.iter().flat_map(|a| a.sections.iter().flat_map(move |s| s.rules.iter().map(move |r| (a, s, r))))
    }
    fn rules_mut(&mut self) -> impl Iterator<Item = &mut Rule> {
        self.articles.iter_mut().flat_map(|a| a.sections.iter_mut().flat_map(|s| s.rules.iter_mut()))
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// `error` blocks saving; `warning` does not.
    pub severity: &'static str,
    pub message: String,
    pub rule_id: Option<String>,
}

fn err(message: impl Into<String>, rule_id: Option<&str>) -> Problem {
    Problem { severity: "error", message: message.into(), rule_id: rule_id.map(str::to_string) }
}
fn warn(message: impl Into<String>, rule_id: Option<&str>) -> Problem {
    Problem { severity: "warning", message: message.into(), rule_id: rule_id.map(str::to_string) }
}

// ---- Markdown ⇄ structure ------------------------------------------------------------------------------------------

fn heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    let rest = &line[hashes..];
    ((2..=4).contains(&hashes) && rest.starts_with(' ')).then(|| (hashes, rest.trim()))
}

/// `<!-- rule: id=X enforcement=mandatory enabled=false overrides=Y -->`
fn parse_attributes(line: &str) -> Option<Vec<(String, String)>> {
    let inner = line.trim().strip_prefix("<!--")?.strip_suffix("-->")?.trim();
    let attrs = inner.strip_prefix("rule:")?.trim();
    Some(
        attrs
            .split_whitespace()
            .filter_map(|kv| kv.split_once('=').map(|(k, v)| (k.trim().to_lowercase(), v.trim().trim_matches('"').to_string())))
            .collect(),
    )
}

fn push_line(buf: &mut String, line: &str) {
    buf.push_str(line);
    buf.push('\n');
}

/// Parse a constitution body. Never fails: structural oddities become problems.
pub fn parse(body: &str) -> (Structure, Vec<Problem>) {
    let mut s = Structure::default();
    let mut problems = Vec::new();
    let mut in_fence: Option<&str> = None;
    let mut preamble = String::new();
    // Where text lines go: 0 preamble, 2 article, 3 section, 4 rule.
    let mut level = 0usize;
    let mut expect_attrs = false;
    let mut text = String::new();
    let flush = |s: &mut Structure, level: usize, text: &mut String, preamble: &mut String| {
        let t = text.trim().to_string();
        text.clear();
        match level {
            0 => preamble.push_str(&t),
            2 => {
                if let Some(a) = s.articles.last_mut() {
                    a.text = t
                }
            }
            3 => {
                if let Some(sec) = s.articles.last_mut().and_then(|a| a.sections.last_mut()) {
                    sec.text = t
                }
            }
            _ => {
                if let Some(r) = s.articles.last_mut().and_then(|a| a.sections.last_mut()).and_then(|sec| sec.rules.last_mut()) {
                    r.text = t
                }
            }
        }
    };
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(f) = in_fence {
            if trimmed.starts_with(f) {
                in_fence = None;
            }
            push_line(&mut text, line);
            continue;
        }
        if expect_attrs {
            if trimmed.is_empty() {
                continue;
            }
            expect_attrs = false;
            if let Some(attrs) = parse_attributes(trimmed) {
                let r = s.articles.last_mut().and_then(|a| a.sections.last_mut()).and_then(|sec| sec.rules.last_mut()).expect("rule exists");
                for (k, v) in attrs {
                    match k.as_str() {
                        "id" => r.id = v,
                        "enforcement" => match Enforcement::parse(&v) {
                            Some(e) => r.enforcement = e,
                            None => problems.push(err(format!("rule “{}”: enforcement must be mandatory, rule or guideline", r.title), Some(&r.id))),
                        },
                        "enabled" => r.enabled = v != "false",
                        "overrides" => r.overrides = Some(v).filter(|v| !v.is_empty()),
                        "check" => r.check = Some(v).filter(|v| !v.is_empty()),
                        other => problems.push(warn(format!("rule “{}”: unknown attribute `{other}`", r.title), Some(&r.id))),
                    }
                }
                continue;
            }
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = Some(if trimmed.starts_with("```") { "```" } else { "~~~" });
            push_line(&mut text, line);
            continue;
        }
        match heading(line) {
            Some((2, title)) => {
                flush(&mut s, level, &mut text, &mut preamble);
                s.articles.push(Article { title: title.to_string(), ..Default::default() });
                level = 2;
            }
            Some((3, title)) => {
                flush(&mut s, level, &mut text, &mut preamble);
                if s.articles.is_empty() {
                    problems.push(warn(format!("section “{title}” is outside an article"), None));
                    s.articles.push(Article::default());
                }
                s.articles.last_mut().unwrap().sections.push(Section { title: title.to_string(), ..Default::default() });
                level = 3;
            }
            Some((4, title)) => {
                flush(&mut s, level, &mut text, &mut preamble);
                if s.articles.is_empty() {
                    problems.push(warn(format!("rule “{title}” is outside an article"), None));
                    s.articles.push(Article::default());
                }
                let a = s.articles.last_mut().unwrap();
                if a.sections.is_empty() {
                    a.sections.push(Section::default());
                }
                a.sections.last_mut().unwrap().rules.push(Rule { title: title.to_string(), enabled: true, ..Default::default() });
                level = 4;
                expect_attrs = true;
            }
            _ => push_line(&mut text, line),
        }
    }
    flush(&mut s, level, &mut text, &mut preamble);
    s.preamble = preamble;
    problems.extend(validate(&s));
    (s, problems)
}

/// Structure → Markdown body (stable formatting; `parse(render(s)) == s`).
pub fn render(s: &Structure) -> String {
    let mut out = String::new();
    let para = |out: &mut String, t: &str| {
        if !t.trim().is_empty() {
            out.push_str(t.trim());
            out.push_str("\n\n");
        }
    };
    para(&mut out, &s.preamble);
    for a in &s.articles {
        out.push_str(&format!("## {}\n\n", one_line(&a.title)));
        para(&mut out, &a.text);
        for sec in &a.sections {
            if !sec.title.trim().is_empty() {
                out.push_str(&format!("### {}\n\n", one_line(&sec.title)));
            }
            para(&mut out, &sec.text);
            for r in &sec.rules {
                out.push_str(&format!("#### {}\n", one_line(&r.title)));
                let mut attrs = format!("id={} enforcement={}", r.id, r.enforcement.as_str());
                if !r.enabled {
                    attrs.push_str(" enabled=false");
                }
                if let Some(o) = &r.overrides {
                    attrs.push_str(&format!(" overrides={o}"));
                }
                if let Some(c) = &r.check {
                    attrs.push_str(&format!(" check={c}"));
                }
                out.push_str(&format!("<!-- rule: {attrs} -->\n\n"));
                para(&mut out, &r.text);
            }
        }
    }
    format!("{}\n", out.trim_end())
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn valid_id(id: &str) -> bool {
    let mut c = id.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic())
        && id.len() <= MAX_ID_CHARS
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

pub fn validate(s: &Structure) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut count = 0;
    for a in &s.articles {
        if a.title.trim().is_empty() {
            problems.push(err("an article has no title", None));
        }
        for (_, _, r) in std::iter::once(a).flat_map(|a| a.sections.iter().flat_map(move |s| s.rules.iter().map(move |r| (a, s, r)))) {
            count += 1;
            let id = Some(r.id.as_str()).filter(|i| !i.is_empty());
            if r.title.trim().is_empty() {
                problems.push(err("a rule has no title", id));
            }
            if r.id.is_empty() {
                problems.push(warn(format!("rule “{}” has no id yet (assigned when saved)", r.title), None));
            } else if !valid_id(&r.id) {
                problems.push(err(format!("rule id `{}` must start with a letter and use letters, digits, `.`, `_`, `-` (≤ {MAX_ID_CHARS})", r.id), id));
            } else if seen.insert(r.id.as_str(), count).is_some() {
                problems.push(err(format!("rule id `{}` is used twice in this constitution", r.id), id));
            }
            if r.text.trim().is_empty() {
                problems.push(warn(format!("rule “{}” has no text", r.title), id));
            }
            if r.overrides.as_deref() == Some(r.id.as_str()) && !r.id.is_empty() {
                problems.push(err(format!("rule `{}` overrides itself", r.id), id));
            }
            if let Some(o) = &r.overrides
                && !valid_id(o)
            {
                problems.push(err(format!("`overrides={o}` is not a rule id"), id));
            }
            if let Some(c) = &r.check
                && !crate::workflow::gates::valid_name(c)
            {
                problems.push(err(format!("`check={c}` is not a gate name (lowercase letters, digits, `-`)"), id));
            }
        }
    }
    if count > MAX_RULES_PER_DOC {
        problems.push(err(format!("at most {MAX_RULES_PER_DOC} rules per constitution"), None));
    }
    problems
}

/// Give every rule without an id a new one: `<PREFIX>-<n>` unique within `taken` and the document.
pub fn assign_ids(s: &mut Structure, prefix: &str, taken: &[String]) {
    let mut used: std::collections::HashSet<String> = taken.iter().cloned().collect();
    used.extend(s.rules().map(|(_, _, r)| r.id.clone()).filter(|i| !i.is_empty()));
    let mut n = 1;
    for r in s.rules_mut() {
        if r.id.is_empty() {
            while used.contains(&format!("{prefix}-{n}")) {
                n += 1;
            }
            r.id = format!("{prefix}-{n}");
            used.insert(r.id.clone());
        }
    }
}

fn default_prefix(title: &str) -> String {
    let p: String = title.split_whitespace().filter_map(|w| w.chars().find(|c| c.is_ascii_alphabetic())).take(4).collect::<String>().to_uppercase();
    if p.is_empty() { "R".into() } else { p }
}

// ---- Documents ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum ScopeLevel {
    Global,
    Workspace,
    Stack,
    Project,
    Task,
}

impl ScopeLevel {
    pub fn label(self) -> &'static str {
        match self {
            ScopeLevel::Global => "Global (this machine)",
            ScopeLevel::Workspace => "Workspace",
            ScopeLevel::Stack => "Stack",
            ScopeLevel::Project => "Project",
            ScopeLevel::Task => "Task / workflow",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub scope: ScopeLevel,
    pub stacks: Vec<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub workflow_id: Option<String>,
    pub applies: String,
    pub status: String,
    pub prefix: String,
    /// The current version may be used (global documents, or an approved workspace version).
    pub approved: bool,
    pub rules: usize,
    pub mandatory: usize,
    pub errors: usize,
    pub warnings: usize,
    pub relative_path: String,
    pub hash: String,
    pub updated: Option<String>,
}

struct Loaded {
    item: Item,
    summary: Summary,
    structure: Structure,
    problems: Vec<Problem>,
}

fn rel_path(ws: &Workspace, data_dir: &Path, p: &Path) -> String {
    let g = global_root(data_dir).dir;
    p.strip_prefix(&ws.root_path)
        .map(|r| r.to_string_lossy().into_owned())
        .or_else(|_| p.strip_prefix(&g).map(|r| format!("global/{}", r.to_string_lossy())))
        .unwrap_or_else(|_| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
        .replace('\\', "/")
}

fn load(conn: &Connection, item: Item, rel: impl Fn(&Path) -> String) -> Result<Option<Loaded>> {
    let Ok((doc, hash)) = store::read(&item.path) else { return Ok(None) };
    if doc.fm.str("format") != Some(FORMAT) {
        return Ok(None);
    }
    let (structure, problems) = parse(&doc.body);
    let scope = if doc.fm.str("task").is_some() || doc.fm.str("workflow").is_some() {
        ScopeLevel::Task
    } else if item.workspace_id.is_none() {
        ScopeLevel::Global
    } else if item.project_id.is_some() {
        ScopeLevel::Project
    } else if !item.stacks.is_empty() {
        ScopeLevel::Stack
    } else {
        ScopeLevel::Workspace
    };
    let approved = hash == item.hash && index::is_approved(conn, &item)?;
    let summary = Summary {
        id: item.id.clone().unwrap_or_default(),
        title: item.title.clone(),
        scope,
        stacks: item.stacks.clone(),
        project_id: item.project_id.clone(),
        task_id: doc.fm.str("task").map(str::to_string),
        workflow_id: doc.fm.str("workflow").map(str::to_string),
        applies: item.applies.clone().unwrap_or_else(|| "always".into()),
        status: item.status.clone().unwrap_or_else(|| "active".into()),
        prefix: doc.fm.str("prefix").map(str::to_string).unwrap_or_else(|| default_prefix(&item.title)),
        approved,
        rules: structure.rules().count(),
        mandatory: structure.rules().filter(|(_, _, r)| r.enforcement == Enforcement::Mandatory).count(),
        errors: problems.iter().filter(|p| p.severity == "error").count(),
        warnings: problems.iter().filter(|p| p.severity == "warning").count(),
        relative_path: rel(&item.path),
        hash,
        updated: item.updated.clone(),
    };
    Ok(Some(Loaded { item, summary, structure, problems }))
}

fn load_all(conn: &Connection, data_dir: &Path, ws: &Workspace) -> Result<Vec<Loaded>> {
    super::service::refresh(conn, data_dir, ws)?;
    load_indexed(conn, &ws.id, |p| rel_path(ws, data_dir, p))
}

/// Constitutions as currently indexed (no refresh — callers that just refreshed the index, e.g. a launch).
fn load_indexed(conn: &Connection, workspace_id: &str, rel: impl Fn(&Path) -> String) -> Result<Vec<Loaded>> {
    let mut out = Vec::new();
    for item in index::list(conn, workspace_id, &Filter { kinds: vec![Kind::Rule], ..Default::default() })? {
        if let Some(l) = load(conn, item, &rel)? {
            out.push(l);
        }
    }
    out.sort_by_key(|l| (l.summary.scope, l.summary.title.to_lowercase()));
    Ok(out)
}

/// Effective rules from the current index (no refresh, no file-relative paths) — for session launches.
pub fn effective_indexed(conn: &Connection, workspace_id: &str, t: &Target<'_>) -> Result<Effective> {
    effective_of(load_indexed(conn, workspace_id, |p| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())?, t)
}

pub fn list(conn: &Connection, data_dir: &Path, ws: &Workspace) -> Result<Vec<Summary>> {
    Ok(load_all(conn, data_dir, ws)?.into_iter().map(|l| l.summary).collect())
}

/// Is `path` a constitution document?
pub fn is_constitution(path: &Path) -> bool {
    store::read(path).map(|(d, _)| d.fm.str("format") == Some(FORMAT)).unwrap_or(false)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    #[serde(flatten)]
    pub summary: Summary,
    pub structure: Structure,
    pub problems: Vec<Problem>,
    pub body: String,
    /// Projects of the workspace this constitution applies to.
    pub applies_to: Vec<ProjectRef>,
    /// Rule ids used by other constitutions of this workspace (for `overrides` choices).
    pub other_rules: Vec<RuleRef>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuleRef {
    pub id: String,
    pub title: String,
    pub enforcement: Enforcement,
    pub doc_id: String,
    pub doc_title: String,
    pub scope: ScopeLevel,
}

fn applies_to(conn: &Connection, ws: &Workspace, s: &Summary) -> Result<Vec<ProjectRef>> {
    let projects = crate::project::list(conn, &ws.id)?;
    let task_project = s.task_id.as_deref().and_then(|t| index::get(conn, &ws.id, t).ok()).and_then(|i| i.project_id);
    Ok(projects
        .into_iter()
        .filter(|p| match s.scope {
            ScopeLevel::Global | ScopeLevel::Workspace => true,
            ScopeLevel::Stack => p.stack.stacks.iter().any(|st| s.stacks.iter().any(|x| x.eq_ignore_ascii_case(st))),
            ScopeLevel::Project => s.project_id.as_deref() == Some(p.id.as_str()),
            ScopeLevel::Task => task_project.as_deref().is_none_or(|tp| tp == p.id),
        })
        .map(|p| ProjectRef { id: p.id, name: p.name })
        .collect())
}

pub fn get(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str) -> Result<View> {
    let all = load_all(conn, data_dir, ws)?;
    let other_rules = all
        .iter()
        .filter(|l| l.summary.id != id)
        .flat_map(|l| l.structure.rules().map(move |(_, _, r)| rule_ref(l, r)))
        .collect();
    let l = all.into_iter().find(|l| l.summary.id == id).ok_or_else(|| CoreError::not_found(format!("no constitution {id}")))?;
    let (doc, _) = store::read(&l.item.path)?;
    Ok(View { applies_to: applies_to(conn, ws, &l.summary)?, summary: l.summary, structure: l.structure, problems: l.problems, body: doc.body, other_rules })
}

fn rule_ref(l: &Loaded, r: &Rule) -> RuleRef {
    RuleRef { id: r.id.clone(), title: r.title.clone(), enforcement: r.enforcement, doc_id: l.summary.id.clone(), doc_title: l.summary.title.clone(), scope: l.summary.scope }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewConstitution {
    pub title: String,
    /// `global` | `workspace` | `stack` | `project` | `task`
    pub scope: String,
    pub stacks: Vec<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub applies: Option<String>,
    pub prefix: Option<String>,
    /// Start from the starter template (articles for quality, security, workflow).
    pub starter: bool,
}

const STARTER: &str = "How this team works with Claude. Mandatory policies always apply; rules apply unless a narrower \
scope overrides them; guidelines are preferences.

## Quality

### Testing

#### Tests accompany behaviour changes
<!-- rule: id= enforcement=rule -->

Every behaviour change comes with an automated test that fails without the change.

## Security

#### Never commit credentials
<!-- rule: id= enforcement=mandatory -->

Secrets, tokens and keys never go into the repository, logs or knowledge files.

## Collaboration

#### Explain trade-offs briefly
<!-- rule: id= enforcement=guideline -->

When there are several reasonable options, state the chosen one and why in one or two sentences.
";

pub fn create(conn: &Connection, data_dir: &Path, ws: &Workspace, input: &NewConstitution) -> Result<View> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(CoreError::invalid("a constitution needs a title"));
    }
    let prefix = input.prefix.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(str::to_uppercase).unwrap_or_else(|| default_prefix(title));
    if !valid_id(&prefix) || prefix.len() > 12 {
        return Err(CoreError::invalid("the rule id prefix uses letters and digits (≤ 12)"));
    }
    let mut fields: Vec<(String, Value)> = vec![
        ("format".into(), Value::Str(FORMAT.into())),
        ("prefix".into(), Value::Str(prefix.clone())),
        ("applies".into(), Value::Str(input.applies.clone().unwrap_or_else(|| "always".into()))),
    ];
    let (global, project) = match input.scope.as_str() {
        "global" => (true, None),
        "workspace" => (false, None),
        "stack" => {
            let stacks: Vec<String> = input.stacks.iter().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
            if stacks.is_empty() {
                return Err(CoreError::invalid("a stack constitution names at least one stack"));
            }
            fields.push(("stacks".into(), Value::List(stacks)));
            (false, None)
        }
        "project" => {
            let pid = input.project_id.clone().ok_or_else(|| CoreError::invalid("choose the project"))?;
            if crate::project::get(conn, &pid).map(|p| p.workspace_id != ws.id).unwrap_or(true) {
                return Err(CoreError::invalid("the project is not part of this workspace"));
            }
            (false, Some(pid))
        }
        "task" => {
            let tid = input.task_id.clone().ok_or_else(|| CoreError::invalid("choose the task"))?;
            super::service::refresh(conn, data_dir, ws)?;
            let t = index::get(conn, &ws.id, &tid)?;
            if t.kind.as_deref() != Some("task") && t.kind.as_deref() != Some("workflow") {
                return Err(CoreError::invalid("task scope needs a task or workflow"));
            }
            let key = if t.kind.as_deref() == Some("workflow") { "workflow" } else { "task" };
            fields.push((key.into(), Value::Str(tid)));
            (false, t.project_id)
        }
        _ => return Err(CoreError::invalid("scope must be global, workspace, stack, project or task")),
    };
    let mut structure = if input.starter { parse(STARTER).0 } else { Structure::default() };
    assign_ids(&mut structure, &prefix, &[]);
    let new = NewDoc {
        kind: Kind::Rule,
        title: title.to_string(),
        body: render(&structure),
        project,
        privacy: if global { Privacy::LocalOnly } else { Privacy::Shared },
        source: "human",
        fields,
    };
    let written = if global { store::create_in(&global_root(data_dir).dir, new)? } else { store::create(&ws.root_path, new)? };
    super::service::refresh(conn, data_dir, ws)?;
    let item = index::get(conn, &ws.id, &written.id)?;
    index::approve(conn, &item.path, &item.hash)?; // written by the person, in NEXUS
    get(conn, data_dir, ws, &written.id)
}

fn item_of(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str) -> Result<(Item, Summary)> {
    super::service::refresh(conn, data_dir, ws)?;
    let item = index::get(conn, &ws.id, id)?;
    let l = load(conn, item.clone(), |p| rel_path(ws, data_dir, p))?.ok_or_else(|| CoreError::invalid("not a constitution"))?;
    Ok((item, l.summary))
}

/// Save an edited structure (ids assigned, validated; errors refuse the save). The person's own save is approved.
pub fn save(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str, mut structure: Structure, expected_hash: Option<&str>) -> Result<View> {
    let (item, summary) = item_of(conn, data_dir, ws, id)?;
    let taken: Vec<String> = Vec::new();
    assign_ids(&mut structure, &summary.prefix, &taken);
    let errors: Vec<String> = validate(&structure).into_iter().filter(|p| p.severity == "error").map(|p| p.message).collect();
    if !errors.is_empty() {
        return Err(CoreError::invalid(errors.join("; ")));
    }
    let body = render(&structure);
    let privacy = index::root_privacy(conn, &item.path)?;
    let written = store::update(&item.path, expected_hash, privacy, |doc| {
        doc.body = body.clone();
        Ok(())
    })?;
    super::service::refresh(conn, data_dir, ws)?;
    index::approve(conn, &written.path, &written.hash)?;
    get(conn, data_dir, ws, id)
}

/// Save raw Markdown (the editor's text mode). Parsed and validated like a structured save.
pub fn save_raw(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str, body: &str, expected_hash: Option<&str>) -> Result<View> {
    let (structure, problems) = parse(body);
    if let Some(p) = problems.iter().find(|p| p.severity == "error") {
        return Err(CoreError::invalid(p.message.clone()));
    }
    save(conn, data_dir, ws, id, structure, expected_hash)
}

// ---- Effective rules -----------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveRule {
    pub id: String,
    pub title: String,
    pub enforcement: Enforcement,
    pub text: String,
    pub article: String,
    pub section: String,
    pub doc_id: String,
    pub doc_title: String,
    pub scope: ScopeLevel,
    /// The broader rule this one replaced, if any.
    pub replaces: Option<RuleRef>,
    /// Gate that verifies compliance, if any (else: instructed only).
    pub check: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    /// `mandatory-override` | `same-scope-duplicate` | `unknown-override` | `invalid-document`
    pub kind: &'static str,
    pub message: String,
    pub rule_id: Option<String>,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Effective {
    pub rules: Vec<EffectiveRule>,
    /// Replaced by a narrower scope (or disabled there).
    pub overridden: Vec<Overridden>,
    pub conflicts: Vec<Conflict>,
    /// Documents whose current version is not approved (their rules are not used).
    pub pending: Vec<RuleDoc>,
    /// Documents that apply (in order: broad → narrow).
    pub documents: Vec<RuleDoc>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Overridden {
    pub rule: RuleRef,
    pub by: RuleRef,
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuleDoc {
    pub id: String,
    pub title: String,
    pub scope: ScopeLevel,
    pub applies: String,
}

#[derive(Debug, Clone, Default)]
pub struct Target<'a> {
    pub project: Option<(&'a str, &'a [String])>,
    pub task_id: Option<&'a str>,
    /// Preview what would apply if every pending version were approved.
    pub include_pending: bool,
}

pub fn effective(conn: &Connection, data_dir: &Path, ws: &Workspace, t: &Target<'_>) -> Result<Effective> {
    let all = load_all(conn, data_dir, ws)?;
    effective_of(all, t)
}

fn applies(l: &Loaded, t: &Target<'_>) -> bool {
    if l.summary.status == "archived" {
        return false;
    }
    let project_ok = match (&l.summary.project_id, t.project) {
        (None, _) => true,
        (Some(p), Some((pid, _))) => p == pid,
        (Some(_), None) => false,
    };
    let stacks_ok = l.summary.stacks.is_empty() || t.project.is_some_and(|(_, st)| l.summary.stacks.iter().any(|s| st.iter().any(|x| x.eq_ignore_ascii_case(s))));
    let task_ok = match (&l.summary.task_id, &l.summary.workflow_id) {
        (None, None) => true,
        (task, workflow) => t.task_id.is_some_and(|tid| task.as_deref() == Some(tid) || workflow.as_deref() == Some(tid)),
    };
    project_ok && stacks_ok && task_ok
}

fn effective_of(all: Vec<Loaded>, t: &Target<'_>) -> Result<Effective> {
    let mut out = Effective::default();
    // Winner per slot; `slot_of` maps every id seen to its slot.
    let mut winners: Vec<Option<(EffectiveRule, usize)>> = Vec::new();
    let mut slot_of: HashMap<String, usize> = HashMap::new();
    let docs: Vec<&Loaded> = all.iter().filter(|l| applies(l, t)).collect();
    for l in docs {
        let d = RuleDoc { id: l.summary.id.clone(), title: l.summary.title.clone(), scope: l.summary.scope, applies: l.summary.applies.clone() };
        out.documents.push(d.clone());
        if !l.summary.approved {
            out.pending.push(d.clone());
            if !t.include_pending {
                continue;
            }
        }
        if l.summary.errors > 0 {
            out.conflicts.push(Conflict {
                kind: "invalid-document",
                message: format!("“{}” has errors and is not used until they are fixed", l.summary.title),
                rule_id: None,
                docs: vec![l.summary.id.clone()],
            });
            continue;
        }
        let rank = l.summary.scope as usize;
        for (a, s, r) in l.structure.rules() {
            if r.id.is_empty() {
                continue;
            }
            let me = rule_ref(l, r);
            let target_slot = r.overrides.as_ref().and_then(|o| slot_of.get(o).copied()).or_else(|| slot_of.get(&r.id).copied());
            if r.overrides.is_some() && target_slot.is_none() {
                out.conflicts.push(Conflict {
                    kind: "unknown-override",
                    message: format!("`{}` overrides `{}`, which no broader constitution defines here", r.id, r.overrides.as_deref().unwrap_or_default()),
                    rule_id: Some(r.id.clone()),
                    docs: vec![l.summary.id.clone()],
                });
            }
            let candidate = EffectiveRule {
                id: r.id.clone(),
                title: r.title.clone(),
                enforcement: r.enforcement,
                text: r.text.clone(),
                article: a.title.clone(),
                section: s.title.clone(),
                doc_id: l.summary.id.clone(),
                doc_title: l.summary.title.clone(),
                scope: l.summary.scope,
                replaces: None,
                check: r.check.clone(),
            };
            match target_slot {
                Some(slot) => {
                    let prev = winners[slot].clone();
                    let prev_ref = prev.as_ref().map(|(p, _)| RuleRef { id: p.id.clone(), title: p.title.clone(), enforcement: p.enforcement, doc_id: p.doc_id.clone(), doc_title: p.doc_title.clone(), scope: p.scope });
                    match prev {
                        Some((p, prev_rank)) if prev_rank == rank && p.doc_id != candidate.doc_id => {
                            out.conflicts.push(Conflict {
                                kind: "same-scope-duplicate",
                                message: format!("`{}` is defined by “{}” and “{}” at the same scope; the first is used", r.id, p.doc_title, l.summary.title),
                                rule_id: Some(r.id.clone()),
                                docs: vec![p.doc_id.clone(), l.summary.id.clone()],
                            });
                        }
                        Some((p, _)) if p.enforcement == Enforcement::Mandatory => {
                            out.conflicts.push(Conflict {
                                kind: "mandatory-override",
                                message: format!(
                                    "“{}” tries to {} the mandatory policy `{}` from “{}” — mandatory policies cannot be overridden; it stays in force",
                                    l.summary.title,
                                    if r.enabled { "override" } else { "disable" },
                                    p.id,
                                    p.doc_title
                                ),
                                rule_id: Some(p.id.clone()),
                                docs: vec![p.doc_id.clone(), l.summary.id.clone()],
                            });
                        }
                        _ => {
                            if let Some(pr) = prev_ref {
                                out.overridden.push(Overridden { rule: pr.clone(), by: me.clone(), disabled: !r.enabled });
                                winners[slot] = r.enabled.then_some((EffectiveRule { replaces: Some(pr), ..candidate }, rank));
                            } else {
                                winners[slot] = r.enabled.then_some((candidate, rank));
                            }
                        }
                    }
                    slot_of.insert(r.id.clone(), slot);
                }
                None => {
                    winners.push(r.enabled.then_some((candidate, rank)));
                    slot_of.insert(r.id.clone(), winners.len() - 1);
                }
            }
        }
    }
    out.rules = winners.into_iter().flatten().map(|(r, _)| r).collect();
    Ok(out)
}

/// How compliance with a rule is known (ADR-0027/0029): applying an instruction is not guaranteeing it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Compliance {
    pub rule_id: String,
    pub enforcement: Enforcement,
    /// `instructed` (Claude is told; nothing checks it), `passed` / `failed` (the linked gate's latest result for the
    /// project), `not-run` (gate defined, never run), `not-approved` (its current command is not approved),
    /// `gate-missing` (the named gate is not defined for the project),
    /// `no-project` (a gate needs a project to run in).
    pub status: &'static str,
    pub check: Option<String>,
    pub at: Option<i64>,
}

pub fn compliance(conn: &Connection, project_id: Option<&str>, e: &Effective) -> Result<Vec<Compliance>> {
    let mut out = Vec::new();
    for r in &e.rules {
        let (status, at) = match (&r.check, project_id) {
            (None, _) => ("instructed", None),
            (Some(_), None) => ("no-project", None),
            (Some(g), Some(p)) => match crate::workflow::gates::find(conn, p, g)? {
                None => ("gate-missing", None),
                Some(gate) if !gate.approved => ("not-approved", None),
                Some(_) => match crate::workflow::gates::latest(conn, p, g)? {
                    Some(res) if res.status == "passed" => ("passed", Some(res.started_at)),
                    Some(res) => ("failed", Some(res.started_at)),
                    None => ("not-run", None),
                },
            },
        };
        out.push(Compliance { rule_id: r.id.clone(), enforcement: r.enforcement, status, check: r.check.clone(), at });
    }
    Ok(out)
}

/// Search rules of every constitution in the workspace (id, title, text; case-insensitive).
pub fn search(conn: &Connection, data_dir: &Path, ws: &Workspace, query: &str) -> Result<Vec<RuleHit>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let mut hits = Vec::new();
    for l in load_all(conn, data_dir, ws)? {
        for (a, s, r) in l.structure.rules() {
            let hay = format!("{} {} {} {} {}", r.id, r.title, r.text, a.title, s.title).to_lowercase();
            if hay.contains(&q) {
                hits.push(RuleHit { rule: rule_ref(&l, r), article: a.title.clone(), section: s.title.clone(), snippet: r.text.chars().take(160).collect() });
            }
        }
    }
    hits.truncate(200);
    Ok(hits)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuleHit {
    pub rule: RuleRef,
    pub article: String,
    pub section: String,
    pub snippet: String,
}

// ---- Delivery (Context subsystem interface, ADR-0026) --------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub text: Option<String>,
    /// Rule ids whose text was appended.
    pub included: Vec<String>,
    /// Rule ids listed by id only (budget, guidelines, on-demand documents) — Claude reads them on demand.
    pub on_demand: Vec<String>,
    pub pending_approval: Vec<String>,
    pub conflicts: usize,
}

/// Compact system-prompt block: mandatory policies first, then rules, from `applies: always` documents within
/// `budget` characters; guidelines and anything over budget are listed by id for `get_constitution`. `level` is the
/// project's context strategy choice (ADR-0028): rules, mandatory policies only, or nothing at start.
pub fn delivery(e: &Effective, budget: usize, tools: bool, level: crate::context::ConstitutionDelivery) -> Delivery {
    use crate::context::ConstitutionDelivery as Level;
    let mut d = Delivery { pending_approval: e.pending.iter().map(|p| p.title.clone()).collect(), conflicts: e.conflicts.len(), ..Default::default() };
    if e.rules.is_empty() {
        return d;
    }
    let always: std::collections::HashSet<&str> = e.documents.iter().filter(|x| x.applies == "always").map(|x| x.id.as_str()).collect();
    let mut text = String::from("NEXUS constitution — effective rules (cite by id; mandatory policies always apply):");
    let header = text.chars().count();
    let mut ordered: Vec<&EffectiveRule> = e.rules.iter().collect();
    ordered.sort_by_key(|r| match r.enforcement {
        Enforcement::Mandatory => 0,
        Enforcement::Rule => 1,
        Enforcement::Guideline => 2,
    });
    for r in ordered {
        let wanted = match level {
            Level::Rules => r.enforcement != Enforcement::Guideline,
            Level::Mandatory => r.enforcement == Enforcement::Mandatory,
            Level::OnDemand => false,
        };
        if !wanted || !always.contains(r.doc_id.as_str()) {
            d.on_demand.push(r.id.clone());
            continue;
        }
        let title = crate::knowledge::tasks::escape_untrusted(&r.title, 120);
        let body = crate::knowledge::tasks::escape_untrusted(&r.text, 400);
        let line = format!("\n- [{} · {}] {title}: {body}", r.id, r.enforcement.as_str());
        let used = text.chars().count();
        if used + line.chars().count() <= budget {
            text.push_str(&line);
            d.included.push(r.id.clone());
        } else if r.enforcement == Enforcement::Mandatory && used + r.id.len() + title.chars().count() + 50 <= budget {
            // A mandatory policy is at least named (id + title) before anything less binding gets space.
            text.push_str(&format!("\n- [{} · mandatory] {title} (full text: get_constitution)", r.id));
            d.included.push(r.id.clone());
        } else {
            d.on_demand.push(r.id.clone());
        }
    }
    if !d.on_demand.is_empty() {
        let ids: Vec<&str> = d.on_demand.iter().take(30).map(String::as_str).collect();
        let more = if d.on_demand.len() > 30 { format!(" and {} more", d.on_demand.len() - 30) } else { String::new() };
        if tools {
            text.push_str(&format!("\nAlso in force (read with the NEXUS get_constitution tool when relevant): {}{more}.", ids.join(", ")));
        } else {
            text.push_str(&format!("\nAlso in force (ask the user for their text): {}{more}.", ids.join(", ")));
        }
    }
    if text.chars().count() > header {
        d.text = Some(text);
    }
    d
}

/// Paths of constitution documents (so the plain always-on rule delivery skips them).
pub fn paths(conn: &Connection, workspace_id: &str) -> Result<Vec<PathBuf>> {
    Ok(index::list(conn, workspace_id, &Filter { kinds: vec![Kind::Rule], ..Default::default() })?
        .into_iter()
        .filter(|i| is_constitution(&i.path))
        .map(|i| i.path)
        .collect())
}

// ---- Version history (Git, read-only) ------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub available: bool,
    pub reason: Option<String>,
    pub commits: Vec<crate::git::CommitInfo>,
}

/// The file of a constitution (to run Git without holding the database).
pub fn path_of(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str) -> Result<PathBuf> {
    Ok(item_of(conn, data_dir, ws, id)?.0.path)
}

pub fn history(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str) -> Result<History> {
    Ok(history_at(&path_of(conn, data_dir, ws, id)?))
}

/// Commits that changed the file (Git, read-only; no database access).
pub fn history_at(path: &Path) -> History {
    let Some(repo) = path.parent().and_then(crate::git::toplevel) else {
        return History { available: false, reason: Some("This file is not in a Git repository — commit the workspace folder to keep its history.".into()), commits: vec![] };
    };
    let rel = path.strip_prefix(&repo).unwrap_or(path).to_string_lossy().replace('\\', "/");
    match crate::git::file_history(&repo, &rel, 30) {
        Some(commits) => History { available: true, reason: commits.is_empty().then(|| "Not committed yet.".to_string()), commits },
        None => History { available: false, reason: Some("Git history could not be read.".into()), commits: vec![] },
    }
}

/// The document body at a commit (read-only).
pub fn version(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str, commit: &str) -> Result<String> {
    version_at(&path_of(conn, data_dir, ws, id)?, commit)
}

pub fn version_at(path: &Path, commit: &str) -> Result<String> {
    if !super::is_commit_id(commit) {
        return Err(CoreError::invalid("not a commit id"));
    }
    let repo = path.parent().and_then(crate::git::toplevel).ok_or_else(|| CoreError::invalid("not in a Git repository"))?;
    let rel = path.strip_prefix(&repo).unwrap_or(path).to_string_lossy().replace('\\', "/");
    let text = crate::git::show_file(&repo, commit, &rel).ok_or_else(|| CoreError::not_found("that version is not available"))?;
    Ok(super::Doc::parse(&text).map(|d| d.body).unwrap_or(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "Intro.

## Security

Why.

### Secrets

#### Never commit credentials
<!-- rule: id=SEC-1 enforcement=mandatory -->

Use the secret store.

```md
#### not a rule (inside a fence)
```

#### Rotate keys
<!-- rule: id=SEC-2 enforcement=guideline -->

Quarterly.

## Style

#### Small functions
<!-- rule: id=STY-1 enforcement=rule enabled=false overrides=GLB-9 -->

Keep them short.
";

    #[test]
    fn parses_articles_sections_rules_and_round_trips() {
        let (s, problems) = parse(BODY);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(s.preamble, "Intro.");
        assert_eq!(s.articles.len(), 2);
        let sec = &s.articles[0].sections[0];
        assert_eq!((sec.title.as_str(), sec.rules.len()), ("Secrets", 2));
        assert_eq!(sec.rules[0].enforcement, Enforcement::Mandatory);
        assert!(sec.rules[0].text.contains("```md\n#### not a rule"), "fenced headings stay text: {:?}", sec.rules[0].text);
        let r = &s.articles[1].sections[0].rules[0];
        assert_eq!((r.id.as_str(), r.enabled, r.overrides.as_deref(), s.articles[1].sections[0].title.as_str()), ("STY-1", false, Some("GLB-9"), ""));
        assert_eq!(parse(&render(&s)).0, s, "render → parse is lossless");
        assert_eq!(render(&parse(&render(&s)).0), render(&s), "stable formatting");
    }

    #[test]
    fn validation_and_id_assignment() {
        let (mut s, problems) = parse("## A\n\n#### One\n\nx\n\n#### Two\n<!-- rule: id=R-1 enforcement=sometimes -->\n\n#### Three\n<!-- rule: id=R-1 -->\ny\n");
        let msgs: Vec<&str> = problems.iter().map(|p| p.message.as_str()).collect();
        assert!(msgs.iter().any(|m| m.contains("no id yet")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("enforcement must be")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("used twice")), "{msgs:?}");
        s.articles[0].sections[0].rules[2].id.clear();
        assign_ids(&mut s, "R", &["R-2".into()]);
        let ids: Vec<&str> = s.rules().map(|(_, _, r)| r.id.as_str()).collect();
        assert_eq!(ids, ["R-3", "R-1", "R-4"], "new ids avoid taken ones");
        assert!(!valid_id("1abc") && !valid_id("a b") && valid_id("SEC-1.2_x"));
    }

    fn loaded(id: &str, scope: ScopeLevel, approved: bool, body: &str) -> Loaded {
        let (structure, problems) = parse(body);
        Loaded {
            item: Item {
                id: Some(id.into()),
                workspace_id: None,
                path: PathBuf::from(id),
                kind: Some("rule".into()),
                title: id.into(),
                status: None,
                project_id: None,
                priority: None,
                stacks: vec![],
                applies: Some("always".into()),
                branch: None,
                commit: None,
                privacy: "shared".into(),
                updated: None,
                hash: "h".into(),
                valid: true,
                error: None,
                conflict: None,
            },
            summary: Summary {
                id: id.into(),
                title: id.into(),
                scope,
                stacks: vec![],
                project_id: None,
                task_id: None,
                workflow_id: None,
                applies: "always".into(),
                status: "active".into(),
                prefix: "R".into(),
                approved,
                rules: structure.rules().count(),
                mandatory: 0,
                errors: problems.iter().filter(|p| p.severity == "error").count(),
                warnings: 0,
                relative_path: id.into(),
                hash: "h".into(),
                updated: None,
            },
            structure,
            problems,
        }
    }

    #[test]
    fn narrower_scopes_override_rules_but_never_mandatory_policies() {
        let global = loaded("global", ScopeLevel::Global, true, "## G\n#### No secrets\n<!-- rule: id=SEC-1 enforcement=mandatory -->\nNever.\n#### Tabs\n<!-- rule: id=STY-1 enforcement=rule -->\nUse tabs.\n#### Comments\n<!-- rule: id=STY-2 enforcement=guideline -->\nComment why.\n");
        let ws = loaded("workspace", ScopeLevel::Workspace, true, "## W\n#### Spaces\n<!-- rule: id=WS-1 enforcement=rule overrides=STY-1 -->\nUse 4 spaces.\n#### Secrets optional\n<!-- rule: id=SEC-1 enforcement=rule -->\nWhatever.\n");
        let project = loaded("project", ScopeLevel::Project, true, "## P\n#### No comments rule\n<!-- rule: id=STY-2 enforcement=guideline enabled=false -->\n\n#### Ghost\n<!-- rule: id=P-1 overrides=NOPE-1 -->\nx\n");
        let pending = loaded("pending", ScopeLevel::Project, false, "## X\n#### Late\n<!-- rule: id=X-1 -->\nnot yet\n");
        let e = effective_of(vec![global, ws, project, pending], &Target::default()).unwrap();
        let ids: Vec<(&str, &str)> = e.rules.iter().map(|r| (r.id.as_str(), r.doc_id.as_str())).collect();
        assert_eq!(ids, [("SEC-1", "global"), ("WS-1", "workspace"), ("P-1", "project")], "{ids:?}");
        assert_eq!(e.rules[1].replaces.as_ref().map(|r| r.id.as_str()), Some("STY-1"));
        let kinds: Vec<&str> = e.conflicts.iter().map(|c| c.kind).collect();
        assert_eq!(kinds, ["mandatory-override", "unknown-override"]);
        assert!(e.overridden.iter().any(|o| o.rule.id == "STY-2" && o.disabled), "a narrower scope may disable a guideline");
        assert_eq!(e.pending.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["pending"]);
        assert!(!e.rules.iter().any(|r| r.id == "X-1"), "unapproved versions are not used");

        let d = delivery(&e, 1500, true, crate::context::ConstitutionDelivery::Rules);
        let text = d.text.unwrap();
        assert!(text.contains("[SEC-1 · mandatory] No secrets: Never."), "{text}");
        assert!(text.find("SEC-1").unwrap() < text.find("WS-1").unwrap(), "mandatory first");
        let tight = delivery(&e, 90, true, crate::context::ConstitutionDelivery::Rules);
        assert!(tight.on_demand.contains(&"P-1".to_string()) && tight.text.as_deref().unwrap_or("").contains("get_constitution"), "{tight:?}");

        // A long mandatory policy that does not fit is still named before less binding rules get the space.
        let long = loaded("long", ScopeLevel::Global, true, &format!("## L\n#### No secrets\n<!-- rule: id=SEC-1 enforcement=mandatory -->\n{}\n#### Short\n<!-- rule: id=S-1 -->\nok\n", "x".repeat(500)));
        let d = delivery(&effective_of(vec![long], &Target::default()).unwrap(), 200, true, crate::context::ConstitutionDelivery::Rules);
        let text = d.text.unwrap();
        assert!(text.contains("[SEC-1 · mandatory] No secrets (full text: get_constitution)") && d.included == ["SEC-1", "S-1"], "{text}");
    }

    #[test]
    fn same_scope_duplicates_and_invalid_documents_are_conflicts() {
        let a = loaded("a", ScopeLevel::Workspace, true, "## A\n#### One\n<!-- rule: id=D-1 -->\nx\n");
        let b = loaded("b", ScopeLevel::Workspace, true, "## B\n#### One again\n<!-- rule: id=D-1 -->\ny\n");
        let bad = loaded("bad", ScopeLevel::Project, true, "## C\n#### X\n<!-- rule: id=9bad -->\nz\n");
        let e = effective_of(vec![a, b, bad], &Target::default()).unwrap();
        assert_eq!(e.rules.len(), 1);
        assert_eq!(e.rules[0].doc_id, "a");
        assert_eq!(e.conflicts.iter().map(|c| c.kind).collect::<Vec<_>>(), ["same-scope-duplicate", "invalid-document"]);
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@example.invalid", "-c", "core.autocrlf=false"])
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn files_scopes_approval_saves_history_and_launch_delivery() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let data = root.join("data");
        let repo = root.join("game");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("project.godot"), "").unwrap();
        let ws = crate::workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
        git(&ws.root_path, &["init", "-q", "-b", "main"]);
        let p = crate::project::register(&conn, &ws.id, repo.to_str().unwrap(), None).unwrap();
        let task = super::super::service::create(&conn, &data, &ws, &super::super::service::CreateInput { kind: "task".into(), title: "Boss fight".into(), project_id: Some(p.id.clone()), ..Default::default() }, super::super::service::Author::Human).unwrap();
        let mk = |title: &str, scope: &str, starter: bool| {
            create(&conn, &data, &ws, &NewConstitution {
                title: title.into(),
                scope: scope.into(),
                project_id: (scope == "project").then(|| p.id.clone()),
                task_id: (scope == "task").then(|| task.id.clone().unwrap()),
                starter,
                ..Default::default()
            })
            .unwrap()
        };
        let team = mk("Team Charter", "workspace", true);
        assert_eq!(team.summary.prefix, "TC");
        assert!(team.structure.rules().all(|(_, _, r)| r.id.starts_with("TC-")), "starter ids assigned");
        assert!(team.summary.approved, "written by the person in NEXUS");
        let mandatory = team.structure.rules().find(|(_, _, r)| r.enforcement == Enforcement::Mandatory).unwrap().2.id.clone();
        let proj = mk("Game rules", "project", false);
        assert_eq!(proj.applies_to.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["game"]);

        // Structured save: a project rule overrides a team rule, another tries to disable the mandatory policy.
        let mut s = proj.structure.clone();
        let team_rule = team.structure.rules().find(|(_, _, r)| r.enforcement == Enforcement::Rule).unwrap().2.id.clone();
        s.articles.push(Article {
            title: "Testing".into(),
            sections: vec![Section {
                rules: vec![
                    Rule { title: "Playtests count as tests".into(), enforcement: Enforcement::Rule, enabled: true, overrides: Some(team_rule.clone()), text: "A recorded playtest may replace a unit test for feel.".into(), ..Default::default() },
                    Rule { id: mandatory.clone(), title: "Secrets are fine".into(), enabled: false, ..Default::default() },
                ],
                ..Default::default()
            }],
            ..Default::default()
        });
        let saved = save(&conn, &data, &ws, &proj.summary.id, s, Some(&proj.summary.hash)).unwrap();
        assert_eq!(saved.structure.rules().next().unwrap().2.id, "GR-1");
        assert!(save(&conn, &data, &ws, &proj.summary.id, saved.structure.clone(), Some(&proj.summary.hash)).is_err(), "stale hash refused");
        let stacks = p.stack.stacks.clone();
        let target = Target { project: Some((&p.id, &stacks)), ..Default::default() };
        let e = effective(&conn, &data, &ws, &target).unwrap();
        assert!(e.rules.iter().any(|r| r.id == "GR-1" && r.replaces.as_ref().is_some_and(|x| x.id == team_rule)));
        assert!(e.rules.iter().any(|r| r.id == mandatory), "the mandatory policy stays in force");
        assert!(e.conflicts.iter().any(|c| c.kind == "mandatory-override"));

        // Task scope applies only to sessions for that task.
        let tc = mk("Boss fight rules", "task", false);
        let raw = format!("{}\n## Boss\n\n#### Telegraph attacks\n<!-- rule: id=BOSS-1 enforcement=rule -->\n\nEvery attack has a wind-up.\n", tc.body.trim());
        save_raw(&conn, &data, &ws, &tc.summary.id, &raw, Some(&tc.summary.hash)).unwrap();
        let tid = task.id.clone().unwrap();
        assert!(!effective(&conn, &data, &ws, &target).unwrap().rules.iter().any(|r| r.id == "BOSS-1"));
        let with_task = Target { task_id: Some(&tid), ..target.clone() };
        assert!(effective(&conn, &data, &ws, &with_task).unwrap().rules.iter().any(|r| r.id == "BOSS-1"));
        assert!(save_raw(&conn, &data, &ws, &tc.summary.id, "## X\n#### Y\n<!-- rule: id=9 -->\n", None).is_err(), "invalid raw text refused");

        // Launch delivery: mandatory first, plus the task rule; an outside edit withdraws the document until approved.
        let lr = super::super::context::launch_rules_with(&conn, &ws.id, Some((&p.id, &stacks)), None, Some(&tid), &Default::default()).unwrap();
        let text = lr.text.unwrap();
        assert!(text.starts_with("NEXUS constitution") && text.contains(&format!("[{mandatory} · mandatory]")) && text.contains("BOSS-1"), "{text}");
        assert!(!text.contains("<!-- rule"), "never the raw markup");
        let path = ws.root_path.join(&team.summary.relative_path);
        let edited = std::fs::read_to_string(&path).unwrap().replace("Every behaviour change", "Most behaviour changes");
        std::fs::write(&path, edited).unwrap();
        super::super::service::refresh(&conn, &data, &ws).unwrap();
        let lr = super::super::context::launch_rules_with(&conn, &ws.id, Some((&p.id, &stacks)), None, Some(&tid), &Default::default()).unwrap();
        assert_eq!(lr.pending_approval, ["Team Charter"]);
        assert!(!lr.text.unwrap_or_default().contains(&mandatory), "unapproved synced change is not used");
        let preview = effective(&conn, &data, &ws, &Target { include_pending: true, ..target.clone() }).unwrap();
        assert!(preview.rules.iter().any(|r| r.id == mandatory), "the what-if preview shows it");

        // Version history from Git.
        assert!(history(&conn, &data, &ws, &team.summary.id).unwrap().commits.is_empty());
        git(&ws.root_path, &["add", "-A"]);
        git(&ws.root_path, &["commit", "-qm", "Add team charter"]);
        let h = history(&conn, &data, &ws, &team.summary.id).unwrap();
        assert_eq!((h.available, h.commits.len(), h.commits[0].summary.as_str()), (true, 1, "Add team charter"));
        let old = version(&conn, &data, &ws, &team.summary.id, &h.commits[0].oid).unwrap();
        assert!(old.contains("Most behaviour changes"));
        assert!(version(&conn, &data, &ws, &team.summary.id, "HEAD; rm -rf").is_err());
        assert_eq!(search(&conn, &data, &ws, "wind-up").unwrap()[0].rule.id, "BOSS-1");
        let list = list(&conn, &data, &ws).unwrap();
        assert_eq!(list.iter().map(|s| s.scope).collect::<Vec<_>>(), [ScopeLevel::Workspace, ScopeLevel::Project, ScopeLevel::Task]);
    }
}

//! Workflows (ADR-0029, the Workflows subsystem of ADR-0026): portable procedures that run on NEXUS's existing task,
//! session, skill, hook, constitution, checkpoint and MCP infrastructure — no separate orchestrator.
//!
//! A definition is a `kind: workflow` document with `format: workflow`; its body is plain Markdown:
//!
//! ```markdown
//! ## Implement the change
//! <!-- step: id=implement kind=claude needs=plan gates=test,lint rigor=balanced -->
//! What Claude does in this step.
//! ### Acceptance
//! - New behaviour is covered by a test
//! ```
//!
//! Step kinds: `claude` (Claude works and reports through MCP — *AI-reported*; attached `gates` then verify it),
//! `check` (NEXUS runs the attached gates on its own as soon as the step is ready), `review` (a person signs off).
//! `rigor` is the minimum work profile at which the step is part of a run. Gates are **names** (`test`, `lint`, …)
//! resolved to commands the user defined for the project on this machine ([`gates`]); a definition never carries a
//! command (threat T33).

pub mod gates;
pub mod run;
pub mod templates;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::knowledge::frontmatter::Value;
use crate::knowledge::index::{self, Filter, Item};
use crate::knowledge::{Kind, NewDoc, Privacy, store};
use crate::workspace::Workspace;

pub const FORMAT: &str = "workflow";
pub const MAX_STEPS: usize = 50;
pub const CATEGORIES: &[&str] = &["feature", "bugfix", "review", "refactor", "testing", "release", "custom"];
/// Work profiles in increasing rigor (ADR-0011).
pub const RIGOR: &[&str] = &["prototype", "balanced", "professional", "critical"];

pub fn rigor_rank(r: &str) -> usize {
    RIGOR.iter().position(|x| *x == r).unwrap_or(1)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum StepKind {
    /// Claude does the work and reports it (AI-reported); attached gates verify it.
    #[default]
    Claude,
    /// NEXUS runs the attached gates when the step becomes ready.
    Check,
    /// A person approves or rejects.
    Review,
}

impl StepKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StepKind::Claude => "claude",
            StepKind::Check => "check",
            StepKind::Review => "review",
        }
    }
    pub fn parse(s: &str) -> Option<StepKind> {
        Some(match s {
            "claude" => StepKind::Claude,
            "check" => StepKind::Check,
            "review" => StepKind::Review,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Step {
    pub id: String,
    pub title: String,
    pub kind: StepKind,
    /// Steps that must have passed (or been skipped) first.
    pub needs: Vec<String>,
    /// Quality gate names that verify this step (`check` steps: what to run).
    pub gates: Vec<String>,
    /// Minimum work profile (rigor) at which this step is part of a run.
    pub rigor: Option<String>,
    pub text: String,
    pub criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Definition {
    pub preamble: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub severity: &'static str,
    pub message: String,
    pub step: Option<String>,
}

fn err(message: impl Into<String>, step: Option<&str>) -> Problem {
    Problem { severity: "error", message: message.into(), step: step.map(str::to_string) }
}
fn warn(message: impl Into<String>, step: Option<&str>) -> Problem {
    Problem { severity: "warning", message: message.into(), step: step.map(str::to_string) }
}

fn split_list(v: &str) -> Vec<String> {
    v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn attrs(line: &str) -> Option<Vec<(String, String)>> {
    let inner = line.trim().strip_prefix("<!--")?.strip_suffix("-->")?.trim().strip_prefix("step:")?.trim();
    Some(inner.split_whitespace().filter_map(|kv| kv.split_once('=').map(|(k, v)| (k.to_lowercase(), v.trim_matches('"').to_string()))).collect())
}

/// Parse a workflow body. Never fails: oddities become problems.
pub fn parse(body: &str) -> (Definition, Vec<Problem>) {
    let mut d = Definition::default();
    let mut problems = Vec::new();
    let mut preamble = String::new();
    let mut fence: Option<&str> = None;
    let mut expect_attrs = false;
    let mut in_acceptance = false;
    for line in body.lines() {
        let t = line.trim_start();
        if let Some(f) = fence {
            if t.starts_with(f) {
                fence = None;
            }
            push_text(&mut d, &mut preamble, line);
            continue;
        }
        if expect_attrs && !t.is_empty() {
            expect_attrs = false;
            if let Some(a) = attrs(t) {
                let s = d.steps.last_mut().expect("step exists");
                for (k, v) in a {
                    match k.as_str() {
                        "id" => s.id = v,
                        "kind" => match StepKind::parse(&v) {
                            Some(k) => s.kind = k,
                            None => problems.push(err(format!("step “{}”: kind must be claude, check or review", s.title), Some(&s.id))),
                        },
                        "needs" => s.needs = split_list(&v),
                        "gates" => s.gates = split_list(&v),
                        "rigor" => s.rigor = Some(v),
                        other => problems.push(warn(format!("step “{}”: unknown attribute `{other}`", s.title), Some(&s.id))),
                    }
                }
                continue;
            }
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(if t.starts_with("```") { "```" } else { "~~~" });
            push_text(&mut d, &mut preamble, line);
            continue;
        }
        if let Some(title) = line.strip_prefix("## ") {
            d.steps.push(Step { title: title.trim().to_string(), ..Default::default() });
            expect_attrs = true;
            in_acceptance = false;
            continue;
        }
        if !d.steps.is_empty() && line.trim().eq_ignore_ascii_case("### acceptance") {
            in_acceptance = true;
            continue;
        }
        if in_acceptance {
            if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
                let item = item.trim_start_matches("[ ] ").trim_start_matches("[x] ").trim();
                if !item.is_empty() {
                    d.steps.last_mut().unwrap().criteria.push(item.to_string());
                }
                continue;
            }
            if t.is_empty() {
                continue;
            }
            in_acceptance = false;
        }
        push_text(&mut d, &mut preamble, line);
    }
    d.preamble = preamble.trim().to_string();
    for s in &mut d.steps {
        s.text = s.text.trim().to_string();
    }
    problems.extend(validate(&d));
    (d, problems)
}

fn push_text(d: &mut Definition, preamble: &mut String, line: &str) {
    let target = match d.steps.last_mut() {
        Some(s) => &mut s.text,
        None => preamble,
    };
    target.push_str(line);
    target.push('\n');
}

pub fn render(d: &Definition) -> String {
    let mut out = String::new();
    if !d.preamble.trim().is_empty() {
        out.push_str(d.preamble.trim());
        out.push_str("\n\n");
    }
    for s in &d.steps {
        out.push_str(&format!("## {}\n", s.title.split_whitespace().collect::<Vec<_>>().join(" ")));
        let mut a = format!("id={} kind={}", s.id, s.kind.as_str());
        if !s.needs.is_empty() {
            a.push_str(&format!(" needs={}", s.needs.join(",")));
        }
        if !s.gates.is_empty() {
            a.push_str(&format!(" gates={}", s.gates.join(",")));
        }
        if let Some(r) = &s.rigor {
            a.push_str(&format!(" rigor={r}"));
        }
        out.push_str(&format!("<!-- step: {a} -->\n\n"));
        if !s.text.trim().is_empty() {
            out.push_str(s.text.trim());
            out.push_str("\n\n");
        }
        if !s.criteria.is_empty() {
            out.push_str("### Acceptance\n\n");
            for c in &s.criteria {
                out.push_str(&format!("- {}\n", c.trim()));
            }
            out.push('\n');
        }
    }
    format!("{}\n", out.trim_end())
}

pub fn valid_id(id: &str) -> bool {
    id.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && id.len() <= 40
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn validate(d: &Definition) -> Vec<Problem> {
    let mut p = Vec::new();
    if d.steps.is_empty() {
        p.push(err("a workflow needs at least one step (`## Step title`)", None));
    }
    if d.steps.len() > MAX_STEPS {
        p.push(err(format!("at most {MAX_STEPS} steps"), None));
    }
    let ids: HashSet<&str> = d.steps.iter().map(|s| s.id.as_str()).collect();
    let mut seen = HashSet::new();
    for s in &d.steps {
        let id = Some(s.id.as_str()).filter(|i| !i.is_empty());
        if !valid_id(&s.id) {
            p.push(err(format!("step “{}”: id must start with a letter and use letters, digits, `-`, `_`", s.title), id));
        } else if !seen.insert(s.id.as_str()) {
            p.push(err(format!("step id `{}` is used twice", s.id), id));
        }
        for n in &s.needs {
            if !ids.contains(n.as_str()) {
                p.push(err(format!("step `{}` needs `{n}`, which does not exist", s.id), id));
            }
        }
        for g in &s.gates {
            if !gates::valid_name(g) {
                p.push(err(format!("step `{}`: `{g}` is not a gate name", s.id), id));
            }
        }
        if s.kind == StepKind::Check && s.gates.is_empty() {
            p.push(err(format!("check step `{}` names no gates", s.id), id));
        }
        if let Some(r) = &s.rigor
            && !RIGOR.contains(&r.as_str())
        {
            p.push(err(format!("step `{}`: rigor must be one of {}", s.id, RIGOR.join(", ")), id));
        }
        if s.title.trim().is_empty() {
            p.push(err("a step has no title", id));
        }
    }
    if p.iter().all(|x| x.severity != "error") && order(d).is_none() {
        p.push(err("the step dependencies form a cycle", None));
    }
    p
}

/// Steps in dependency order (Kahn; definition order breaks ties), or `None` for a cycle.
pub fn order(d: &Definition) -> Option<Vec<String>> {
    let mut indeg: HashMap<&str, usize> = d.steps.iter().map(|s| (s.id.as_str(), s.needs.len())).collect();
    let mut out = Vec::new();
    let mut done: HashSet<&str> = HashSet::new();
    while out.len() < d.steps.len() {
        let next = d.steps.iter().find(|s| !done.contains(s.id.as_str()) && indeg.get(s.id.as_str()) == Some(&0))?;
        done.insert(&next.id);
        out.push(next.id.clone());
        for s in &d.steps {
            if s.needs.iter().any(|n| n == &next.id) {
                *indeg.get_mut(s.id.as_str()).unwrap() -= 1;
            }
        }
    }
    Some(out)
}

// ---- Documents ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub category: String,
    /// Default rigor (work profile) for runs.
    pub rigor: String,
    /// `kit:skill` references activated only for this workflow's sessions.
    pub skills: Vec<String>,
    pub steps: usize,
    pub gates: Vec<String>,
    pub approved: bool,
    pub errors: usize,
    pub status: String,
    pub relative_path: String,
    pub hash: String,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    #[serde(flatten)]
    pub summary: Summary,
    pub definition: Definition,
    pub problems: Vec<Problem>,
    pub body: String,
}

fn load(conn: &Connection, ws: &Workspace, item: &Item) -> Result<Option<View>> {
    let Ok((doc, hash)) = store::read(&item.path) else { return Ok(None) };
    if doc.fm.str("format") != Some(FORMAT) {
        return Ok(None);
    }
    let (definition, problems) = parse(&doc.body);
    let mut all_gates: Vec<String> = definition.steps.iter().flat_map(|s| s.gates.clone()).collect();
    all_gates.sort();
    all_gates.dedup();
    let approved = hash == item.hash && index::is_approved(conn, item)?;
    let summary = Summary {
        id: item.id.clone().unwrap_or_default(),
        title: item.title.clone(),
        category: doc.fm.str("category").unwrap_or("custom").to_string(),
        rigor: doc.fm.str("rigor").unwrap_or(crate::workprofile::DEFAULT).to_string(),
        skills: doc.fm.list("skills"),
        steps: definition.steps.len(),
        gates: all_gates,
        approved,
        errors: problems.iter().filter(|p| p.severity == "error").count(),
        status: doc.status().unwrap_or("active").to_string(),
        relative_path: item.path.strip_prefix(&ws.root_path).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default(),
        hash,
        project_id: item.project_id.clone(),
    };
    Ok(Some(View { summary, definition, problems, body: doc.body }))
}

pub fn list(conn: &Connection, data_dir: &Path, ws: &Workspace) -> Result<Vec<Summary>> {
    crate::knowledge::service::refresh(conn, data_dir, ws)?;
    let mut out = Vec::new();
    for item in index::list(conn, &ws.id, &Filter { kinds: vec![Kind::Workflow], ..Default::default() })? {
        if let Some(v) = load(conn, ws, &item)? {
            out.push(v.summary);
        }
    }
    out.sort_by_key(|s| (s.category.clone(), s.title.to_lowercase()));
    Ok(out)
}

pub fn get(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str) -> Result<View> {
    crate::knowledge::service::refresh(conn, data_dir, ws)?;
    let item = index::get(conn, &ws.id, id)?;
    load(conn, ws, &item)?.ok_or_else(|| CoreError::invalid("not a structured workflow (`format: workflow`)"))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewWorkflow {
    pub title: String,
    pub category: String,
    pub rigor: Option<String>,
    pub skills: Vec<String>,
    /// Start from a built-in template ([`templates::TEMPLATES`]).
    pub template: Option<String>,
    pub project_id: Option<String>,
}

fn check_skills(skills: &[String]) -> Result<()> {
    for s in skills {
        let ok = s.split_once(':').is_some_and(|(k, n)| !k.is_empty() && !n.is_empty() && s.len() <= 120 && !s.contains(char::is_whitespace));
        if !ok {
            return Err(CoreError::invalid(format!("skill `{s}` must be written as kit:skill")));
        }
    }
    Ok(())
}

pub fn create(conn: &Connection, data_dir: &Path, ws: &Workspace, input: &NewWorkflow) -> Result<View> {
    let template = input.template.as_deref().map(|t| templates::get(t).ok_or_else(|| CoreError::invalid(format!("no template `{t}`")))).transpose()?;
    let title = if input.title.trim().is_empty() { template.map(|t| t.title.to_string()).unwrap_or_default() } else { input.title.trim().to_string() };
    if title.is_empty() {
        return Err(CoreError::invalid("a workflow needs a title"));
    }
    let category = if input.category.is_empty() { template.map(|t| t.category).unwrap_or("custom").to_string() } else { input.category.clone() };
    if !CATEGORIES.contains(&category.as_str()) {
        return Err(CoreError::invalid(format!("category must be one of {}", CATEGORIES.join(", "))));
    }
    let rigor = input.rigor.clone().unwrap_or_else(|| crate::workprofile::DEFAULT.to_string());
    if !RIGOR.contains(&rigor.as_str()) {
        return Err(CoreError::invalid(format!("rigor must be one of {}", RIGOR.join(", "))));
    }
    check_skills(&input.skills)?;
    let body = match template {
        Some(t) => t.body.to_string(),
        None => "Describe what this workflow is for.\n\n## Do the work\n<!-- step: id=work kind=claude -->\n\nWhat Claude does.\n".to_string(),
    };
    let mut fields = vec![
        ("format".to_string(), Value::Str(FORMAT.into())),
        ("category".to_string(), Value::Str(category)),
        ("rigor".to_string(), Value::Str(rigor)),
        ("status".to_string(), Value::Str("active".into())),
    ];
    if !input.skills.is_empty() {
        fields.push(("skills".into(), Value::List(input.skills.clone())));
    }
    let written = store::create(&ws.root_path, NewDoc { kind: Kind::Workflow, title, body, project: input.project_id.clone(), privacy: Privacy::Shared, source: "human", fields })?;
    crate::knowledge::service::refresh(conn, data_dir, ws)?;
    let item = index::get(conn, &ws.id, &written.id)?;
    index::approve(conn, &item.path, &item.hash)?; // written by the person, in NEXUS
    get(conn, data_dir, ws, &written.id)
}

/// Save an edited definition (and optionally its settings); errors refuse the save; the person's save is approved.
pub fn save(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str, def: &Definition, settings: Option<(&str, &[String])>, expected_hash: Option<&str>) -> Result<View> {
    if let Some(e) = validate(def).into_iter().find(|p| p.severity == "error") {
        return Err(CoreError::invalid(e.message));
    }
    if let Some((rigor, skills)) = settings {
        if !RIGOR.contains(&rigor) {
            return Err(CoreError::invalid("unknown rigor"));
        }
        check_skills(skills)?;
    }
    let item = index::get(conn, &ws.id, id)?;
    let privacy = index::root_privacy(conn, &item.path)?;
    let body = render(def);
    let w = store::update(&item.path, expected_hash, privacy, |doc| {
        doc.body = body.clone();
        if let Some((rigor, skills)) = settings {
            doc.fm.set("rigor", Value::Str(rigor.into()));
            doc.fm.set("skills", Value::List(skills.to_vec()));
        }
        Ok(())
    })?;
    crate::knowledge::service::refresh(conn, data_dir, ws)?;
    index::approve(conn, &w.path, &w.hash)?;
    get(conn, data_dir, ws, id)
}

pub fn save_raw(conn: &Connection, data_dir: &Path, ws: &Workspace, id: &str, body: &str, expected_hash: Option<&str>) -> Result<View> {
    let (def, problems) = parse(body);
    if let Some(e) = problems.into_iter().find(|p| p.severity == "error") {
        return Err(CoreError::invalid(e.message));
    }
    save(conn, data_dir, ws, id, &def, None, expected_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "Ship a feature.

## Plan
<!-- step: id=plan kind=claude -->

Restate the goal.

### Acceptance

- Plan recorded as a checkpoint

## Implement
<!-- step: id=implement kind=claude needs=plan gates=test,lint -->

Write the code.

```md
## not a step
```

### Acceptance

- [ ] Tests cover it
- Lint is clean

## Release check
<!-- step: id=check kind=check needs=implement gates=build rigor=professional -->

## Sign-off
<!-- step: id=review kind=review needs=check -->
";

    #[test]
    fn parses_steps_dependencies_gates_and_criteria_and_round_trips() {
        let (d, problems) = parse(BODY);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(d.steps.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["plan", "implement", "check", "review"]);
        let i = &d.steps[1];
        assert_eq!((i.needs.clone(), i.gates.clone(), i.criteria.clone()), (vec!["plan".to_string()], vec!["test".to_string(), "lint".to_string()], vec!["Tests cover it".to_string(), "Lint is clean".to_string()]));
        assert!(i.text.contains("## not a step"), "fences stay text");
        assert_eq!((d.steps[2].kind, d.steps[2].rigor.as_deref()), (StepKind::Check, Some("professional")));
        assert_eq!(parse(&render(&d)).0, d);
        assert_eq!(order(&d).unwrap(), ["plan", "implement", "check", "review"]);
    }

    #[test]
    fn validation_catches_cycles_unknown_needs_and_bad_gates() {
        let msgs = |b: &str| parse(b).1.into_iter().map(|p| p.message).collect::<Vec<_>>();
        assert!(msgs("## A\n<!-- step: id=a needs=b -->\n## B\n<!-- step: id=b needs=a -->\n").iter().any(|m| m.contains("cycle")));
        assert!(msgs("## A\n<!-- step: id=a needs=zzz -->\n").iter().any(|m| m.contains("does not exist")));
        assert!(msgs("## A\n<!-- step: id=a kind=check -->\n").iter().any(|m| m.contains("names no gates")));
        assert!(msgs("## A\n<!-- step: id=a gates=rm;-rf -->\n").iter().any(|m| m.contains("not a gate name")));
        assert!(msgs("## A\n<!-- step: id=a -->\n## B\n<!-- step: id=a -->\n").iter().any(|m| m.contains("used twice")));
        assert!(msgs("no steps").iter().any(|m| m.contains("at least one step")));
    }
}

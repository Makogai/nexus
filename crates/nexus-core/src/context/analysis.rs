//! Onboarding analysis (ADR-0028): deterministic facts about a project's context sources and a recommended strategy
//! with reasons, plus what each strategy would append at launch (computed, not guessed).

use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use super::{Settings, Strategy, docs};
use crate::error::Result;
use crate::knowledge::{constitution, context as kctx, service};
use crate::{project, workspace};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub project_id: String,
    pub project: String,
    pub docs: DocsFacts,
    pub instructions: InstructionFacts,
    pub knowledge: kctx::Summary,
    pub constitution_rules: usize,
    pub constitution_mandatory: usize,
    /// Characters each strategy appends to the system prompt for rules and constitutions (excluding the work profile
    /// and task brief, which do not depend on the strategy), and the SessionStart upper bound.
    pub estimates: Vec<Estimate>,
    pub recommendation: Strategy,
    pub reasons: Vec<String>,
    pub configured: Option<Strategy>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocsFacts {
    pub files: usize,
    pub bytes: u64,
    pub sample: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionFacts {
    pub claude_md: bool,
    pub agents_md: bool,
    pub claude_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Estimate {
    pub strategy: Strategy,
    pub rules_chars: usize,
    pub startup_chars_max: usize,
    pub rules_in_prompt: usize,
    pub on_demand: usize,
}

pub fn analyze(conn: &Connection, data_dir: &Path, project_id: &str) -> Result<Analysis> {
    let p = project::get(conn, project_id)?;
    let ws = workspace::get(conn, &p.workspace_id)?;
    if ws.available {
        service::refresh(conn, data_dir, &ws)?;
    }
    let report = match p.local_path.as_ref().filter(|d| d.is_dir()) {
        Some(root) => docs::refresh(conn, &p.id, root)?,
        None => docs::IndexReport::default(),
    };
    let sample = docs::list(conn, &p.id)?.into_iter().take(8).map(|d| d.path).collect();
    let scope = Some((p.id.as_str(), p.stack.stacks.as_slice()));
    let knowledge = kctx::summary(conn, &ws.id, scope)?;
    let eff = constitution::effective_indexed(conn, &ws.id, &constitution::Target { project: scope, ..Default::default() })?;
    let mut estimates = Vec::new();
    for s in [Strategy::Minimal, Strategy::Balanced, Strategy::Deep] {
        let settings = Settings::preset(s);
        let lr = kctx::launch_rules_with(conn, &ws.id, scope, None, None, &kctx::RuleOptions::from(&settings))?;
        estimates.push(Estimate {
            strategy: s,
            rules_chars: lr.text.as_deref().map(|t| t.chars().count()).unwrap_or(0),
            startup_chars_max: settings.startup_chars,
            rules_in_prompt: lr.included.len(),
            on_demand: lr.over_budget.len() + lr.constitution.as_ref().map(|c| c.on_demand.len()).unwrap_or(0),
        });
    }
    let (recommendation, reasons) = recommend(&knowledge, report.bytes, report.files, eff.rules.len(), p.stack.has_claude_md);
    let configured = super::get(conn, &p.id)?;
    Ok(Analysis {
        project_id: p.id.clone(),
        project: p.name.clone(),
        docs: DocsFacts { files: report.files, bytes: report.bytes, sample, skipped: report.skipped },
        instructions: InstructionFacts { claude_md: p.stack.has_claude_md, agents_md: p.stack.has_agents_md, claude_dir: p.stack.has_claude_dir },
        constitution_mandatory: eff.rules.iter().filter(|r| r.enforcement == constitution::Enforcement::Mandatory).count(),
        constitution_rules: eff.rules.len(),
        knowledge,
        estimates,
        recommendation,
        reasons,
        configured: configured.configured.then_some(configured.strategy),
    })
}

/// Rule-based recommendation (explained). Deep for knowledge-rich projects, minimal when Claude's own CLAUDE.md
/// already carries the project and NEXUS has little to add, balanced otherwise.
pub fn recommend(k: &kctx::Summary, doc_bytes: u64, doc_files: usize, constitution_rules: usize, claude_md: bool) -> (Strategy, Vec<String>) {
    let mut reasons = Vec::new();
    let knowledge_items = k.decisions + k.notes + k.specs + k.open_tasks;
    if k.decisions >= 10 || k.open_tasks >= 10 || doc_bytes > 150_000 || constitution_rules >= 20 {
        if k.decisions >= 10 {
            reasons.push(format!("{} decisions are recorded — a startup index helps Claude find them", k.decisions));
        }
        if k.open_tasks >= 10 {
            reasons.push(format!("{} open tasks", k.open_tasks));
        }
        if doc_bytes > 150_000 {
            reasons.push(format!("{} KB of project documentation across {doc_files} files", doc_bytes / 1024));
        }
        if constitution_rules >= 20 {
            reasons.push(format!("{constitution_rules} constitution rules apply — a larger budget keeps more of them in the prompt"));
        }
        return (Strategy::Deep, reasons);
    }
    if claude_md && knowledge_items <= 2 && constitution_rules == 0 && k.always_rules == 0 {
        reasons.push("the project's CLAUDE.md already gives Claude its instructions".into());
        reasons.push("NEXUS knowledge for this project is still small".into());
        return (Strategy::Minimal, reasons);
    }
    reasons.push("a moderate amount of project knowledge: rules at start, everything else on demand".into());
    (Strategy::Balanced, reasons)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommendations_follow_the_facts() {
        let small = kctx::Summary::default();
        assert_eq!(recommend(&small, 0, 0, 0, true).0, Strategy::Minimal);
        assert_eq!(recommend(&small, 0, 0, 0, false).0, Strategy::Balanced);
        let rich = kctx::Summary { decisions: 12, ..Default::default() };
        let (s, reasons) = recommend(&rich, 0, 0, 0, true);
        assert_eq!(s, Strategy::Deep);
        assert!(reasons[0].contains("12 decisions"));
        assert_eq!(recommend(&small, 200_000, 40, 0, false).0, Strategy::Deep);
    }
}

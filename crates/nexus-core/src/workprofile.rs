//! Engineering-rigor work profiles (ADR-0011). Data only: a short instruction fragment appended to
//! Claude's system prompt. Never changes permissions or security settings.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkProfile {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    pub characteristics: &'static [&'static str],
    /// Appended to Claude's system prompt (kept short: it is paid for on every request).
    pub prompt: &'static str,
}

const BUILTINS: &[WorkProfile] = &[
    WorkProfile {
        id: "prototype",
        name: "Prototype",
        summary: "Experiments, tiny apps, game demos. Minimal ceremony.",
        characteristics: &["Short planning", "Focused implementation", "Essential checks only", "No subagents unless asked"],
        prompt: "NEXUS work profile: PROTOTYPE. Optimize for speed of iteration. Plan in a sentence or two, \
                 build the smallest working version, run only essential checks, keep documentation minimal, \
                 and do not spawn subagents or reviews unless asked. Security rules still apply.",
    },
    WorkProfile {
        id: "balanced",
        name: "Balanced",
        summary: "Everyday development with sensible verification.",
        characteristics: &["Lightweight planning", "Targeted tests", "Concise docs", "Optional review"],
        prompt: "NEXUS work profile: BALANCED. Plan briefly, implement cleanly, add or run targeted tests for \
                 what you change, verify the result, and keep documentation concise. Use an independent review \
                 only for risky changes.",
    },
    WorkProfile {
        id: "professional",
        name: "Professional",
        summary: "Maintained, production-oriented projects.",
        characteristics: &["Clear requirements", "Architecture awareness", "Meaningful tests", "Code review", "Acceptance checks"],
        prompt: "NEXUS work profile: PROFESSIONAL. Confirm requirements and acceptance criteria first, respect the \
                 existing architecture, write meaningful tests, review your own diff critically before finishing, \
                 update relevant documentation, and verify every acceptance criterion explicitly.",
    },
    WorkProfile {
        id: "critical",
        name: "Critical",
        summary: "Complex or high-impact changes.",
        characteristics: &["Detailed planning", "Deep analysis", "Thorough testing", "Specialist review", "Explicit risk analysis"],
        prompt: "NEXUS work profile: CRITICAL. Analyze the affected code deeply before changing it, write a plan \
                 with explicit risks and rollback, test thoroughly (including failure paths), obtain an independent \
                 or specialist review for security- or data-sensitive parts, and verify acceptance criteria with \
                 evidence before declaring completion.",
    },
];

pub const DEFAULT: &str = "balanced";

pub fn builtins() -> &'static [WorkProfile] {
    BUILTINS
}

pub fn get(id: &str) -> Option<&'static WorkProfile> {
    BUILTINS.iter().find(|w| w.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_short_and_unique() {
        let ids: Vec<_> = builtins().iter().map(|w| w.id).collect();
        assert_eq!(ids, ["prototype", "balanced", "professional", "critical"]);
        for w in builtins() {
            // ~4 chars per token: keep each fragment comfortably under ~120 tokens.
            assert!(w.prompt.len() < 520, "{} prompt too long ({})", w.id, w.prompt.len());
            assert!(!w.prompt.contains('\n'));
        }
        assert!(get(DEFAULT).is_some());
        assert!(get("nope").is_none());
    }
}

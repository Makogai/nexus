//! Context Manager (ADR-0028, the Context subsystem of ADR-0026): how much NEXUS gives each Claude session, from which
//! sources, and what actually happened. Per project, machine-local (a developer chooses their own depth):
//!
//! * a **strategy** — minimal, balanced (the default; NEXUS's behaviour before this module), deep, or custom — that
//!   resolves to concrete [`Settings`] (sources, budgets);
//! * a rebuildable index of the project's own documentation ([`docs`]) for targeted retrieval with citations;
//! * the **Context Inspector** ([`inspector`]): configured / predicted / observed / available on demand / retrieved,
//!   and retrieval metrics.
//!
//! Everything NEXUS appends stays bounded (CLAUDE.md rule: never histories or whole knowledge).

pub mod analysis;
pub mod docs;
pub mod inspector;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::now_ms;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Strategy {
    Minimal,
    #[default]
    Balanced,
    Deep,
    Custom,
}

impl Strategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Strategy::Minimal => "minimal",
            Strategy::Balanced => "balanced",
            Strategy::Deep => "deep",
            Strategy::Custom => "custom",
        }
    }
    pub fn parse(s: &str) -> Option<Strategy> {
        Some(match s {
            "minimal" => Strategy::Minimal,
            "balanced" => Strategy::Balanced,
            "deep" => Strategy::Deep,
            "custom" => Strategy::Custom,
            _ => return None,
        })
    }
}

/// Which constitution rules go into the system prompt at launch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConstitutionDelivery {
    /// Mandatory policies only; everything else on demand.
    Mandatory,
    /// Mandatory policies and rules (guidelines on demand).
    Rules,
    /// Nothing at start; all on demand through `get_constitution`.
    OnDemand,
}

/// Concrete context settings for a project (what a strategy resolves to).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Approved always-on rule documents are appended at launch (else named for on-demand reading).
    pub always_rules: bool,
    pub constitution: ConstitutionDelivery,
    /// Characters for rules + constitution in the system prompt.
    pub rules_budget: usize,
    /// SessionStart: one-line knowledge summary and checkpoint pointer.
    pub startup_summary: bool,
    /// SessionStart: titles of open tasks and recent decisions of the project (escaped data).
    pub startup_index: bool,
    /// SessionStart: the latest checkpoints of the project on a fresh start.
    pub checkpoints_at_start: bool,
    /// Upper bound for SessionStart context.
    pub startup_chars: usize,
    /// The project's README/docs are searchable through the NEXUS MCP tools.
    pub project_docs: bool,
    /// Record which instruction files Claude loads (InstructionsLoaded hook) for the Context Inspector.
    pub observe_instructions: bool,
    /// Retrieval bounds for MCP search / documents.
    pub retrieval_limit: usize,
    pub retrieval_chars: usize,
}

impl Settings {
    pub fn preset(s: Strategy) -> Settings {
        match s {
            Strategy::Minimal => Settings {
                always_rules: false,
                constitution: ConstitutionDelivery::Mandatory,
                rules_budget: 600,
                startup_summary: false,
                startup_index: false,
                checkpoints_at_start: false,
                startup_chars: 900,
                project_docs: true,
                observe_instructions: false,
                retrieval_limit: 5,
                retrieval_chars: 4_000,
            },
            Strategy::Balanced | Strategy::Custom => Settings {
                always_rules: true,
                constitution: ConstitutionDelivery::Rules,
                rules_budget: 1_500,
                startup_summary: true,
                startup_index: false,
                checkpoints_at_start: false,
                startup_chars: 1_800,
                project_docs: true,
                observe_instructions: true,
                retrieval_limit: 10,
                retrieval_chars: 20_000,
            },
            Strategy::Deep => Settings {
                always_rules: true,
                constitution: ConstitutionDelivery::Rules,
                rules_budget: 3_000,
                startup_summary: true,
                startup_index: true,
                checkpoints_at_start: true,
                startup_chars: 3_000,
                project_docs: true,
                observe_instructions: true,
                retrieval_limit: 20,
                retrieval_chars: 40_000,
            },
        }
    }

    /// Clamp custom values to NEXUS's hard bounds.
    pub fn bounded(mut self) -> Settings {
        self.rules_budget = self.rules_budget.clamp(300, 4_000);
        self.startup_chars = self.startup_chars.clamp(300, 4_000);
        self.retrieval_limit = self.retrieval_limit.clamp(1, 20);
        self.retrieval_chars = self.retrieval_chars.clamp(1_000, 40_000);
        self
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings::preset(Strategy::Balanced)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub project_id: String,
    pub strategy: Strategy,
    pub settings: Settings,
    /// `false` until someone chose a strategy for this project (the wizard is offered).
    pub configured: bool,
    pub updated_at: Option<i64>,
}

/// The project's configuration (balanced defaults when none was chosen).
pub fn get(conn: &Connection, project_id: &str) -> Result<Config> {
    let row: Option<(String, Option<String>, i64)> = conn
        .query_row("SELECT strategy, custom, updated_at FROM context_configs WHERE project_id = ?1", [project_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    Ok(match row {
        None => Config { project_id: project_id.into(), strategy: Strategy::Balanced, settings: Settings::default(), configured: false, updated_at: None },
        Some((s, custom, at)) => {
            let strategy = Strategy::parse(&s).unwrap_or_default();
            let settings = match (strategy, custom) {
                (Strategy::Custom, Some(j)) => serde_json::from_str::<Settings>(&j).map(Settings::bounded).unwrap_or_default(),
                (s, _) => Settings::preset(s),
            };
            Config { project_id: project_id.into(), strategy, settings, configured: true, updated_at: Some(at) }
        }
    })
}

/// Settings for an optional project (sessions without a project get the balanced defaults).
pub fn settings_for(conn: &Connection, project_id: Option<&str>) -> Settings {
    project_id.and_then(|p| get(conn, p).ok()).map(|c| c.settings).unwrap_or_default()
}

pub fn set(conn: &Connection, project_id: &str, strategy: Strategy, custom: Option<Settings>) -> Result<Config> {
    crate::project::get(conn, project_id)?;
    let custom = match strategy {
        Strategy::Custom => {
            let s = custom.ok_or_else(|| CoreError::invalid("custom strategy needs settings"))?.bounded();
            Some(serde_json::to_string(&s).map_err(|source| CoreError::Json { what: "context settings".into(), source })?)
        }
        _ => None,
    };
    conn.execute(
        "INSERT INTO context_configs(project_id, strategy, custom, updated_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(project_id) DO UPDATE SET strategy = excluded.strategy, custom = excluded.custom, updated_at = excluded.updated_at",
        params![project_id, strategy.as_str(), custom, now_ms()],
    )?;
    get(conn, project_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_defaults_custom_bounds_and_round_trip() {
        let conn = crate::db::open_in_memory().unwrap();
        let t = tempfile::tempdir().unwrap();
        let ws = crate::workspace::create(&conn, t.path().join("ws").to_str().unwrap(), "W", "").unwrap();
        std::fs::create_dir_all(t.path().join("p")).unwrap();
        let p = crate::project::register(&conn, &ws.id, t.path().join("p").to_str().unwrap(), None).unwrap();
        let c = get(&conn, &p.id).unwrap();
        assert_eq!((c.strategy, c.configured), (Strategy::Balanced, false));
        assert_eq!(c.settings, Settings::preset(Strategy::Balanced), "unconfigured projects keep the earlier behaviour");
        let deep = set(&conn, &p.id, Strategy::Deep, None).unwrap();
        assert!(deep.configured && deep.settings.startup_index && deep.settings.rules_budget > c.settings.rules_budget);
        let wild = Settings { rules_budget: 1_000_000, retrieval_limit: 0, ..Settings::preset(Strategy::Minimal) };
        let custom = set(&conn, &p.id, Strategy::Custom, Some(wild)).unwrap();
        assert_eq!((custom.settings.rules_budget, custom.settings.retrieval_limit, custom.settings.always_rules), (4_000, 1, false));
        assert!(set(&conn, &p.id, Strategy::Custom, None).is_err());
        assert!(set(&conn, "no-such-project", Strategy::Deep, None).is_err());
        assert_eq!(settings_for(&conn, None), Settings::default());
    }
}

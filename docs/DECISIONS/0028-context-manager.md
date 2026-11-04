# ADR-0028 — Context Manager: per-project strategies, cited retrieval from project docs, and an inspector built on observation

Status: Accepted, implemented (2026-11-04, stage milestones E and F)

## Context
NEXUS gives sessions a bounded amount of context (ADR-0017): approved always-on rules, constitutions (ADR-0027), a
task brief, a SessionStart pointer, and everything else through MCP. Projects differ: a small repo with a good
CLAUDE.md needs almost nothing from NEXUS; a knowledge-rich one benefits from an index at start. The owner asks for
onboarding analysis and a wizard, strategies (minimal / balanced / deep / custom), configurable sources, targeted
retrieval with budgets, dedupe, freshness, citations and branch awareness, and an inspector that separates what is
configured, observed, predicted, available on demand and actually retrieved — with metrics.

## Decision
* **Strategy per project** (`context_configs`, migration 0011, machine-local: each developer chooses their depth)
  resolves to concrete `Settings`: always-on rules at launch or on demand; constitution delivery (rules / mandatory
  only / on demand); rule budget; SessionStart summary, project index (titles of open tasks and decisions, escaped
  data) and recent checkpoints; SessionStart limit; project docs as a source; instruction observation; retrieval
  limits. **Balanced is the default and equals NEXUS's earlier behaviour**, so unconfigured projects are unchanged.
  Custom values are clamped to hard bounds (rules ≤ 4,000 chars, SessionStart ≤ 4,000, ≤ 20 results, ≤ 40,000
  chars per document slice).
* **Onboarding analysis** (deterministic): docs found, CLAUDE.md presence, knowledge counts, constitution rules,
  and — computed, not estimated — what each strategy would append; a rule-based recommendation with reasons
  (deep for knowledge-rich projects, minimal when CLAUDE.md carries the project and NEXUS has little to add).
* **Project docs** as a retrieval source: README and Markdown under conventional docs folders (≤ 256 KiB per file,
  ≤ 300 files, ≤ 8 MiB; no symlinks; files that look like they contain credentials are skipped), indexed into
  SQLite FTS (rebuildable). `search_knowledge` merges knowledge and doc hits, deduplicates by content hash, and gives
  each a citation (`path`, or `path:line § heading` — the line matching most terms) and its last change;
  `get_document { path, fromLine }` returns a bounded slice of an **indexed** doc only (no arbitrary file reads,
  ADR-0009). The working tree is indexed, so results follow the checked-out branch.
* **Observation instead of estimation.** With observation on (balanced/deep), the session-scoped settings subscribe
  `InstructionsLoaded`; `nexus hook` records each file Claude reports (absolute path, memory type, load reason —
  machine-local). Retrieval tools record result counts, sizes and document ids — never the query text.
* **Inspector**: *configured* (strategy and settings), *predicted* (instruction layers for a chosen profile + what
  NEXUS appends, ADR-0019), *observed* (the latest session: each loaded file marked predicted or unexpected, predicted
  files that were not reported, what NEXUS appended), *on demand* (knowledge counts, project docs, constitution rule
  ids, tools), *retrieved* (calls of the latest session), and metrics over recent sessions (calls, empty results,
  characters, by tool, most retrieved documents).
* **Integration (milestone F).** The same observation detects cross-profile instruction bleed live (threat T21): a
  file from another profile's configuration folder (including the default `~/.claude`) in a session raises one
  warning notification per file (ADR-0025) that points at instruction isolation (ADR-0020). Constitution delivery
  follows the strategy. Review notifications for rule changes open the Constitution Manager.
* **Surfaces**: desktop *Context* view (project page → Context: analysis, recommendation, strategy cards with
  computed sizes, custom sources and budgets, project docs; Inspector; Retrieval metrics); `nexus context
  show|set|analyze|inspect`; the existing MCP tools (no new tool).

## Verification
Core tests (presets/bounds, docs index + citations + indexed-only reads + credential skip, recommendation, startup
per strategy, recording/metrics, bleed detection); CLI `context_strategies_docs_retrieval_and_the_inspector`;
vitest `context.test.ts`; desktop `e2e/context.mjs` (10 checks). Real Claude 2.1.311 with synthetic profiles:
`e2e/context-real-claude.mjs` — the CLI reported exactly the four instruction files the inspector predicted (user
memory, project CLAUDE.md, CLAUDE.local.md, a project rule), none missing; `e2e/security-real-claude.mjs` — the
unisolated control run raises the bleed warning for the three default-profile files, the isolated run none.

## Alternatives
* A portable (synced) strategy — teams may want that later; a per-developer preference is the safer default.
* Embedding-based retrieval — non-deterministic, needs a model or a heavy local index; FTS with citations first.
* Estimating loaded instructions from file sizes — the hook reports what actually loaded; the estimate stays the
  prediction it is compared with.

## Consequences
* InstructionsLoaded adds one short `nexus hook` process per instruction file at session start (off in minimal).
* Sessions started outside NEXUS are not observed (no NEXUS hooks).
* Doc search covers Markdown only; source code stays Claude's own tools' job.

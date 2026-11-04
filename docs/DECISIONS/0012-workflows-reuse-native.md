# ADR-0012 — Workflows reuse native skills/subagents; no custom orchestrator

Status: Accepted (implementation Phase 4)

## Decision
* A NEXUS workflow is a versioned definition (steps, gates, required tools, budgets) rendered as a **skill**
  that Claude executes, plus deterministic steps NEXUS itself runs (e.g. a test command) through the process
  manager. Claude reports progress through `checkpoint_session` / `update_task`.
* Dynamic multi-agent execution uses Claude Code's native subagents/workflows. NEXUS observes and records; it
  does not schedule agents.
* Independent review uses an *adapter*: `codex review --uncommitted` in an isolated worktree, with findings
  captured as task activity. (The locally installed "codex-harness" plugin delegates asset generation, not
  review, so it is complementary rather than a replacement.)

## Consequences
* No second agent runtime to maintain; NEXUS benefits automatically from Claude Code improvements.

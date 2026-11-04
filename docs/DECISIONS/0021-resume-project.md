# ADR-0021 — Resume project: verified state, saved history, one evidence-based next step

Status: Accepted, implemented (2026-11-04, M18)

## Context
Coming back to a project after hours or days, a developer needs to know where it stands and what to do next. The
answer must be concise, evidence-based, cheap in context, and honest about what was checked now versus what was
recorded earlier (which may be stale). No model should guess.

## Decision
* `nexus_core::resume::summarize(project)` returns three parts:
  * **verified** — read from Git now: branch / detached, HEAD commit, upstream ahead/behind (as of the last fetch —
    NEXUS never fetches), staged/modified/untracked/conflicted counts, the last five commits. Git stays authoritative.
  * **saved** — history, not re-verified: open tasks of the project (in-progress → blocked → review → ready →
    backlog, ≤ 8, checklist progress and last log entry), notes and decisions relevant to the checked-out branch
    (ADR-0018; other branches only counted), the latest five checkpoints (flagged when recorded on another branch),
    the last three sessions (with whether the conversation can be resumed), pending memory proposals (ADR-0022).
  * **next** — one recommended action from fixed rules over those facts: resolve conflicts → resume the latest
    resumable session of an in-progress/blocked/review task → unblock a blocked task → continue an in-progress task →
    review uncommitted changes → pull (hint only) → review a task in review → start the best ready/backlog task →
    launch Claude to plan. Every evidence line is prefixed `verified:` or `saved:`.
* Warnings surface uncertainty: folder missing, not a Git repository, checkpoints from another branch, the last
  session was interrupted, the recommended task is linked to another branch (NEXUS never switches branches).
* All text from files or from Claude is escaped and clipped; the summary is bounded (a few KB).
* Surfaces: desktop **Resume project** view (project page button, command palette) with the recommended action as
  the primary button (Resume session / Launch Claude for the task / Launch Claude) and per-item Launch/Resume;
  `nexus projects resume [project]` (prints the command to run); MCP `get_resume_summary` (read-only).
* Hosts (desktop, CLI) first run the deterministic proposal extraction (ADR-0022) so sessions that ended without
  hooks — e.g. NEXUS was closed — are not missed; the MCP tool does not write.

## Alternatives
* Model-written summaries — costly, non-deterministic and indistinguishable from facts; rejected.
* Storing a "project status" record — duplicates Git and the task files and goes stale.
* Auto-running `git fetch` — a network write-ish action without consent; NEXUS reports "as of the last fetch".

## Consequences
* Recommendations are predictable and testable (`resume::tests` covers each rule) but deliberately simple.
* Freshness depends on the user's own fetches; the view says so.

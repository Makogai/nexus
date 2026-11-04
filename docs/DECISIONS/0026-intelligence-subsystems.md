# ADR-0026 — Five intelligence subsystems with separate responsibilities and shared infrastructure

Status: Accepted (2026-11-04, stage "Intelligence, Constitutions, Context & Notifications", milestone A)

## Context
NEXUS already has a knowledge engine, always-on rules, tasks, checkpoints, memory proposals, kits, an MCP server,
hooks and an instructions inspector. The next stage adds a Constitution Manager and a Context Manager and must not
rebuild working functionality. The owner asks to keep **Constitutions, Context, Memory, Skills and Workflows**
conceptually separate even where they share infrastructure.

## Decision
Each subsystem answers one question and owns its rules of change. Shared infrastructure stays shared.

| Subsystem | Question | Owns | Built on (existing) |
| --- | --- | --- | --- |
| **Constitutions** | What must / should Claude do here? (normative) | constitution documents: articles → sections → rules with stable ids, enforcement (mandatory policy / rule / guideline), scope (global, workspace, stack, project, task/workflow), inheritance and overrides, approval, effective-rule computation, conflicts | knowledge kind `rule` (ADR-0013/0017) and its hash approval; a constitution is a rule document with structure — no second store |
| **Context** | What does a session see, when, and at what cost? | strategies (minimal / balanced / deep / custom), source configuration per project, retrieval budgets, the Context Inspector (configured / observed loaded / predicted / on demand / retrieved), retrieval metrics | `claude::prepare` (the only builder of session context), `knowledge::context` (hierarchy, lazy index, FTS), inspector (ADR-0019), `InstructionsLoaded` hook |
| **Memory** | What did we learn? (facts, history) | notes, decisions, checkpoints, branch relevance, proposals and their review | `knowledge::service` / `store`, `checkpoint`, `proposals` (ADR-0018/0021/0022) |
| **Skills** | What can Claude do? (capabilities) | kits, per-profile selection, diagnostics inventory | `kits`, `claude::diagnostics` (ADR-0023/0024) |
| **Workflows** | How is a recurring procedure run? | workflow documents now; the engine is **deferred** (ADR-0012: reuse native skills/subagents, no custom orchestrator) | knowledge kind `workflow` |

Interfaces between them are narrow:
* Constitutions → Context: `effective_rules(scope) -> [Rule]` (ids, enforcement, text, provenance); Context decides
  how much of it reaches a session (always-on only for approved mandatory rules within budget; the rest on demand
  through MCP).
* Memory → Context: search and get by id (FTS, branch relevance); Context never copies whole histories.
* Skills → Context: per-profile overlay paths and token estimates; Context reports them, Skills decides selection.
* Workflows → (later) Tasks + Skills: a run is a task with the workflow's steps and a session-scoped skill.
* All subsystems → Notifications (ADR-0025) through `notify::publish` (e.g. a rule that needs approval, a changed
  kit, review queues). Activity facts keep using `session_events`.

Shared infrastructure: Markdown + frontmatter files (`knowledge::store`, validated, atomic, optimistic), the SQLite
index (rebuildable), content-hash approval, `claude::prepare`, the MCP server (ADR-0009 bounds), hooks (ADR-0016).

## Consequences
* New modules are thin: `knowledge::constitution` (parse/validate structure, effective rules, conflicts) and a
  context configuration module — both on the existing store/index, so files stay authoritative and portable.
* The desktop gets dedicated views (Constitutions, Context) instead of growing the Knowledge view; Memory stays in
  Knowledge (notes/decisions/proposals), Skills in AI Kits.
* Workflows remain documents until a separate decision; nothing in this stage schedules agents.

# ADR-0017 — Context hierarchy and lazy context loading

Status: Accepted, implemented (2026-11-04, M16 + M19 core)

## Context
The original NEXUS spec describes a context hierarchy: global constitutions, workspace rules, stack-specific rules,
project rules, skills, workflows, specifications, decisions, tasks and persistent memory. Loading all of it into
every session would waste context, slow Claude down and turn every synced workspace file into prompt text. The
owner asked for **lazy loading**: Claude retrieves only what is relevant. ADR-0013 already fixes the file format,
identity, atomic writes, index and privacy boundaries; this ADR maps the hierarchy onto it and decides what reaches
Claude when.

## Decision

### Where each layer lives

| Layer | Storage | Kind / location |
| --- | --- | --- |
| Global constitution | machine-local, NEXUS data dir | `<data>/global/rules/<id>.md` (also `workflows/`, `knowledge/notes/`) — applies to every workspace on this machine |
| Workspace rules | workspace folder (shared) | `rules/<id>.md`, no `project`, no `stacks` |
| Stack rules | workspace folder | `rules/<id>.md` with `stacks: [rust, vue]` — apply to projects whose detected stack intersects |
| Project rules | workspace folder | `rules/<id>.md` with `project: <project id>` |
| Skills | native Claude format | workspace `skills/<name>/SKILL.md`, project `.claude/skills/<name>/SKILL.md` — catalogued (name, description), never re-implemented (ADR-0012) |
| Workflows | workspace folder | `workflows/<id>.md` (kind `workflow`, status `active`/`draft`/`archived`) |
| Specifications | workspace folder | `knowledge/specs/<id>.md` (kind `spec`) |
| Decisions | workspace folder | `knowledge/decisions/<id>.md` (status `proposed`/`accepted`/`superseded`) |
| Tasks | workspace folder | `tasks/<id>.md` (ADR-0013 schema) |
| Persistent memory | workspace folder | `knowledge/notes/<id>.md` (`verification`), plus checkpoints and session activity in SQLite (ADR-0016) |

Every layer can also be `privacy: local-only` under `.nexus-local/` (same layout, git-ignored). Rules carry
`applies: always | on-demand` (default on-demand). Precedence is the order above: later layers are more specific
and are listed after earlier ones.

### What reaches Claude, and when

1. **At launch — approved always-on rules only.** The effective `applies: always` rules for the session's project
   (global → workspace → stack → project) are appended to `--append-system-prompt`, after the work profile, within
   a **1,500-character budget**. Rules that do not fit are named (title + id) so Claude can fetch them. The outcome
   (included / pending approval / over budget) is recorded in `sessions.integrations.rules`.
2. **At SessionStart — a pointer, never content.** The hook's additional context gains one line of counts
   ("NEXUS knowledge: 3 rules, 4 open tasks, 2 decisions.") and the tool names. It reads only the index (no file
   access) so the hook stays fast and deterministic.
3. **On demand — MCP tools** (workspace-scoped, bounded, ADR-0009):

   | Tool | Returns |
   | --- | --- |
   | `get_context_index` | counts, applicable rules (scope, delivery), open tasks, decisions, workflows, specs and skills — **titles and ids only**, plus index problems |
   | `search_knowledge` | FTS5 hits (bm25, title-weighted) with short snippets; the query is reduced to plain terms |
   | `get_document` | one document's fields and body (≤ 20,000 chars) and its content hash |
   | `list_tasks` / `create_task` / `update_task` | task board access; `update_task` takes `expectedHash` (optimistic concurrency) |
   | `record_memory` | a note (`verification: unverified`) or a decision (`status: proposed`) |

   Claude may create **tasks, notes and decisions** only. Rules, specs, workflows and global documents are written
   by people (desktop, CLI, editor) — Claude cannot widen its own always-on context.

### Trust of always-on rules (threat T19)
A synced workspace is untrusted input: a collaborator (or a compromised remote) can add or change a rule file. An
always-on **workspace** rule is delivered only when its *current content hash* was approved on this machine
(`knowledge_approvals`). Rules a person creates in NEXUS are approved on creation; editing a file withdraws its
approval until re-approved; the launch re-reads the file and compares it with the approved hash right before use.
Global documents live in the user's own data dir and need no approval. On-demand content is not approved because
Claude reads it as tool output, never as system prompt.

### Index refresh
The index is refreshed (incremental: size + mtime → hash → parse) on every launch, before MCP reads, after every
NEXUS write, when the desktop Knowledge view is shown or the window regains focus, and on demand
(`nexus knowledge reindex [--rebuild]`, "Rebuild index"). Invalid files and duplicate ids are reported (CLI
`--invalid`, desktop "Problems" tab, `get_context_index.indexProblems`) and stay invisible to Claude.

## Alternatives
* **Inject everything at SessionStart** — simple, but unbounded context, no relevance, and synced text becomes
  system prompt. Rejected.
* **Write rules into the project's `CLAUDE.md` / `.claude/rules`** — Claude would load them natively, but NEXUS
  would modify repositories and Claude config surfaces it does not own (ADR-0004). Rejected.
* **Embeddings / semantic search** — needs a model or a heavy local dependency; FTS5 is deterministic, offline and
  enough for titles and short documents. Revisit if relevance is insufficient.
* **A full YAML parser** — bigger attack surface (anchors, tags, billion laughs); the strict subset covers the
  schema and reports anything else as invalid (ADR-0013).

## Consequences
* Session start cost is one bounded prompt block plus one line; everything else costs context only when used.
* Approvals are per machine: after cloning a workspace on a new machine its always-on rules must be reviewed once.
* Skills stay native Claude skills; NEXUS only lists them so Claude (and the user) can see what exists.
* Still open: branch-aware notes (M17), CLAUDE.md layers in the effective-instructions inspector (M20), launching
  Claude from a task (M19), manifest `sync`/`classification` keys and the pre-push secret scan (ADR-0013, Phase 6).

# NEXUS Roadmap

Each milestone is a vertical slice that ends **tested and usable**. Status: ✅ done · 🚧 in progress · ⏳ next · ☐ planned.
Keep this file and `CURRENT_STATE.md` in sync at the end of every working session.

## MVP definition (end of Phase 1)

On Windows (macOS/Linux compile and run, but are not yet polished), a developer can:

1. Launch NEXUS Desktop; the SQLite database is created and migrated in the local data dir.
2. Create a local workspace (or import an existing workspace folder). `nexus-workspace.json` is written.
3. Register an existing local Git repository as a project. NEXUS detects stack, package manager and scripts
   deterministically and shows branch, ahead/behind and change counts from `git`.
4. See detected Claude installations and Claude profiles (default `~/.claude` and `~/.claude-*` dirs),
   register one manually, or create a new empty NEXUS-managed profile. Each profile shows auth status
   (`claude auth status --json`) and whether organisation policy applies.
5. Pick project + Claude profile + work profile (+ optional native effort) and launch Claude Code in the
   integrated terminal: real PTY, correct `CLAUDE_CONFIG_DIR` (removed for the default profile), scrubbed
   environment, NEXUS-assigned session ID.
6. Open plain shell terminals (PowerShell / cmd / Git Bash) in a project, with multiple tabs and resizing.
7. Close the session or the whole app, reopen NEXUS, and see the session as *exited* or *interrupted* with a
   working **Resume** (`claude --resume <id>` under the same profile and directory) when a conversation exists.

Out of MVP scope: MCP, hooks, tasks, memory, kits, process manager, usage, sync, packaging.

## Phase 1 — Desktop foundation (MVP)

| # | Milestone | Acceptance check | Status |
| --- | --- | --- | --- |
| M0 | Discovery, architecture, ADRs, threat model, roadmap | Docs reviewed; environment recorded in `docs/RESEARCH.md` | ✅ |
| M1 | Cargo workspace + `nexus-core` storage: data dir, SQLite, migrations | `cargo test -p nexus-core db::` (fresh DB, re-open, newer-version refusal) | ✅ |
| M2 | Workspaces: create, import, list, activate; manifest atomic I/O; manifest→DB reconcile | unit tests on temp dirs; `.gitignore` excludes `repos/` | ✅ |
| M3 | Projects: register path, stack detection, git status (porcelain v2) | tests with synthetic fixtures + real temp git repos (incl. hostile fsmonitor config) | ✅ |
| M4 | Claude discovery: executables + version, profile detection, policy detection, auth status, profile CRUD | tests against synthetic home/config dirs | ✅ |
| M5 | Launch spec builder + env hygiene + work profiles | pure unit tests (default vs explicit dir, scrub, resume, args) | ✅ |
| M6 | Desktop shell: Tauri app, layout, theming, onboarding, workspaces/projects/profiles views | `npm run build`, vitest component tests, app launches | ✅ |
| M7 | Integrated terminal: PTY manager, xterm pool, shells, tabs, resize, ring-buffer re-attach | PTY integration test (echo round trip) | ✅ |
| M8 | Claude sessions: launch, records, exit tracking, startup reconcile, resume, stop | `fake-claude` PTY integration test asserts argv/env; manual run with a real profile | ✅ |
| M9 | Reliability baseline: DB migration/recovery tests + quarantine, env-isolation regression suite, frontend component tests, release-like E2E in CI, session hosts (ADR-0014) | all suites green locally; CI job runs the smoke test on a debug release build | ✅ |

## Phase 2 — Claude integration depth

| # | Milestone | Acceptance check |
| --- | --- | --- |
| M10 | `nexus` CLI: `doctor`, `workspaces`, `projects`, `profiles`, `sessions`, `launch` (reuses core; `launch` is a session host) | 5 end-to-end tests of the real binary incl. killed-host reconciliation ✅ |
| M11 | Session hosts + leases ✅ (ADR-0014); still open: reconciliation with `claude agents --json`, durable-session experiment (`--bg` + `attach`) | ADR update with measured behaviour |
| M12 | MCP server v1 (workspace-scoped tools incl. `checkpoint`; 19 tools after M16–M19, M18, `request_attention` and `get_constitution`) via per-session `--mcp-config`; policy-aware omission; checkpoints live in the desktop | unit + stdio E2E + official-SDK interop + desktop E2E ✅ |
| M13 | Lifecycle hooks via `--settings` (exec form → `nexus hook`): start/end, compaction recovery, activity counters, files, command categories, attention; policy-aware | merge semantics verified on 2.1.311; unit + CLI + desktop E2E + real-Claude hook test ✅ |
| M14 | Profile diagnostics (ADR-0023): executable/version, policy effects, settings (allowlisted), memory, skills, agents, commands, MCP servers, hooks, plugins, marketplaces, kits, context estimate, warnings; desktop view + `nexus profiles diagnose` — allowlist clone ☐ | fixture tests + real-CLI comparison with the init event ✅ |
| M15 | WSL shells; login-shell PATH resolution on macOS/Linux | per-platform CI job |

## Phase 3 — Knowledge and continuity

| # | Milestone | Acceptance check |
| --- | --- | --- |
| M16 | Workspace knowledge files (ADR-0013) + context hierarchy and lazy loading (ADR-0017): rules (global/workspace/stack/project, always-on with approval), decisions, notes, specs, workflows, skills catalog; FTS5 index with rebuild and health; MCP `get_context_index`/`search_knowledge`/`get_document`/`record_memory`; `nexus knowledge`; desktop Knowledge view | unit + CLI E2E (round trip, hierarchy, approval, cross-workspace) + SDK interop + desktop E2E ✅ |
| M17 | Branch-aware notes and session checkpoints (ADR-0018): `branch`/`commit` on documents, relevance from the live Git history (this branch / inherited / other branch), `record_memory` scope `branch`, task progress as checkpoints | CLI E2E with checkout + merge; desktop E2E ✅. Proposed memories from session activity with human approval (ADR-0022) ✅ |
| M18 | Resume project (ADR-0021): verified Git state vs saved history (tasks, branch notes, decisions, checkpoints, sessions, pending proposals) + one rule-based next step with evidence; desktop view with direct resume/launch, `nexus projects resume`, MCP `get_resume_summary` | rule unit tests + Git integration + CLI E2E + desktop E2E after a restart ✅ |

## Phase 4 — Engineering productivity

| # | Milestone | Acceptance check |
| --- | --- | --- |
| M19 | Tasks as Markdown files + Kanban board; launch Claude from a task (project, profile, work profile, brief; desktop + `nexus launch --task`); MCP `get_task`/`update_task` (status, log, checklist, branch link) with optimistic concurrency; resume keeps the task (ADR-0018) | CLI E2E + desktop E2E with a NEXUS restart (`e2e/task-flow.mjs`) ✅ |
| M20 | Effective-instructions inspector (ADR-0019): managed/user/ancestor/project/local CLAUDE.md, rules, imports, excludes in Claude's order; NEXUS append with duplicate suppression; `nexus instructions`, desktop panel | fixture tests + real-CLI comparison (`e2e/instructions-real-claude.mjs`) ✅ |
| M21 | AI Kit & Skill Manager (ADR-0024; includes M23): local/Git kits, inventory, Claude validator, hash approval, per-profile enablement with skill selection, hooks/MCP opt-in, conflicts, policy-aware session-scoped `--plugin-dir` overlays; desktop view + `nexus kits` — NEXUS's own workflow skills ☐ | unit + CLI E2E + desktop E2E + real-CLI per-profile check ✅ |
| M22 | Project creation wizard + stack templates | template snapshot tests |

## Phase 5 — Advanced integration

| # | Milestone | Acceptance check |
| --- | --- | --- |
| M23 | ~~AI Kit registry~~ — delivered in M21 | — |
| M24 | Process manager: approved (hash-pinned) commands, logs, process trees (Job Objects / process groups) | Scenario J; T6 tests |
| M25 | Graphify adapter (detect graph, freshness, `graphify update`, MCP pass-through) | fixture graph |
| M26 | Codex review adapter (`codex review` in isolated worktree) | Scenario M |
| M27 | Usage analytics with provenance | Scenario L; "unavailable" for org profiles |
| M28 | Playwright integration | sample project |

## Stage: Intelligence, Constitutions, Context & Notifications (owner specification, 2026-11-04)

Branch `milestone/attention-constitutions-context`. Order set by the owner: notifications first, then the
Constitution Manager, then the Context Manager. No workflow engine in this stage.

| # | Milestone | Acceptance check | Status |
| --- | --- | --- | --- |
| A | Architecture alignment: five subsystems (Constitutions, Context, Memory, Skills, Workflows) mapped to existing modules, narrow interfaces, unified event sink (ADR-0026, ADR-0025) | ADRs reviewed against the code | ✅ |
| B | Notification infrastructure: typed events, SQLite persistence (migration 0010), dedupe/rate limits/retention, notification center (bell, counts, inbox/history, filters, read/dismiss/snooze/open), preferences, `nexus notifications` | core + CLI + vitest | ✅ |
| C | Claude attention: PermissionRequest / Notification / AskUserQuestion / Elicitation / Stop / StopFailure hooks (observe-only), MCP `request_attention`, ownership checks, session status with certainty, native desktop notifications with click routing, focus/quiet hours/mute, lifecycle producers | `e2e/attention.mjs` 23/23; real-CLI StopFailure; permission/question events with documented payloads (MANUAL_CHECKS 23) | ✅ |
| D | Constitution Manager (ADR-0027): articles/sections/rules with stable ids, enforcement levels, scopes + inheritance (global → task), tree + Markdown editor, search, conflicts, effective-rules preview, applicable projects, Git history, approval of synced changes, launch delivery, MCP `get_constitution`, `nexus constitution` | core + CLI E2E + vitest + `e2e/constitutions.mjs` 13/13 | ✅ |
| E | Context Manager: onboarding analysis + wizard, strategies (minimal/balanced/deep/custom), configurable sources, budgets, Context Inspector (configured / observed / predicted / on demand / retrieved), retrieval metrics | | ⏳ |
| F | Integration and refinement across the subsystems | | ☐ |

## Phase 6 — Product maturity

| # | Milestone | Acceptance check |
| --- | --- | --- |
| M29 | Workspace Git sync UI (fetch/pull/push with outgoing-diff review) | Scenario K |
| M30 | Packaging + GitHub Releases CI + updater signing (stable/prerelease channels) | artifacts verified on clean VMs |
| M31 | Backup/restore, performance budget, user documentation | Scenario A on a clean machine |

### Gates before the first public release (owner decision, 2026-11-04)

These are deliberately open and must be closed before anything is published:

1. **Application identifier** — `dev.nexus.desktop` is provisional. It determines the WebView2 user-data folder,
   installer/registry identity and updater identity, so it cannot change after release without a migration.
2. **License** — none yet; no LICENSE file is added until the owner decides.
3. **Code signing** — Windows Authenticode / macOS notarization certificates (Tauri updater key is separate).
4. ~~Release builds must not honour WebView2 remote debugging~~ — done (ADR-0015, `e2e/t18-check.mjs`); remaining:
   make sure no distributed build enables the `e2e-webview-debugging` feature (release CI check).
5. Manual check with a real profile (`docs/MANUAL_CHECKS.md`) passed on the release candidate.

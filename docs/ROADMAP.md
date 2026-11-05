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
| M12 | MCP server v1 (workspace-scoped tools incl. `checkpoint`; 21 tools after M16–M19, M18, `request_attention`, `get_constitution`, `get_workflow_run` and `report_step`) via per-session `--mcp-config`; policy-aware omission; checkpoints live in the desktop | unit + stdio E2E + official-SDK interop + desktop E2E ✅ |
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
| E | Context Manager (ADR-0028): onboarding analysis + wizard, strategies (minimal/balanced/deep/custom) with clamped custom sources and budgets, project docs as a cited FTS source (dedupe, freshness, indexed-only reads), InstructionsLoaded observation, retrieval recording, Context Inspector (configured / predicted / observed / on demand / retrieved), metrics, `nexus context` | core + CLI E2E + vitest + `e2e/context.mjs` 10/10 + `e2e/context-real-claude.mjs` (observed == predicted on the real CLI) | ✅ |
| F | Integration: live cross-profile bleed warning from observed instructions (T21 → notification → isolation), constitution delivery per strategy, review notifications open the Constitution Manager, project page → Context | real CLI: `security-real-claude.mjs` bleed checks | ✅ |

## Stage: Workflow Engine (owner specification, 2026-11-04)

Branch `milestone/workflow-engine`. No heavyweight orchestrator: workflows run on tasks, sessions, skills, hooks,
constitutions, checkpoints and MCP (ADR-0012 → ADR-0029).

| # | Milestone | Acceptance check | Status |
| --- | --- | --- | --- |
| W1 | Definitions (portable, validated, approval), six templates, library UI and `nexus workflow` | core + vitest | ✅ |
| W2 | Trusted quality gates per project (argv, no shell, machine-local), suggestions, runner in the desktop and CLI, crash reconciliation | core + CLI E2E + desktop E2E (kill during a gate) | ✅ |
| W3 | Runs: dependencies, acceptance criteria, AI-reported vs verified vs human, rigor (work profiles), retry, cancel, failure handling, task/checkpoint memory | core + CLI + desktop E2E | ✅ |
| W4 | Session integration: task launch with pointer, SessionStart brief, MCP `get_workflow_run`/`report_step`, session-only skills with profile isolation | CLI E2E + real CLI `workflow-real-claude.mjs` 7/7 | ✅ |
| W5 | Notifications (input, review, gate failed, completed, failed) and launch points (project page, task dialog, library) | desktop E2E | ✅ |
| W6 | Pre-release: NEXUS as notification sender, click routing incl. `nexus://`, withdrawal, lock-screen content policy; constitution compliance (instructed vs verified) | opt-in Windows self-test, attention E2E 27/27, core | ✅ |
| W7 | Gate approval and versions: each gate command version needs explicit approval before it runs (separate from workflow approval); waiting steps resume once approved | core + CLI E2E + desktop E2E `workflows.mjs` 22/22 | ✅ |

## Stage: Usage analytics and sync preparation (owner specification, 2026-11-05)

Branch `milestone/usage-analytics` (off `main` at `cba2b7a`). ADR-0030 (usage), ADR-0031 (Git workspace sync).

| # | Milestone | Acceptance check | Status |
| --- | --- | --- | --- |
| G1 | Gate approvals show which waiting steps and runs resume (desktop dialog, `nexus gates list` / `approve`); approval resumes exactly those | core + CLI E2E + desktop E2E `workflows.mjs` | ✅ |
| U1 | Transcript reader (incremental, deduplicated, numbers only, subagents, version compatibility) and ingestion with resume/`/clear` attribution | core + real CLI `usage-real-claude.mjs` | ✅ |
| U2 | Provenance on every figure (parsed / estimate / unavailable, ADR-0010); cost only for API-billed profiles; telemetry never changed | core + CLI E2E | ✅ |
| U3 | Reports: totals, trend, breakdowns by project, profile, task, workflow, context strategy, work profile, model; efficiency; coverage | core + CLI E2E | ✅ |
| U4 | Usage dashboard (filters, tiles, stacked trend with table view, breakdowns, strategy comparison, sessions) and summaries on project, workflow run, task | vitest + desktop E2E `usage.mjs` | ✅ |
| S0 | Git workspace sync architecture (ADR-0031) and read-only readiness (`nexus workspaces sync-status`) | core + CLI E2E | ✅ |
| U5 | OpenTelemetry to a local receiver for opt-in personal profiles (*measured* provenance, ADR-0010 source 1) | needs a signed-in profile for verification | planned |

## Stage: M29 Workspace Git synchronisation (owner specification, 2026-11-05)

Branch `milestone/m29-workspace-sync` (off `main` at `7bd238e`). ADR-0031; threat model T39–T41.

| # | Milestone | Acceptance check | Status |
| --- | --- | --- | --- |
| Y1 | Fetch, incoming/outgoing commits and files, diffs; status offline from the last fetch | core + CLI E2E | ✅ |
| Y2 | Fast-forward-only pull with incoming inspection, overlap/diverged/in-progress refusals and rollback; approvals required again for changed rules, constitutions, workflows, AI Kits | core + desktop E2E | ✅ |
| Y3 | Reviewed commit and push with tokens and confirmations; pre-commit and pre-push inspection; never forced | core + CLI E2E + desktop E2E | ✅ |
| Y4 | Onboarding from Git and restore checklist; refusals clean up | core + CLI E2E + desktop E2E | ✅ |
| Y5 | History, one operation at a time, interrupted operations, classified remote failures | core + desktop E2E | ✅ |
| Y6 | Sync Center (desktop) and `nexus sync` | vitest + desktop E2E `sync.mjs` | ✅ |

## Stage: Beta readiness (owner specification, 2026-11-05)

Branch `milestone/beta-readiness` (off `main` at `efd3dd4`). No large product features: safe, reliable, installable.

| # | Milestone | Acceptance check | Status |
| --- | --- | --- | --- |
| B1 | Upgrade safety: identity check, failed migrations never start fresh, startup error dialog; backups (manual, daily, pre-restore) with checksums; verified restore with rollback (ADR-0032) | core + CLI | ✅ |
| B2 | Packaging: NSIS per-user installer with the CLI sidecar, fixed binary name, program/data folders apart, `--self-check`, `nexus://` path fix (ADR-0033) | `e2e/installed.mjs` 27/27 (debug and release installer) + UI 18/18 | ✅ |
| B3 | Release security: compile-time ban of the test feature in release, release check, checksums, provenance, draft-only release workflow | `release-check.mjs`, T18 on the release program | ✅ |
| B4 | User documentation: first run, user guide, backups, troubleshooting, local vs. synced | docs/user | ✅ |
| B5 | Decisions for the owner: application identity (ADR-0034), licence (ADR-0035), code signing, updater | — | proposed |

## Phase 6 — Product maturity

| # | Milestone | Acceptance check |
| --- | --- | --- |
| M29 | Workspace Git sync: Sync Center + `nexus sync` (fetch, fast-forward pull, reviewed commit/push, inspection, onboarding/restore) — ADR-0031 | core + CLI E2E + desktop E2E `sync.mjs` 18/18 | ✅ (2026-11-05) |
| M30 | Packaging + GitHub Releases CI + updater signing (stable/prerelease channels) — packaging and draft-release CI done in beta readiness; signing and updater pending owner decisions | artifacts verified on clean VMs |
| M31 | Backup/restore ✅ and user documentation ✅ (beta readiness); performance budget open | Scenario A on a clean machine |

### Gates before the first public release (owner decision, 2026-11-04)

These are deliberately open and must be closed before anything is published:

1. **Application identifier** — `dev.nexus.desktop` is provisional. It determines the WebView2 user-data folder,
   installer/registry identity and updater identity, so it cannot change after release without a migration.
2. **License** — none yet; no LICENSE file is added until the owner decides.
3. **Code signing** — Windows Authenticode / macOS notarization certificates (Tauri updater key is separate).
4. ~~Release builds must not honour WebView2 remote debugging~~ — done (ADR-0015, `e2e/t18-check.mjs`); remaining:
   make sure no distributed build enables the `e2e-webview-debugging` feature (release CI check).
5. Manual check with a real profile (`docs/MANUAL_CHECKS.md`) passed on the release candidate.

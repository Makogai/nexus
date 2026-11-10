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

## Stage: Portable Workspace (owner specification, 2026-11-05)

Branch `milestone/portable-workspace` (off `main` at `7beef88`). Audit, gap analysis and plan:
[`PORTABLE_WORKSPACE.md`](PORTABLE_WORKSPACE.md). Goal: carry the whole AI development environment and its knowledge
between computers through one portable Git repository.

| # | Milestone | Acceptance check | Status |
| --- | --- | --- | --- |
| P0 | Audit of the implementation against the specification, gap analysis, migration strategy, ADR-0036 | docs | ✅ |
| P1 | Workspace schema 2 (ADR-0036): version/template/feature/layout fields, `projects/` for new workspaces, `constitutions/` + `project-context/` scanned, newer workspaces read-only (writes refused centrally), explained migration with backup and rollback; `nexus workspaces info/migrate`, desktop banner | core + CLI E2E + vitest + desktop E2E `portable.mjs` 11/11 | ✅ |
| P2 | Templates Minimal / Developer / AI Engineering Pro (ADR-0037): 27 constitutions (14 general + 13 stack rule sets, 319 rules — 284 after the P9 review), 11 skills, 7 workflows, 4 integration manifests; lock file with per-file plan (add / update / keep yours / keep removed / skip / retire) and rollback; workspace skills as a workspace-scoped AI Kit; on-demand constitutions listed by document; `docker`/`sql` detection; `nexus templates`, onboarding choice | core (content gate, install, update, rollback) + CLI E2E + vitest + desktop E2E `portable.mjs` 19/19 + kits/workflows E2E + real-Claude kits/workflow | ✅ |
| P3 | Workspace Doctor: manifest/schema, required files, template and lock file, knowledge validity and duplicates, constitution problems, pending approvals, workspace skills and workflow skill references, integration manifests (tools by id only), projects, Claude Code, Git, sync; recommendations, navigation, repairs with a plan first (migrate, .gitignore, README, clone folder, index rebuild, register skills for review, template update); `nexus workspaces doctor [--repair ID --yes]`; desktop *Workspace* view (Doctor + template card) | core + CLI E2E + vitest + desktop E2E `portable.mjs` 24/24 | ✅ |
| P4 | Project context `project-context/<slug>/` (project.json, overview from the README with sources, architecture structure + draft summary slot, generated current state; project tasks/decisions/memory/specs/constitutions written there) on registration; discovery of the clone folder (registered / known by remote / new — nothing registered automatically), clone of missing projects from recorded remotes with stable ids, link of hand-made clones; `nexus projects scan|clone|context`; Projects view discovery panel, project page context card; Doctor checks and repairs (clone missing, link known, create context) | core (two computers, real Git, local bare remote) + CLI E2E + vitest + desktop E2E `portable.mjs` 33/33 + regression suites | ✅ |
| P5 | Environment requirements from the project's own files (runtimes and versions with their source, package managers, Docker/Compose services/Dev Container, env var **names** from example files, README setup section) recorded as `environment.json`; check against this computer (tools from NEXUS's table asked for their version; ecosystem constraint matching; channels reported, not guessed); install commands only typed into a terminal; Claude Code setup (detection, official methods verified against code.claude.com, next steps); `nexus projects env [--check]`, `nexus setup`; Doctor toolchain findings | core + CLI E2E + vitest + desktop E2E `portable.mjs` 38/38 | ✅ |
| P6 | Development records (ADR-0038): one per meaningful session in `project-context/<slug>/development/`, Verified (Git commits since the session's starting commit, gates NEXUS ran) / Observed (hook file edits, command categories) / Reported by Claude (checkpoints, unverified); NEXUS checkpoint at compaction; written on every end incl. crashed hosts (`finalize_pending`), idempotent; current state regenerated; Development History view and `nexus projects history` with record sync state | core (commits, gates, abrupt end, opt-out) + CLI + vitest + desktop E2E `portable.mjs` 43/43 (real hook + MCP) + task-flow/attention/context | ✅ |
| P7 | Sync policies (ADR-0039): machine-local manual / automatic with review / trusted automatic; trusted publishes only NEXUS-generated records through M29's review → commit → fetch → review → push with tokens, falls back to asking on any finding, remote/branch change, remote ahead, foreign unpushed commits or managed policy; offline retry; audited `auto-sync`; managed `policy.json`; Sync Center card, `nexus sync policy` | core (local bare remotes: allowlist, findings, foreign commits, never force, managed policy, offline) + vitest + desktop E2E `portable.mjs` 50/50 + `sync.mjs` | ✅ |
| P8 | New-computer restoration (`restoration::plan`): ordered steps (Claude Code, schema, index, approvals, workspace skills, projects, tools, profiles, gates, continue) with state and reviewable repairs; what came with the workspace and what cannot be recovered; onboarding from Git leads to the Restore view (fixed: the clone form's event was lost when onboarding unmounted); `nexus workspaces restore`; user guide `docs/user/PORTABLE_WORKSPACE.md` | core + CLI E2E + vitest + desktop E2E `restore.mjs` 12/12 (two computers, separate data folders, local bare remotes) | ✅ |
| P9 | Quality review of the template library before merging ([`TEMPLATE_REVIEW.md`](TEMPLATE_REVIEW.md)): 319 → 284 rules (duplicates merged, generic and default-restating rules removed, versions checked, proportionality scoped, unsafe execution forbidden: CORE-6, SEC-16, OPS-9, GIT-4, TST-12), 11 skill descriptions ≤ 160 characters, two workflow dependency fixes; on-demand listing ordered mandatory → task/project/stack → workspace; independent repositories never enter the workspace repository (projects registered inside it are git-ignored, nested repositories and gitlinks block the commit review, Doctor `projects-ignored`) | core + CLI 286, clippy ×2, vitest 90, desktop E2E (portable 50, restore 12, constitutions 13, sync 18, context 9, workflows 24, smoke 27, task-flow 29, MCP 9, profiles-kits 9, attention 27, usage 18), real Claude (kits 19, context 7, security 14) | ✅ |
| P10 | New workspace in a prepared place (owner feedback after the merge; branch `fix/workspace-setup`): create in a new folder, an existing empty folder or an **empty Git repository** (cloned, set up with the template, first reviewed push goes there); opening an empty repository or a folder without a manifest offers that setup; a folder or repository with other content (a project) is refused; an existing `.gitignore`/README is kept (NEXUS lines appended); standard folders `constitutions/ knowledge/ project-context/ workflows/` with READMEs from the start (index skips READMEs); shared form in onboarding and Settings (ADR-0036 amendment) | core 220 + CLI (288 total), clippy ×2, vitest 90, build, desktop E2E `workspace-setup.mjs` 14/14 + sync 18, restore 12, portable 50, smoke 27, constitutions 13, context 9, task-flow 29, workflows 24, profiles-kits 9, attention 27, usage 18, MCP 9, real-Claude kits 19 | ✅ (not merged) |

## Stage: Project workbench (owner requests, 2026-11-05)

Branch `milestone/project-workbench` (off `fix/workspace-setup`). Decisions: ADR-0040. The owner chose CodeMirror 6
read-only, a guided Claude session whose drafts they approve, and Graphify used when installed and run by them.

| # | Milestone | Acceptance check | Status |
| --- | --- | --- | --- |
| W1 | Add a project from Git: desktop *Clone from Git* and `nexus projects add <url> [--into]`; clone into `projects/<name>` or an empty folder, registered with its context; duplicates, bad URLs, occupied folders refused; failed clones leave nothing; clones get 20 min | core `clone_new`, CLI, E2E `workspace-setup.mjs` 17/17 | ✅ |
| W2 | Knowledge read formatted (Markdown with raw HTML escaped; constitutions as rule cards) and edited in place (`update_doc`: optimistic, validated, approved here); Constitution Manager opens in Read mode; calmer lists (no native grey button rows), compact empty terminal panel | core, vitest markdown safety, E2E `workbench.mjs` | ✅ |
| W3 | Project explorer: Files (lazy tree, CodeMirror read-only viewer, secrets never shown, nothing leaves the project, symlinks not followed) and Commits (paged history, changed files with renames and counts, coloured diffs; no external diff/textconv/signature programs) | core `explorer`, E2E `workbench.mjs` | ✅ |
| W4 | Learn a project: NEXUS's own map (no model), a learning task carrying it, `draft_project_document` (MCP, 22 tools) → unverified draft notes, *Use* / *Set aside* by the person, Graphify offered when installed (offline code-only build typed into a terminal, `graphify-out/` excluded locally) | core `learn`, CLI MCP test, MCP interop 22, E2E `workbench.mjs` 22/22 | ✅ |
| W5 | Lean learning (ADR-0041): built-in plugin `nexus` with the skill `learn-project` for learning sessions only, rules LEARN-1–7 (one session, no subagents unless asked, read only, budget), depth Quick/Standard/Deep; complete Graphify build (`cluster-only --no-label`), later graph joins the task, *Start right away* | core `builtin`/`prepare`/`learn`/`kits`, `claude plugin validate`, E2E `workbench.mjs` + `task-flow.mjs` (CI, all platforms green) | ✅ |
| W6 | Daily-use pass (owner feedback, 2026-11-06): explorer ignore fix (CRLF `.gitignore` + trailing-slash Git quirk), confirmations on top, Back/Forward + version, page spacing, autonomous workspace sync + Claude's commit messages (ADR-0042), push requests approved in a dialog, finished tasks (review → Done/Reopen, learning closes itself), Start in: own/running/new session (ADR-0043), sessions by project, NEXUS tools page, E2E builds beside an installed NEXUS | core + CLI + E2E (task-flow 37, workbench 33, context 12, sync 18, portable 50, attention 27, mcp-interop 9) | ✅ |
| W7 | Signed auto-updates (ADR-0044): updater plugin with the owner's public key, signed updater files + `latest.json` from the release workflow, install at start / offer later, never over running sessions without asking | `tests/updates.test.ts`, `tests/updater-manifest.test.ts`; first real update with the release after 0.1.2 | ✅ |
| W8 | Learning context (ADR-0045, owner feedback): the Learn tab notices a graph built in a terminal (cheap watch, re-read on coming back, *Check again*); notes for Claude in the learning task; folders it may also read — the surrounding repository offered — machine-local, checked, `--add-dir` + LEARN-8 for learning sessions only | core `learn`/`prepare`/`launch`, vitest `project-learn.test.ts`, E2E `workbench.mjs` 38/38 | ✅ |
| W9 | Owner reports after 0.1.3: automatic sync stops only on real risks (blocking, possible secret, not fully inspected) and names its findings (kind, file, line); machine paths are noted; one open learning draft per topic (`draft_project_document` revises); no absolute paths in learning output (LEARN-5/8); `.env.*.example` templates shown in the explorer | core `workspace_sync`/`learn`, CLI MCP test | ✅ |
| W10 | Skills page (ADR-0046): built-in, workspace, project and AI Kit skills in one list; *New skill* and in-place editing for workspace and project skills (validated, optimistic, no links); a save approves workspace skills only when nothing else waits for review | core `skills`, vitest `skills-view.test.ts`, E2E `workbench.mjs` 43/43 | ✅ |

## Stage: 0.5 alpha (owner request, 2026-11-10)

Branch `milestone/v0.5-alpha`. Decisions: ADR-0047 (push outcomes), ADR-0048 (action feedback, project page, "Needs
you"), ADR-0042 amendment, ADR-0049 (knowledge watcher), ADR-0050 (code signing, deferred). Owner decisions: Claude
writes the NX constitution and the owner approves it; *Tell Claude* types without pressing Enter; focus-based refresh
first, then the watcher (owner, same day: "ship file system watcher"); signing decided by Claude on the owner's behalf
and shipped later; plain task session.

| # | Milestone | Acceptance check | Status |
| --- | --- | --- | --- |
| A0 | Rules and guardrails: NX constitution (project scope, awaiting approval), drift guards (IPC §8, schema §7, tool counts from `--list-tools`), CLAUDE.md UI conventions | desktop `ipc_docs`, core `every_migration`, CLI MCP test, `mcp-interop` | ✅ (constitution approval and gates: owner) |
| A1 | Pushes never fail silently: fetch-first review (ahead/behind/incoming, Push disabled while behind), failure kinds (migration 0020), action notification, failed banner until dismissed/superseded, dialog result panel, *Tell Claude*, MCP `failureKind`, conflicts and operation on the project page, auto-sync warning + pause on rejection | core push_request (teammate + declining hook), sync policy test, CLI `request_push`, vitest `push-request`, E2E `task-flow.mjs` | ✅ |
| A2 | Project page tabs with counts and Ctrl+1…8; Sessions, Tasks, Knowledge, Settings tabs; Overview dashboard with "Needs you"; calmer header; terminal project chips and filter | core `inbox`, vitest `project-page`, E2E `workbench.mjs` (+Sessions/Tasks), suites follow moved elements | ✅ |
| A3 | Feedback everywhere: `useAction`/`ActionButton` across the views, persistent error toasts with *Copy details*, background failures in the notification center, results link onward, optimistic Done/Reopen and drafts, progress text | vitest `action-feedback-a/b`, `project-learn`, `project-page` | ✅ |
| A4 | Harness: Home "Needs you", session rows with task/workflow/context, focus-based knowledge refresh, pruning of session artifacts, one builder for prepare/preview | core `inbox`, `prune`, `prepare` agreement test, vitest `needs-you` | ✅ |
| A5 | Release 0.5.0: versions, release notes, ADRs, docs, full verification | see CURRENT_STATE verification log | ✅ merged, tagged `v0.5.0` |
| A6 | Knowledge watcher: the active workspace's knowledge roots watched by the desktop, index refreshed on its own connection, `nexus://knowledge`; open documents, Constitutions and Learn drafts follow without clobbering edits; code signing decided (ADR-0050, deferred) | core `knowledge::watch`, desktop `watcher` (real folders), vitest `knowledge`/`constitutions`/`project-learn`, E2E `workbench.mjs` (+4) | ✅ merged, tagged `v0.5.0` |

## Stage: Agents & Dev Environment (owner request, 2026-11-10)

Branch `milestone/agents-dev-environment` (off `main` at `431f8c6`). Decisions: ADR-0051 (dev environment), ADR-0052
(agents beyond Claude). Owner: Codex first, built so further agents are easy to add; install commands stay typed, never
run (ADR-0051 §7).

| # | Milestone | Acceptance check | Status |
| --- | --- | --- | --- |
| E1 | Tool catalog by category, ~50 tools (JDK, Maven, Gradle, .NET, Ruby, Bundler, Deno, version managers, make/CMake, kubectl/Helm/Terraform, cloud CLIs, database clients, AI agents); WinGet ids and Homebrew formulae/casks verified; platform-only tools | core unit tests (ids unique, package forms, platform filter) | ☐ |
| E2 | Inventory: every tool probed in parallel, every copy on PATH, which projects need it and whether the version fits; `nexus tools [--needed\|--missing] [--json]` | core + CLI E2E | ☐ |
| E3 | Version-manager hints (fnm, Volta, nvm for Windows, pyenv, uv, rustup, mise) for exact versions only, validated; typed, never run | core unit tests | ☐ |
| E4 | Dev Environment page: categories, filters, needed-by with version fit, copies on PATH, install/hint typed into a terminal, check again | vitest + desktop E2E | ☐ |
| G1 | Agent registry and capability table; `agent_profiles` + `sessions.agent`/`agent_profile_id` (migration 0021); environment policy per agent; Claude-only subsystems filter `agent = 'claude'` | core | ☐ |
| G2 | Codex adapter: argv builder (`-c` TOML overrides: MCP server, `developer_instructions`, `notify`), refused extra arguments, `codex::prepare` (task delivery through tools), `nexus agent-notify` (thread id + turn ended, no text stored), resume only with a recorded thread id; `fake-codex` | core + PTY integration with `fake-codex` + CLI E2E | ☐ |
| G3 | Desktop and CLI: Codex profiles (detect default, register a home), agent choice when launching from a project or task, agent badges, capability explanations; `nexus launch --agent codex`, `nexus agents` | vitest + desktop E2E with `fake-codex` | ☐ |
| G4 | Real Codex check by the owner: NEXUS tools listed in a session, thread id recorded after a turn, resume | `docs/MANUAL_CHECKS.md` | ☐ |

Later: a workspace toolchain baseline in the manifest; Codex review as a workflow step (M26); Gemini CLI, GitHub Copilot
CLI and OpenCode adapters (each measured first, as Codex was).

## Phase 6 — Product maturity

| # | Milestone | Acceptance check |
| --- | --- | --- |
| M29 | Workspace Git sync: Sync Center + `nexus sync` (fetch, fast-forward pull, reviewed commit/push, inspection, onboarding/restore) — ADR-0031 | core + CLI E2E + desktop E2E `sync.mjs` 18/18 | ✅ (2026-11-05) |
| M30 | Packaging + GitHub Releases CI + updater signing (stable/prerelease channels) — packaging done in beta readiness; release CI for Windows, macOS (Apple silicon + Intel) and Linux with one draft release, CI on all three platforms, GitHub Pages site (2026-11-05); signing and updater pending owner decisions | artifacts verified on clean VMs |
| M31 | Backup/restore ✅ and user documentation ✅ (beta readiness); performance budget open | Scenario A on a clean machine |

### Gates before the first public release (owner decision, 2026-11-04)

These are deliberately open and must be closed before anything is published:

1. ~~**Application identifier**~~ — decided 2026-11-05: `io.github.makogai.nexus`, publisher `Makogai` (ADR-0034).
2. ~~**License**~~ — decided 2026-11-05: MIT OR Apache-2.0 (ADR-0035), with `THIRD_PARTY_NOTICES.md`.
3. **Code signing** — Windows Authenticode / macOS notarization certificates (Tauri updater key is separate). Still
   open: v0.1.0 is built unsigned — SmartScreen and Gatekeeper warn on first start.
4. ~~Release builds must not honour WebView2 remote debugging~~ — done (ADR-0015, `e2e/t18-check.mjs`); remaining:
   make sure no distributed build enables the `e2e-webview-debugging` feature (release CI check).
5. Manual check with a real profile (`docs/MANUAL_CHECKS.md`) passed on the release candidate.

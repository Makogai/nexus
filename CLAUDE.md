# NEXUS — instructions for Claude Code sessions

Start every session by reading `CURRENT_STATE.md` (what works, what's next). Read `docs/ROADMAP.md` for the
milestone you are on. Open other docs only when the task touches them.

## Commands (Windows: Rust lives in `%USERPROFILE%\.cargo\bin`)

```text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && npm test && npm run typecheck && npx vite build
cargo build -p nexus-cli                    # desktop sessions start `nexus mcp` from next to nexus-desktop.exe
cd apps/desktop && npm run tauri dev        # use NEXUS_DATA_DIR=<empty dir> for an isolated database
cd apps/desktop && npx tauri build --debug --no-bundle && node e2e/t18-check.mjs   # production build ignores CDP env
cd apps/desktop && npx tauri build --debug --no-bundle --features e2e-webview-debugging && node e2e/run-smoke.mjs
cd apps/desktop && node e2e/mcp-interop.mjs # official MCP SDK client against `nexus mcp`
cd apps/desktop && node e2e/task-flow.mjs   # project → task → launch → MCP update → restart → resume (E2E build)
cd apps/desktop && node e2e/hooks-real-claude.mjs # real Claude runs NEXUS hooks (synthetic profile)
cd apps/desktop && node e2e/instructions-real-claude.mjs # inspector vs. real Claude InstructionsLoaded
cd apps/desktop && node e2e/security-real-claude.mjs # isolation + task-text-as-data vs. real Claude
cd apps/desktop && node e2e/profiles-kits.mjs # diagnostics + kits through the desktop UI (E2E build)
cd apps/desktop && node e2e/kits-real-claude.mjs # diagnostics + per-profile kits vs. real Claude
cd apps/desktop && node e2e/agents.mjs     # Codex profile, launch, notify, resume (fake-codex) + Dev Environment page (E2E build)
cd apps/desktop && node e2e/attention.mjs   # notifications + multi-session attention (E2E build, recording notifier)
cd apps/desktop && node e2e/constitutions.mjs # Constitution Manager through the UI (E2E build)
cd apps/desktop && node e2e/context.mjs     # Context Manager: wizard, strategy, inspector (E2E build)
cd apps/desktop && node e2e/context-real-claude.mjs # inspector prediction vs. real InstructionsLoaded via NEXUS hook
cd apps/desktop && node e2e/workflows.mjs   # Workflow Engine: runs, gates, reviews, crash recovery (E2E build)
cd apps/desktop && node e2e/workflow-real-claude.mjs # workflow skills only in the run's session (real Claude)
cd apps/desktop && node e2e/usage.mjs       # Usage dashboard: transcripts → scanner → tiles, trend, breakdowns (E2E build)
cd apps/desktop && node e2e/usage-real-claude.mjs # real transcript format parsed; nothing invented (real Claude)
cd apps/desktop && node e2e/sync.mjs        # Sync Center: local bare remote, reviewed commit/push, ff pull (E2E build)
cd apps/desktop && node e2e/portable.mjs    # Portable Workspace: schema, templates, Doctor, projects, env, records, sync policy (E2E build)
cd apps/desktop && node e2e/restore.mjs     # second computer: open from Git → Restore view → clone, approve → history (E2E build)
cd apps/desktop && node e2e/workbench.mjs     # knowledge reading/editing, project explorer, learning a project (E2E build)
cd apps/desktop && node e2e/workspace-setup.mjs # new workspace in an empty repository / empty folder; projects refused (E2E build)
cd apps/desktop && npx tauri build --debug --config src-tauri/tauri.installer.conf.json && node e2e/installed.mjs # real installer: install, check, uninstall
cd apps/desktop && node scripts/release-check.mjs ../../target/release/bundle/nsis # after `npx tauri build --config src-tauri/tauri.installer.conf.json`
cargo test -p nexus-desktop --test toast_selftest -- --ignored # opt-in: real Windows notification sender
```

## Architecture rules

* Domain logic lives in `crates/nexus-core` (no Tauri, no UI). `apps/desktop/src-tauri` is a thin IPC + PTY layer.
* The frontend talks to the backend only through `apps/desktop/src/lib/ipc.ts`; keep `src/lib/types.ts` in sync
  with Rust DTOs (serde camelCase).
* Source of truth: Git for repo state; Claude config dirs for Claude state (read-only); `nexus-workspace.json` and
  workspace files for portable data; SQLite for machine-local state (ADR-0005).
* New schema → append a migration in `crates/nexus-core/src/db/migrations/` (never edit a released one).
* Claude integration is session-scoped CLI flags only (ADR-0004). Never write into a Claude config dir NEXUS did
  not create.
* Other agents (ADR-0052) get the same treatment: per-session overrides only (Codex: TOML-escaped `-c`, built by
  `codex::launch`), never a write into the agent's home, task text only through MCP tools, extra arguments allowlisted,
  no sandbox/hook-trust bypass. One entry point for every host (`agent::launch::prepare`); each agent states its
  capabilities (`agent::Kind::capabilities`) and the UI shows them instead of guessing. Claude-only subsystems (usage,
  kits, inspector, diagnostics) select `sessions.agent = 'claude'`.
* Dev tools are known by catalog id only (`toolchain::TOOLS`, ADR-0051): package ids verified when added; NEXUS runs
  only version commands — install commands and version-manager hints are typed into a terminal, never run.
* Every process that owns sessions is a session host (ADR-0014): register, heartbeat, `host::shutdown` on exit.
* MCP tools stay workspace-scoped, bounded and free of exec/network/arbitrary-file access (ADR-0009); their only file
  writes are knowledge records (task/note/decision) through `knowledge::service` (ADR-0017).
* Knowledge files are authoritative; SQLite only indexes them (rebuildable). Writes go through `knowledge::store`
  (validated, atomic, optimistic). Always-on workspace rules reach Claude only when their current hash is approved.
  The desktop watches the active workspace's knowledge roots (one recursive watch per root, `knowledge::watch` rules)
  and refreshes the index on its own connection (ADR-0049); NEXUS's own writes re-index in the same call.
* Hooks stay deterministic and cheap: no model calls, no prompt text or raw commands stored, always exit 0 (ADR-0016).
* Everything a session gets from NEXUS is built by `claude::prepare` (both hosts). Appended context stays bounded:
  work profile, approved always-on rules not already in CLAUDE.md, a task brief — never histories or whole knowledge.
* NEXUS reads Claude's instruction files (inspector) but never writes, reorders or replaces them (ADR-0019).
* Profile diagnostics are read-only and allowlisted (never `.credentials.json`; from `.claude.json` only `mcpServers`;
  names/counts instead of values). Kits reach sessions only via per-profile `--plugin-dir` overlays (ADR-0023/0024).
* User-facing events go through `notify::publish` (ADR-0025); attention comes only from reported signals (hooks, MCP,
  process lifecycle), identity from the session row; NEXUS hooks never print a decision for PermissionRequest/PreToolUse.
* Constitutions, Context, Memory, Skills and Workflows stay separate subsystems on shared infrastructure (ADR-0026).
  Constitutions are rule documents (`format: constitution`, ADR-0027): stable rule ids, mandatory policies are never
  overridable by narrower scopes, synced versions need approval, delivery is compact and budgeted.
* Workflows (ADR-0029) run on tasks and normal sessions; Claude only *reports* steps (MCP), gates *verify* them in a
  NEXUS host (desktop runner / `nexus workflow verify`), never in the MCP server. Gate commands are machine-local,
  defined by the person, argv without a shell — workflow files name gates, never commands. Each gate command
  version needs its own approval (checked in `gates::begin`), separate from approving a workflow.
* Context strategies (ADR-0028) decide what `claude::prepare` and SessionStart add; balanced = the earlier behaviour.
  Project docs are read only when indexed (allowlisted Markdown); retrieval records counts and ids, never queries.
* Workspace-controlled text (task titles/descriptions, synced files) never goes into the system prompt or `--name`;
  it reaches Claude only as escaped, labelled data (SessionStart context, tool output) — ADR-0020, threat T22.
* Usage analytics (ADR-0030) reads Claude's transcripts read-only and stores numbers only; every figure carries
  provenance (parsed / estimate / unavailable, ADR-0010); cost only for API-billed profiles; NEXUS never sets or
  forwards telemetry. Workspace sync (ADR-0031) is Git in the person's repository: commits and pushes only with the
  token of a confirmed review and after the inspection (`workspace_sync::inspect`) did not block; pulls fast-forward
  only; never force, never automatic; machine-local state never travels; synced rules, constitutions, workflows and
  kits need approval again. Tests use temporary repositories and local bare remotes only.
* Portable workspace (ADR-0036/0037): one manifest (`nexus-workspace.json`, schema 2), newer schemas read-only, explained
  migrations with backups; templates install through the stores with a lock file and never overwrite changed files;
  only the core constitution is always on; workspace skills reach only their workspace's sessions; project knowledge
  lives in `project-context/<slug>/`; scanning never registers or runs anything; tools are named by id (`toolchain`).
* Skills (ADR-0046): only the person writes skills, through the desktop Skills page — `<workspace>/skills/<name>` or
  `<project>/.claude/skills/<name>`, validated, optimistic, never through links; no MCP tool writes skills. A save
  approves the workspace skills only when nothing else waits for review.
* Data safety (ADR-0032): never start with an empty database instead of a failed one; migrations are transactions;
  backups before upgrades, daily and before restores; restore verifies and rolls back. Installer (ADR-0033): programs in
  `%LOCALAPPDATA%\Programs\NEXUS`, data in `%LOCALAPPDATA%\NEXUS` — never mixed; the CLI is the installer's sidecar;
  the e2e feature cannot compile into release builds. Identifier and licence changes wait for the owner (ADR-0034/0035).
* Keep the crate MSRV (`rust-version`) at the newest MSRV of the Tauri crates; the MSRV-aware resolver otherwise picks
  older Tauri crates than the npm packages and `tauri build` fails its version check.

## UI conventions (desktop)

* Every control that calls the backend shows it is working: run it through `useAction()` (`src/composables/useAction.ts`)
  or `<ActionButton>` — disabled and `aria-busy` while pending, re-entrancy guarded, optional pending label.
* Errors stay visible until the person dismisses them: never clear an error in a reload; error toasts persist (with
  *Copy details*); failures of background work (push, sync, gates, clone, graph build) also reach the notification
  center through `notify::publish`. Success names the result and links onward where there is a next place to go.
* The project page keeps its tab in `?tab=`; kept-alive views reload in `onActivated` and on `app.externalChanges`.
  Open documents follow `app.freshness` (files changed on disk, ADR-0049) — quietly, and never over unsaved edits.
* E2E suites select by `data-*` attributes: when an element moves, its attribute moves with it, or the suite is updated
  in the same commit.

## Security rules (non-negotiable)

* No shell strings: spawn programs with argument vectors (`proc::command`, PTY `CommandBuilder`).
* Child environments go through `nexus_core::env::compose` / `claude::launch::profile_env`.
* Paths from the UI or from workspace files go through `paths::validate_user_path` / `paths::resolve_within`.
* Git read calls go through `git.rs` (fsmonitor disabled, no prompts). Strip credentials from URLs.
* Never log or display secrets: env values, auth payloads, policy values other than the allowlisted keys.
* `webview_guard::harden` stays the first call in `main`; the `e2e-webview-debugging` feature is test-only (ADR-0015).
* `unsafe` is denied workspace-wide; the only exception is the audited env removal in `webview_guard`.
* Tests use temp dirs and `fake-claude`; never run tests against real Claude profiles or real repositories.

## Working style

* Vertical slices with tests; update `CURRENT_STATE.md` and `docs/ROADMAP.md` at the end of a session.
* Significant decisions get an ADR in `docs/DECISIONS/`.

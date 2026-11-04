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
* Every process that owns sessions is a session host (ADR-0014): register, heartbeat, `host::shutdown` on exit.
* MCP tools stay workspace-scoped, bounded and free of exec/network/arbitrary-file access (ADR-0009); their only file
  writes are knowledge records (task/note/decision) through `knowledge::service` (ADR-0017).
* Knowledge files are authoritative; SQLite only indexes them (rebuildable). Writes go through `knowledge::store`
  (validated, atomic, optimistic). Always-on workspace rules reach Claude only when their current hash is approved.
* Hooks stay deterministic and cheap: no model calls, no prompt text or raw commands stored, always exit 0 (ADR-0016).
* Everything a session gets from NEXUS is built by `claude::prepare` (both hosts). Appended context stays bounded:
  work profile, approved always-on rules not already in CLAUDE.md, a task brief — never histories or whole knowledge.
* NEXUS reads Claude's instruction files (inspector) but never writes, reorders or replaces them (ADR-0019).
* Profile diagnostics are read-only and allowlisted (never `.credentials.json`; from `.claude.json` only `mcpServers`;
  names/counts instead of values). Kits reach sessions only via per-profile `--plugin-dir` overlays (ADR-0023/0024).
* Workspace-controlled text (task titles/descriptions, synced files) never goes into the system prompt or `--name`;
  it reaches Claude only as escaped, labelled data (SessionStart context, tool output) — ADR-0020, threat T22.
* Keep the crate MSRV (`rust-version`) at the newest MSRV of the Tauri crates; the MSRV-aware resolver otherwise picks
  older Tauri crates than the npm packages and `tauri build` fails its version check.

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

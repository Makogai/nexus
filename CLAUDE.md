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
cd apps/desktop && npx tauri build --debug --no-bundle && node e2e/run-smoke.mjs   # real-app E2E (Windows)
cd apps/desktop && node e2e/mcp-interop.mjs # official MCP SDK client against `nexus mcp`
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
* MCP tools stay workspace-scoped, bounded and free of exec/file/network access (ADR-0009).
* Keep the crate MSRV (`rust-version`) at the newest MSRV of the Tauri crates; the MSRV-aware resolver otherwise picks
  older Tauri crates than the npm packages and `tauri build` fails its version check.

## Security rules (non-negotiable)

* No shell strings: spawn programs with argument vectors (`proc::command`, PTY `CommandBuilder`).
* Child environments go through `nexus_core::env::compose` / `claude::launch::profile_env`.
* Paths from the UI or from workspace files go through `paths::validate_user_path` / `paths::resolve_within`.
* Git read calls go through `git.rs` (fsmonitor disabled, no prompts). Strip credentials from URLs.
* Never log or display secrets: env values, auth payloads, policy values other than the allowlisted keys.
* Tests use temp dirs and `fake-claude`; never run tests against real Claude profiles or real repositories.

## Working style

* Vertical slices with tests; update `CURRENT_STATE.md` and `docs/ROADMAP.md` at the end of a session.
* Significant decisions get an ADR in `docs/DECISIONS/`.

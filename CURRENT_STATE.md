# CURRENT_STATE

_Last updated: 2026-11-04 (end of session 2)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

* `main` = reviewed baseline commit `68affc0` (Phase 1 MVP). Nothing has ever been pushed.
* Branch **`milestone/m9-m10-reliability-cli`** holds session 2 (M9 reliability baseline, M10 CLI, M12 MCP server,
  ADR-0013/0014). **Uncommitted** — waiting for the owner's go-ahead to commit.
* Phases: Phase 1 ✅ · M9 ✅ · M10 ✅ · M11 partial (session hosts done; `claude agents --json` open) · M12 ✅ ·
  M13 (hooks) next.

## Verified behaviour (2026-11-04)

| Area | Evidence |
| --- | --- |
| Core logic (storage, workspaces, projects, git, profiles, policy, launch spec, env scrub, sessions, hosts, checkpoints, doctor) | `cargo test -p nexus-core`: 79 unit + git (5) + crash (2) + isolation (4) |
| Migrations & recovery: v1→v2 table rebuild keeps self-references, failed migration rolls back, FK-checked rebuilds, backups rotate, real `abort()` mid-transaction, garbage/page-damaged DB quarantined (never deleted) | `db::tests`, `tests/db_crash.rs` |
| Env isolation: every scrubbed name in any letter case; helper spawns (`auth status`, `--version`) and sessions see no parent Claude markers, messaging token, API keys or OTEL; golden argv | `env::tests`, `tests/isolation.rs`, `launch::tests`, PTY tests, CLI tests, E2E |
| Session hosts (ADR-0014): leases, reconcile, sleep recovery, crashed predecessor, final orderly shutdown; a **really killed** `nexus launch` → interrupted | `host::tests`, `cli::a_killed_cli_host_is_reconciled_to_interrupted` |
| CLI (`nexus`): doctor, workspaces, projects, profiles, sessions, launch (`--resume last`), exit-code propagation | `cargo test -p nexus-cli` (8 E2E + 5 MCP unit) |
| MCP server: JSON-RPC over stdio, workspace scoping (other workspaces invisible by id/name/prefix), bounded inputs, policy-blocked → omitted with reason | MCP unit tests, stdio E2E, **official TS SDK interop** (`e2e/mcp-interop.mjs`) |
| Desktop E2E on the production-mode build (embedded frontend + CSP): launch isolation, MCP config, resume, MCP checkpoint and `nexus launch` session appear **live** in the desktop | `e2e/run-smoke.mjs`: 18/18, 3 consecutive runs |
| Frontend | 26 vitest (logic + LaunchDialog, ProfileDialog, SessionRow, CommandPalette, change-detection race); `vue-tsc` clean |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` clean |

Not verified (deliberately): anything with the owner's real Claude profiles → `docs/MANUAL_CHECKS.md`.
Not yet run: the GitHub Actions workflow (nothing pushed) — Windows GUI E2E on hosted runners is plausible but unproven.

## Bugs found and fixed this session (all with regression coverage)

* `tauri build` failed on an npm/crate version mismatch: the MSRV-aware resolver picked older Tauri crates under
  `rust-version = 1.85`. MSRV is now 1.90 (CLAUDE.md rule). The earlier "release" binary was a dev-mode leftover.
* Desktop change detection missed external writes landing between the initial load and the first poll (baseline
  race) — found by the E2E, fixed, unit-tested.
* SessionRow showed a made-up "0s" duration for interrupted sessions; resumed sessions are now marked.

## Known issues / decisions to revisit

* Release gates (owner decision): identifier `dev.nexus.desktop` provisional, no license, code signing, release
  builds must ignore WebView2 remote debugging (T18), bundle `nexus` CLI as a Tauri sidecar (MCP needs it next to the
  app). See `docs/ROADMAP.md` → "Gates before the first public release".
* portable-pty 0.9 Windows `kill` reports success as an error — worked around (`pty.rs`).
* `freezePrototype` is off (breaks xterm.js).
* `<data dir>/sessions/<id>/mcp.json` files are not pruned yet (tiny; add housekeeping).
* Project inspection holds the UI DB mutex during short git calls.
* `npm audit`: moderate advisories in dev-only transitive deps.

## Next actions (in order)

1. Owner: review the branch; approve committing it; run `docs/MANUAL_CHECKS.md` with the Personal profile.
2. M13: lifecycle hooks via `--settings` (exec form → `nexus hook <event>`): SessionStart/SessionEnd/PreCompact/Stop
   checkpoints; verify `--settings` hook merging and `allowManagedHooksOnly` behaviour on Claude Code 2.1.x.
3. M11 remainder: reconcile with `claude agents --json`; measure `claude --bg` + `attach` for the future service host.
4. M14: profile diagnostics in the desktop (reuse `nexus_core::doctor`), allowlist profile cloning.
5. Phase 3: knowledge files per ADR-0013 (format, atomic writes, FTS index, conflict reporting).

## How to run

```text
# Rust (PATH needs %USERPROFILE%\.cargo\bin); MSRV 1.90
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p nexus-cli -p fake-claude
# Frontend / app
cd apps/desktop && npm install
npm test && npm run typecheck
npm run tauri dev                                   # NEXUS_DATA_DIR=<empty dir> for an isolated database
npx tauri build --debug --no-bundle && node e2e/run-smoke.mjs   # real-app E2E (Windows)
node e2e/mcp-interop.mjs                            # official MCP SDK client vs nexus mcp
# CLI
target/debug/nexus --help
```

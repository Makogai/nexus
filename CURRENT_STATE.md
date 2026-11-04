# CURRENT_STATE

_Last updated: 2026-11-04 (end of session 1)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

**Phase 1 MVP is functional on Windows** (M0–M8 done, M9 hardening in progress). The app launches, onboards a
workspace, registers projects and Claude profiles, runs real Claude Code and shells in the integrated terminal,
records sessions truthfully and resumes conversations.

## Verified behaviour (2026-11-04)

| What | How verified |
| --- | --- |
| Core domain logic (DB/migrations, paths, workspaces, projects, git, profiles, policy, auth parsing, launch spec, env scrub, sessions) | 58 unit + 5 integration tests (`cargo test -p nexus-core`) |
| Hostile repo `core.fsmonitor` is not executed by NEXUS git calls (control run proves plain git would execute it) | `tests/git_integration.rs` |
| Launch through the real PTY: correct `CLAUDE_CONFIG_DIR` (removed for default profile), no inherited `CLAUDECODE`/`CLAUDE_CODE_*`/messaging token/`ANTHROPIC_API_KEY`/`OTEL_*`, input round trip, exit codes, terminate | 4 tests in `apps/desktop/src-tauri/tests/pty_launch.rs` (fake-claude) |
| Real app end to end (real IPC + PTY + UI) while NEXUS itself runs inside a Claude session | `apps/desktop/e2e/smoke.mjs`: 15/15 checks |
| Real Claude Code 2.1.311 TUI renders and accepts keyboard input in the integrated terminal | manual CDP run with a synthetic (empty) config dir |
| Hard crash of NEXUS: Claude child does not survive as an orphan; on restart the session shows *Interrupted*, resumable only when a transcript exists | manual CDP run (taskkill) |
| PowerShell terminal in a project dir, commands run, `CLAUDECODE` scrubbed for shells too | manual CDP run |
| Frontend logic (fuzzy matcher, formatting, launch-dialog defaults) | 11 vitest tests; `vue-tsc` clean; production build OK |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` clean |

Not yet verified: launching with the user's *real* profiles (deliberately avoided — the user's config dirs must not
be touched during development). The owner should do the manual smoke checklist in `docs/TESTING.md` once.

## Implemented components

* `crates/nexus-core` — all domain logic (see `docs/ARCHITECTURE.md` §1).
* `apps/desktop/src-tauri` — ~45 IPC commands, PTY manager (binary channels, ring buffer, ConPTY handshake,
  query-stripped replay), single instance, minimal capabilities, CSP.
* `apps/desktop/src` — Vue 3 UI: onboarding, Home, Projects (+detail), Sessions, Claude profiles, Settings,
  terminal panel (tabs, resize, maximize, search, copy/paste), launch dialog, command palette (Ctrl+K /
  Ctrl+Shift+P), quit confirmation with live sessions, dark/light themes.
* `apps/desktop/e2e` — CDP client (`cdp.mjs`) and smoke test (`smoke.mjs`).
* `.github/workflows/ci.yml` — Rust (Windows + Ubuntu) and frontend jobs. **Unverified until first push.**

## Known issues / decisions to revisit

* portable-pty 0.9 Windows bug: kill reports success as `Err(os error 0)` — worked around in `pty.rs`.
* Tauri `freezePrototype` is off (breaks xterm.js) — recorded in ADR-0007 and threat T15.
* `project_inspect_path` / `project_register` hold the UI DB mutex during short git calls (~50–150 ms).
* Session duration in lists does not tick live; running sessions show the duration at render time.
* Light theme: the terminal switches theme only when the theme is changed through preferences (correct path).
* `npm audit`: 5 moderate advisories in dev-only transitive deps (glob/minimatch). Not shipped.
* Tauri identifier `dev.nexus.desktop` is provisional (affects install paths) — decide before the first release.
* No LICENSE (owner decision). **Nothing is committed yet** — the repository has no commits; the owner decides
  when to commit (no remote pushes were made).
* Component tests with mocked IPC are thin (logic is covered by pure-function tests + the smoke test).

## Next actions (in order)

1. Owner: run the manual smoke checklist (`docs/TESTING.md`) with a real profile; commit the baseline.
2. M9: `tauri-driver` WebDriver E2E for CI; component tests for LaunchDialog/ProfileDialog with mocked IPC;
   live-ticking durations; empty/error-state review.
3. Phase 2 (M10): `crates/nexus-cli` (`nexus doctor | profiles | projects | sessions`) on top of nexus-core.
4. M11: reconcile sessions with `claude agents --json`; evaluate durable sessions (`claude --bg` + `attach`).
5. M12–M13: `nexus mcp` (rmcp, stdio, scope bound by `NEXUS_SESSION_ID`) via `--mcp-config`; hooks via
   `--settings` exec form; respect `disableSideloadFlags` / `allowManagedHooksOnly` (policy summary already
   computed per profile in `claude::policy`).

## How to run

```text
# Rust (PATH needs %USERPROFILE%\.cargo\bin)
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
# Frontend / app
cd apps/desktop && npm install
npm test && npm run typecheck
npm run tauri dev       # set NEXUS_DATA_DIR to an empty folder for an isolated database
node e2e/smoke.mjs      # with the app running under CDP — see docs/TESTING.md
```

# CURRENT_STATE

_Last updated: 2026-11-04 (session 7, in progress)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

* `main` = baseline + merges `92475ec`, `8f51af2`, `1c3f9bb`, `b0df57c`, **`c3c90f9` (M14/M21)**, each verified before
  a `--no-ff` merge. Nothing has been pushed.
* Branch **`milestone/attention-constitutions-context`** (off `main` after `c3c90f9`, not merged, not pushed): the
  owner's stage "Intelligence, Constitutions, Context & Notifications". Order: notifications ✅ → Constitution
  Manager 🚧 → Context Manager ☐. No workflow engine in this stage.
* Phases: Phase 1 ✅ · M9–M10 ✅ · M11 partial · M12–M14 ✅ · M16–M21 ✅ · stage milestones A–C ✅, D 🚧.

## What exists now (session 7)

* **Subsystem boundaries** (ADR-0026): Constitutions, Context, Memory, Skills, Workflows mapped onto the existing
  modules with narrow interfaces; workflows stay documents (ADR-0012).
* **Notifications & attention** (ADR-0025): one event sink (`notify::publish`, SQLite `notifications`, migration
  0010) used by hooks, MCP, CLI and desktop; dedupe (×n), rate limits, retention, resolve-on-continue.
  Observe-only hooks: PermissionRequest, Notification types, PreToolUse(AskUserQuestion), Elicitation, Stop,
  StopFailure; session status with certainty (`reported` / `assumed`, never inferred from silence); ownership
  checks (identity from the session row, foreign conversations ignored); MCP `request_attention` (bound, limited).
  Desktop: bell with unread/action counts and the sessions that need you, notification center (inbox, history,
  filters, read/dismiss/snooze/open), preferences (desktop, sound, focused session, quiet hours, per category,
  muted workspaces/profiles, test notification, OS availability), session badges, attention board; native WinRT
  toasts with click → focus → workspace → terminal; recording adapter for E2E. CLI: `nexus notifications`,
  ATTENTION column in `nexus sessions`.
* Earlier: profile diagnostics, AI Kits, lifecycle hooks, knowledge and tasks with lazy context, task-driven
  sessions, branch-aware notes, instruction inspector, profile instruction isolation, resume project, proposals.

## Verified behaviour (2026-11-04, session 7 — notification slice)

| Area | Evidence |
| --- | --- |
| Rust | `cargo test --workspace`: 192 passed — core 140 unit + git 5 + crash 2 + isolation 4; CLI 25 E2E + 6 MCP unit; desktop 6 unit + 4 PTY |
| Frontend | 51 vitest (new: bell, notification click routing incl. workspace switch / vanished session, status badges); `vue-tsc` clean; `vite build` OK |
| Desktop E2E | `run-smoke.mjs` 27/27 · `task-flow.mjs` 29/29 · `profiles-kits.mjs` 9/9 · **`attention.mjs` 23/23** |
| T18 / MCP interop | production build check OK · official SDK 18 tools |
| Real Claude 2.1.311 (synthetic profiles) | hooks (now incl. attention: hooks accepted; real `StopFailure` → "Claude is not signed in for this profile" notification of that session) · instructions · security · kits — all OK |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` clean (also with the E2E feature) |

Not verifiable here: permission/idle/question/elicitation events need a signed-in model turn (tested with documented
payloads; MANUAL_CHECKS step 23 with the owner's Personal profile); real Windows toasts (production notifier) and
their click from the Action Center; macOS/Linux notifier (compiles in CI only).

## Next actions (in order)

1. Milestone D — Constitution Manager (in progress, uncommitted start: `knowledge/constitution.rs`, git file history
   helpers, schema keys `format`/`prefix`/`task`/`workflow`): IPC + desktop view (tree + raw editor, search,
   conflicts, effective preview, applicable projects, history), launch delivery, MCP `get_constitution`, CLI, tests.
2. Milestone E — Context Manager; then F (integration).
3. Owner: MANUAL_CHECKS 13–23 with the Personal profile.

## Known issues / decisions to revisit

* Release gates (owner decision): identifier `dev.nexus.desktop` provisional, no license, code signing, no
  distributed build with `e2e-webview-debugging`, bundle `nexus` CLI as a Tauri sidecar.
* Development builds' toasts are attributed to "Windows PowerShell" (no AppUserModelID for unpackaged executables).
* Permission prompts for tools outside the PostToolUse matcher resolve at the next turn end/prompt.
* Kits reach only NEXUS-launched sessions by design; project `.claude/` applies to every profile (Claude behaviour).
* Knowledge files are not watched; `<data dir>/sessions/<id>/` and kit overlays are not pruned; `freezePrototype`
  off (xterm.js); portable-pty Windows kill workaround; ADR-0015 HKCU policy writability unverified.

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
npx tauri build --debug --no-bundle && node e2e/t18-check.mjs
npx tauri build --debug --no-bundle --features e2e-webview-debugging
node e2e/run-smoke.mjs && node e2e/task-flow.mjs && node e2e/profiles-kits.mjs && node e2e/attention.mjs
node e2e/mcp-interop.mjs                            # official MCP SDK client vs nexus mcp
node e2e/hooks-real-claude.mjs                      # real Claude runs NEXUS hooks (synthetic profile)
node e2e/instructions-real-claude.mjs               # inspector vs. real Claude InstructionsLoaded
node e2e/security-real-claude.mjs                   # isolation + task text as data vs. real Claude
node e2e/kits-real-claude.mjs                       # diagnostics + per-profile kits vs. real Claude
# CLI
target/debug/nexus notifications list ; nexus notifications read --all
target/debug/nexus profiles diagnose <profile> ; nexus kits list
```

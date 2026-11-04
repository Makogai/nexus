# CURRENT_STATE

_Last updated: 2026-11-04 (end of session 7)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

* `main` = baseline + merges `92475ec`, `8f51af2`, `1c3f9bb`, `b0df57c`, `c3c90f9` (M14/M21), each verified before a
  `--no-ff` merge. Nothing has been pushed.
* Branch **`milestone/attention-constitutions-context`** (off `main` after `c3c90f9`, not merged, not pushed) — the
  owner's stage "Intelligence, Constitutions, Context & Notifications", milestones A–F ✅:
  `5641872` (A–C notifications & attention), `1cb6f3d` (D Constitution Manager), E+F commit (Context Manager and
  integration). No workflow engine in this stage (ADR-0012 stands).
* Phases: Phase 1 ✅ · M9–M10 ✅ · M11 partial · M12–M14 ✅ · M16–M21 ✅ · stage A–F ✅.

## What exists now (session 7)

* **Subsystem boundaries** (ADR-0026): Constitutions, Context, Memory, Skills, Workflows on shared infrastructure.
* **Notifications & attention** (ADR-0025): one event sink (`notify::publish`, migration 0010) for hooks, MCP, CLI
  and desktop; dedupe (×n), rate limits, retention, resolve-on-continue. Observe-only hooks (PermissionRequest,
  Notification types, PreToolUse(AskUserQuestion), Elicitation, Stop, StopFailure); session status with certainty
  (never inferred from silence); identity from the session row; MCP `request_attention`. Desktop bell, notification
  center, preferences (desktop, sound, focused session, quiet hours, per category, muted workspaces/profiles),
  attention board, session badges, WinRT toasts with click → focus → workspace → terminal; recording adapter for
  E2E. CLI `nexus notifications`, ATTENTION column in `nexus sessions`.
* **Constitution Manager** (ADR-0027): `format: constitution` rule documents — articles → sections → rules with
  stable ids and enforcement (mandatory policy / rule / guideline); scopes global → workspace → stack → project →
  task/workflow; overrides/disabling except of mandatory policies (conflict); synced versions need approval (review
  notification); compact budgeted delivery, the rest via MCP `get_constitution`. Desktop *Constitutions* (tree +
  Markdown editor, search, problems, approval, applicable projects, Git history, effective-rules preview);
  `nexus constitution`.
* **Context Manager** (ADR-0028): per-project strategy minimal / balanced (default = earlier behaviour) / deep /
  custom (clamped sources and budgets, migration 0011); onboarding analysis with computed per-strategy sizes and an
  explained recommendation; project README/docs as a cited, deduplicated FTS source (indexed-only, allowlisted,
  credential files skipped); InstructionsLoaded observation and retrieval recording (counts and ids, never queries);
  Context Inspector (configured / predicted / observed / on demand / retrieved) and metrics. Desktop *Context*
  (project page → Context); `nexus context show|set|analyze|inspect`. 19 MCP tools.
* **Integration** (F): live cross-profile instruction bleed warning (T21) from observed instructions, pointing at
  instruction isolation; constitution delivery follows the strategy; review notifications open the right view.
* Earlier: profile diagnostics, AI Kits, lifecycle hooks, knowledge and tasks with lazy context, task-driven
  sessions, branch-aware notes, instruction inspector, profile instruction isolation, resume project, proposals.

## Verified behaviour (2026-11-04, end of session 7)

| Area | Evidence |
| --- | --- |
| Rust | `cargo test --workspace`: 204 passed — core 150 unit + git 5 + crash 2 + isolation 4; CLI 27 E2E + 6 MCP unit; desktop 6 unit + 4 PTY |
| Frontend | 58 vitest (notifications, constitutions, context added); `vue-tsc` clean; `vite build` OK |
| Desktop E2E | `run-smoke` 27/27 · `task-flow` 29/29 · `profiles-kits` 9/9 · **`attention` 23/23** · **`constitutions` 13/13** · **`context` 10/10** |
| T18 / MCP interop | production build check OK · official SDK, 19 tools |
| Real Claude 2.1.311 (synthetic profiles) | hooks (attention hooks accepted; real StopFailure → "not signed in" notification) · instructions · **security 14/14** (incl. live bleed warning in the control run, none with isolation) · kits 17/17 · **context 7/7** (observed InstructionsLoaded == inspector prediction: user memory, CLAUDE.md, CLAUDE.local.md, rule) |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` clean (also with the E2E feature) |

Not verifiable here: permission/idle/question/elicitation events need a signed-in model turn (documented payloads
in tests; MANUAL_CHECKS 23); real Windows toasts from the production notifier and Action Center clicks;
macOS/Linux notifier (compiles in CI only); anything with the owner's real profiles (MANUAL_CHECKS 13–25); the
GitHub Actions workflow (nothing pushed).

## Next actions (in order)

1. Owner: review `milestone/attention-constitutions-context`; approve merging into `main`; MANUAL_CHECKS 13–25.
2. Workflow engine slice (recommendations below); token analytics slice.
3. M14 remainder (allowlist profile cloning); M11 remainder (`claude agents --json`); housekeeping (prune session
   artifacts and kit overlays, knowledge file watching).

## Recommendations for the workflow engine (unchanged principle: ADR-0012, no custom orchestrator)

1. A run = a task whose checklist is the workflow's steps + a session-scoped skill; progress, resume, checkpoints,
   proposals and now notifications (task blocked/done, `workflow-failed` is reserved) work unchanged.
2. Workflow-scoped constitutions already exist (`workflow:` scope) — gates can be mandatory policies of the run.
3. The Context Manager's retrieval metrics show whether a workflow's skill/docs are actually read.

## Known issues / decisions to revisit

* Release gates (owner decision): identifier `dev.nexus.desktop` provisional, no license, code signing, no
  distributed build with `e2e-webview-debugging`, bundle `nexus` CLI as a Tauri sidecar.
* Development builds' toasts are attributed to "Windows PowerShell" (no AppUserModelID for unpackaged executables).
* Permission prompts for tools outside the PostToolUse matcher resolve at the next turn end/prompt.
* InstructionsLoaded costs one short hook process per instruction file at start (off in minimal).
* Context strategy is machine-local (per developer); a portable team default could come later.
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
node e2e/run-smoke.mjs && node e2e/task-flow.mjs && node e2e/profiles-kits.mjs
node e2e/attention.mjs && node e2e/constitutions.mjs && node e2e/context.mjs
node e2e/mcp-interop.mjs                            # official MCP SDK client vs nexus mcp
node e2e/hooks-real-claude.mjs && node e2e/instructions-real-claude.mjs && node e2e/security-real-claude.mjs
node e2e/kits-real-claude.mjs && node e2e/context-real-claude.mjs   # real Claude, synthetic profiles
# CLI
target/debug/nexus notifications list ; nexus constitution effective --project <p> ; nexus context analyze --project <p>
```

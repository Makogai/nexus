# CURRENT_STATE

_Last updated: 2026-11-05 (end of session 7)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

* `main` = baseline + merges `92475ec`, `8f51af2`, `1c3f9bb`, `b0df57c`, `c3c90f9` (M14/M21), **`c0bb3f9`**
  (notifications, Constitution Manager, Context Manager — stage A–F, fully verified before the merge). Nothing pushed.
* Branch **`milestone/workflow-engine`** (off `main` at `c0bb3f9`): the Workflow Engine (ADR-0029), two pre-release
  items (notification sender/privacy, constitution compliance) and gate approval/versioning. **Committed locally**
  after the sequential verification below passed in full; not merged, not pushed, no release.

## What exists now (session 7, committed on the branch)

* **Workflow Engine** (ADR-0029): portable `format: workflow` definitions (steps, dependencies, acceptance criteria,
  gate names, minimum rigor, session-only skills), six templates (feature, bugfix, review, refactor, testing,
  release), approval of synced versions; machine-local **quality gates** (argv, no shell, defined by the person,
  suggestions from detected scripts); runs on tasks (migration 0012) with AI-reported / verified / human / skipped per
  step, rigor from the work profiles, retry, cancel, failure handling, crash reconciliation; desktop runner thread and
  `nexus workflow verify`; session integration (pointer, SessionStart brief, MCP `get_workflow_run` / `report_step`,
  21 tools), skills only in the run's session and only from kits enabled for the profile; notifications (input,
  review, gate failed, completed, failed); desktop *Workflows* (library, runs, gates), entry points from project page
  and task dialog; `nexus workflow …`, `nexus gates …`.
* **Notifications pre-release** (ADR-0025 amendment): own WinRT toasts with NEXUS as registered sender, tagged toasts
  withdrawn when handled, stale ones cleared, `nexus://notification/<id>` activation (installed builds), lock-screen
  content policy (generic while locked by default / always generic / always details), NSIS uninstall hook.
* **Constitution compliance** (ADR-0027 amendment): rules may name a gate (`check=`); effective view, CLI and MCP
  show *instructed only* vs the gate's latest result; `nexus constitution verify`, workflow `gates=constitution`.
* **Gate approval and versions** (ADR-0029 amendment, migration 0013): a gate runs only when its current command
  (program + args, hashed) was explicitly approved; a changed command is a new version that needs approval again;
  checked in the single execution choke point `gates::begin`; separate from workflow approval. Steps waiting for a
  gate approval are blocked without executing anything, notify, and resume verification once approved. CLI
  `nexus gates approve --version n` / `revoke`, desktop Approve (confirmation shows the exact command) / Revoke,
  results record the gate version, constitution compliance `not-approved`.

## Verification log (sequential groups, `CARGO_BUILD_JOBS=2`; latest code = after gate approval, requirement 10)

Run one group at a time; free memory checked before each group. "Latest" = run against the final code of this stage.

| # | Command | Result | Latest |
| --- | --- | --- | --- |
| 0 | `cargo test -p nexus-core --lib workflow` | passed 13/13 | yes |
| 1 | `cargo test --workspace --no-fail-fast` | passed — 220 tests, 0 failed (core 163, CLI 28 + 6, db_crash 2, git 5, isolation 4, desktop lib 8, pty 4; toast self-test ignored = opt-in) | yes |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | passed, no warnings (re-checked after touching all crates) | yes |
| 3 | `cargo clippy -p nexus-desktop --all-targets --features e2e-webview-debugging -- -D warnings` | passed, no warnings | yes |
| 4 | `npm test` · `npm run typecheck` · `npx vite build` (apps/desktop) | 63/63 vitest · typecheck clean (first run found a test fixture missing `gateVersion`; fixed) · build ok | yes |
| 5 | `npx tauri build --debug --no-bundle` + `node e2e/t18-check.mjs` | build ok · T18 OK 4/4 (no debugging switches, no DevTools endpoint, override ignored and reported) | yes |
| 6 | `npx tauri build --debug --no-bundle --features e2e-webview-debugging` (once) + `cargo build -p nexus-cli` | ok (used by 7–13) | yes |
| 7 | `node e2e/run-smoke.mjs` | passed 27/27 | yes |
| 8 | `node e2e/task-flow.mjs` | passed 29/29 | yes |
| 9 | `node e2e/profiles-kits.mjs` | passed 9/9 | yes |
| 10 | `node e2e/attention.mjs` | passed 27/27 | yes |
| 11 | `node e2e/constitutions.mjs` | passed 13/13 | yes |
| 12 | `node e2e/context.mjs` | passed 9/9 | yes |
| 13 | `node e2e/workflows.mjs` | passed 22/22 (incl. gate approval through the UI, forged approval refused, changed command waits for re-approval and resumes) | yes |
| 14 | `node e2e/mcp-interop.mjs` | passed 9/9 (official SDK; 21 tools, read-only annotated) | yes |
| 15 | `node e2e/hooks-real-claude.mjs` | passed 9/9 (Claude Code 2.1.311) | yes |
| 16 | `node e2e/instructions-real-claude.mjs` | passed 6/6 | yes |
| 17 | `node e2e/security-real-claude.mjs` | passed 14/14 | yes |
| 18 | `node e2e/kits-real-claude.mjs` | passed 17/17 | yes |
| 19 | `node e2e/context-real-claude.mjs` | passed 7/7 | yes |
| 20 | `node e2e/workflow-real-claude.mjs` | passed 7/7 | yes |
| 21 | `cargo test -p nexus-desktop --test toast_selftest -- --ignored` (opt-in, real Windows sender; writes and removes a temporary HKCU key) | passed earlier in the stage, not rerun | no — but `notifier.rs` is unchanged since that pass |

After the log, only the gate-redaction test fixture changed (test code, see below); `cargo test -p nexus-core --lib
workflow::gates` (2/2) and `cargo clippy -p nexus-core --all-targets -- -D warnings` were rerun and passed.

Real-Claude suites use synthetic profiles in temp dirs and temporary repositories only. Peak load stayed well
within memory (free RAM 4.3–5.8 GB between groups, low-memory watchdog never fired); no leftover processes.

**Pre-commit security inspection** (all changed and untracked files): no NUL bytes; no keys, tokens, JWTs,
passwords or connection strings in added lines (the gate-redaction test's synthetic `ghp_` fixture is now built at
runtime like the one in `context/docs.rs`); the only email addresses are synthetic git identities on
`example.invalid`; no personal paths or names; no new `unsafe`, no shell strings; registry writes only under NEXUS's
own HKCU keys (`AppUserModelId\dev.nexus.desktop[.dev]`, `Classes\nexus` in installed builds), removed by the
uninstaller hook.

## Next actions (in order)

1. Owner: review `milestone/workflow-engine`; MANUAL_CHECKS 13–27 (27 = installed build notifications); merge when
   approved (`--no-ff`, no push yet).
2. Rerun the opt-in `toast_selftest` and MANUAL_CHECKS 27 on the release candidate build.
3. Token analytics; workspace synchronisation (M29); release gates.

## Known issues / decisions to revisit

* Release gates: identifier `dev.nexus.desktop` provisional, no license, code signing, no distributed build with
  `e2e-webview-debugging`, bundle `nexus` CLI as a Tauri sidecar; MANUAL_CHECKS 27 on the release candidate.
* Lock detection is best effort (LogonUI process); the displayed sender name comes from the registration (visual
  check in MANUAL_CHECKS 27). Development builds register `dev.nexus.desktop.dev` in HKCU on their first notification.
* Gates run with the user's rights; a run needs the project folder on this machine to verify. Gate approval is
  machine-local like the gates themselves: it guards against unseen command changes, not against a person approving
  a harmful command. Approving one gate resumes every step of that project waiting on gates (they re-block with a
  fresh reason if another gate is still unapproved).
* Context strategy is machine-local; InstructionsLoaded costs one hook process per instruction file at start.
* Knowledge files are not watched; session artifacts and kit overlays are not pruned.

## How to run

```text
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
cargo build -p nexus-cli -p fake-claude
cd apps/desktop && npm test && npm run typecheck
npx tauri build --debug --no-bundle && node e2e/t18-check.mjs
npx tauri build --debug --no-bundle --features e2e-webview-debugging
node e2e/run-smoke.mjs; node e2e/task-flow.mjs; node e2e/profiles-kits.mjs; node e2e/attention.mjs
node e2e/constitutions.mjs; node e2e/context.mjs; node e2e/workflows.mjs; node e2e/mcp-interop.mjs
node e2e/hooks-real-claude.mjs; node e2e/instructions-real-claude.mjs; node e2e/security-real-claude.mjs
node e2e/kits-real-claude.mjs; node e2e/context-real-claude.mjs; node e2e/workflow-real-claude.mjs
cargo test -p nexus-desktop --test toast_selftest -- --ignored   # opt-in, real Windows notifications
```

# CURRENT_STATE

_Last updated: 2026-11-05 (session 8)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

* `main` = baseline + merges … `c0bb3f9` (notifications, constitutions, context) + **`cba2b7a`** (Workflow Engine,
  gate approval, notification and compliance pre-release — verified in full before the merge). Nothing pushed.
* Branch **`milestone/usage-analytics`** (off `main` at `cba2b7a`): approval preview for gates, token usage
  analytics (ADR-0030) and the Git workspace sync architecture with a read-only readiness check (ADR-0031).
  Committed locally after the sequential verification below; not merged, not pushed, no release.

## What exists now (session 8, on the branch)

* **Gate approval preview** (ADR-0029 amendment): the approval dialog and `nexus gates list` show how many waiting
  workflow steps (in how many runs) resume automatically and how many keep waiting for other gates; `approve` reports
  it. Resuming is now exact — only steps whose gates are then all defined and approved (no re-block noise). Same
  single approval; the security guarantees and their tests are unchanged.
* **Usage analytics** (ADR-0030, migration 0014): per-response token counts parsed from the transcripts Claude Code
  writes into each profile's config dir (read-only, incremental, deduplicated, numbers only, subagent files, version
  compatibility list `2.1`); attribution to NEXUS sessions across `--resume` and `/clear`; provenance on every figure —
  *parsed* tokens, *estimates* (Claude Code's own cost for API-billed profiles only — subscriptions are never shown as
  spend — and NEXUS-added context ÷ 4), *unavailable* with the reason; breakdowns by project, profile, task, workflow
  run, context strategy, work profile and model with efficiency figures; coverage. Desktop *Usage* dashboard and
  summaries on the project page, workflow run and task; `nexus usage [--by …]`, `nexus usage sessions|scan`.
  Telemetry is never set, forwarded or changed; organisation-managed telemetry is reported.
* **Workspace sync preparation** (ADR-0031): Git in the person's repository, no sync service; read-only
  `nexus workspaces sync-status` (what would travel; databases, `.env`, keys and credential files by name; local-only
  knowledge and the data folder; pending approvals; ahead/behind as of the last fetch).
* Previous stage (merged): Workflow Engine, gate approval and versions, notification sender/privacy, constitution
  compliance — see ADR-0029, ADR-0025/0027 amendments.

## Verification log (sequential groups, `CARGO_BUILD_JOBS=2`)

One group at a time; free memory checked between groups. "Latest" = run against the final code of this stage.

| # | Command | Result | Latest |
| --- | --- | --- | --- |
| 1 | `cargo test --workspace --no-fail-fast` (foreground, 2 jobs; first attempt in the background was stopped by the system for low memory, rerun after the owner freed memory) | passed — 233 tests, 0 failed (core 174, CLI 30 + 6, db_crash 2, git 5, isolation 4, desktop lib 8, pty 4; toast self-test ignored = opt-in); rerun after the clippy fixes | yes |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | passed after 3 fixes (complex type → alias, collapsible `if`, `is_multiple_of`) | yes |
| 3 | `cargo clippy -p nexus-desktop --all-targets --features e2e-webview-debugging -- -D warnings` | passed, no warnings | yes |
| 4 | `npm test` · `npm run typecheck` · `npx vite build` (apps/desktop) | 69/69 vitest · typecheck clean · build ok | yes |
| 5 | `npx tauri build --debug --no-bundle` + `node e2e/t18-check.mjs` | build ok · T18 OK 4/4 | yes |
| 6 | `npx tauri build --debug --no-bundle --features e2e-webview-debugging` (once) + `cargo build -p nexus-cli -p fake-claude` | ok (used by 7–14) | yes |
| 7 | `node e2e/run-smoke.mjs` | passed 27/27 | yes |
| 8 | `node e2e/task-flow.mjs` | passed 29/29 | yes |
| 9 | `node e2e/profiles-kits.mjs` | passed 9/9 | yes |
| 10 | `node e2e/attention.mjs` | passed 27/27 | yes |
| 11 | `node e2e/constitutions.mjs` | passed 13/13 | yes |
| 12 | `node e2e/context.mjs` | passed 9/9 | yes |
| 13 | `node e2e/workflows.mjs` | passed 24/24 (incl. the approval dialog's resume preview) | yes |
| 14 | `node e2e/usage.mjs` | passed 18/18 | yes |
| 15 | `node e2e/mcp-interop.mjs` | passed 9/9 (21 tools) | yes |
| 16 | `node e2e/hooks-real-claude.mjs` | passed 9/9 (Claude Code 2.1.311) | yes |
| 17 | `node e2e/instructions-real-claude.mjs` | passed 6/6 | yes |
| 18 | `node e2e/security-real-claude.mjs` | passed 14/14 | yes |
| 19 | `node e2e/kits-real-claude.mjs` | passed 17/17 | yes |
| 20 | `node e2e/context-real-claude.mjs` | passed 7/7 | yes |
| 21 | `node e2e/workflow-real-claude.mjs` | passed 7/7 | yes |
| 22 | `node e2e/usage-real-claude.mjs` | passed 7/7 (real transcript format parsed; nothing invented) | yes |

Real-Claude suites use synthetic profiles in temp dirs and temporary repositories only.

**Memory:** measured with a 3-second sampler — free RAM 12.6 GB at the start, lowest 9.7 GB (during the E2E desktop
build with 2 jobs), i.e. the heaviest group used about 3 GB. The first, background attempt of group 1 was stopped by
the system while only ~7 GB were free; it was not relaunched until the owner had freed memory.

## Next actions (in order)

1. Owner: review `milestone/usage-analytics`; MANUAL_CHECKS 28 (usage with a signed-in profile vs `/cost`), 29, 30;
   merge when approved (`--no-ff`, no push yet).
2. M29 workspace sync flow on the readiness check (outgoing diff review, explicit commit/push, fast-forward pull).
3. U5 OpenTelemetry opt-in for personal profiles (*measured* provenance); release gates; MANUAL_CHECKS 27.

## Known issues / decisions to revisit

* Release gates: identifier `dev.nexus.desktop` provisional, no license, code signing, no distributed build with
  `e2e-webview-debugging`, bundle `nexus` CLI as a Tauri sidecar; MANUAL_CHECKS 27 on the release candidate.
* Lock detection is best effort (LogonUI process); the displayed sender name comes from the registration (visual
  check in MANUAL_CHECKS 27). Development builds register `dev.nexus.desktop.dev` in HKCU on their first notification.
* Gates run with the user's rights; a run needs the project folder on this machine to verify. Gate approval is
  machine-local like the gates themselves: it guards against unseen command changes, not against a person approving
  a harmful command. Gates that existed before the workflow stage need one approval after the upgrade.
* Context strategy is machine-local; InstructionsLoaded costs one hook process per instruction file at start.
* Knowledge files are not watched; session artifacts and kit overlays are not pruned.
* Usage token counts are *parsed* (ADR-0010): Claude Code's transcript format is undocumented; versions outside
  `VERIFIED_FORMATS` are flagged. Not verified with a signed-in profile yet (the automated real-CLI check uses a
  signed-out synthetic profile) — MANUAL_CHECKS 28. Claude's background helper calls are not in the per-response
  counts. Sessions started outside NEXUS are not attributed to projects.
* Cost estimates need the profile's billing classification (from a sign-in check); unknown billing is withheld.
* Sync: readiness only; no fetch/commit/push yet (M29). The dashboard has no sync UI yet.

## How to run

```text
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
cargo build -p nexus-cli -p fake-claude
cd apps/desktop && npm test && npm run typecheck
npx tauri build --debug --no-bundle && node e2e/t18-check.mjs
npx tauri build --debug --no-bundle --features e2e-webview-debugging
node e2e/run-smoke.mjs; node e2e/task-flow.mjs; node e2e/profiles-kits.mjs; node e2e/attention.mjs
node e2e/constitutions.mjs; node e2e/context.mjs; node e2e/workflows.mjs; node e2e/usage.mjs; node e2e/mcp-interop.mjs
node e2e/hooks-real-claude.mjs; node e2e/instructions-real-claude.mjs; node e2e/security-real-claude.mjs
node e2e/kits-real-claude.mjs; node e2e/context-real-claude.mjs; node e2e/workflow-real-claude.mjs
node e2e/usage-real-claude.mjs
cargo test -p nexus-desktop --test toast_selftest -- --ignored   # opt-in, real Windows notifications
```

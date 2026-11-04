# CURRENT_STATE

_Last updated: 2026-11-04 (end of session 3)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

* `main` = baseline `68affc0` + merge `92475ec` of the approved M9/M10/M12 milestone (`--no-ff`, verified from a
  clean worktree before merging). Nothing has ever been pushed.
* Branch **`milestone/m13-hooks-t18`** (off `main`, not merged, not pushed) holds session 3:
  * `fix(security)` T18 — production builds ignore WebView2 debugging overrides (ADR-0015);
  * `feat(hooks)` M13 — Claude lifecycle hooks with deterministic activity capture (ADR-0016);
  * `feat(knowledge)` M16 + M19 core — portable workspace knowledge and tasks, context hierarchy, lazy loading
    (ADR-0013 implemented, ADR-0017).
* Phases: Phase 1 ✅ · M9 ✅ · M10 ✅ · M11 partial · M12 ✅ · M13 ✅ · M16 ✅ · M17 partial (checkpoints) ·
  M19 🚧 (launch from a task open) · M20/M21 partial (rules inspector, skills catalog).

## What exists now

* **Hooks (M13)**: per-session `--settings` with exec-form handlers → `nexus hook` (always exit 0, no prompt text
  or raw commands stored); session start/end, compaction recovery, activity counters, files touched, command
  categories, "waiting for you"; omitted with a reason under `disableAllHooks` / `allowManagedHooksOnly`.
* **Knowledge (ADR-0013/0017)**: Markdown + strict YAML-subset frontmatter for rules, tasks, decisions, notes,
  specs, workflows; file name = UUIDv7 id; atomic fsync'd writes with optimistic concurrency (content hash);
  SQLite FTS5 index (migration 0005), incremental refresh, rebuild, invalid-file and duplicate-id reporting;
  `.nexus-local/` (git-ignored) for local-only; global constitutions in `<data dir>/global/`; secrets and absolute
  paths refused.
* **Context hierarchy and lazy loading**: global → workspace → stack → project rules; only *approved* always-on
  rules (≤ 1,500 chars) are appended to the system prompt; SessionStart adds a one-line count pointer; everything
  else via MCP (`get_context_index`, `search_knowledge`, `get_document`, `list_tasks`, `create_task`,
  `update_task`, `record_memory`). Claude may write tasks, notes (unverified) and decisions (proposed) only.
  Workspace and project skills are catalogued (name + description).
* **Surfaces**: `nexus knowledge reindex|list|search|show|rules|approve|new`, `nexus tasks list|add|move`; desktop
  **Knowledge** view (task board with drag/select, per-kind lists, search, review-then-approve for always-on
  rules, Problems tab, create dialog, rescan/rebuild); MCP server now has 15 tools (11 read-only).

## Verified behaviour (2026-11-04, end of session 3)

| Area | Evidence |
| --- | --- |
| Rust | `cargo test --workspace`: 156 passed — core 115 unit + git 5 + crash 2 + isolation 4; CLI 14 E2E + 6 MCP unit; desktop 6 unit (webview guard) + 4 PTY |
| Knowledge E2E (real `nexus` binary) | round trip through files and index incl. editor edits, broken files, duplicate ids, rebuild; rules reach launches by hierarchy only once approved and are withdrawn when edited; MCP knowledge invisible across workspaces |
| Frontend | 34 vitest (incl. TaskBoard, KnowledgeDialog, KnowledgeView, knowledge helpers); `vue-tsc` clean; `vite build` via `tauri build` |
| Desktop E2E (production frontend + CSP, test-only CDP feature) | `e2e/run-smoke.mjs`: **27/27** — adds: synced rule flagged → reviewed → approved in the UI → reaches the system prompt; UI task is a file; board move rewrites it; Claude's MCP task appears on the board |
| T18 | `e2e/t18-check.mjs` on the production build: hostile WebView2 env ignored, no DevTools endpoint, override logged |
| MCP interop | official TS SDK: 15 tools, 11 read-only, knowledge write + context index |
| Real Claude hooks | `e2e/hooks-real-claude.mjs` (Claude Code 2.1.311, synthetic signed-out profile): SessionStart/SessionEnd reach NEXUS |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` clean |

Not verified (deliberately): anything with the owner's real Claude profiles → `docs/MANUAL_CHECKS.md`.
Not yet run: the GitHub Actions workflow (nothing pushed).

## Bugs found and fixed this session

* Frontmatter: an empty `key:` mid-document became an empty list (now null); credential heuristic matched across
  spaces (now requires a contiguous token); local-only writes failed when the workspace folder was missing.
* Smoke test drove the Knowledge view before the app left onboarding (fixed by reloading after IPC setup).

## Known issues / decisions to revisit

* Release gates (owner decision): identifier `dev.nexus.desktop` provisional, no license, code signing, no
  distributed build with `e2e-webview-debugging`, bundle `nexus` CLI as a Tauri sidecar (MCP and hooks need it next
  to the app). See `docs/ROADMAP.md`.
* ADR-0015: writability of `HKCU\Software\Policies` by a non-admin user was not verified (the check reads it anyway).
* Approvals of always-on rules are per machine — after cloning a workspace elsewhere they must be reviewed once.
* Knowledge files are not watched; the desktop re-scans on view activation, window focus and DB changes.
* The desktop has no in-app editor for document bodies (files are edited in the user's editor; "Show file").
* `<data dir>/sessions/<id>/` artifacts are not pruned yet; project inspection holds the UI DB mutex during short
  git calls; `freezePrototype` off (xterm.js); portable-pty 0.9 Windows kill workaround.

## Next actions (in order)

1. Owner: review branch `milestone/m13-hooks-t18` (3 commits); approve merging into `main`; run
   `docs/MANUAL_CHECKS.md` with the Personal profile (now also: hooks fire, knowledge rules in a real session).
2. M19 remainder: launch Claude from a task (task id in the launch, task pointer at SessionStart, status update).
3. M18 "Resume project" view: Git state + last sessions + checkpoints + open tasks.
4. M17: branch-aware notes; consolidation of session activity into proposed notes (human review, no per-call AI).
5. M11 remainder (`claude agents --json`), M14 profile diagnostics, M20 CLAUDE.md layers in the inspector.

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
npx tauri build --debug --no-bundle --features e2e-webview-debugging && node e2e/run-smoke.mjs
node e2e/mcp-interop.mjs                            # official MCP SDK client vs nexus mcp
node e2e/hooks-real-claude.mjs                      # real Claude runs NEXUS hooks (synthetic profile)
# CLI
target/debug/nexus knowledge --help
target/debug/nexus tasks --help
```

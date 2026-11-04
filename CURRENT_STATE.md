# CURRENT_STATE

_Last updated: 2026-11-04 (end of session 4)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

* `main` = baseline `68affc0` + merge `92475ec` (M9/M10/M12) + merge `8f51af2` (T18, M13 hooks, ADR-0013 knowledge),
  each verified before a `--no-ff` merge. Nothing has ever been pushed.
* Branch **`milestone/m17-m19-m20-task-flow`** (off `main`, not merged, not pushed) holds session 4: task-driven
  sessions (M19), branch-aware knowledge (M17), the effective-instructions inspector (M20) — ADR-0018, ADR-0019.
* Phases: Phase 1 ✅ · M9 ✅ · M10 ✅ · M11 partial · M12 ✅ · M13 ✅ · M16 ✅ · M17 ✅ (activity consolidation open) ·
  M19 ✅ · M20 ✅ · M21 partial (skills catalog).

## Security follow-up (session 5, on the same branch before merging)

* **Profile instruction isolation** (ADR-0020, migration 0007): opt-in per profile; session-scoped `claudeMdExcludes`
  for exactly the default profile's `~/.claude/CLAUDE.md` and `~/.claude/rules/**` (literal paths + bracket-escaped
  glob, case from the session cwd); works under hooks-blocking policy; never touches Claude configs or managed values.
  Desktop profile dialog option with warning; `nexus profiles add --isolate-instructions`, `nexus profiles isolate`.
* **Task text as untrusted data**: the system prompt carries only a pointer with the task id; title/description/
  checklist arrive as one escaped `<nexus-task-data>` block in SessionStart context; Claude's `--name` is
  `NEXUS task <id>`. Adversarial tests (unit, CLI) and a real-CLI check.
* Found by the new tests and fixed: the task title reached Claude verbatim as `--name`; the inspector called
  external imports loaded (they need approval) and matched excludes case-insensitively (Claude is case-sensitive).
* Verified against the real CLI: `e2e/security-real-claude.mjs` 12/12 (synthetic `home (x)`, control + isolated runs).

## What exists now (session 4)

* **Launch from a task** — desktop (project page *Open tasks* → Launch, board card ✦, task dialog) and
  `nexus launch --task <id|title>`: the task's project, the chosen profile and its work profile, task title as
  session name; backlog/ready → in-progress (optional); Claude gets a ≤ 900-char brief (no history, framed as data).
  `sessions.task_id` links the session; resume keeps it; after resume/compaction SessionStart adds the task's
  *current* state (a resumed conversation keeps its original system prompt).
* **Task workflow through MCP** — `get_task` and `update_task` (default: the session's task): status, dated
  `[claude]` log line, checklist `check`/`uncheck`, `linkBranch`, `expectedHash`; status/log also become session
  checkpoints. Desktop task dialog: checklist toggles, recent log, sessions with Resume. `nexus tasks show`,
  `tasks move --check`. MCP has 16 tools (12 read-only).
* **Branch-aware knowledge** — `branch:`/`commit:` on any document; relevance from the live Git history
  (this branch / inherited after merge / other branch hidden / unbound); `record_memory` scope `branch`;
  `knowledge new --branch current`; `knowledge list --project` shows relevance; project page lists notes for the
  checked-out branch.
* **Effective-instructions inspector** — Claude's own layers in Claude's order (managed, user, every ancestor's
  `CLAUDE.md`/`.claude/CLAUDE.md`/rules/`CLAUDE.local.md`, imports, excludes, conditional rules) plus what NEXUS
  appends; always-on rules already present in a loaded file are not sent twice. `nexus instructions`, desktop
  *Effective instructions* panel. Verified against the real CLI.
* **One preparation path** — `claude::prepare` builds MCP, hooks, rules and task brief for desktop and CLI.

## Verified behaviour (2026-11-04, end of session 4)

| Area | Evidence |
| --- | --- |
| Rust | `cargo test --workspace`: 167 passed — core 123 unit + git 5 + crash 2 + isolation 4; CLI 17 E2E + 6 MCP unit; desktop 6 unit + 4 PTY |
| CLI E2E (new) | task flow: launch --task → brief/profile/project → MCP get/update_task + branch note → file → resume with current state + SessionStart task line; inspector layers + duplicate suppression; branch relevance across checkout and merge |
| Frontend | 38 vitest (new: launch dialog task mode, board Launch, task dialog checklist, instructions panel); `vue-tsc` clean |
| Desktop E2E | `run-smoke.mjs` 27/27; **`task-flow.mjs` 20/20**: project → task → launch → Claude's MCP update visible on the board and in the file → app hard-killed and restarted → interrupted + resumable → Resume continues the conversation with the current task state; branch note and inspector render |
| T18 | `t18-check.mjs` on the production build |
| MCP interop | official TS SDK: 16 tools, 12 read-only |
| Real Claude (2.1.311, synthetic signed-out profiles) | `hooks-real-claude.mjs` OK; **`instructions-real-claude.mjs`**: the inspector's at-start set equals the 11 files Claude reports via `InstructionsLoaded` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` clean |

Not verified (deliberately): anything with the owner's real Claude profiles → `docs/MANUAL_CHECKS.md` (steps 13–17).
Not yet run: the GitHub Actions workflow (nothing pushed; it now includes the task-flow E2E).

## Findings and fixes this session

* **Cross-profile instruction bleed (Claude behaviour, verified)**: with `CLAUDE_CONFIG_DIR` set, the default
  profile's `~/.claude/CLAUDE.md` and `~/.claude/rules/` still load as project memory for projects inside the home
  folder. NEXUS does not change it silently; the inspector warns (threat T21). Follow-up: opt-in exclusion.
* AGENTS.md never loaded in the probes (documented fallback); shown as such, not claimed.
* Bug fixed: CLI short ids used the *start* of UUIDv7 ids, which collide for documents created together; the CLI
  now shows and matches the last 8 hex digits (ADR-0013 already said so).
* Prompt-injection hardening: the task brief is framed as data (threat T22).

## Known issues / decisions to revisit

* Release gates (owner decision): identifier `dev.nexus.desktop` provisional, no license, code signing, no
  distributed build with `e2e-webview-debugging`, bundle `nexus` CLI as a Tauri sidecar. See `docs/ROADMAP.md`.
* Managed `CLAUDE.md` / managed `claudeMd` handling is from the docs only (writing them needs admin rights).
* Approvals of always-on rules are per machine; knowledge files are not watched (re-scan on view/focus/DB change).
* `<data dir>/sessions/<id>/` artifacts are not pruned; project inspection holds the UI DB mutex during short git
  calls; `freezePrototype` off (xterm.js); portable-pty Windows kill workaround; ADR-0015 HKCU policy writability
  unverified.

## Next actions (in order)

1. Owner: review `milestone/m17-m19-m20-task-flow`; approve merging into `main`; run `docs/MANUAL_CHECKS.md`
   steps 13–17 with the Personal profile.
2. Opt-in per-profile exclusion of the default profile's home-folder instructions (session `claudeMdExcludes`, T21).
3. M18 "Resume project" view (Git state + last sessions + checkpoints + open tasks + next step).
4. M17 follow-up: consolidate session activity into *proposed* notes for human review (no per-call AI).
5. M11 remainder (`claude agents --json`), M14 profile diagnostics, M21 skills via `--plugin-dir`.

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
node e2e/run-smoke.mjs && node e2e/task-flow.mjs   # real-app E2E (Windows)
node e2e/mcp-interop.mjs                            # official MCP SDK client vs nexus mcp
node e2e/hooks-real-claude.mjs                      # real Claude runs NEXUS hooks (synthetic profile)
node e2e/instructions-real-claude.mjs               # inspector vs. real Claude InstructionsLoaded
# CLI
target/debug/nexus launch --task "<title>"
target/debug/nexus instructions --project <name>
```

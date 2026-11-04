# CURRENT_STATE

_Last updated: 2026-11-04 (end of session 5)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

* `main` = baseline `68affc0` + merges `92475ec` (M9/M10/M12), `8f51af2` (T18, M13, ADR-0013 knowledge) and
  `1c3f9bb` (M17/M19/M20 + security follow-up), each verified before a `--no-ff` merge. Nothing has been pushed.
* Branch **`milestone/m18-resume-project`** (off `main`, not merged, not pushed): M18 Resume project (ADR-0021) and
  proposed memories with human approval (ADR-0022).
* Phases: Phase 1 ✅ · M9 ✅ · M10 ✅ · M11 partial · M12 ✅ · M13 ✅ · M16 ✅ · M17 ✅ · M18 ✅ · M19 ✅ · M20 ✅ ·
  M21 partial (skills catalog).

## What exists now

* **Security follow-up (merged)** — ADR-0020:
  * opt-in per-profile *instruction isolation*: session-scoped `claudeMdExcludes` for exactly the default profile's
    `~/.claude/CLAUDE.md` and `~/.claude/rules/**`; never touches Claude configs or managed values; works under
    hooks-blocking policy;
  * task text is untrusted data: the system prompt carries only a pointer with the task id; title/description/
    checklist arrive as one escaped `<nexus-task-data>` block in SessionStart context; Claude's `--name` is neutral.
* **Resume project (M18)** — desktop view (project page button, command palette), `nexus projects resume`, MCP
  `get_resume_summary`: *verified now* (Git: branch, HEAD, changes, upstream, recent commits) kept apart from *saved
  context* (open tasks with checklist/last progress, notes and decisions for the checked-out branch, latest
  checkpoints flagged by branch, recent sessions, pending proposals), warnings, and one rule-based next step whose
  evidence lines say `verified:` or `saved:`. The primary button resumes the session or launches Claude for the task.
* **Proposed memories** — Claude's `decision`/`blocker`/`note` checkpoints become machine-local proposals at
  SessionEnd (deterministic SQL; also for sessions that ended without hooks). Review in Knowledge → Proposals or
  `nexus knowledge proposals approve|reject` (edit title/body, scope workspace/project/branch, local-only). Only an
  approval writes a knowledge file (accepted decision / verified note, provenance, secrets still refused).
* Earlier milestones: lifecycle hooks, portable knowledge and tasks, context hierarchy with lazy loading,
  task-driven sessions, branch-aware knowledge, effective-instructions inspector (see ADR-0013 … ADR-0019).

## Verified behaviour (2026-11-04, end of session 5)

| Area | Evidence |
| --- | --- |
| Rust | `cargo test --workspace`: 176 passed — core 128 unit + git 5 + crash 2 + isolation 4; CLI 21 E2E + 6 MCP unit; desktop 6 unit + 4 PTY |
| Frontend | 42 vitest (new: Resume view verified/saved separation + actions, proposal review, profile isolation option); `vue-tsc` clean |
| Desktop E2E | `run-smoke.mjs` 27/27; **`task-flow.mjs` 29/29**: … Claude records a decision → NEXUS hard-killed and restarted → Resume project separates verified Git state from saved history, shows the pending proposal, recommends resuming → its button resumes the same conversation → the proposal is approved into an accepted decision file |
| T18 | `t18-check.mjs` on the production build |
| MCP interop | official TS SDK: 17 tools, 13 read-only |
| Real Claude 2.1.311 (synthetic profiles) | `hooks-real-claude.mjs`; `instructions-real-claude.mjs` (inspector = `InstructionsLoaded`, incl. an outside import needing approval); **`security-real-claude.mjs` 12/12** (isolation control + isolated run with a `home (x)` folder; planted task instructions absent from the system prompt, NEXUS SessionStart hook `outcome: success` with one escaped data block) |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` clean |

Not verified (deliberately or not possible here): anything with the owner's real Claude profiles →
`docs/MANUAL_CHECKS.md` steps 13–20; how the model weighs SessionStart data (needs a signed-in model call);
organisation-managed `CLAUDE.md`/settings interplay (needs admin rights — docs + construction only); the GitHub
Actions workflow (nothing pushed).

## Findings and fixes this session

* Real-CLI facts: `claudeMdExcludes` is case-sensitive; globs over paths with parentheses fail unless bracket-escaped;
  outside-the-project imports need approval; `stream-json` exposes hook responses (docs/RESEARCH.md).
* Bugs caught by the new adversarial tests: the task title reached Claude as `--name`; the inspector reported external
  imports as loaded and matched excludes case-insensitively.
* Tooling: a shell heredoc wrote a NUL byte into `git.rs` (caught by the binary-file scan, fixed).

## Known issues / decisions to revisit

* Release gates (owner decision): identifier `dev.nexus.desktop` provisional, no license, code signing, no
  distributed build with `e2e-webview-debugging`, bundle `nexus` CLI as a Tauri sidecar. See `docs/ROADMAP.md`.
* Task sessions under policies that block both hooks and MCP get only the task id (by design, ADR-0020).
* Proposal quality depends on Claude recording checkpoints; ahead/behind is "as of the last fetch" (NEXUS never
  fetches).
* Approvals of always-on rules are per machine; knowledge files are not watched; `<data dir>/sessions/<id>/` artifacts
  are not pruned; `freezePrototype` off (xterm.js); portable-pty Windows kill workaround; ADR-0015 HKCU policy
  writability unverified.

## Next actions (in order)

1. Owner: review `milestone/m18-resume-project`; approve merging into `main`; run `docs/MANUAL_CHECKS.md` 13–20.
2. M11 remainder: reconcile with `claude agents --json`; measure `--bg` + `attach` for the future service host.
3. M14: profile diagnostics in the desktop, allowlist profile cloning.
4. M21: NEXUS skills via `--plugin-dir` (policy-aware), workflows as skills.
5. Housekeeping: prune old session artifacts; knowledge file watching.

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
node e2e/security-real-claude.mjs                   # isolation + task text as data vs. real Claude
# CLI
target/debug/nexus projects resume <project>
target/debug/nexus knowledge proposals list
```

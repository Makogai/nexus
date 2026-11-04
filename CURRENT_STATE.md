# CURRENT_STATE

_Last updated: 2026-11-04 (end of session 6)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

* `main` = baseline + merges `92475ec`, `8f51af2`, `1c3f9bb`, `b0df57c` (M18 + proposals), each verified before a
  `--no-ff` merge. Nothing has been pushed.
* Branch **`milestone/m14-m21-profiles-kits`** (off `main`, not merged, not pushed): M14 profile diagnostics
  (ADR-0023) and M21 AI Kit & Skill Manager (ADR-0024, absorbs M23), plus `docs/PROFILE_ISOLATION.md`.
* Phases: Phase 1 ✅ · M9–M10 ✅ · M11 partial · M12–M13 ✅ · M14 ✅ (allowlist clone open) · M16–M20 ✅ · M21 ✅.

## What exists now (session 6)

* **Profile diagnostics** — desktop *Claude profiles → Diagnostics* and `nexus profiles diagnose`: executable and
  version, organisation policy with plain-language effects, user settings (allowlisted values, env *names*),
  memory, skills/agents/commands by source (user, plugin, kit), MCP servers by scope (user, local, plugin, managed,
  kit), hooks (programs by file name), installed plugins and marketplaces, NEXUS kits, a context-at-start estimate and
  warnings. Read-only: `.credentials.json` never opened; from `.claude.json` only `mcpServers`.
* **AI Kits** — desktop *AI Kits* and `nexus kits`: register local folders or Git repositories (shallow clone into
  NEXUS's data folder, hardened), inventory, Claude's own validator, hash approval (any change withdraws it), per
  profile enablement with skill selection (large kits start with none), hooks and MCP opt-in with warnings, same-name
  conflicts refused, policy-aware. Sessions get a per-profile overlay through `--plugin-dir`; no Claude config is
  modified and other profiles never see the kit.
* Earlier: lifecycle hooks, knowledge and tasks with lazy context, task-driven sessions, branch-aware notes,
  instruction inspector, profile instruction isolation, task text as data, resume project, reviewed memory proposals.

## Verified behaviour (2026-11-04, end of session 6)

| Area | Evidence |
| --- | --- |
| Rust | `cargo test --workspace`: 184 passed — core 133 unit + git 5 + crash 2 + isolation 4; CLI 24 E2E + 6 MCP unit; desktop 6 unit + 4 PTY |
| Frontend | 47 vitest (new: kit helpers, profile/kit selection card, Kits view approve/enable, register dialog, diagnostics view); `vue-tsc` clean |
| Desktop E2E | `run-smoke.mjs` 27/27 · `task-flow.mjs` 29/29 · **`profiles-kits.mjs` 9/9** (UI review → approve → one skill for Work → Work diagnostics → Work launch gets only that skill, Personal nothing) |
| T18 / MCP interop | production build check OK · official SDK 17 tools |
| Real Claude 2.1.311 (synthetic profiles) | hooks · instructions · security 12/12 · **`kits-real-claude.mjs` 17/17**: NEXUS diagnostics equal the real init event (skills, agents, plugins, MCP servers) with no planted secret; the kit appears only in the real Work session with only the selected skill, from NEXUS's overlay; MCP server only after opt-in; Personal sees nothing; config dirs untouched; a changed kit withheld |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` clean |

Not verified (deliberately or not possible here): anything with the owner's real profiles (`docs/MANUAL_CHECKS.md`
13–22); organisation-managed files live (needs admin rights); private Git kits (credentials are the user's Git
setup); the GitHub Actions workflow (nothing pushed).

## Findings this session

* Real-CLI facts recorded in docs/RESEARCH.md: the init event lists skills/agents/plugins/MCP before sign-in;
  plugin content is namespaced; skills-only folders load as plugins; same-name plugins merge namespaces;
  `claude plugin list --json` prints MCP arguments (so NEXUS reads files instead); validator exit codes.
* Fixed during development: an MCP entry serialised two `name` keys (now `qualifiedName`); the diagnostics command
  held the database lock while running `claude --version`.

## Recommendations (next stage)

**Workflow engine** — keep ADR-0012's principle: no custom orchestrator.
1. A workflow is a workspace document (`workflows/*.md`, already a knowledge kind) plus an optional kit skill.
   "Run workflow" creates a task whose checklist is the workflow's steps and launches Claude for it — progress,
   resume, checkpoints and proposals then work unchanged.
2. Deliver the workflow's skill only to that session (a session-scoped kit selection override on the existing overlay
   mechanism), so workflows cost context only when used.
3. Quality gates as deterministic checks: checklist items that the PostToolUse hook ticks evidence for (test/build
   command categories are already captured) — never a model judging its own work.
4. Parallel steps via Claude's native subagents; NEXUS tracks, it does not schedule.

**Token analytics** — keep ADR-0010's provenance rule.
1. Start with per-session totals from the transcript's `usage` fields after a session ends, labelled
   "from transcript (unofficial format)", stored machine-local; no network.
2. Personal profiles may opt in to OpenTelemetry to a local NEXUS receiver (session-scoped `OTEL_*`); profiles with
   managed telemetry show "managed by your organisation — not available"; NEXUS never overrides it.
3. Show measured usage next to the static context-at-start estimate from diagnostics, so users see what kits,
   plugins and memory cost before and after.
4. Needs a signed-in real session to verify — plan a manual check; automated tests can only use recorded fixtures.

## Known issues / decisions to revisit

* Release gates (owner decision): identifier `dev.nexus.desktop` provisional, no license, code signing, no
  distributed build with `e2e-webview-debugging`, bundle `nexus` CLI as a Tauri sidecar.
* Kits reach only NEXUS-launched sessions by design; `claude plugin install` for NEXUS-created profiles (with
  confirmation) is a possible later option.
* Project `.claude/` content applies to every profile (Claude behaviour) — see docs/PROFILE_ISOLATION.md.
* Knowledge files are not watched; `<data dir>/sessions/<id>/` and kit overlays are not pruned; `freezePrototype`
  off (xterm.js); portable-pty Windows kill workaround; ADR-0015 HKCU policy writability unverified.

## Next actions (in order)

1. Owner: review `milestone/m14-m21-profiles-kits`; approve merging into `main`; run `docs/MANUAL_CHECKS.md` 13–22.
2. Workflow engine slice (above), then token analytics slice (above).
3. M14 remainder: allowlist profile cloning; M11 remainder (`claude agents --json`).
4. Housekeeping: prune session artifacts and stale kit overlays; knowledge file watching.

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
node e2e/run-smoke.mjs && node e2e/task-flow.mjs && node e2e/profiles-kits.mjs   # real-app E2E (Windows)
node e2e/mcp-interop.mjs                            # official MCP SDK client vs nexus mcp
node e2e/hooks-real-claude.mjs                      # real Claude runs NEXUS hooks (synthetic profile)
node e2e/instructions-real-claude.mjs               # inspector vs. real Claude InstructionsLoaded
node e2e/security-real-claude.mjs                   # isolation + task text as data vs. real Claude
node e2e/kits-real-claude.mjs                       # diagnostics + per-profile kits vs. real Claude
# CLI
target/debug/nexus profiles diagnose <profile>
target/debug/nexus kits add <folder> | kits add --git <url> ; kits approve <kit> ; kits enable <kit> -p <profile> --skills a,b
```

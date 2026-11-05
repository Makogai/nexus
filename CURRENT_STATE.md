# CURRENT_STATE

_Last updated: 2026-11-05 (session 10, Portable Workspace)._ Read this first, then `docs/ROADMAP.md` and
`docs/PORTABLE_WORKSPACE.md` (audit, gap analysis, milestone plan P0–P8).

## Where we are

* `main` = … `efd3dd4` (M29) + **`7beef88`** (Beta readiness, merged `--no-ff` after the verification of session 9).
  Nothing pushed.
* Branch **`milestone/portable-workspace`** (off `main` at `7beef88`): stage *Portable Workspace*. P0 (audit) and
  P1 (workspace schema 2), P2 (templates), P3 (Workspace Doctor) P4 (projects), P5 (environment, Claude Code setup), P6 (development records, history), P7 (sync policies) and P8 (new-computer restoration) done and committed; the stage's full sequential verification is below. Not merged, not pushed, no release.

## Portable Workspace — progress (session 10)

* **P0** — `docs/PORTABLE_WORKSPACE.md`: what exists per area, gap per specification section, target layout reconciled
  with the implemented storage, migration strategy, milestones. ADR-0036.
* **P1 — workspace schema 2 (ADR-0036)**: `nexus-workspace.json` stays the one manifest (JSON — the spec's
  `nexus.yaml`), now with `createdWith`, `migratedWith`, `minNexusVersion`, `template`, `features`, `layout.projects`,
  per-project `context`. New workspaces: schema 2, `projects/` (not `repos/`), `.gitignore` with `projects/`, `repos/`,
  `.nexus-local/`. Knowledge scans `constitutions/` and `project-context/` too; `store::create_in_folder`. A newer
  schema or a higher `minNexusVersion` opens **read-only** (`workspace::compat`; writes refused centrally in
  `write_manifest` and the knowledge store via `ensure_writable[_at]`) instead of being refused. Migration 1 → 2
  (`workspace::migrate`): explained plan, copies of the changed files in `<data>/workspace-backups/<id>/<time>/`,
  rollback on failure, idempotent, nothing moved. `nexus workspaces info|migrate [--yes]`; IPC
  `workspace_migration_plan`/`workspace_migrate`; desktop `WorkspaceBanner` (read-only reason, review → migrate).
  Verified: core 192/192, CLI 33 + 6, vitest banner 2/2, desktop E2E `portable.mjs` 11/11, `run-smoke` 27/27,
  `sync` 18/18, clippy clean.
* **P2 — templates (ADR-0037)**: built-in packages embedded by `crates/nexus-core/build.rs` from
  `crates/nexus-core/templates/<id>/` — *Minimal* (core agreement), *Developer* (+ engineering, code quality, testing,
  Git, documentation; 5 skills; 4 workflows), *AI Engineering Pro* (+ architecture, security, performance,
  accessibility, database, frontend, backend, DevOps; 13 stack rule sets Angular … Docker; 11 skills; 7 workflows;
  4 integration manifests: Graphify, Codex CLI, Playwright MCP, GitHub CLI — researched against official sources,
  never installed automatically). 27 constitutions / 319 rules. `templates::plan/apply` with `nexus.lock.json`
  (add / update / keep yours + copy in `.nexus-local/template-updates/` / keep removed / skip own file / retire),
  validation before writing, rollback; approvals for what the person installs; the `skills/` folder becomes the
  workspace AI Kit (`kits.workspace_id`, migration 0016, plugin `workspace`, only that workspace's sessions).
  Progressive delivery: only the core (6 rules, ~950 characters) is always on; on-demand documents are listed by
  title + id pattern (`get_constitution` accepts `"TST-*"`), stack sets only for matching projects — a Vue/TS project
  gets 1 463 characters for the whole library. Detection adds `docker` and `sql`. `nexus templates list|show|apply`,
  `workspaces create --template`, IPC `template_list/plan/apply`, onboarding template choice. Content written by four
  delegated writers against one brief, then gated by `templates::tests` (structure, ids, lengths, stacks, skills,
  integration schema, no credentials). Verified: core 201, CLI 34 + 6, vitest 79, desktop E2E `portable.mjs` 19/19,
  `profiles-kits` 9/9, `workflows` 24/24, real-Claude `kits` 17/17 and `workflow` 7/7, clippy clean.
* **P3 — Workspace Doctor** (`workspace_doctor`, `toolchain`): checks the manifest and schema (read-only, migration),
  `.gitignore`, README, clone folder, `nexus.lock.json` and template updates (and pending comparison copies), knowledge
  validity and duplicate ids, constitution errors, rules/constitutions/workflows waiting for approval, the workspace
  skills kit (missing, needs review, changed), workflows naming missing `workspace:` skills, integration manifests
  (invalid JSON; enabled ones whose tools are missing — tools only by id from `toolchain::TOOLS`, never a program
  named by a file), projects missing on this computer, Claude Code and Git, and whether the workspace is a Git
  repository with a remote. Findings carry a recommendation, a desktop target and, where safe, a repair that first
  returns its plan (migrate schema, add `.gitignore` lines, README, clone folder, rebuild index, register the skills
  *for review*, update the template). Approvals stay explicit reviews in their views. `nexus workspaces doctor
  [--repair ID [--yes]]` (exit 1 on errors), IPC `workspace_doctor/repair_plan/repair`, desktop **Workspace** view
  (sidebar) with the Doctor and the template card (install, switch, check for updates with the per-file plan).
  Verified: core doctor 3/3, CLI E2E, vitest 81, desktop E2E `portable.mjs` 24/24, clippy clean.
* **P4 — projects** (`project::context`, `project::discovery`): registering a project in a schema 2 workspace
  (`register_with_context`, CLI and desktop) creates `project-context/<slug>/` — `project.json` (portable record: id,
  name, remote, stacks, package managers, commands with their source file; no machine paths), `overview.md` (README
  excerpt quoted with its source, repository, detected technology and commands), `architecture.md` (`status: draft`,
  structure read from the file system, an empty summary slot for a reviewed draft), `current-state.md` (generated:
  verified Git state + open tasks). Overview/architecture are created once and never overwritten; project.json and the
  current state are regenerated. New project tasks/decisions/memory/specs/constitutions are written into the context
  folder (`context::doc_folder`, used by `knowledge::service::create`). Discovery scans the clone folder (and `repos/`)
  for Git repositories: *registered*, *known* (same normalised remote as a missing project → link) or *new* (review →
  register; nothing registered or run automatically); missing projects with a supported remote can be cloned into
  `projects/<slug>` (`git clone --no-recurse-submodules`, no prompts) and keep their stable id. `nexus projects
  scan|clone|context`; IPC `project_scan/clone/context_init` on their own connection; Projects view discovery panel
  (clone selected, link, register after confirmation); project page *Portable context* card. Doctor: missing projects
  (repair: clone), known clones (repair: link), unregistered repositories, projects without context (repair: create).
  The on-demand constitution listing names a document's rules when it has at most three. File-system watching is not
  implemented (scan on view load + Rescan). Verified: core 205, CLI 36 (4 tests updated for the new folders), vitest
  84, desktop E2E `portable.mjs` 33/33, `run-smoke` 27, `task-flow` 29 (decision path updated), `context` 9,
  `attention` 27, `constitutions` 13, `workflows` 24, `usage` 18, `sync` 18, `profiles-kits` 9, `mcp-interop` 9.
* **P5 — environment and Claude Code setup** (`project::environment`, `toolchain`, `claude::setup`): requirements are
  read from the project's files — `.nvmrc`, `.node-version`, `engines`, `volta`, `packageManager`, lock files,
  `.tool-versions`, `.python-version`, `requires-python`, `uv.lock`/`poetry.lock`, `rust-toolchain(.toml)`,
  `rust-version`, `composer.json require.php`, `pubspec.yaml environment.sdk`, `go.mod`, Dockerfile, Compose services,
  Dev Container image — each with its source; environment variables by name from `.env.example`-style files only;
  the README's setup section by heading. Recorded in `project-context/<slug>/environment.json` on every context refresh
  (the project files stay authoritative; the record serves computers where the project is not cloned yet). `check`
  probes only tools from the fixed `toolchain::TOOLS` table with their version command and matches ecosystem
  constraints (`20`, `>=18 <21`, `^8.2`, `~=3.11`, `||`); channels (`stable`, `lts/*`) are `unknown`, never guessed.
  Missing tools get the official page and, where published, a WinGet/Homebrew command — typed into a new terminal
  (`lib/terminal.ts`), never run. `claude::setup`: installations, startability, version, profiles, Git for Windows,
  and the official methods (code.claude.com, verified 2026-11-05: native PowerShell/CMD/sh installer, WinGet,
  Homebrew, npm with Node 22+), with next steps; sign-in stays Claude Code's browser flow. `nexus projects env
  [--check]`, `nexus setup` (exit 1 when not ready), IPC `project_environment`, `claude_setup_status`; desktop
  *Claude Code setup* view (from Claude profiles and the Doctor) and the project page *Environment* card; Doctor
  `toolchains` finding. Threat model T44. Verified: core 211, CLI 37 + 6, vitest 86, desktop E2E `portable.mjs` 38/38
  (the install command typed into PowerShell, not executed), clippy clean.
* **P6 — development records and history (ADR-0038)** (`devrecord`, migration 0017): sessions record their starting
  commit (`head_start`); when a Claude session in a project ends — desktop PTY exit, `nexus launch` exit, or a dead
  host found at start/heartbeat (`finalize_pending`) — NEXUS writes, once (`record_at`), a record for meaningful
  sessions into `project-context/<slug>/development/`: Verified (branch/HEAD, commits since the start, gates run),
  Observed (files edited, command categories — results not observed), Reported by Claude (checkpoints, unverified),
  Next steps (last reported state, open blockers). No model call; missing facts are stated as missing; chat-only
  sessions leave no record; the `development-records` feature (on for new and migrated workspaces) opts out when
  removed. Compaction adds a NEXUS checkpoint of what changed so far. `current-state.md` follows each record.
  `devrecord::history` / `nexus projects history` / IPC `project_history` / desktop **Development history** (project
  page): records with sync state (not committed / committed, not pushed / synced / workspace not in Git), sessions,
  open tasks, decisions, recent commits, gate results, blockers. Verified: core 214, CLI 37 + 6, vitest 87, desktop E2E
  `portable.mjs` 43/43, `task-flow` 29, `attention` 27, `context` 9, clippy clean.
* **P7 — sync policies (ADR-0039)** (`workspace_sync::policy`): machine-local per workspace — *manual* (default),
  *automatic with review* (notification → Sync Center), *trusted automatic* (opt-in with a private-remote
  confirmation; approved remote + branch stored). Trusted publishes only `project-context/*/development/*.md`,
  `current-state.md`, `project.json`, `environment.json` via M29's `review_commit → commit → fetch → review_push →
  push` (tokens, inspections) and pauses to ask on any finding, remote/branch change, remote ahead (never forced),
  outgoing commits outside the allowlist, or a managed policy (`<ProgramData>\NEXUS\policy.json`, strict) that forbids
  it; offline → committed locally, retried after 10 min. Records mark their workspace due; hosts run `run_due` after a
  record and on heartbeats. Audited as `auto-sync` (migration 0017 rebuilds `sync_history` to allow it). Sync Center
  *Sync policy* card, `nexus sync policy`, IPC `sync_policy_get/set`. Threat model T45. Verified: core 216 (2 policy
  tests with local bare remotes), CLI 37 + 6, vitest 89, desktop E2E `portable.mjs` 50/50 (trusted publish through the
  UI), `sync.mjs` 18/18, clippy clean.
* **P8 — new-computer restoration** (`restoration`): one ordered plan — Claude Code, workspace schema, knowledge index
  (rebuilt here), approvals of synced rules/constitutions/workflows, workspace skills (register for review → approve),
  projects (clone missing from their remotes with their ids, link clones), the projects' tools, Claude profiles,
  quality gates, continue — each with done/to do/blocked/optional, a Doctor repair (plan first) or a view; plus what came
  with the workspace (counts incl. development records and the template) and what cannot be recovered (transcripts,
  uncommitted source, local-only knowledge, machine-local state, approvals and gate commands, profiles and sign-ins).
  Onboarding from Git now leads to **Restore on this computer** (fixed: `CloneWorkspaceForm` emitted `cloned` after the
  store reload had already unmounted onboarding, so the navigation never happened); also reachable from the Workspace
  view. `nexus workspaces restore`, IPC `workspace_restoration`. User guide `docs/user/PORTABLE_WORKSPACE.md`.
  Verified: core restoration test, CLI E2E, vitest, desktop E2E `restore.mjs` 12/12 (two app instances with separate
  data folders and local bare remotes), `sync.mjs` 18/18.

## Previous stage: Beta readiness (merged as `7beef88`)

* **Upgrade safety and backups (ADR-0032)**: the database carries a NEXUS `application_id`; a foreign SQLite file is
  refused untouched; a failed migration is reported (never treated as corruption, never replaced by an empty
  database) and the desktop shows a startup error dialog instead of starting fresh. Backups: a copy before every
  schema upgrade (`nexus-v<schema>-<ts>.db`), a daily automatic backup (7 kept), manual backups, and a safety backup
  before every restore — each a `VACUUM INTO` copy plus `global/` and a manifest with SHA-256, schema and app version.
  Restore verifies (checksum, `application_id`, `quick_check`, schema not newer than this NEXUS), moves the current
  files aside into `backups/<ts>-replaced/` (never deleted), and puts them back if opening the restored state fails.
  Desktop: *Settings → Backups* (list, back up now, restore = scheduled + restart, delete), schema line, restore
  outcome toasts. CLI: `nexus backup create|list|restore <id> [--yes]|delete <id>`; restore refuses while a NEXUS
  session host is running.
* **Packaging (ADR-0033)**: NSIS per-user installer, no admin rights; installs `nexus-desktop.exe`, the `nexus` CLI
  (MCP server, hooks; Tauri sidecar via `tauri.installer.conf.json` + `scripts/stage-sidecar.mjs`) and the
  uninstaller — nothing else — into `%LOCALAPPDATA%\Programs\NEXUS`; data stays in `%LOCALAPPDATA%\NEXUS` and
  survives uninstalling. Uninstall removes NEXUS's own registrations (AppUserModelID, `nexus://`).
  `nexus-desktop.exe --self-check <report.json>` checks an installed app without a window. *Settings → Command-line
  tool* shows the CLI path (NEXUS never edits PATH). Fixed: the `nexus://` handler was registered with a `\\?\` path
  Windows Shell cannot launch (clicks after a restart did nothing).
* **Release security**: `compile_error!` makes a release build with `e2e-webview-debugging` impossible (verified);
  `scripts/release-check.mjs` (configuration, frontend bundle freshness, no source maps / credentials / machine paths,
  version, `SHA256SUMS.txt`); T18 against the release program.
* **CI** (`.github/workflows/`, not pushed): `ci.yml` also builds the installer and runs the installed-app test;
  `release.yml` (tag `v*` / manual) tests, builds the release installer, runs T18, the installed-app test and the
  release check, attests build provenance, uploads artifacts and creates a **draft** release only. Signing and the
  updater are disabled placeholders.
* **User documentation** `docs/user/`: first run, user guide, backup and restore, troubleshooting, what stays local
  vs. what Git can synchronise. `docs/RELEASE_NOTES.md` for the beta.
* **Proposals for the owner**: ADR-0034 application identity (recommends `io.github.<user-or-org>.nexus`, explicit
  `bundle.publisher`; no data migration needed), ADR-0035 licence (recommends Apache-2.0, or dual MIT/Apache-2.0;
  dependency inventory: 293 Rust packages, 288 permissive + 5 MPL-2.0; frontend bundle MIT/ISC/Apache; no GPL).

## Verification log — Portable Workspace stage (sequential groups, `CARGO_BUILD_JOBS=2`, final code `7c491fb`)

Each milestone was verified before it was committed (see the progress entries above); then the whole stage once more,
one group at a time, on the final code. (The Beta readiness log is in that stage's commit `82d72ab`.)

| # | Command | Result |
| --- | --- | --- |
| 1 | `cargo test --workspace` | passed — 285 tests, 0 failed (core 217, CLI 38 + 6, db_crash 2, git 5, isolation 4, desktop lib 9, pty 4; toast self-test ignored = opt-in) |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | passed |
| 3 | `cargo clippy -p nexus-desktop --all-targets --features e2e-webview-debugging -- -D warnings` | passed |
| 4 | `npm test` · `npm run typecheck` · `npm run build` | 90/90 vitest (21 files) · typecheck clean · build ok |
| 5 | `npx tauri build --debug --no-bundle` + `node e2e/t18-check.mjs` | T18 OK 4/4 |
| 6 | `npx tauri build --debug --no-bundle --features e2e-webview-debugging` (once) | ok (used by 7–18) |
| 7 | `node e2e/run-smoke.mjs` | 27/27 |
| 8 | `node e2e/task-flow.mjs` | 29/29 |
| 9 | `node e2e/profiles-kits.mjs` | 9/9 |
| 10 | `node e2e/attention.mjs` | 27/27 |
| 11 | `node e2e/constitutions.mjs` | 13/13 |
| 12 | `node e2e/context.mjs` | 9/9 |
| 13 | `node e2e/workflows.mjs` | 24/24 |
| 14 | `node e2e/usage.mjs` | 18/18 |
| 15 | `node e2e/sync.mjs` | 18/18 |
| 16 | `node e2e/portable.mjs` (P1–P7) | 50/50 |
| 17 | `node e2e/restore.mjs` (P8, two computers) | 12/12 |
| 18 | `node e2e/mcp-interop.mjs` | 9/9 |
| 19 | `node e2e/hooks-real-claude.mjs` | 9/9 (Claude Code 2.1.311, synthetic profiles) |
| 20 | `node e2e/instructions-real-claude.mjs` | 6/6 |
| 21 | `node e2e/security-real-claude.mjs` | 14/14 |
| 22 | `node e2e/kits-real-claude.mjs` | 19/19 (incl. a real session getting the five `workspace:` template skills) |
| 23 | `node e2e/context-real-claude.mjs` | 7/7 |
| 24 | `node e2e/workflow-real-claude.mjs` | 7/7 |
| 25 | `node e2e/usage-real-claude.mjs` | 7/7 |
| 26 | `npx tauri build --debug --config src-tauri/tauri.installer.conf.json` + `node e2e/installed.mjs` | installer 9.09 MiB · 27/27 |

All tests use temporary workspaces, temporary repositories, local bare remotes (`file://`) and synthetic Claude
profiles; no real profile, repository or remote was used or changed. **Memory:** 5.5 GB free at the start, lowest
2.0 GB (E2E build + suites); no group was stopped. **Pre-commit inspection** before every commit (secrets, personal
data, connection strings — fixtures with credential shapes are built at runtime; the only hits were reserved
`example.test` addresses and Git remote URLs in tests).

## Next actions (in order)

1. Owner: review `milestone/portable-workspace` — MANUAL_CHECKS 33 (template library review) and a real first
   workspace (`docs/user/PORTABLE_WORKSPACE.md`); merge when approved (`--no-ff`, no push).
2. Owner decisions still open from Beta readiness (B5): identifier + publisher (ADR-0034), product name and `nexus://`,
   licence (ADR-0035), code signing, updater.
3. Owner manual checks: MANUAL_CHECKS 27 (notification look, lock screen, click after quitting), 28–32.
4. Third-party notices from the ADR-0035 inventory; first tagged draft release from CI.
5. Later: project file-system watching, an AI-drafted architecture summary flow with review, U5 OpenTelemetry.

## Known issues / decisions to revisit

* Portable Workspace: manifests stay JSON (`nexus-workspace.json`, `nexus.lock.json`, `project.json`,
  `environment.json`) — the specification's YAML names map to them (ADR-0036). Project discovery scans on view load and
  on Rescan; there is no file-system watcher. Whether a remote is *private* cannot be verified for arbitrary hosts —
  trusted sync records the person's confirmation and stops when the remote URL changes. Editing a workspace skill
  withholds the skills until they are approved again (same rule as every kit). Development records need workspace
  schema 2 and the `development-records` feature; sessions from before migration 0017 have no starting commit, and
  test results are *observed* by category only — only gates NEXUS ran are *verified*. The architecture summary is left
  for a reviewed draft (skill `workspace:architecture-documentation`); NEXUS calls no model. The template library
  (v1.0.0) passed the structural gate in `templates::tests`; its content still wants the owner's review (MANUAL 33).
* Release gates: identifier `dev.nexus.desktop` and manufacturer `nexus` provisional (ADR-0034); no licence file
  (ADR-0035); installers unsigned (SmartScreen warns); no updater. The installer downloads the WebView2 runtime only
  if it is missing (preinstalled on Windows 11).
* Tauri's NSIS installer reads its options from its own path too — scripts must start it by an absolute Windows path;
  the remembered install location (`HKCU\Software\nexus\NEXUS`) survives uninstalling (ADR-0033).
* Restore replaces the whole machine-local state (database + `global/`); workspace files are in Git, not in backups.
  The desktop restores at the next start; the CLI refuses while a NEXUS host runs.
* Lock detection is best effort (LogonUI process); the displayed sender name comes from the registration (visual
  check in MANUAL_CHECKS 27). Development builds register `dev.nexus.desktop.dev` in HKCU on their first notification.
* Gates run with the user's rights; a run needs the project folder on this machine to verify. Gate approval is
  machine-local like the gates themselves: it guards against unseen command changes, not against a person approving
  a harmful command. Gates that existed before the workflow stage need one approval after the upgrade.
* Context strategy is machine-local; InstructionsLoaded costs one hook process per instruction file at start.
* Knowledge files are not watched; session artifacts and kit overlays are not pruned.
* Usage token counts are *parsed* (ADR-0010): Claude Code's transcript format is undocumented; versions outside
  `VERIFIED_FORMATS` are flagged. Not verified with a signed-in profile yet (MANUAL_CHECKS 28). Claude's background
  helper calls are not in the per-response counts. Sessions started outside NEXUS are not attributed to projects.
* Cost estimates need the profile's billing classification (from a sign-in check); unknown billing is withheld.
* Sync: NEXUS never merges, rebases or force-pushes — diverged branches and conflicts are integrated in the person's
  Git tool. The inspection is pattern-based (common credential formats), not a guarantee; the outgoing review stays
  the person's responsibility. Kit folders inside the workspace must be registered per machine. No automatic
  background fetch (fetch is explicit).

## How to run

```text
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
cargo build -p nexus-cli -p fake-claude
cd apps/desktop && npm test && npm run typecheck
npx tauri build --debug --no-bundle && node e2e/t18-check.mjs
npx tauri build --debug --no-bundle --features e2e-webview-debugging
node e2e/run-smoke.mjs; node e2e/task-flow.mjs; node e2e/profiles-kits.mjs; node e2e/attention.mjs
node e2e/constitutions.mjs; node e2e/context.mjs; node e2e/workflows.mjs; node e2e/usage.mjs; node e2e/sync.mjs
node e2e/mcp-interop.mjs
node e2e/hooks-real-claude.mjs; node e2e/instructions-real-claude.mjs; node e2e/security-real-claude.mjs
node e2e/kits-real-claude.mjs; node e2e/context-real-claude.mjs; node e2e/workflow-real-claude.mjs
node e2e/usage-real-claude.mjs
npx tauri build --debug --config src-tauri/tauri.installer.conf.json && node e2e/installed.mjs   # Windows, no NEXUS installed
npx tauri build --config src-tauri/tauri.installer.conf.json && node scripts/release-check.mjs ../../target/release/bundle/nsis
cargo test -p nexus-desktop --test toast_selftest -- --ignored   # opt-in, real Windows notifications
```

# CURRENT_STATE

_Last updated: 2026-11-05 (session 9, Beta readiness)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

* `main` = … `7bd238e` (usage analytics) + **`efd3dd4`** (M29 workspace Git sync, merged `--no-ff` after the
  verification of session 8). Nothing pushed.
* Branch **`milestone/beta-readiness`** (off `main` at `efd3dd4`): stage *Beta readiness* B1–B4 done, B5 = owner
  decisions (proposed in ADR-0034/0035). Committed locally after the verification below; not merged, not pushed, no
  release published, identifier and licence unchanged.

## What exists now (session 9, beta readiness on the branch)

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

## Verification log (sequential groups, `CARGO_BUILD_JOBS=2`)

One group at a time; free memory sampled every 3 s. "Latest" = run against the final code of this stage.

| # | Command | Result | Latest |
| --- | --- | --- | --- |
| 1 | `cargo test --workspace` | passed — 250 tests, 0 failed (core 188, CLI 32 + 6, db_crash 2, git 5, isolation 4, desktop lib 9, pty 4; toast self-test ignored = opt-in) | yes |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | passed | yes |
| 3 | `cargo clippy -p nexus-desktop --all-targets --features e2e-webview-debugging -- -D warnings` | passed | yes |
| 4 | `cargo check -p nexus-desktop --release --features e2e-webview-debugging` | **refused at compile time** (expected: the test feature cannot reach a release build) | yes |
| 5 | `npm test` · `npm run typecheck` · `npx vite build` (apps/desktop) | 75/75 vitest (13 files) · typecheck clean · build ok | yes |
| 6 | `npx tauri build --debug --no-bundle` + `node e2e/t18-check.mjs` | build ok · T18 OK 4/4 | yes |
| 7 | `npx tauri build --debug --no-bundle --features e2e-webview-debugging` (once) | ok (used by 8–17) | yes |
| 8 | `node e2e/run-smoke.mjs` | passed 27/27 | yes |
| 9 | `node e2e/task-flow.mjs` | passed 29/29 | yes |
| 10 | `node e2e/profiles-kits.mjs` | passed 9/9 | yes |
| 11 | `node e2e/attention.mjs` | passed 27/27 | yes |
| 12 | `node e2e/constitutions.mjs` | passed 13/13 | yes |
| 13 | `node e2e/context.mjs` | passed 9/9 | yes |
| 14 | `node e2e/workflows.mjs` | passed 24/24 | yes |
| 15 | `node e2e/usage.mjs` | passed 18/18 | yes |
| 16 | `node e2e/sync.mjs` | passed 18/18 | yes |
| 17 | `node e2e/mcp-interop.mjs` | passed 9/9 | yes |
| 18 | `node e2e/hooks-real-claude.mjs` | passed 9/9 (Claude Code 2.1.311, synthetic profiles) | yes |
| 19 | `node e2e/instructions-real-claude.mjs` | passed 6/6 | yes |
| 20 | `node e2e/security-real-claude.mjs` | passed 14/14 | yes |
| 21 | `node e2e/kits-real-claude.mjs` | passed 17/17 | yes |
| 22 | `node e2e/context-real-claude.mjs` | passed 7/7 | yes |
| 23 | `node e2e/workflow-real-claude.mjs` | passed 7/7 | yes |
| 24 | `node e2e/usage-real-claude.mjs` | passed 7/7 | yes |
| 25 | `npx tauri build --debug --config src-tauri/tauri.installer.conf.json` + `node e2e/installed.mjs` | passed 27/27 (production configuration, debug profile) | yes |
| 26 | test installer (`--features e2e-webview-debugging`, kept outside the bundle folder) + `node e2e/installed.mjs --ui <it>` | passed 18/18 | yes |
| 27 | `npx tauri build --config src-tauri/tauri.installer.conf.json` (release) | ok — 4 m 09 s, installer 7.00 MiB (rebuilt on the final code so `dist/` matches it) | yes |
| 28 | `node e2e/t18-check.mjs` against `target/release/nexus-desktop.exe` | T18 OK 4/4 | yes |
| 29 | `node e2e/installed.mjs ../../target/release/bundle/nsis/NEXUS_0.1.0_x64-setup.exe` | passed 27/27 (twice before the rebuild, once after) | yes |
| 30 | `node scripts/release-check.mjs ../../target/release/bundle/nsis` | RELEASE CHECK OK 13/13 · SHA-256 `3370f9a9dfe03b07fb1ae7cf97ca4475f16bcadcdf0405341c0a2d0562e6339c` | yes |

All tests use temporary data folders, temporary repositories, local bare remotes and synthetic Claude profiles
(fake CLI, or the real CLI with an isolated `CLAUDE_CONFIG_DIR`) — no real profile was read or changed.
**Memory:** 7.4–7.7 GB free between groups; the lowest sample of the session was 3.26 GB (first release build, LTO
link); no group was stopped.

Findings fixed during the verification: the Start Menu shortcut was missing and the app was relaunched when the
release installer was started by a forward-slash path — Tauri's NSIS template reads `/NS` and `/R` from its own path
(`…/nsis/…`, `…/release/…`); the test now resolves the path (ADR-0033 *Known installer behaviour*). An earlier run
that hit this relaunched the installed app without the test's data folder and created a real `%LOCALAPPDATA%\NEXUS`
(fresh database + one automatic backup, no user data); the installer now gets a temporary `NEXUS_DATA_DIR`, the test
refuses to start with a remembered install location and removes it at the end. The release check gained a
`dist/` freshness check (a later debug build had written source maps into `dist/`).

**Pre-commit security inspection** (47 changed and untracked files): no NUL bytes, keys, tokens, JWTs, passwords,
connection strings, personal paths, names or e-mail addresses; no `unsafe`; no shell strings (PowerShell/`cmd` only
as argv in `e2e/installed.mjs`); registry writes only NEXUS's own HKCU sender and `nexus://` registration (notifier,
now without `\\?\`); no telemetry variables set. `src-tauri/binaries/` (staged CLI) is ignored.

## Next actions (in order)

1. Owner decisions (B5): identifier + publisher (ADR-0034), product name and `nexus://` scheme for a public release,
   licence (ADR-0035), code-signing certificate, updater (signing key, channel) — then implement them in one change
   before the first installer leaves this machine.
2. Owner manual checks: MANUAL_CHECKS 27 (what the toast looks like, lock screen, click after quitting), 28–31,
   32 (installer by hand, SmartScreen while unsigned).
3. Third-party notices from the ADR-0035 inventory into the installer; first tagged draft release from CI.
4. U5 OpenTelemetry opt-in for personal profiles (*measured* usage provenance); performance budget (M31).

## Known issues / decisions to revisit

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

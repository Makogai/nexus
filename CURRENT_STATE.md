# CURRENT_STATE

_Last updated: 2026-11-05 (session 8, M29)._ Read this first, then `docs/ROADMAP.md`.

## Where we are

* `main` = … `cba2b7a` (Workflow Engine) + **`7bd238e`** (usage analytics, gate approval preview, sync readiness —
  verified in full before the merge). Nothing pushed.
* Branch **`milestone/m29-workspace-sync`** (off `main` at `7bd238e`): M29 Workspace Git synchronisation (ADR-0031).
  Committed locally after the sequential verification below; not merged, not pushed, no release.

## What exists now (session 8, M29 on the branch)

* **Sync Center** (desktop *Sync*) and **`nexus sync`** (`status`, `fetch`, `pull`, `diff`, `review`, `commit`,
  `push`, `history`, `clone`, `restore`) on the workspace's own Git repository — no sync service:
  * status offline from the last fetch: state (up to date / incoming / outgoing / diverged / conflicts / not pushed
    yet / no remote / not a repository / operation in progress), branch, upstream, remote (credentials removed), last
    fetch, local changes with name/path findings, incoming and outgoing commits and files, per-file diffs, history;
  * **fetch**; **pull = fast-forward only** after an incoming inspection; refused when diverged, conflicted, detached,
    a Git operation is in progress (`index.lock`, merge, rebase), or local changes overlap incoming files; rolled
    back if the fast-forward fails;
  * **reviewed commit** (only the reviewed files; new files unstaged again if the commit fails) and **reviewed push**
    (never forced; refused when the remote is ahead; the first push inspects the whole history) — each needs the token
    of the review the person confirmed;
  * **pre-commit / pre-push inspection**: credentials (keys, tokens, JWT, bearer, URL passwords), sensitive files by
    name, local-only knowledge, machine-local Claude configuration, `repos/`, symlinks, non-portable paths, huge files
    block; password assignments, machine paths, submodules, large files warn; values are never shown;
  * after a pull: knowledge re-indexed, manifest applied; changed rules, constitutions, workflows and AI Kits are
    inactive until approved again (notification → Sync Center);
  * **onboarding from Git** (welcome screen and Sync Center) and a **restore checklist** (projects to locate,
    documents to approve, kit folders to register, gates to define, profiles to add);
  * one operation at a time; interrupted operations reconciled; remote failures classified (offline, auth,
    remote-missing, rejected, timeout); history machine-local (migration 0015).
* Previous stages (merged): usage analytics (ADR-0030), Workflow Engine and gate approval (ADR-0029), notifications,
  constitutions, context.

## Verification log (sequential groups, `CARGO_BUILD_JOBS=2`)

One group at a time; free memory sampled. "Latest" = run against the final code of this stage.

| # | Command | Result | Latest |
| --- | --- | --- | --- |
| 1 | `cargo test --workspace --no-fail-fast` (foreground) | passed — 244 tests, 0 failed (core 184, CLI 31 + 6, db_crash 2, git 5, isolation 4, desktop lib 8, pty 4; toast self-test ignored = opt-in) | yes |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | passed (after 2 fixes in the new code) | yes |
| 3 | `cargo clippy -p nexus-desktop --all-targets --features e2e-webview-debugging -- -D warnings` | passed | yes |
| 4 | `npm test` · `npm run typecheck` · `npx vite build` (apps/desktop) | 73/73 vitest · typecheck clean · build ok | yes |
| 5 | `npx tauri build --debug --no-bundle` + `node e2e/t18-check.mjs` | build ok · T18 OK 4/4 | yes |
| 6 | `npx tauri build --debug --no-bundle --features e2e-webview-debugging` (once) + `cargo build -p nexus-cli -p fake-claude` | ok (used by 7–15) | yes |
| 7 | `node e2e/run-smoke.mjs` | passed 27/27 | yes |
| 8 | `node e2e/task-flow.mjs` | passed 29/29 | yes |
| 9 | `node e2e/profiles-kits.mjs` | passed 9/9 | yes |
| 10 | `node e2e/attention.mjs` | passed 27/27 | yes |
| 11 | `node e2e/constitutions.mjs` | passed 13/13 | yes |
| 12 | `node e2e/context.mjs` | passed 9/9 | yes |
| 13 | `node e2e/workflows.mjs` | passed 24/24 | yes |
| 14 | `node e2e/usage.mjs` | passed 18/18 | yes |
| 15 | `node e2e/sync.mjs` | passed 18/18 | yes |
| 16 | `node e2e/mcp-interop.mjs` | passed 9/9 | yes |
| 17 | `node e2e/hooks-real-claude.mjs` | passed 9/9 (Claude Code 2.1.311) | yes |
| 18 | `node e2e/instructions-real-claude.mjs` | passed 6/6 | yes |
| 19 | `node e2e/security-real-claude.mjs` | passed 14/14 | yes |
| 20 | `node e2e/kits-real-claude.mjs` | passed 17/17 | yes |
| 21 | `node e2e/context-real-claude.mjs` | passed 7/7 | yes |
| 22 | `node e2e/workflow-real-claude.mjs` | passed 7/7 | yes |
| 23 | `node e2e/usage-real-claude.mjs` | passed 7/7 | yes |

All tests use temporary repositories, local bare remotes (`file://`) and synthetic profiles — no real remotes.
**Memory:** free RAM 10.1 GB at the start, lowest 7.4 GB (compiling the test binaries); no group was stopped.

After the log only test fixtures changed (credential-shaped strings in two detector tests are now built at runtime);
`cargo test -p nexus-core --lib -- git::tests workspace_sync::inspect` (9/9) and core clippy were rerun and passed.

**Pre-commit security inspection** (all changed and untracked files): no NUL bytes, no keys, tokens, JWTs, passwords
or connection strings in added lines (test fixtures build tokens at runtime), only synthetic `example.invalid`
identities, no personal paths or names, no `unsafe`, no shell strings, no registry writes, no telemetry variables set.

## Next actions (in order)

1. Owner: review `milestone/m29-workspace-sync`; MANUAL_CHECKS 28–31 (31 = a real private remote you create);
   merge when approved (`--no-ff`, no push yet).
2. First personal-use beta: see the beta checklist in the session report (signing, identifier, packaging the CLI,
   updater off by default, backup/restore, release notes).
3. U5 OpenTelemetry opt-in for personal profiles (*measured* usage provenance).

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
cargo test -p nexus-desktop --test toast_selftest -- --ignored   # opt-in, real Windows notifications
```

# NEXUS Testing Strategy

Principles: test behaviour at the cheapest layer that can observe it; never touch real Claude profiles, real
company repositories or the user's real NEXUS database in automated tests; every bug fix gets a regression test.

## Layers

| Layer | Tooling | What it covers | Isolation |
| --- | --- | --- | --- |
| Rust unit | `cargo test -p nexus-core` | path validation, migrations, manifest I/O, stack detection, porcelain parsing, profile detection, policy parsing, launch-spec building, env scrubbing, session state transitions | `tempfile` dirs; synthetic homes/config dirs passed explicitly (no global `HOME` mutation) |
| Rust integration | `cargo test -p nexus-core --test '*'`, `cargo test -p nexus-desktop` | real `git` against temp repos; real PTY spawn (echo round trip); full launch through the PTY of the `fake-claude` binary that prints argv + selected env as JSON | temp dirs, `NEXUS_DATA_DIR`-style explicit paths |
| Frontend unit/component | `vitest` + `@vue/test-utils` + `happy-dom` | pure logic (fuzzy, format, launch defaults); LaunchDialog, ProfileDialog, SessionRow, CommandPalette, TaskBoard, KnowledgeDialog, KnowledgeView (approval only after review, hash-carrying task moves, broken files under Problems) mounted with real Pinia stores and a mocked `lib/ipc` boundary | no Tauri runtime |
| Desktop E2E | `apps/desktop/e2e/smoke.mjs` over the WebView2 DevTools protocol (Windows, today); `tauri-driver` WebDriver for CI later | workspace → project → profile → UI launch → env isolation → exit → resume, against the real app | empty `NEXUS_DATA_DIR`, `fake-claude` profile, temp fixtures |
| Security | part of unit/integration suites | hostile paths, injection strings in names, scrub list in every letter case, helper spawns (`auth status`, `--version`) under a hostile parent env (`tests/isolation.rs`), golden argv, malicious `core.fsmonitor`, policy-parser allowlist, newer-DB refusal | fixtures only |
| Knowledge | `cargo test -p nexus-core knowledge::` | strict frontmatter (rejects anchors, tags, nested maps, duplicates), schema/privacy validation, secret refusal, atomic + optimistic writes, frozen keys, incremental refresh, deletions, duplicate-id conflicts, rebuild, context hierarchy order, approval per content hash, prompt budget, skills catalog | temp dirs, in-memory DB |
| CLI end-to-end | `cargo test -p nexus-cli` | the real `nexus` binary: setup, launch isolation under a hostile parent env, exit-code propagation, `--resume last`, killed-host reconciliation, MCP binding, policy-blocked MCP, JSON-RPC over stdio, knowledge/tasks round trip through files and index (editor edits, broken files, duplicate ids, rebuild), always-on rules reaching launches by hierarchy only once approved (and withdrawn on edit), MCP knowledge scoped to the session's workspace | temp data dirs, fake-claude |
| Real-Claude hooks | `node e2e/hooks-real-claude.mjs` (local; skips without `claude`) | the real Claude CLI runs the NEXUS-generated exec-form hooks in a synthetic signed-out profile; SessionStart/SessionEnd reach NEXUS | synthetic config dir |
| MCP interoperability | `node e2e/mcp-interop.mjs` | the official MCP TypeScript SDK client against `nexus mcp`: handshake, tools/list (15), annotations, calls, knowledge write + lazy context index, errors | temp data dir |
| Storage reliability | `cargo test -p nexus-core db:: --test db_crash` | incremental migration with data, failed migration rolls back, FK-checked table rebuilds, backup + rotation, real-process crash (abort mid-transaction), garbage and page-damaged files quarantined | temp dirs |
| Manual smoke | checklist below | things automation cannot see (TUI rendering, real auth) | user's real profiles, read-only, by the developer |

## The `fake-claude` test double

`crates/fake-claude` builds a tiny binary that prints a JSON line with its arguments, working directory and the
values (or absence) of `CLAUDE_CONFIG_DIR`, `NEXUS_*`, `CLAUDECODE`, `CLAUDE_CODE_*` and `ANTHROPIC_*`, then
exits with the code in `FAKE_CLAUDE_EXIT` (`FAKE_CLAUDE_STAY=1` waits for a line of input; `FAKE_CLAUDE_REPORT=<file>` also writes the JSON to a file). Launch tests register it as a profile's executable, spawn it through
the real PTY code and assert on the JSON. This verifies profile-aware launching end-to-end without an account.

## End-to-end against the real app (Windows)

WebView2 exposes the Chrome DevTools protocol when started with a debugging port, so the real app (real IPC,
real PTY, real UI) can be driven without extra drivers. **Development/CI only** — release builds must not honour it
(release gate, threat T18).

Self-contained run (what CI does) — production-mode frontend and CSP embedded in a debug build:

```bash
cargo build -p fake-claude -p nexus-cli
cd apps/desktop
npx tauri build --debug --no-bundle     # production mode; check the exit code (fails on npm/crate version mismatches)
node e2e/t18-check.mjs                  # T18: hostile WebView2 env is ignored by the production build
npx tauri build --debug --no-bundle --features e2e-webview-debugging   # explicit test-only build
node e2e/run-smoke.mjs                  # starts the app with an empty temp database, stops it afterwards
```

Against a running dev app (for iterating on UI): start `npm run tauri dev` with an **empty** `NEXUS_DATA_DIR` and
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`, then `node e2e/smoke.mjs`.
Ad hoc: `node e2e/cdp.mjs eval|wait|shot|send|console …`.

Native folder pickers cannot be automated; the smoke test calls the same IPC commands they feed and drives
everything else through the UI. It also checks cross-process behaviour: a checkpoint written by `nexus mcp` and a
`nexus launch` session and a Notification hook ("waiting for you") appear live in the running desktop. Knowledge:
a rule file dropped into the workspace (as by `git pull`) is flagged, reviewed and approved in the Knowledge view and
then reaches the launched session's system prompt; a task created and moved on the board is a Markdown file whose
status changes on disk; a task Claude creates through `nexus mcp` appears on the board. Last run: 2026-11-04,
27/27 passed.

## Commands

```text
cargo test --workspace                 # all Rust tests
cargo clippy --workspace -- -D warnings
cd apps/desktop && npm test            # vitest
cd apps/desktop && npm run typecheck   # vue-tsc
cd apps/desktop && npm run tauri dev   # run the app (uses the real data dir unless NEXUS_DATA_DIR is set)
```

Use `NEXUS_DATA_DIR=<temp dir>` to run the app against an isolated database.

## Manual smoke checklist (Windows, per release candidate)

1. Fresh `NEXUS_DATA_DIR`: app starts, onboarding appears, create workspace in an empty folder.
2. Register a local Git repo: stack chips and branch/ahead/behind/changes match `git status`.
3. Profiles view lists the default profile and `~/.claude-*` profiles; auth status shows logged-in state.
4. Open PowerShell and Git Bash tabs in the project; type, resize the panel, run a colourful command.
5. Launch Claude with a personal profile and the Prototype work profile; Claude's TUI renders, `/status`
   inside Claude shows the expected config directory/account.
6. Exit Claude (`/exit`), session shows *exited* with code; Resume reopens the same conversation.
7. Kill the NEXUS process from Task Manager while a session runs; restart; session shows *interrupted*;
   Resume works.
8. Toggle light/dark theme; command palette (Ctrl+K) opens and navigates.

## Coverage expectations

* `nexus-core`: every public function has at least one test; launch/env/policy/path modules are
  table-driven and cover hostile inputs.
* Desktop IPC commands are thin; they are covered through core tests plus E2E.
* UI components that contain logic (palette, launch dialog validation) have component tests.

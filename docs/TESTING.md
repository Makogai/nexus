# NEXUS Testing Strategy

Principles: test behaviour at the cheapest layer that can observe it; never touch real Claude profiles, real
company repositories or the user's real NEXUS database in automated tests; every bug fix gets a regression test.

## Layers

| Layer | Tooling | What it covers | Isolation |
| --- | --- | --- | --- |
| Rust unit | `cargo test -p nexus-core` | path validation, migrations, manifest I/O, stack detection, porcelain parsing, profile detection, policy parsing, launch-spec building, env scrubbing, session state transitions | `tempfile` dirs; synthetic homes/config dirs passed explicitly (no global `HOME` mutation) |
| Rust integration | `cargo test -p nexus-core --test '*'`, `cargo test -p nexus-desktop` | real `git` against temp repos; real PTY spawn (echo round trip); full launch through the PTY of the `fake-claude` binary that prints argv + selected env as JSON | temp dirs, `NEXUS_DATA_DIR`-style explicit paths |
| Frontend unit/component | `vitest` + `@vue/test-utils` + `happy-dom` | stores, formatting, fuzzy matcher, components with mocked IPC (`@tauri-apps/api/mocks`) | no Tauri runtime |
| Desktop E2E | `apps/desktop/e2e/smoke.mjs` over the WebView2 DevTools protocol (Windows, today); `tauri-driver` WebDriver for CI later | workspace → project → profile → UI launch → env isolation → exit → resume, against the real app | empty `NEXUS_DATA_DIR`, `fake-claude` profile, temp fixtures |
| Security | part of unit/integration suites | hostile paths, injection strings in names, scrub list, malicious `core.fsmonitor`, policy-parser allowlist, newer-DB refusal | fixtures only |
| Manual smoke | checklist below | things automation cannot see (TUI rendering, real auth) | user's real profiles, read-only, by the developer |

## The `fake-claude` test double

`crates/fake-claude` builds a tiny binary that prints a JSON line with its arguments, working directory and the
values (or absence) of `CLAUDE_CONFIG_DIR`, `NEXUS_*`, `CLAUDECODE`, `CLAUDE_CODE_*` and `ANTHROPIC_*`, then
exits with the code in `FAKE_CLAUDE_EXIT` (`FAKE_CLAUDE_STAY=1` waits for a line of input; `FAKE_CLAUDE_REPORT=<file>` also writes the JSON to a file). Launch tests register it as a profile's executable, spawn it through
the real PTY code and assert on the JSON. This verifies profile-aware launching end-to-end without an account.

## End-to-end against the real app (Windows)

WebView2 exposes the Chrome DevTools protocol when started with a debugging port, so the real app (real IPC,
real PTY, real UI) can be driven without extra drivers. **Development only** — never enable this in production.

```bash
cargo build -p fake-claude
cd apps/desktop
# Git Bash; use an EMPTY data dir so the smoke run starts from onboarding
NEXUS_DATA_DIR="$TEMP/nexus-smoke" WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333 npm run tauri dev
node e2e/smoke.mjs                      # 15 checks, prints SMOKE OK / SMOKE FAILED
node e2e/cdp.mjs shot out.png           # ad-hoc: screenshot, eval, wait, send (keys), console
```

Native folder pickers cannot be automated; the smoke test calls the same IPC commands they feed and drives
everything else through the UI. Last run: 2026-11-04, 15/15 passed.

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

# NEXUS

**Your development universe, connected.** NEXUS Desktop is a local-first operating environment for AI-first
software development with [Claude Code](https://code.claude.com). It manages *who* is working (Claude profiles —
separate accounts or configurations), *where* (workspaces and independent project repositories), *how
rigorously* (work profiles), and *what happened* (sessions you can resume) — while Claude Code keeps its own
native terminal experience.

> Status: **Phase 1 MVP** (Windows-first; macOS/Linux compile but are not yet polished). See
> [`CURRENT_STATE.md`](CURRENT_STATE.md) and [`docs/ROADMAP.md`](docs/ROADMAP.md).

## What works today

* Workspaces: create or open a folder with a portable `nexus-workspace.json`; switch between work/personal.
* Projects: register local repositories; deterministic stack/script detection; live Git status (hardened
  against malicious repository configs; credentials stripped from remote URLs).
* Claude profiles: detect `~/.claude` and `~/.claude-*` configurations (read-only), register them or create new
  ones, see sign-in status and organisation policy per profile.
* Launch Claude Code in the integrated terminal (xterm.js + ConPTY) with the right `CLAUDE_CONFIG_DIR`, a
  scrubbed environment (no inherited Claude session markers, API keys or telemetry), a NEXUS-assigned session id,
  and an engineering-rigor work profile.
* Shell terminals (PowerShell, cmd, Git Bash, login shells on Unix) with tabs, resize, search and copy/paste.
* Sessions survive NEXUS restarts as truthful records (*exited*, *interrupted*, …) and resume with
  `claude --resume` under the same profile and directory.
* Claude gets NEXUS tools through a per-session MCP server (`nexus mcp`): workspace/project/session context, Git
  status and checkpoints that appear live in the desktop. Omitted automatically when organisation policy forbids it.

NEXUS never edits Claude configurations it did not create, never stores credentials and never runs repository
scripts on its own.

## Command line

`nexus` (crate `crates/nexus-cli`) works without the desktop app and shares its database and rules:

```bash
nexus doctor                                   # health of NEXUS, git, Claude installs, profiles, workspace
nexus workspaces create ~/nexus-personal --name Personal
nexus projects add ~/code/my-game
nexus profiles detect                          # read-only scan for ~/.claude and ~/.claude-*
nexus profiles add --name Personal --config-dir ~/.claude-personal
nexus launch -p Personal -w prototype          # Claude in this terminal, isolated like the desktop does it
nexus launch --resume last                     # continue the last conversation in this folder
nexus sessions                                 # desktop and terminal sessions; add --json for scripts
```

## Build and run

Prerequisites: Rust (stable, MSVC on Windows), Node.js 22+, Git, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS (WebView2 is preinstalled on Windows 11).

```bash
cd apps/desktop
npm install
npm run tauri dev          # development app
npm run tauri build        # installers in target/release/bundle (unsigned)
```

Set `NEXUS_DATA_DIR` to an empty folder to run against an isolated database.

## Tests

```bash
cargo test --workspace                          # Rust unit + integration (incl. PTY launch tests)
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && npm test && npm run typecheck
node e2e/smoke.mjs                              # end-to-end against a running dev app (see docs/TESTING.md)
```

## Repository layout

```text
crates/nexus-core      domain logic shared by every NEXUS process (no UI, no Tauri)
crates/fake-claude     test double for the claude executable
apps/desktop           Tauri app: Vue 3 UI (src/), Rust shell + PTY manager (src-tauri/), e2e tools (e2e/)
docs/                  architecture, ADRs, threat model, roadmap, testing, research, spec review
```

## Documentation

[Product](docs/PRODUCT.md) · [Architecture](docs/ARCHITECTURE.md) · [Decisions](docs/DECISIONS/README.md) ·
[Threat model](docs/THREAT_MODEL.md) · [Roadmap](docs/ROADMAP.md) · [Testing](docs/TESTING.md) ·
[Research](docs/RESEARCH.md) · [Spec review](docs/SPEC_REVIEW.md)

# ADR-0003 — Claude profile isolation and child-environment hygiene

Status: Accepted (2026-11-04)

## Context
Users run several Claude Code environments (different accounts, or one account with different kits) using
`CLAUDE_CONFIG_DIR`. On the dev machine a PowerShell function sets it to `~/.claude-personal`; the default
`~/.claude` is organisation-managed (server-managed settings, managed OpenTelemetry). Verified facts:

* With `CLAUDE_CONFIG_DIR` set, *everything* — including `.claude.json` and (Windows/Linux)
  `.credentials.json` — lives under that directory. Without it, global state is `~/.claude.json` + `~/.claude/`.
* Claude Code marks itself and its children with `CLAUDECODE`, `CLAUDE_CODE_CHILD_SESSION`,
  `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_ENTRYPOINT`, `CLAUDE_CODE_MESSAGING_SOCKET`/`_TOKEN`, `CLAUDE_PID`,
  `CLAUDE_EFFORT`. A `claude` started with `CLAUDE_CODE_CHILD_SESSION` is excluded from `--resume`/history.

## Decision
1. A profile has `config_dir: Option<PathBuf>`. `None` means *default location*: the child env has
   `CLAUDE_CONFIG_DIR` **removed**. `Some(dir)` sets it to the absolute, normalized path.
2. The child environment starts from the NEXUS process environment and then:
   * removes `CLAUDECODE`, `CLAUDE_PID`, `CLAUDE_EFFORT`, `CLAUDE_CONFIG_DIR` and every variable starting
     with `CLAUDE_CODE_` (session markers, messaging socket/token, entrypoint);
   * removes `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL` unless the profile explicitly
     opts in (Phase 2) — an ambient key would silently override the profile's own login;
   * sets `CLAUDE_CONFIG_DIR` (when explicit), `NEXUS_SESSION_ID`, `NEXUS_WORKSPACE_ID`, `NEXUS_PROJECT_ID`,
     `TERM=xterm-256color`, `COLORTERM=truecolor`.
3. Claude is spawned directly with an argument vector. No shell string is ever constructed. Only native
   executables are launched; npm `.cmd` shims are diagnosed with a fix (`claude install`).
4. NEXUS never writes to a profile directory it did not create (`managed_by_nexus = false`). Cloning (Phase 2)
   copies an **allowlist** (`settings.json` minus its `env` block, `CLAUDE.md`, `rules/`, `skills/`, `agents/`,
   `commands/`) and never `.credentials.json`, `.claude.json`, transcripts, history or caches.
5. Detection is read-only and checks only for the *existence* of marker files: `~/.claude`, `~/.claude-*`
   directories, and the NEXUS process's own `CLAUDE_CONFIG_DIR`. Shell aliases are not parsed; manual
   registration covers custom launchers.
6. Organisation policy is detected from machine managed settings (`managed-settings.json`, `.d/`,
   `HKLM\SOFTWARE\Policies\ClaudeCode`) and per-profile `remote-settings.json` by extracting **only** a fixed
   set of policy keys. Other values (e.g. telemetry headers) are never stored, logged or shown.

## Alternatives
* Copying Claude installations per profile — wasteful and unsupported.
* Setting `HOME`/`USERPROFILE` per profile — breaks Git, SSH and every other tool.

## Consequences
* Scenarios B/C (multiple accounts; one account with different kits) work without re-authentication.
* Machine-wide managed settings apply to every profile; the UI says so instead of implying isolation.

# Research Findings

Recorded 2026-11-04. Re-verify when Claude Code's minor version changes significantly.

## Development machine (inspection was read-only; no credential contents were read)

| Item | Finding |
| --- | --- |
| OS | Windows 11 Pro 10.0.26200; WebView2 runtime 156 present |
| Claude Code | 2.1.311, native build in `~/.local/bin/claude.exe` |
| Git | 2.55.0.windows.2 (Git Credential Manager bundled) |
| Rust | not installed at start → installed via rustup: stable 1.99.1 MSVC; VS 2022 Build Tools present |
| Node | 24.18.1, npm 11.16 (no pnpm/yarn/bun) |
| GitHub CLI | **not installed** |
| Codex CLI | 0.166.0 — has `codex review [--uncommitted]` and `codex exec` |
| Graphify | 0.9.93 (`graphify`, `graphify-mcp` stdio/HTTP MCP server, `graphify update <path>` needs no LLM) |
| Python / uv | 3.14.6 / 0.11.26 |
| WSL | only `docker-desktop` distro |
| Claude profiles | default `~/.claude` (server-managed settings: org policy + managed OpenTelemetry); `~/.claude-personal` (launched by a PowerShell profile function that sets `CLAUDE_CONFIG_DIR`) |
| Plugins | both profiles have `codex-harness` (an asset-generation delegation plugin: Markdown docs + PNG images via Codex) |
| Machine managed settings | none (`C:\Program Files\ClaudeCode\` absent; no `HKLM\SOFTWARE\Policies\ClaudeCode`) |

## Claude Code facts relied on (verified against docs and/or the installed CLI)

* `CLAUDE_CONFIG_DIR` relocates *all* of `~/.claude` **and** `.claude.json`; credentials on Windows/Linux are
  `<dir>/.credentials.json`, on macOS a keychain item keyed per config dir. `%APPDATA%\Anthropic` and keyless
  Console sign-in are not separated by it.
* Settings precedence: managed > CLI (`--settings`, flags) > local > project > user. Arrays (permissions) merge;
  hooks from multiple sources are additive (docs do not name `--settings` explicitly — verify in Phase 2).
* Session-scoped flags: `--session-id <uuid>`, `--resume <id>`, `-n/--name`, `--append-system-prompt[-file]`,
  `--settings`, `--mcp-config`, `--strict-mcp-config`, `--plugin-dir` (repeatable), `--add-dir`, `--effort`
  (low|medium|high|xhigh|max), `--model`. Session-scoped flags are **not** restored on resume.
* `--system-prompt-snapshot` (default on) freezes the system prompt incl. appended text per conversation.
* `-p` skips the workspace trust dialog.
* Native sessions: `claude agents --json` lists active sessions `{pid, cwd, kind, startedAt, sessionId, name, status}`;
  `--bg`, `attach`, `logs`, `stop`, `respawn` manage background sessions.
* `claude auth status --json` → `{loggedIn, authMethod, apiProvider, subscriptionType, orgName, email, configDirectory, projectsDirectory, …}`.
* Managed keys affecting NEXUS: `disableSideloadFlags` (rejects `--plugin-dir`, `--plugin-url`, `--agents`,
  `--mcp-config` and exits), `allowManagedHooksOnly`, `disableAllHooks`, `allowManagedMcpServersOnly`,
  `strictPluginOnlyCustomization`, `strictKnownMarketplaces`, `allowManagedPermissionRulesOnly`.
  Locations: `C:\Program Files\ClaudeCode\managed-settings.json` (+ `managed-settings.d\`, `managed-mcp.json`),
  `HKLM\SOFTWARE\Policies\ClaudeCode` value `Settings` (HKCU lowest); macOS
  `/Library/Application Support/ClaudeCode/`; Linux `/etc/claude-code/`; server-managed cached in
  `<config dir>/remote-settings.json`.
* Hooks: events include SessionStart (source: startup|resume|clear|compact|fork), SessionEnd (reason:
  clear|resume|logout|prompt_input_exit|other; 1.5 s budget), Stop, PreCompact/PostCompact, PreToolUse,
  PostToolUse, Notification, SubagentStart/Stop, CwdChanged and more. Common payload: `session_id`,
  `transcript_path`, `cwd`, `hook_event_name`, `permission_mode`. Exec form (`args` array) spawns directly —
  on Windows the command must be a real `.exe`. Handler types: command, http, mcp_tool, prompt, agent.
  Hooks inherit the environment except `OTEL_*`.
* Transcripts: `<config dir>/projects/<cwd with non-alphanumerics → '-'>/<session-id>.jsonl`; format is
  documented as internal and version-unstable; retention `cleanupPeriodDays` (default 30).
* Usage: documented via OpenTelemetry (`claude_code.token.usage`, `claude_code.cost.usage`,
  `claude_code.api_request` events), status-line JSON (`cost.total_cost_usd`, `context_window.*`,
  `rate_limits.*`) and `-p --output-format json`.
* Child markers: `CLAUDECODE=1`, `CLAUDE_CODE_CHILD_SESSION=1`, `CLAUDE_CODE_SESSION_ID`; a nested `claude` is
  excluded from `--resume`/history unless `CLAUDE_CODE_FORCE_SESSION_PERSISTENCE=1`.
* Skills: progressive disclosure (descriptions only until invoked); locations: managed, user, project,
  `--add-dir` dirs, plugins.

## Stack versions selected (2026-11-04)

Tauri 2.x (CLI 2.12.4; 3.0 is alpha), Vue 3.5, Vite 8, vitest 5, Pinia 4, vue-router 5, TypeScript 5.9,
@xterm/xterm 6, portable-pty 0.9, rusqlite 0.41 (bundled), uuid 1, serde 1, thiserror 2, which 8, dirs 7.

## Open verification items

1. ~~`--settings` hooks merge with user hooks~~ — **verified 2026-11-04**: user, `--settings` and `--plugin-dir` hooks all
   fire (additive); exec-form hooks work on Windows; payload `session_id` = `--session-id`; hooks inherit NEXUS's env;
   SessionStart/SessionEnd fire even in a signed-out `-p` session (ADR-0016).
2. Whether `allowManagedHooksOnly` blocks hooks of a `--plugin-dir` plugin (M21) — NEXUS omits its hooks under that
   policy regardless.
3. Transcript creation timing with `--session-id` when the user exits before the first message (M8 manual).
4. `claude agents --json` scope across config dirs (M11).

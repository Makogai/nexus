# ADR-0016 — Claude lifecycle hooks: deterministic activity capture and checkpoint recovery

Status: Accepted, implemented (2026-11-04, M13)

## Context
NEXUS needs reliable session start/end information, a sense of what happened in a session (files, tests, commits,
"waiting for the user"), and recovery of earlier checkpoints after Claude compacts its context — without expensive
AI summarisation, without storing prompts, and without a second persistence mechanism.

Verified on Claude Code 2.1.311 with a synthetic, signed-out profile (`claude -p` runs SessionStart/SessionEnd):
* hooks from `--settings` are **added** to user-settings hooks and plugin hooks (all three fired — nothing replaced);
* exec-form handlers (`command` = real `.exe`, `args` array, no shell) work on Windows;
* the payload `session_id` equals the `--session-id` NEXUS assigns; payloads carry `transcript_path`, `cwd`,
  `hook_event_name`, `source` (start) and `reason`, `prompt_id` (end);
* hooks inherit the environment NEXUS gives Claude.

## Decision
* **Injection**: a per-session `--settings` file (`<data dir>/sessions/<id>/settings.json`) containing *only*
  `hooks`; each handler is `nexus --data-dir <dir> hook --session <nexus id> <event>` in exec form, timeout 5 s.
  Omitted, with the reason stored on the session, under `disableAllHooks` or `allowManagedHooksOnly`. `--settings`
  is not a sideload flag, so `disableSideloadFlags` does not affect hooks.
* **Events** (all deterministic, each a ~10 ms process: open DB without integrity check or migrations, one write):

  | Hook | Recorded |
  | --- | --- |
  | SessionStart | event (`source`); transcript path; after `/clear`/fork the session follows the *new* conversation id so Resume continues it; returns bounded recovery context |
  | SessionEnd | event + `end_reason` (normalised to known values) |
  | PreCompact | event (`manual`/`auto`) |
  | Stop / UserPromptSubmit | counters only (`turns`, `prompts`, last activity) — prompt text is never read into storage |
  | PostToolUse (matcher `Write|Edit|MultiEdit|NotebookEdit|Bash`) | file paths relative to the project (deduplicated; outside paths keep only the file name); Bash → a *category* (`git-commit`, `git-push`, `test`, `build`, `dependencies`) — raw commands are never stored |
  | Notification | `attention` ("waiting for you", shown live in the desktop) until the next activity |
  | SubagentStop | counter |
* **Storage**: existing core + SQLite — `session_events` (now without a `kind` CHECK; kinds validated in code) and
  session counters (migration 0004). At most 1000 hook events per session; counters keep counting.
* **Recovery (lazy, bounded ≤ 1200 chars)**: startup/`clear` → at most a one-line pointer ("N checkpoints available;
  use the NEXUS MCP tools") and only when the session has the MCP server; `resume` → the 3 latest project
  checkpoints; `compact` → this session's checkpoints and changed files (what the compaction removed). Text is
  framed as data recorded earlier.
* **Never disturbs Claude**: `nexus hook` always exits 0, reads ≤ 1 MiB, prints only for SessionStart
  (`hookSpecificOutput.additionalContext`) and stays silent for UserPromptSubmit/Stop (whose stdout would be injected
  or interpreted). Missing database or different schema → no-op.
* **Consolidation later**: the MCP tool `get_session_activity` exposes files, command categories, turns and
  compactions of the session (and up to 5 previous project sessions) so durable memory can be written deliberately
  (ADR-0013) — never automatically after each tool call.

## Alternatives
* A NEXUS plugin via `--plugin-dir` with `hooks/hooks.json` — also additive, but blocked by `disableSideloadFlags`
  and it would need a per-install plugin directory; kept for skills (Phase 4).
* Hook type `http` to a local NEXUS endpoint — adds a listener (ADR-0009 rejects local endpoints).
* Prompt/agent hooks — model calls on every event; contrary to the cost goal.

## Verification
Unit (`activity`, `claude::hooks`), CLI end-to-end (`nexus hook` robustness, recovery output, policy omission),
desktop E2E (hook settings passed; Notification shows live), and `e2e/hooks-real-claude.mjs`: the real Claude CLI runs
the generated hooks and NEXUS records SessionStart and SessionEnd for the right session.

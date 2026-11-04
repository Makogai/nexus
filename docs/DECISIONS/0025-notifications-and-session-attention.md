# ADR-0025 — Notifications and session attention: reported signals only, one event sink, desktop delivery

Status: Accepted, implemented (2026-11-04, stage "Intelligence, Constitutions, Context & Notifications", milestones B/C)

## Context
With several Claude sessions running in different workspaces and profiles, the owner needs to know which one waits
for them — a permission prompt, a question, a finished turn, a failure — and to get there in one click, also when
NEXUS is minimised. Producers live in different processes: Claude runs NEXUS's hooks (`nexus hook`), the NEXUS MCP
server (`nexus mcp`), `nexus launch`, the desktop host. Constraints: never approve, deny or answer anything for the
user; never inject keystrokes; never claim to detect every question; no session may raise attention for another
profile or workspace; no prompt text, tool input or model output stored.

## Decision
* **One sink.** Every producer calls `notify::publish(conn, NewEvent)` — deterministic SQL, safe inside a hook. A
  typed event has a category (input-required, permission-requested, agent-finished, task-completed, task-blocked,
  session-finished, session-interrupted, workflow-failed *(reserved)*, process-issue, review-required, info), a
  priority (critical, action, success, warning, info), its identity (workspace, project, profile, session, task) and
  an "Open" target. Rows live in SQLite (`notifications`, migration 0010) — machine-local state (ADR-0005).
* **Signals, not guesses** (`attention.rs`). Supported hooks are subscribed in the session-scoped `--settings`:
  `PermissionRequest` (tool name only), `Notification` (`permission_prompt`, `idle_prompt`, elicitation and
  `agent_needs_input` types; older CLIs: the message), `PreToolUse` matcher `AskUserQuestion`, `Elicitation`,
  `Stop`, `StopFailure` (allowlisted error codes) — plus `UserPromptSubmit`/`PostToolUse` (incl. `AskUserQuestion`)
  as "Claude continued". The session row gets a structured `attention_kind` (permission | input | idle | error);
  the UI status (running, waiting-for-input, permission-required, idle, error, completed, failed, interrupted,
  unknown) carries a certainty: `reported`, or `assumed` when no signal exists (a quiet session is "Running", a
  session without hooks "Unknown" — never "Permission required").
* **Observe only.** `nexus hook` prints nothing for these events and exits 0, so Claude's own dialogs decide.
* **Ownership.** Identity always comes from the NEXUS session row; a hook payload naming a different Claude
  conversation is ignored. MCP `request_attention(reason, message)` is bound to the server's own session (it has
  no session argument), escaped, ≤ 200 characters.
* **Noise control.** Same situation → one notification (`dedupe_key`, 10 min window; repeats count `×n` and are not
  re-delivered); ≤ 8 new notifications per session per minute; ≤ 3 MCP requests per session per 10 minutes; a
  resolved situation (Claude continued, session ended) is marked `resolved_at`; retention 30 days.
* **Lifecycle producers.** Session exit (desktop `on_exit`, `nexus launch`) → finished / interrupted (not when the
  user stopped it in NEXUS); host reconciliation (crash) → interrupted; `update_task` done/blocked from Claude;
  memory proposals created at SessionEnd → review-required.
* **Desktop delivery.** Only the desktop shows notifications: a poller decides each pending row once
  (`desktop_state` pending → sent | suppressed with a reason) from preferences — desktop on/off, per-category in-app
  and desktop, muted workspaces/profiles, quiet hours (critical passes), sound, and focus (no toast for the session
  whose terminal is visible in the focused window unless enabled). Windows: WinRT toasts; a click focuses NEXUS and
  opens the target (switching workspace; a vanished session is explained). Development builds show as "Windows
  PowerShell" (unpackaged executables have no AppUserModelID — the same rule as Tauri's notification plugin);
  installed builds use the bundle identifier. macOS/Linux: Tauri's notification plugin (display only). The OS
  setting (Windows `NotificationSetting`) is shown when notifications are off. A recording adapter replaces the
  OS in E2E builds (`e2e-webview-debugging` + `NEXUS_E2E_NOTIFIER=record`).
* **Surfaces.** Bell with unread/action counts and a session attention list; notification center (inbox, history,
  filters by workspace/project/profile/session/priority, read, dismiss, snooze, open); preferences; session-row
  badges; `nexus notifications list|read|dismiss|clean`; an ATTENTION column in `nexus sessions`.

## Verification
* Core: `notify::tests`, `attention::tests` (permission flow, idle/turn end, questions, elicitation, API errors,
  foreign conversations, no-hook sessions, notification-type table).
* CLI: `attention_hooks_and_mcp_raise_notifications_without_acting` (real `nexus hook` processes and `nexus mcp`).
* Desktop: `e2e/attention.mjs` — three synthetic sessions in two workspaces/profiles; delivery, ×n, isolation,
  board/badges, click on a desktop notification (workspace switch), center click, focus rule, mute, quiet hours,
  vanished session, session exit, crash + restart (persisted, no re-delivery, interruptions announced).
* Real Claude 2.1.311 (synthetic signed-out profile, `e2e/hooks-real-claude.mjs`): the CLI accepts all attention
  hooks; its `StopFailure` (`authentication_failed`) becomes "Claude is not signed in for this profile".
* Not verifiable with signed-out synthetic profiles: permission/idle/question/elicitation events need a model turn.
  They are tested with payloads of the documented schema; MANUAL_CHECKS step 23 covers them with a real profile.

## Alternatives
* Inferring prompts from terminal output (regexes on the TUI) — brittle, language-dependent, would claim more than
  it knows. Rejected.
* Answering permissions from NEXUS (`PermissionRequest` decisions, `--permission-prompt-tool`) — explicitly out of
  scope; it would move the approval out of Claude's own UI.
* Tauri's notification plugin on Windows too — it offers no click callback on desktop.

## Consequences
* Sessions started outside NEXUS produce no notifications (no NEXUS hooks), by design.
* Permission prompts for tools outside the `PostToolUse` matcher resolve at the next turn end or prompt.
* Toast clicks need NEXUS running; after a restart old toasts in the Action Center do nothing.

## Amendment — pre-release review (2026-11-04, workflow-engine stage)
* **Sender identity.** NEXUS builds its toasts itself (WinRT, no wrapper crate) and registers its own sender:
  `HKCU\Software\Classes\AppUserModelId\dev.nexus.desktop` "NEXUS" (installed builds, at start) or
  `….dev` "NEXUS (development)" (builds under `target\`, on their first notification). No "Windows PowerShell"
  attribution any more. Verified on Windows 11 with an opt-in self-test (`toast_selftest`, temporary sender, no
  popup): Windows accepts the toast and files it under NEXUS's sender; `Setting()` is "Element not found" until the
  first toast and Enabled after it (so that error is not read as "notifications off"); withdrawal works. The NSIS
  uninstaller removes NEXUS's keys (`windows/hooks.nsh`).
* **Clicks.** In-process activation while the toast object lives (kept per notification); installed builds also
  register `nexus://notification/<id>` so a click from the Action Center works after a restart: Windows starts NEXUS
  with the URI, the single-instance plugin hands it to the running NEXUS (or the new one takes it at start). The
  handler accepts only a notification UUID and only focuses NEXUS and opens that notification's target (T36).
  E2E: a second start with the URI opens the right session.
* **Withdrawal.** Notifications read, dismissed or resolved in NEXUS are removed from the Action Center; stale ones
  of an earlier run are cleared at start.
* **Lock screen.** Desktop text follows a content policy (`lockScreen`): *details, but generic while the workstation
  is locked* (default; best-effort detection: the lock screen's `LogonUI.exe` is running), *always generic* (never
  project, session, task, tool or file names), or *always details*. In-app notifications keep the details. E2E: the
  generic text names nothing while the in-app entry keeps the details. Windows' own lock-screen settings still apply.

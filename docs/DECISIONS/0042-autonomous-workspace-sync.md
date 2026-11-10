# ADR-0042 — Autonomous workspace sync and commit messages Claude proposes

Status: Accepted (2026-11-06, owner feedback; branch `milestone/ux-pass`). Extends ADR-0039 (sync policies).

## Context

ADR-0039's *trusted automatic* mode publishes only NEXUS's own records (development records, current state,
`project.json`, `environment.json`) and stops when anything else is pending. For the owner it "never really worked":
Claude's work in the workspace — tasks, notes, decisions, drafts — never travelled on its own, and the commit messages
for reviewed commits had to be typed by hand. The owner: "for that wrapper I really want autonomous pushes; Claude
instances that do changes should always write the commit messages."

## Decision

1. **A fourth policy, autonomous ("Everything, automatically")** — opt-in per workspace on this machine, with the same
   private-remote confirmation as trusted, forbiddable by the organisation (`workspaceSync.autonomousSync`, and it is off
   whenever `trustedAutoSync` is off).
   * **When:** a Claude turn ends (Stop hook) or a session ends. The hook only marks the workspace due in 20 s
     (`policy::SETTLE_MS`), so a burst of turns becomes one commit; the session-exit path marks it due at once (sessions
     without hooks are covered). The host's heartbeat runs due syncs every 15 s on its own database connection, so a push
     never holds the connection the UI uses.
   * **What:** every change in the workspace repository, through M29's route unchanged — the inspected commit (secrets,
     private files, nested repositories block it), fetch, the push review, push. Never forced; a finding, a changed
     remote or branch, or a remote that is ahead stops it and asks once in a notification. Machine-local state never
     travels (the repository's ignore rules and the inspection keep it out).
2. **Commit messages from the session that made the changes.**
   * MCP tool `propose_commit_message` (bounded: a subject of ≤ 100 characters and a short body, control characters
     removed), stored with the session; one NEXUS-authored system-prompt line asks sessions with NEXUS tools to use it.
   * `message::suggest` takes the sessions' proposals since the last commit (a commit marks them used); without one, it
     composes a message from the changed paths: `NEXUS: umbra — 2 tasks, 1 note, overview; workspace — 1 rule`.
   * The Sync Center pre-fills the reviewed commit with it (the person can edit it); autonomous sync commits with it.

## Consequences

* The workspace remote follows Claude's work within about half a minute, with readable history.
* The person's own edits in the workspace travel too under autonomous — the confirmation dialog says so.
* Another computer still approves synced rules, constitutions, workflows and kits before its sessions use them.
* Hooks stay cheap: a settings write, no Git or network work in the hook.

## Alternatives considered

* **Widening the trusted allowlist** — would change what an existing confirmation covered; a separate mode asks again.
* **Committing from the hook** — Git and network work in a hook would slow Claude and break ADR-0016.

## Amendment (2026-11-06, owner report; branch `fix/autosync-and-drafts`)

Every automatic sync after a learning session stopped with "the security inspection reported 2 finding(s)" — the
findings were `machine-path` warnings (absolute paths of a computer in Claude's memories and a task), not secrets, and
the message did not say which. Now:

* **Only real risks stop it:** a blocking finding, a `possible-secret` warning, or a change too large to inspect.
  Portability and size warnings (`machine-path`, a large file below the limit) are named in the history entry ("noted,
  not stopping: …") — a reviewed commit shows them the same way and never blocked on them.
* **The reason is precise:** the history entry and the notification name the findings — kind, file and line, never the
  text found (`secret in project-context/shop/tasks/a.md:1`, up to three, "+N more").

## Amendment (2026-11-10, ADR-0047; branch `milestone/v0.5-alpha`)

A failed automatic publication only wrote a history entry and retried every ten minutes — also when the remote refused
the push. Now:

* **The first failure is announced:** a warning in the notification center (offline fetch or failed push), one open
  notification until a publication succeeds — retries do not repeat it.
* **A rejected push pauses and asks**, like a remote that is ahead: the history says why, the Sync Center notification
  asks the person to pull first, and no retry is scheduled (the remote would keep refusing). NEXUS never force-pushes.

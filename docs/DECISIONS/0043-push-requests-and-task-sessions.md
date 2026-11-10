# ADR-0043 — Push requests, finished tasks, and where a task's work starts

Status: Accepted (2026-11-06, owner feedback; branch `milestone/ux-pass`). Push outcomes amended by ADR-0047.

## Context

Three owner reports from daily use:

1. Claude fixed a security bug in a project, but its permissions blocked `git push`; the owner had to push from a
   terminal by hand — "would be nice to approve it in a popup".
2. An urgent task Claude fixed, pushed and branched, and a finished learning task, stayed under "Open tasks".
3. "Is a brand-new session per task really the best way, token- and UX-wise?"

## Decision

1. **Push requests.**
   * MCP tool `request_push` (one-line reason) records a request for the session's project: the checked-out branch and
     its commit. The MCP server writes only the request (migration 0018, machine-local) — no Git writes, no network
     (ADR-0009). One pending request per branch; asking again reports the outcome (pending, pushed, declined, failed).
   * A notification and a banner on the project page open the dialog: Claude's reason (labelled), the branch → remote,
     the commits not on the remote branch, and the same security inspection as a workspace push.
   * **Push** sends exactly the reviewed commit (a token), never forced, with a first push setting the upstream; a
     blocking finding or a branch that moved since the review stops it. **Decline** records the decision.
2. **Finished tasks.**
   * The task pointer (NEXUS-authored) tells Claude to set the status to *review* and log what it did when the work is
     complete; the person marks it *done*.
   * The project page lists tasks ready for review first (then in progress; urgent before the rest), each with *Done*;
     *Recently done* (five) offers *Reopen*. Both are logged in the task file.
   * Reviewing the last learning draft closes the learning task (overview accepted, architecture written, nothing left
     to review); *Finish learning* closes one reviewed earlier.
3. **Where a task's work starts** — the launch dialog's *Start in*:
   * **New session** — a clean context; Claude reads the project again. Best for unrelated work.
   * **The task's own session** — resume it (or open it while it runs). The default when it exists: the cheapest way
     to pick the same task up again.
   * **A running session of the project** with NEXUS tools — it already has the project's context and a warm prompt
     cache; cheapest for small, related tasks, but its context keeps growing, so it is never the default. NEXUS types a
     fixed message with the validated task id into that terminal ("work on NEXUS task <id> next…"); the task's text
     reaches Claude through `get_task`, labelled as data (T22). The session's task link moves; the previous one is kept
     as a `task-switch` event.
   * *Start right away* (the fixed first message) is on by default for task launches and remembers the last choice.

## Consequences

* Pushing stays the person's decision, made where the context is (the commits and findings), in one click.
* Open tasks show what needs the person; finished work leaves the list without manual file edits.
* Handing over saves re-reading the project for related work; the person chooses it knowingly.

## Alternatives considered

* **Letting the MCP server push** — would give the MCP server network and Git write access (ADR-0009).
* **Auto-marking tasks done when a session ends** — "ended" is not "finished"; Claude reports *review*, the person
  decides *done*.
* **Injecting the task text into the running session** — workspace text would become a user message; the id-only
  hand-off keeps it data.

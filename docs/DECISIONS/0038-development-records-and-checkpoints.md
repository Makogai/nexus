# ADR-0038 — Development records, durable checkpoints and project history

Status: Accepted (2026-11-05, stage "Portable Workspace", milestone P6)

## Context
The owner wants NEXUS to preserve meaningful development state automatically — task progress, files changed, branch
and commit references, tests and verified results, decisions, blockers, discoveries, next steps — without calling a
model on every tool call, without fabricating decisions or results, and with verified facts kept apart from what
Claude reports. Runtime state (events, checkpoints) already exists in SQLite (ADR-0013/0016), but it is machine-local
and disappears from view when work continues on another computer.

## Decision
1. **A development record per meaningful session**, written by NEXUS (no model) when a Claude session in a project
   ends: `project-context/<slug>/development/<date>-<session>.md`, a `kind: note` document with
   `topic: development-record`, `verification: unverified`, `project`, `branch`, `commit` (HEAD at the end), `session`
   and `task`. *Meaningful* = files edited, commits made, gates run or checkpoints other than notes; chat-only sessions
   leave none.
2. **Three labelled layers** in every record: **Verified** — branch and HEAD from Git, the commits between the HEAD at
   session start (`sessions.head_start`, recorded at launch) and the end, gate results NEXUS ran during the session;
   **Observed** — files edited and command *categories* from Claude Code's hooks (results are not observed); **Reported
   by Claude (unverified)** — its checkpoints. Missing facts are stated as missing ("commits unknown: no starting
   commit"), never filled in. Untrusted text (commit subjects, checkpoint text) is escaped and bounded.
3. **Durable checkpoints**: at context compaction NEXUS adds its own checkpoint of what changed so far (from the event
   log); task and workflow completion already add checkpoints. Checkpoints stay machine-local runtime state; the record
   is the portable, reviewable result.
4. **Every end is covered, once**: the desktop's PTY exit, `nexus launch`'s exit, and — for sessions whose host died —
   `finalize_pending` at host start and on every heartbeat. `sessions.record_at` makes it idempotent (migration 0017).
   A record is written only when the workspace keeps them (schema 2, feature `development-records`, writable); the
   feature is on for new and migrated workspaces and can be removed from the manifest to opt out. The project's
   `current-state.md` is regenerated after each record.
5. **Development History** (`devrecord::history`, `nexus projects history`, desktop view per project): records with
   their sync state in the workspace repository (*not committed* / *committed, not pushed* / *synced* / *workspace not
   in Git*), sessions, open tasks, decisions, recent commits, gate results and reported blockers — all read from the
   stores.
6. Records never run in a hook process (hooks stay cheap, ADR-0016); they are written by session hosts.

## Consequences
* Development records are ordinary knowledge: searchable, branch-aware, synced with the workspace through the reviewed
  sync (P7 adds policies), and subject to the same pre-commit inspection.
* A record contains project-relative file names and commit subjects; it never contains prompts, command text or tool
  output.
* Sessions started before migration 0017 have no starting commit; their records say so.

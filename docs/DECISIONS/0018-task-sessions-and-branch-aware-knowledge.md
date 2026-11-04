# ADR-0018 — Task-driven sessions and branch-aware knowledge

Status: Accepted, implemented (2026-11-04, M17 + M19). **Amended by ADR-0020**: the task brief no longer goes into
the system prompt; the system prompt carries only a pointer with the task id and the task text arrives as an escaped
data block in SessionStart context.

## Context
Tasks are Markdown files (ADR-0013). The owner asked for: launching Claude directly from a task with the right
workspace, project, profile, work profile, task details and relevant context; Claude updating task status and
recording meaningful progress through the NEXUS MCP server; a flow that survives a NEXUS restart and resumes the
work; branch-aware memories with Git authoritative for branch state; and no task histories, knowledge bases or
constitutions injected wholesale into sessions.

## Decision

### One preparation step for every session host
`nexus_core::claude::prepare` builds everything a session gets from NEXUS — MCP config, hook settings, approved
always-on rules (minus duplicates of loaded CLAUDE.md text, ADR-0019) and the task brief — for both the desktop and
`nexus launch`. The outcome is stored in `sessions.integrations` (`mcp`, `hooks`, `rules`, `task`).

### Launching for a task
* `sessions.task_id` (migration 0006) links a session to a task id; the task file stays authoritative. Resumed
  sessions inherit it.
* A launch for a task uses the task's project (a task bound to another project is refused), the chosen profile
  and its work profile, the task title as the session name, and moves a `backlog`/`ready` task to `in-progress`
  with a log line (optional in the desktop dialog). Resuming never changes the status.
* *(Superseded by ADR-0020 — kept for history.)* **Brief, not history**: Claude gets ≤ 900 characters appended after the work profile and rules — id, title,
  status, priority, branch, a 360-character excerpt of the description, acceptance criteria and up to eight
  numbered checklist items — framed as data from a workspace file (threat T22). The `## Log` section is never
  included.
* Resume: Claude keeps a conversation's original system prompt (`--system-prompt-snapshot`), so the brief there
  may be stale. SessionStart (`resume`, `compact`) therefore adds one line with the task's *current* state from
  its file: status, checklist progress, last log entry, and the `get_task` pointer.

### Task workflow through MCP
* `get_task` (read-only; defaults to the session's task): status, description (≤ 4,000 chars), numbered checklist,
  last five log entries, content hash.
* `update_task` (defaults to the session's task): `status`, a dated `log` line (marked `[claude]`), `check` /
  `uncheck` checklist items, `linkBranch` (records the checked-out branch and HEAD commit), `expectedHash`.
  Status and log changes are also written as session checkpoints (`blocked` → blocker, `done` → done, otherwise
  progress), so compaction recovery, resume and the desktop timeline see them. Marking a task done with open
  checklist items returns a warning.
* Body convention stays plain Markdown: `- [ ]` / `- [x]` items before `## Log`; dated `- YYYY-MM-DD …` lines
  after it. People edit the same file by hand.

### Branch-aware knowledge (Git authoritative)
* Any document may carry `branch:` (a validated branch name) and `commit:` (a full commit id — the stable
  identifier). NEXUS never stores which commits a branch contains.
* Relevance is decided at query time from the project's live Git state: **this-branch** (name matches the
  checked-out branch), **inherited** (the recorded commit is reachable from HEAD — e.g. after the branch was
  merged), **other-branch** (hidden by default), **unbound** (no branch).
* `record_memory` takes `scope: workspace | project | branch`; branch scope stamps the branch and HEAD commit.
  `nexus knowledge new --branch current` does the same from the CLI.
* `get_context_index` lists this-branch and inherited notes by title only and counts hidden other-branch items;
  the SessionStart pointer counts notes for the session's branch by name from the index (no Git call in a hook).
  Git history is read only when a document names a branch or commit (`git rev-list --max-count=5000 HEAD`).

### Ids in the CLI
Document ids are UUIDv7 (time-ordered): their first digits are a timestamp and collide for documents created
together. The CLI shows the **last** eight hex digits and resolves an id, its unique end, an exact title, or a
unique start (`knowledge::service::resolve`). Session ids (UUIDv4) keep prefix matching.

## Alternatives
* Initial prompt ("work on task X") as a positional argument — starts consuming quota before the user says so;
  rejected. The brief lets the user start with one word.
* Task state in SQLite — breaks portability (ADR-0005); the file stays the record, SQLite only links sessions.
* Storing branch membership (commit lists) in the index — duplicates Git and goes stale on every fetch/rebase.
* Branch name only — renames and deleted branches break the link; the commit id is the stable anchor.

## Consequences
* A task can be picked up on another machine: status, checklist and log travel with the workspace; which
  sessions worked on it is machine-local.
* Rebased-away commits make a branch note "other-branch" until the user re-links it; this is visible, not silent.
* Still open: launching several tasks in parallel worktrees, task dependencies, and consolidating session
  activity into proposed notes (M17 follow-up, human-reviewed, no per-call AI).

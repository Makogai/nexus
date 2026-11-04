# ADR-0005 — Portable files vs machine-local SQLite

Status: Accepted (2026-11-04)

## Context
The spec makes SQLite authoritative for project registrations, tasks and memory, but also requires that a
workspace can be reconnected on another machine without pushing the database (§6.4–6.5).

## Decision
* **Workspace directory (portable, Git-friendly)**: `nexus-workspace.json` — schema-versioned manifest with
  id, name, description and project entries (id, name, remote URL, path *relative to the workspace* when the
  project lives inside it). From Phase 3–4, `knowledge/` and `tasks/` hold Markdown with front-matter.
  All writes are atomic (temp file + rename). Files are human-editable.
* **SQLite (machine-local)**: workspace roots on this machine, absolute project paths, Claude profiles,
  sessions, processes, usage samples, command approvals, app settings, and FTS indexes *derived* from
  workspace files.
* On workspace open NEXUS reconciles manifest → SQLite: new entries are added, entries with no local path are
  shown as "not on this machine" (clone/locate), nothing is deleted silently.
* The workspace `.gitignore` excludes `repos/` (default clone location) so independent project repositories
  are never absorbed into the workspace repository.

## Alternatives
* SQLite-only + export/import — loses Git diffs/merges and turns the DB into a sync artifact.
* Files-only — sessions/processes/usage are high-churn, machine-specific and never human-edited.

## Consequences
* Two writers (UI and MCP) may edit task files; writes are atomic and the index tolerates external edits.
* A workspace repo shared with others is untrusted input (threat T6).

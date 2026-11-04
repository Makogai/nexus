# ADR-0013 — Portable knowledge files: identity, schema, atomic writes, indexing, sync boundaries

Status: Accepted (2026-11-04, owner-approved refinement of ADR-0005). Implemented 2026-11-04 in
`nexus_core::knowledge` (M16, M19 core); see "Implementation notes" at the end. ADR-0017 adds the kinds `rule`,
`spec` and `workflow` and the context hierarchy.

## Context
ADR-0005 makes workspace files the source of truth for portable knowledge and SQLite a rebuildable index. The
owner approved it with explicit requirements: stable IDs, frontmatter schemas, atomic writes, indexing recovery
and explicit privacy/sync boundaries.

## Decision

### Authority
* **Files are authoritative** for tasks, decisions and durable project/workspace memory.
* **SQLite is a cache** for them (index + full-text search) and is **authoritative** only for machine-local runtime
  state: sessions, session hosts, processes, usage samples, command approvals, profile registrations, path bindings,
  UI preferences. Dropping every index table must never lose information.

### Layout (inside a workspace)
```text
tasks/<id>.md                              one task per file
knowledge/decisions/<id>.md                architecture/product decisions (project field scopes them)
knowledge/notes/<id>.md                    durable memory: facts, conventions, known issues
.nexus-local/                              local-only records (same format), git-ignored, never synced
```
Filenames are the record id: renaming a title never renames a file, so Git history stays readable and merges stay
simple. Humans see titles through NEXUS or the frontmatter.

### Identity
* `id` is a **UUIDv7** (time-ordered, globally unique, generated once, immutable). The short display key is the
  last 8 hex digits. IDs are never reused; deleting a record deletes its file.
* Cross-references use ids (`depends_on: [<id>]`, `decision: <id>`), never paths or titles.

### Frontmatter schema (version 1)
YAML frontmatter delimited by `---`, followed by a Markdown body.

| Key | Kinds | Type / values | Required |
| --- | --- | --- | --- |
| `nexus` | all | file schema version, integer (`1`) | yes |
| `id` | all | UUIDv7 | yes |
| `kind` | all | `task` \| `decision` \| `note` | yes |
| `title` | all | string, 1–200 chars | yes |
| `project` | all | project id, or absent = workspace-scoped | no |
| `created`, `updated` | all | RFC 3339 UTC | yes |
| `source` | all | `human` \| `claude` \| `import` | yes |
| `status` | task | `backlog` \| `ready` \| `in-progress` \| `review` \| `blocked` \| `done` | yes |
| `status` | decision | `proposed` \| `accepted` \| `superseded` | yes |
| `priority` | task | `low` \| `medium` \| `high` \| `urgent` | no |
| `branch`, `labels`, `acceptance` | task | string, string[], string[] | no |
| `verification` | note | `verified` \| `unverified` \| `ai-interpretation` | yes |
| `privacy` | all | `shared` \| `local-only` (must match the folder) | no (default `shared`) |

Rules: unknown keys are preserved on rewrite (forward compatibility); known keys are type-checked; invalid files
are **reported, never dropped or rewritten** (the index stores the validation error). The YAML parser runs in a
restricted mode: no anchors/aliases, no custom tags, 256 KiB frontmatter limit, 2 MiB file limit. Content written
by Claude defaults to `source: claude` and, for notes, `verification: unverified`.

### Atomic writes and concurrency
* Every write goes through one core module (used by desktop, CLI and MCP): write a temp file in the same
  directory, flush + `fsync`, then rename over the target (already the pattern of `workspace::write_atomic`).
* Optimistic concurrency: an update carries the content hash the writer read; if the file on disk changed since,
  the write fails with `conflict` and the caller re-reads (MCP `update_task` returns the current version).
* NEXUS never holds file locks; external editors and `git pull` are expected writers.

### Index and recovery
* Index tables (`knowledge_items`, FTS5 `knowledge_fts`) record id, workspace, kind, path, title, status, project,
  `updated`, content hash, size, mtime, validity and error.
* Refresh is incremental: compare (path, size, mtime) → re-hash changed files → re-parse; detect deletions. Runs on
  workspace activation, after NEXUS writes, after Git sync, and on demand (`nexus index rebuild`).
* Recovery: the index can always be dropped and rebuilt from files (also done automatically when its schema version
  changes or the database was quarantined). **Duplicate ids** (e.g. after a merge copied a file) are flagged as a
  conflict in the UI; both files stay untouched until the user resolves it.

### Privacy and sync boundaries
* **Never** in workspace files: credentials or tokens, absolute machine paths, session transcripts, raw terminal
  output, usage metrics, database files.
* `privacy: local-only` records live under `.nexus-local/` (git-ignored by the generated `.gitignore`).
* The manifest gains `sync: git | none` and `classification: personal | organization`. NEXUS never copies records
  between workspaces of different classification without an explicit user action, and MCP scope never crosses
  workspaces (ADR-0009).
* Before any NEXUS-initiated push (Phase 6) the outgoing diff is scanned for secret patterns and absolute paths;
  matches block the push with an explanation.

## Alternatives
* SQLite-authoritative tasks with export — contradicts portability; merges become database merges.
* One big `tasks.md` — merge conflicts on every concurrent edit; no stable per-record identity.
* TOML/JSON frontmatter — less pleasant to edit by hand; YAML (restricted) is the de-facto Markdown convention.

## Consequences
* Two-way editing (humans, Claude, Git) is safe by construction; conflicts surface instead of being lost.
* The index must tolerate half-written external edits (invalid files are reported and retried on the next refresh).

## Implementation notes (2026-11-04)
* Module `nexus_core::knowledge`: `frontmatter` (strict YAML subset: scalars, flow/block lists of scalars,
  comments; anchors, aliases, tags, nested maps, multi-line blocks and duplicate keys are rejected with a line
  number), `store` (create/update/atomic write, frozen `nexus`/`id`/`kind`/`created`), `index` (SQLite + FTS5,
  migration 0005), `service` (shared by desktop, CLI and MCP), `context` (ADR-0017), `skills`.
* Kinds and folders: `rules/`, `tasks/`, `knowledge/decisions/`, `knowledge/notes/`, `knowledge/specs/`,
  `workflows/`; `rule`/`spec`/`workflow` have `status: active | draft | archived` (optional), rules also
  `applies: always | on-demand` and `stacks: [..]`. Scanning is limited to those folders (depth 6, 10,000 files
  per root, hidden files and symlinks skipped).
* The content hash is FNV-1a 64 over the file bytes (change detection and optimistic concurrency, not security);
  approvals of always-on rules (ADR-0017) compare the same hash.
* Secret check (`check_portable`): PEM headers and token prefixes (`sk-ant-`, `ghp_`, `github_pat_`, `xoxb-`,
  `xoxp-`, `AKIA`) followed by a contiguous token, anywhere; absolute machine paths in frontmatter. Writes that
  match are refused; existing files that match are reported invalid.
* Writing a `local-only` record first ensures `.nexus-local/` is listed in the workspace `.gitignore`.
* Recovery: `nexus knowledge reindex --rebuild` / desktop "Rebuild index" drops the workspace's index rows and
  rebuilds them from the files; tested to recover the same documents.
* Not yet implemented: manifest keys `sync` and `classification`, the pre-push secret scan (Phase 6, M29), and
  `depends_on`/`decision` cross-reference validation.

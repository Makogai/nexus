# ADR-0036 — Workspace schema 2: portable layout, versioning, compatibility and migration

Status: Accepted (2026-11-05, stage "Portable Workspace", milestone P1)

## Context
The workspace repository is meant to carry a person's whole AI development environment between computers
(owner specification, *Portable Workspace*). Schema 1 (ADR-0005/0013) has a manifest with identity and projects,
knowledge folders, and `repos/` for clones. It has no record of which NEXUS created it, what it needs, which template
it came from or which features it uses; a newer manifest cannot be opened at all; and there is no standard place for
portable *project* knowledge. The specification proposes `nexus.yaml`, `nexus.lock.yaml`, `project-context/<id>/` with
`project.yaml` / `environment.yaml`, and `projects/` for clones.

## Decision
1. **One manifest, still `nexus-workspace.json`, now schema 2.** It is the specification's `nexus.yaml`. JSON stays
   the format of machine-written manifests (`nexus-workspace.json`, `nexus.lock.json`, `project-context/*/project.json`,
   `environment.json`); documents stay Markdown with the strict frontmatter subset (ADR-0013). Reasons: every existing
   workspace, the sync inspection and older NEXUS versions read this file; JSON parsing is strict and already
   dependency-free; YAML for nested manifests would need a full YAML parser (aliases/anchors, a new dependency) and a
   second manifest would split the source of truth. Two formats, each with one job: JSON for data NEXUS writes,
   Markdown + frontmatter for text people write.
2. **Schema 2 fields** (all optional when read, so schema 1 files stay valid):
   `createdWith` (NEXUS version that created the workspace), `migratedWith` (the last NEXUS that migrated it),
   `minNexusVersion` (oldest NEXUS that may *write* it), `template` (`{ id, version }` — the starter template; its
   component versions live in the lock file), `features` (e.g. `project-context`, `development-records`),
   `layout.projects` (`projects` for new workspaces, `repos` for migrated ones), per project `context`
   (`project-context/<slug>`). Unknown fields keep being preserved.
3. **Compatibility instead of refusal.** A manifest whose `schemaVersion` is newer than this NEXUS understands, or
   whose `minNexusVersion` is higher than this NEXUS, opens **read-only**: it can be browsed, its projects launched,
   its knowledge read and searched; every write to workspace files (knowledge store, manifest, templates, migration)
   is refused with the reason. A manifest that cannot be parsed at all is still an error.
4. **Layout for new workspaces**: `projects/` (ignored by Git) instead of `repos/`; `constitutions/` and
   `project-context/` join the scanned knowledge folders; nothing else is created empty. `.gitignore` gains
   `projects/`, `repos/` and `.nexus-local/`.
5. **Migration 1 → 2 is explicit and recoverable.** `plan` lists every change (manifest fields added, `.gitignore`
   lines appended) — nothing is moved or deleted: `repos/` and top-level project documents keep working. `apply`
   copies each file it will change into `<data dir>/workspace-backups/<workspace id>/<time>/` with the plan, writes
   atomically, and puts the copies back if any step fails. Running it twice is a no-op.
6. **Template versions are separate from the schema**: the manifest names the template; `nexus.lock.json` records each
   installed component's template version and original content hash (ADR-0037).

## Consequences
* Older NEXUS builds (schema 1 readers) refuse a schema 2 workspace — as they always refused newer manifests. From now
  on newer workspaces degrade to read-only instead.
* Write paths call `workspace::ensure_writable` (knowledge store for workspace roots, manifest writes, migration,
  templates); the global root in the data folder is unaffected.
* The spec's YAML file names map to JSON files; if a later stage needs YAML, the schema version is the switch.

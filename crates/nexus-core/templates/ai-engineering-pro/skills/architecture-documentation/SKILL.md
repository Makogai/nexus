---
name: architecture-documentation
description: Writes or updates architecture docs from the code - context, modules, data flows, dependencies, ADRs - in Markdown with Mermaid, every claim traceable. Use when documenting system design.
---

# Architecture documentation

Produce architecture documentation that a newcomer can read in about fifteen minutes and that is true: every
statement traces to files in the repository, and everything inferred is marked as such.

## Ground rules

- The code and configuration are the source of truth, not memory, folder names or the intended design. Each
  section cites files (`path`, or `path:line` for specific behaviour).
- Mark anything not provable from the repository (runtime topology, deployment, intent, traffic) with
  `> Inferred - please verify:`. List unknowns instead of filling them in.
- Never invent decision history. An ADR written after the fact records what the code shows and marks the
  rationale "unknown - please fill in" unless a source (commit message, issue, doc) states it.
- Prefer updating existing docs over adding new files. Ask before introducing a new docs structure.
- Never copy secrets or connection strings into docs; name the configuration key instead.

## Procedure

1. **Purpose and audience.** Onboarding, design review, compliance, or giving Claude a map of the system. This sets
   the depth. If the NEXUS tools are available: `get_context_index` (decisions, specs) and `search_knowledge` for
   "architecture" with kinds `decision` and `spec`; `get_constitution` with `["ARC-*", "DOC-*"]`.
2. **Find existing docs.** `ARCHITECTURE.md`, `docs/architecture*`, ADR folders (`docs/adr`, `docs/decisions`),
   README sections, diagram sources. Decide: update in place, or create a new file (agree on the location with
   the person). Note every place where existing docs and code disagree.
3. **Inventory from the code.**
   - Deployables (containers in the C4 sense): from workspace manifests, binary targets, frontend apps,
     Dockerfiles, compose files, deployment manifests and CI deploy jobs.
   - Modules: packages or top-level directories with their responsibility, taken from their main types and
     functions rather than names alone; their public interfaces.
   - Dependencies between modules: from imports, `use` statements and workspace dependencies. Note direction
     violations and cycles.
   - External systems: databases and migration folders, queues, caches, third-party APIs (client code and
     configuration key names), identity providers, file or object storage.
   - Cross-cutting concerns: configuration loading, authentication and authorization, error handling, logging,
     background jobs and scheduling.
4. **Trace key flows.** Pick two to four: the main user journey, a write path, an asynchronous or background path,
   authentication. Trace each through the code with file:line hops; these become sequence diagrams.
5. **Decisions.** List significant decisions visible in the code (framework, persistence model, sync vs async,
   module boundaries, security model, deployment shape) and match them against existing ADRs. For a significant
   undocumented decision, draft an ADR with status `Proposed` and the rationale marked unknown unless sourced. With
   NEXUS, `record_memory` kind `decision` proposes it in the workspace - only with the person's agreement.
6. **Write the document** using the template and diagram conventions in reference.md. Diagrams: `flowchart` with
   subgraphs for context, containers and modules; `sequenceDiagram` for flows; `erDiagram` only for core data. Keep
   each diagram to about fifteen nodes and split larger ones. Node labels use the real names from the code so they
   can be searched.
7. **Validate.**
   - Re-read each claim against its cited file; delete or mark what you cannot support.
   - Check that every cited path exists (`git ls-files <path>`).
   - Check Mermaid syntax by eye: balanced brackets, labels with special characters quoted, one statement per line.
     If a Mermaid renderer or CLI is already available, offer to render the diagrams.
8. **Freshness stamp.** Start the document with "Last verified against commit <short sha> on <date>" and list what
   would make it stale (a new deployable, a new external system, a changed module boundary).
9. **Record.** With NEXUS: `checkpoint` kind `done` naming the documents written, and `update_task` if a task
   drives the work.

## Exit criteria

- Context, deployables, modules, key flows, data, cross-cutting concerns and decisions are covered to the depth
  the audience needs.
- Every statement cites files or is marked inferred; every cited path exists.
- Discrepancies between old docs and code are fixed or listed; undocumented significant decisions have proposed
  ADRs or are listed for the person.

## Report

```markdown
## Architecture docs: <system>
Written or updated: <files>
Verified against: <commit>
Sources examined: <main directories and manifests>
Inferred - needs verification: <list>
Proposed ADRs: <titles>
Discrepancies found in existing docs: <list or "none">
Stale triggers: <what would require an update>
```

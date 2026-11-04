# ADR-0027 — Constitutions: structured rule documents with stable ids, enforcement levels and scoped inheritance

Status: Accepted, implemented (2026-11-04, stage milestone D)

## Context
NEXUS already delivers always-on rule documents (ADR-0017) with per-version approval for synced files (T19). Teams
need more structure: rules they can cite by id, a clear difference between non-negotiable policy and preference,
inheritance from global to task scope, and a way to see the result for a project before Claude gets it. ADR-0026
keeps Constitutions separate from Context, Memory, Skills and Workflows while sharing the knowledge infrastructure.

## Decision
* **Format.** A constitution is a `kind: rule` document with `format: constitution` (optional `prefix`, and `task:` /
  `workflow:` for task scope). Body: `##` article → `###` section (optional) → `####` rule, each rule followed by
  `<!-- rule: id=… enforcement=mandatory|rule|guideline [enabled=false] [overrides=…] -->`. Plain Markdown: readable
  and diffable in any editor and in Git; headings inside code fences stay text. Parsing never fails — oddities are
  problems (errors block saving; warnings do not). `parse(render(s)) == s`.
* **Ids** are stable (`[A-Za-z][A-Za-z0-9._-]*`, ≤ 48) and assigned on save (`<PREFIX>-<n>`); they never change
  when rules are edited, moved or reordered.
* **Scopes** reuse the existing hierarchy — global (this machine) < workspace < stack < project — plus **task /
  workflow** (applies only to sessions for that task). Narrower rules replace broader ones with the same id or via
  `overrides=<id>`, and may disable them (`enabled=false`).
* **Mandatory policies cannot be overridden or disabled** by a narrower scope: the attempt is a `mandatory-override`
  conflict and the policy stays in force. Other conflicts: the same id at the same scope (first wins), `overrides`
  naming an unknown rule, a document with errors (not used until fixed).
* **Approval.** Saving in NEXUS (desktop or `nexus constitution new`) approves that version; a version changed
  elsewhere (editor, `git pull`) is withheld until approved (`knowledge_approve`, same table as rules). A launch
  with withheld documents raises a *review-required* notification (ADR-0025) that opens the document.
* **Delivery** (the Context interface, ADR-0026): from `applies: always` documents, mandatory policies first, then
  rules, one escaped line each, within the always-on budget (at least 600 characters); a mandatory policy that does
  not fit is still named (id + title). Guidelines, on-demand documents and anything over budget are listed by id for
  the read-only MCP tool `get_constitution` (approved versions only, bounded). Rules whose text is already in a
  CLAUDE.md Claude loads are skipped (ADR-0019). The raw markup never reaches Claude.
* **Precedence.** Organisation-managed Claude settings and instructions are outside NEXUS and always win; a
  constitution adds instructions, it cannot loosen managed policy (stated in the UI and the tool output).
* **History.** Git history of the file (`git log --follow`, `git show <commit>:<path>`, read-only, through `git.rs`);
  without a repository the UI says so.
* **Surfaces.** Desktop *Constitutions*: documents by scope with state, rule search, structured tree editor
  (articles/sections/rules, enforcement, enabled, overrides, reorder), Markdown mode, problems, approval, applicable
  projects, history; *Effective rules* for a project/task with conflicts, overrides and a what-if for pending
  versions. CLI: `nexus constitution list|show|effective|check|new`. MCP: `get_constitution`.

## Verification
`constitution::tests` (round trip, fences, validation, ids, scoped overrides, mandatory protection, conflicts,
budgeted delivery, files + approval + Git history + task scope + launch); CLI
`constitutions_reach_launches_by_scope_only_approved_and_through_mcp`; vitest (`constitutions.test.ts`); desktop
`e2e/constitutions.mjs` (13 checks: UI create/edit/save, synced change withheld then approved, conflict preview,
launch delivery, review notification opens the document).

## Alternatives
* A separate constitution store or YAML schema — duplicates the knowledge engine, loses plain-Markdown review.
* Semantic contradiction detection (model-based) — not deterministic; only structural conflicts are reported.
* Letting narrower scopes override everything — would let a synced project file switch off team policy.

## Consequences
* Plain always-on rules keep working unchanged; constitution documents are skipped by the plain path.
* Rule ids are a contract: renaming one is a new rule (and an `unknown-override` where it was referenced).

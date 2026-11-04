# ADR-0022 — Proposed memories: deterministic extraction, human approval before knowledge

Status: Accepted, implemented (2026-11-04)

## Context
Sessions produce useful facts — decisions taken, blockers found, things worth remembering. They should not be lost,
but neither should every session write into the shared knowledge base: Claude can be wrong, prompt-injected or
simply verbose, and knowledge files are authoritative and synced. The owner asked for proposed memory extraction
with review and approval before knowledge becomes authoritative, without per-call AI summarisation.

## Decision
* **Sources** — only what Claude explicitly recorded as a checkpoint through MCP: `decision` → proposed decision;
  `blocker` → proposed note "Known issue: …"; `note` → proposed note. `progress`/`done` checkpoints, file lists,
  command categories and prompts are activity, never proposed. Summaries shorter than 20 characters are ignored;
  at most 10 proposals per session.
* **Extraction** is deterministic SQL in the SessionEnd hook (never fails the hook) and, for sessions that ended
  without hooks, when the desktop opens Proposals or Resume project and in `nexus knowledge proposals list`.
  Titles are the first sentence (≤ 100 chars).
* **Storage** — `memory_proposals` (migration 0008), machine-local SQLite, status `pending | approved | rejected`.
  Proposals are **not knowledge**: they are invisible to Claude's knowledge tools and to other machines.
* **Dedupe** — a normalised content fingerprint is unique per workspace (an approved or rejected proposal is never
  proposed again); a proposal whose title already exists as a document is skipped.
* **Review** — desktop Knowledge → Proposals (edit title/body, choose workspace / project / branch scope, local-only,
  approve or reject), `nexus knowledge proposals approve|reject`. Resume project shows the pending count.
* **Approval** writes the document through the validated knowledge store (secrets and absolute paths are still
  refused; a failed approval leaves the proposal pending): decisions become `accepted`, notes `verified`,
  `source: human` (a person approved it), label `from-session`, and a provenance line naming the session and date.
* Claude has no tool to approve, list or create proposals.

## Alternatives
* Model-based summarisation of whole transcripts — expensive, reads prompt text, non-deterministic; rejected.
* Writing proposals as files under `.nexus-local/` — would make unreviewed content look like knowledge to the
  indexer and to anyone browsing the folder.
* Auto-approving after a delay — removes the human decision the owner asked for.

## Consequences
* Memory quality depends on Claude recording checkpoints; the MCP tool descriptions encourage it.
* Approved knowledge is clearly attributed; rejected noise stays rejected.

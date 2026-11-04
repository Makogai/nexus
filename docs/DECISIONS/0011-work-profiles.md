# ADR-0011 — Work profiles as data, separate from native effort

Status: Accepted (2026-11-04)

## Decision
* A work profile (Prototype, Balanced, Professional, Critical; custom later) is data: id, name, description and
  a short instruction fragment (≤ ~120 tokens) passed with `--append-system-prompt`. Later it may name default
  skills/workflows. It never changes permission modes or security settings.
* Native `--effort` (low | medium | high | xhigh | max, from the installed CLI's help) and `--model` are
  separate, optional launch options. NEXUS does not invent values.
* The selected work profile is frozen per conversation (system-prompt snapshot) and displayed on the session.
* Recommendation is deterministic (project kind, task priority/labels) — no model call.

## Consequences
* Rigor costs a handful of tokens per conversation, not a document injection.

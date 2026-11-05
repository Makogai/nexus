# ADR-0010 — Usage metrics with explicit provenance

Status: Accepted. Source 3 (transcripts, *parsed*) implemented by ADR-0030 (2026-11-05); sources 1–2 not yet.

## Decision
Every number shown carries a provenance: **measured** (documented interface), **parsed** (undocumented local
format read through a versioned compatibility layer, labelled "may be inaccurate"), **estimated** (derived,
e.g. list-price cost) or **unavailable**. Subscription usage is never converted into spend.

Sources per profile, in order of preference:

1. OpenTelemetry to a NEXUS-local OTLP receiver — **personal, opt-in profiles only**. Never for profiles whose
   managed settings configure telemetry: organisation telemetry must not be redirected or duplicated.
2. Status-line JSON (documented) — only when the profile has no custom status line, or through an explicit
   wrapper that still calls the user's own status line.
3. Transcript `usage` fields through a compat layer keyed on Claude version — labelled *parsed*.

## Consequences
* The usage view can legitimately show "unavailable" for organisation-managed profiles. That is correct.

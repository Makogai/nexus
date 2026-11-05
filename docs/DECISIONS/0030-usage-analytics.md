# ADR-0030 — Usage analytics: per-response counts parsed from local transcripts, provenance on every figure

Status: Accepted, implemented (2026-11-05, stage "Usage analytics"; implements ADR-0010's transcript source)

## Context
The owner wants local token usage reporting per session and per project, connected to profiles, projects, tasks,
workflows and context strategies, with measured counts, estimates and unavailable values clearly apart, respecting
organisation-managed telemetry and never modifying it. ADR-0010 already fixes the vocabulary — *measured*
(documented interface), *parsed* (undocumented local format through a versioned compatibility layer, "may be
inaccurate"), *estimated*, *unavailable* — and two rules: organisation telemetry is never redirected or duplicated,
and subscription usage is never converted into spend.

Interfaces available in Claude Code 2.1.311, checked against the real CLI in a synthetic, signed-out profile:

| Interface | Documented | Per session / request | Needs NEXUS to change Claude's config | Org-managed profiles |
| --- | --- | --- | --- | --- |
| OpenTelemetry (`claude_code.token.usage`) | yes | per session, aggregated | yes: enable telemetry + exporter env per session | excluded (ADR-0010) |
| Status-line JSON | yes | session totals only | yes: `statusLine` via `--settings` (replaces the user's) | possible, intrusive |
| Session transcript (`message.usage`, `cost-state`) | no | **per API response** | no — already written by Claude | **works** |

## Decision
* **Source: the transcripts Claude Code already writes** into each profile's config dir —
  `projects/<encoded cwd>/<id>.jsonl` plus `<id>/subagents/*.jsonl` — read-only, incrementally (byte offsets, 8 MiB per
  file and scan, complete lines only), bounded (line size, file count), parsed with a typed allowlist: per
  `assistant` line `message.id`, `model`, timestamp and the `usage` counts (input, output, cache creation, cache read,
  web searches); per `cost-state` line `totalCostUSD`, `startTime`, `hasUnknownModelCost`. Prompts, responses and tool
  data are skipped by the parser and **never stored**. Responses are deduplicated by message id (one line per content
  block); messages Claude generates locally (`<synthetic>`, `isApiErrorMessage`) are counted as local errors, not
  requests. Works for every profile, including organisation-managed ones, without touching any configuration.
* **Provenance per figure** (ADR-0010): token counts are **parsed** — measured by the API, read from an undocumented
  format. The compatibility layer is keyed on the transcript's Claude Code version (`VERIFIED_FORMATS`, now `2.1`);
  other versions are read but flagged "not verified — may be inaccurate". **Estimated**: Claude Code's own cost
  estimate (its cost-state snapshots turned into deltas) — shown **only for profiles billed per API use**, learned from
  `claude auth status` as a classification (`subscription` / `api` / `unknown`, no identity stored); subscription
  sessions are counted as "not converted into spend", unknown billing is withheld. NEXUS's estimate of the context
  it added itself (appended system prompt + SessionStart context, characters ÷ 4). **Unavailable**, with the reason:
  no transcript (deleted, Claude's cleanup, never started), unreadable, not yet written, not attributable (cost per
  model). Nothing is ever shown as zero when it is unknown.
* **Attribution.** A conversation's responses belong to the NEXUS session that joined it most recently before the
  response (`session_conversations`: launch, `--resume` of the same id, and the new id after `/clear`, recorded by the
  SessionStart hook). Earlier responses of a resumed external conversation are "outside NEXUS" and kept apart.
  Associations come only from records NEXUS made itself: session → profile, project, task, work profile; launch record
  → workflow run and context strategy.
* **Telemetry is never changed.** NEXUS sets and forwards no `OTEL_*` / `CLAUDE_CODE_ENABLE_TELEMETRY` (they are
  removed from child environments, ADR-0003); profiles whose managed settings configure telemetry are reported as such.
* **Scanning** is a host role: the desktop scanner thread (every 8 s, a few sessions per tick: live ones, just ended,
  never scanned, still catching up) and `nexus usage` / `nexus usage scan`. Reports are read-only SQL over
  machine-local tables (migration 0014) — usage never leaves the machine and is not synced.
* **Reports**: totals, local-day trend (weekly beyond 120 days), breakdowns by project, profile, task, workflow run,
  context strategy, work profile and model, with efficiency figures (tokens per session, context per request, output
  per request, cache-hit share, NEXUS-added share as an estimate), coverage (measured / no requests / pending /
  unavailable / outside NEXUS) and a sessions list. Desktop *Usage* dashboard (filters in one row, stat tiles, stacked
  trend with legend, tooltip and table view, breakdown, strategy comparison, sessions) and compact summaries on the
  project page, workflow run and task; CLI `nexus usage [--by …] [filters]`, `nexus usage sessions`, `nexus usage scan`.

## Verification
Core (incremental reads with a partial last line, deduplication, subagent files, rewritten files, cost deltas,
nothing but numbers stored, resume and `/clear` attribution, outside-NEXUS responses, missing / no-requests /
pending / unreadable, grouping, filters, efficiency, estimates, billing classification, unverified formats); CLI
E2E with the fake CLI writing transcripts in the real format (billing unknown → API key → subscription; database
holds no text; transcripts unchanged); vitest (helpers, trend chart, dashboard labels); desktop E2E `e2e/usage.mjs`;
real Claude 2.1.311 `e2e/usage-real-claude.mjs` (real transcript located and parsed, "no API requests" with the reason
for a signed-out profile, nothing invented, transcript unchanged).

## Alternatives
* **OpenTelemetry to a NEXUS-local receiver** (ADR-0010's first choice for personal, opt-in profiles) — documented and
  therefore *measured*, but it requires NEXUS to turn telemetry on in each session, is per session rather than per
  response, cannot be used for organisation-managed profiles, and cannot be verified without a signed-in profile.
  Kept as the planned opt-in upgrade; it would show alongside the parsed values with provenance *measured*.
* **Status line** — replaces the user's own status line; session totals only.
* **Computing cost from a NEXUS price table** — prices drift and plans differ; Claude Code's own estimate is used
  for API-billed profiles only.

## Consequences
* A Claude Code update can change the transcript format: values are then flagged as unverified until the
  compatibility list is extended after a check against the real CLI (MANUAL_CHECKS 28).
* Background calls Claude Code makes outside the conversation (e.g. small-model helpers) are not in the per-response
  counts; Claude's cost estimate includes them.
* Usage of sessions started outside NEXUS is not reported per project (no reliable association); responses of such
  conversations later resumed in NEXUS are shown as "outside NEXUS".

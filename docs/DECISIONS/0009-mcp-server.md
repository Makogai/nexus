# ADR-0009 — stdio MCP server in the `nexus` binary, scope bound at launch

Status: Accepted; implemented 2026-11-04 (M12) — see "Implementation notes"

## Decision
* `nexus mcp` is a stdio MCP server (originally planned on the `rmcp` SDK; implemented as a minimal in-house
  protocol subset — see the implementation notes). Claude spawns it from the per-session
  `--mcp-config` that NEXUS generates; it opens the SQLite DB directly, so it works even if the desktop window
  is closed.
* Scope is bound **at launch**: the generated config passes the NEXUS session id (`--session <id>`); the server loads that session
  row and serves only its workspace (and the project's linked repositories). Inputs naming other workspaces are
  rejected. Text supplied by Claude is data, never instructions to NEXUS.
* Tool names carry no `nexus_` prefix (Claude shows `mcp__nexus__<tool>`): `get_project_context`,
  `get_git_status`, `list_tasks`, `get_task`, `update_task`, `search_memory`, `record_memory`,
  `checkpoint_session`, `list_managed_processes`, `get_process_logs`. Output is bounded (byte caps, paging).
* No shell-execution tool. Starting a managed process is limited to user-approved, hash-pinned definitions.
* The desktop notices MCP writes by polling `PRAGMA data_version` (2 s, only while the window is visible).

## Alternatives
* MCP over localhost HTTP served by the desktop — adds an authenticated endpoint and fails when the app is closed.

## Consequences
* The trust boundary is the OS user. MCP scoping protects against a confused or prompt-injected Claude, not
  against other processes of the same user (which could open the DB anyway).

## Implementation notes (2026-11-04)
* **Protocol**: a minimal in-house implementation of the MCP subset NEXUS needs — `initialize` (version negotiation
  over 2025-11-25 / 2025-06-18 / 2025-03-26 / 2024-11-05), `ping`, `tools/list`, `tools/call` — as newline-delimited
  JSON-RPC 2.0 over stdio (`crates/nexus-cli/src/mcp.rs`). Chosen over the `rmcp` SDK to avoid an async runtime and a
  fast-moving macro API for ~250 lines of protocol; revisit when resources/prompts/sampling are needed.
  Interoperability is verified against the **official TypeScript SDK client** (`apps/desktop/e2e/mcp-interop.mjs`, CI).
* **Binding**: the session id is passed as an argument (`nexus --data-dir <dir> mcp --session <id>`) inside the
  per-session config written to `<data dir>/sessions/<id>/mcp.json`, rather than relying on environment inheritance.
  An unknown session yields a server whose every tool returns an error.
* **Tools** (all bounded, workspace-scoped): `get_session_context`, `list_projects`, `get_project`,
  `get_git_status`, `list_recent_sessions`, `checkpoint` (the only write: a ≤ 2000-char note on the bound session),
  `get_checkpoints`. Read-only tools carry `readOnlyHint`. Unknown tools are JSON-RPC `-32602`; tool failures are
  `isError` results.
* **Policy**: `--mcp-config` is omitted (and the reason stored on the session) under `disableSideloadFlags`,
  `allowManagedMcpServersOnly` or `strictPluginOnlyCustomization` (`claude::mcp::blocked_by`).
* **Desktop updates**: the desktop polls `PRAGMA data_version` every 2 s while visible; the baseline is taken before
  the initial data load (a race found by the E2E smoke test and covered by a regression test).

## Implementation notes (2026-11-05, ADR-0040)

`draft_project_document` (topic overview or architecture) lets a session propose a draft of its project's overview or
architecture summary. It writes an unverified, labelled note through `knowledge::service` — still only a knowledge
record — and the person decides in the desktop whether it becomes the document. 22 tools.

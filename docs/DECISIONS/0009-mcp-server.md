# ADR-0009 — stdio MCP server in the `nexus` binary, scope bound at launch

Status: Accepted (implementation Phase 2)

## Decision
* `nexus mcp` is a stdio MCP server (official Rust SDK `rmcp`). Claude spawns it from the per-session
  `--mcp-config` that NEXUS generates; it opens the SQLite DB directly, so it works even if the desktop window
  is closed.
* Scope is bound **at launch**: the generated config passes `NEXUS_SESSION_ID`; the server loads that session
  row and serves only its workspace (and the project's linked repositories). Inputs naming other workspaces are
  rejected. Text supplied by Claude is data, never instructions to NEXUS.
* Tool names carry no `nexus_` prefix (Claude shows `mcp__nexus__<tool>`): `get_project_context`,
  `get_git_status`, `list_tasks`, `get_task`, `update_task`, `search_memory`, `record_memory`,
  `checkpoint_session`, `list_managed_processes`, `get_process_logs`. Output is bounded (byte caps, paging).
* No shell-execution tool. Starting a managed process is limited to user-approved, hash-pinned definitions.
* The desktop notices MCP writes by polling `PRAGMA data_version` (~1 s, only while the window is visible).

## Alternatives
* MCP over localhost HTTP served by the desktop — adds an authenticated endpoint and fails when the app is closed.

## Consequences
* The trust boundary is the OS user. MCP scoping protects against a confused or prompt-injected Claude, not
  against other processes of the same user (which could open the DB anyway).

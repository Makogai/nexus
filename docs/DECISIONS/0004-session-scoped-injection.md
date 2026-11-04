# ADR-0004 — Session-scoped injection instead of mutating Claude configs

Status: Accepted (2026-11-04)

## Context
NEXUS wants to give Claude its MCP tools, lifecycle hooks, rigor instructions and AI Kits. The spec suggests
installing these into profile directories, which contradicts "do not modify existing configurations" and
duplicates Claude's own plugin management.

## Decision
NEXUS composes everything **per session** with documented CLI flags, generating files into
`<data dir>/sessions/<id>/` when needed:

| Concern | Flag |
| --- | --- |
| Rigor / NEXUS context pointers | `--append-system-prompt` |
| NEXUS MCP server (Phase 2) | `--mcp-config <generated mcp.json>` |
| Lifecycle hooks (Phase 2) | `--settings <generated settings.json>` with exec-form hooks (`args`, real `.exe`) |
| AI Kits, NEXUS skills (Phase 4–5) | `--plugin-dir <kit dir>` (repeatable) |

Before launch NEXUS evaluates the profile's effective managed policy:

* `disableSideloadFlags` → omit `--plugin-dir`/`--mcp-config`/`--agents` (Claude would refuse to start).
* `allowManagedHooksOnly` / `disableAllHooks` → omit hook settings.
* `allowManagedMcpServersOnly` → omit the NEXUS MCP server unless the managed allowlist names it.
* `strictPluginOnlyCustomization` → treat skills/hooks/MCP from non-plugin sources as blocked.

The session record stores which surfaces were active; the UI explains omissions. Persistent installation,
when a user wants it, goes through `claude plugin install` (native and policy-aware).

## Alternatives
* Writing into `settings.json` / `.claude.json` — invasive, racy with Claude itself, hard to roll back.
* A NEXUS marketplace as the only kit path — still requires installing into each profile.

## Consequences
* Zero footprint in user configs: uninstalling NEXUS leaves Claude exactly as it was.
* Kits are only active in NEXUS-launched sessions. Intentional, and shown in the UI.
* Resume must re-pass every flag (Claude does not restore them); the launch builder handles both paths.
* Open verification items (Phase 2, against a real profile): whether `--settings` hooks merge with user hooks,
  and whether `allowManagedHooksOnly` also blocks `--plugin-dir` plugin hooks.

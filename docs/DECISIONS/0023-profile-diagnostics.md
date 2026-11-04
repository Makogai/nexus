# ADR-0023 — Claude profile diagnostics: read-only, allowlisted, verified against Claude's own view

Status: Accepted, implemented (2026-11-04, M14)

## Context
Users run several Claude profiles (personal, company, experiments). To keep them apart and to understand a
session's behaviour they need to see what each profile actually contains: executable and version, organisation
policy, settings, memory, skills, agents, commands, MCP servers, hooks, plugins, marketplaces, NEXUS kits — and what
that costs in context. Profile folders hold credentials and secrets; organisation-managed settings must be respected.

## Decision
* `nexus_core::claude::diagnostics::diagnose` inspects files only, read-only:
  * `<config>/settings.json` — values only for allowlisted, non-sensitive keys (`model`, `permissions.defaultMode`,
    `outputStyle`); counts for permission rules; **names** of `env` variables (credential-like names flagged);
    presence of `apiKeyHelper` / cloud credential helpers / status line; `enabledPlugins`,
    `extraKnownMarketplaces`; names of other keys;
  * `.claude.json` (`<config>/.claude.json`, or `~/.claude.json` for the default profile) — only `mcpServers` and
    `projects.*.mcpServers` (local scope shown by project folder name only); everything else (account, API key) is
    never read;
  * `.credentials.json` is never opened;
  * user skills, agents, commands, output styles, hooks; installed plugins from `plugins/installed_plugins.json`
    (v2) and their folders; marketplaces from `plugins/known_marketplaces.json` (URLs without credentials);
    managed `managed-mcp.json` names; NEXUS kits for the profile (ADR-0024).
  * Hooks are reported as event, handler count, types and the **file name** of the program; MCP servers as
    transport plus executable file name or URL host. Arguments, environment values, headers, URL paths and query
    strings never leave `claude::plugin`.
* Names are reported as Claude shows them (`skill`, `plugin:skill`, MCP `plugin:<plugin>:<server>`).
* **Policy**: the existing allowlisted policy summary plus plain-language effects (what disableSideloadFlags,
  strictPluginOnlyCustomization, allowManagedMcpServersOnly, hook restrictions, managed permissions/telemetry/
  CLAUDE.md mean for the profile and for NEXUS).
* **Context estimate**: listing characters / 4 for skills, agents, commands, plus user memory — labelled as an
  estimate that excludes built-ins and project files; a warning above ~3,000 tokens.
* **Warnings**: enabled-but-not-installed plugins, missing plugin files, credential-like env names, default-profile
  instruction bleed (ADR-0020), duplicate plugin/kit names (Claude merges them into one namespace), large listings.
* The executable version comes from `claude --version` with the profile's own environment (read-only). The desktop
  probes it without holding the database lock.
* Surfaces: `nexus profiles diagnose <profile> [--json]`; desktop *Claude profiles → Diagnostics*.

## Verification
`e2e/kits-real-claude.mjs` populates a synthetic profile through Claude's own `claude mcp add` and
`claude plugin marketplace add/install` plus user skills/agents, and requires NEXUS's report to equal the real
Claude's init event (`claude -p --output-format stream-json`): same non-built-in skills, agents, plugins and MCP
server names; no planted secret value in NEXUS's output.

## Alternatives
* `claude plugin list --json` / `claude mcp list` — the former prints MCP arguments (secrets), the latter starts
  servers (code execution, network). Not used for diagnostics.
* A live `claude -p` probe of real profiles — would contact the API for signed-in profiles and write session files;
  used only in tests with synthetic profiles.

## Consequences
* Diagnostics can lag behind Claude when it adds new configuration locations; the real-CLI test detects drift.
* Project-level configuration (`.claude/` in a repository, `.mcp.json`) is not part of a *profile's* diagnostics; the
  effective-instructions inspector covers project memory (ADR-0019).

# Profile isolation, configuration precedence and organisation-managed settings — what NEXUS can and cannot do

NEXUS keeps Claude profiles apart by launching each session with the profile's own configuration folder
(`CLAUDE_CONFIG_DIR`, removed for the default location) and by adding everything else **per session** (flags and a
generated `--settings` file). It never edits a Claude configuration folder it did not create. This page lists the
limits of that model, verified with Claude Code 2.1.311 unless marked *documented*.

## What a profile separates

| Separated by `CLAUDE_CONFIG_DIR` | Not separated |
| --- | --- |
| user settings, user `CLAUDE.md` and rules, skills, agents, commands, hooks, installed plugins and marketplaces, user/local MCP servers (`.claude.json`), transcripts and session history, sign-in (`.credentials.json` on Windows/Linux) | instruction files in **ancestor folders** of the project, including the default profile's `~/.claude/CLAUDE.md` and `~/.claude/rules/` for projects inside the home folder (verified; opt-in fix: *instruction isolation*, ADR-0020) |
| | **project configuration** in the repository: `.claude/settings.json` (hooks, permissions), `.claude/skills`, `.claude/agents`, `.mcp.json`, `CLAUDE.md` — it applies to every profile that opens the project |
| | machine-wide **managed settings** and managed `CLAUDE.md` (apply to all profiles by design) |
| | `%APPDATA%\Anthropic` and keyless Console sign-in (documented; see RESEARCH.md) |
| | environment variables NEXUS passes through (provider credentials are scrubbed unless the profile opts in) |

Sessions started **outside NEXUS** (`claude` in a terminal) get none of NEXUS's per-session additions: no NEXUS
tools, hooks, instruction isolation or AI Kits.

## Configuration precedence (documented, consistent with observations)

1. **Managed** (machine files, registry, server-managed `remote-settings.json`) — always wins; cannot be overridden
   by anything NEXUS passes.
2. **Command line** — NEXUS's per-session flags and `--settings` file (hooks, `claudeMdExcludes`).
3. Local project settings (`.claude/settings.local.json`).
4. Project settings (`.claude/settings.json`).
5. User settings (`<config>/settings.json`).

Arrays such as `permissions` rules and `claudeMdExcludes` **merge** across levels; hooks from all levels are
**additive** (verified for user + `--settings` + plugin). Plugins loaded with `--plugin-dir` add to the profile's
installed plugins; two plugins with the same name share one namespace (verified) — NEXUS refuses to enable two kits
with the same name for one profile and warns about collisions with installed plugins.

## Organisation-managed settings

NEXUS reads only an allowlist of managed keys and never displays their values. What they mean for NEXUS:

| Managed setting | Effect on NEXUS sessions |
| --- | --- |
| `disableSideloadFlags` | no NEXUS MCP tools (`--mcp-config`) and no AI Kits (`--plugin-dir`) |
| `strictPluginOnlyCustomization` | treated as: no NEXUS MCP tools and no AI Kits |
| `allowManagedMcpServersOnly` | no NEXUS MCP tools; kit MCP servers dropped |
| `allowManagedHooksOnly`, `disableAllHooks` | no NEXUS lifecycle hooks; kit hooks dropped; instruction isolation still applies (it is a setting) |
| managed `claudeMd` / managed `CLAUDE.md` | always loaded; cannot be excluded by instruction isolation |
| managed `claudeMdExcludes` | merged with NEXUS's exclusions |

Every blocked integration is recorded on the session with its reason and shown in the desktop; NEXUS never tries to
work around a managed restriction.

## Known gaps

* Organisation-managed files were not tested live (writing them needs administrator rights); behaviour follows the
  documentation and NEXUS only ever *adds* session-scoped values.
* Instruction isolation targets only the default profile's `~/.claude` files. Other ancestor `CLAUDE.md` files (e.g.
  `~/CLAUDE.md`) are shown by the inspector but not excluded automatically.
* A project's own `.claude/` content is trusted by Claude for every profile; NEXUS shows it (inspector) but does not
  filter it per profile.
* Diagnostics read static files; settings Claude computes at run time (feature flags, server-side configuration)
  are not visible.

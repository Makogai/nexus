# ADR-0024 — AI Kits: reviewed plugins enabled per profile through session-scoped `--plugin-dir`

Status: Accepted, implemented (2026-11-04, M21; absorbs the planned M23 kit registry)

## Context
Teams share skills, agents, commands, hooks and MCP servers as Claude Code plugins ("AI Kits"). The owner wants to
register local or Git-backed kits, inspect them, check compatibility and conflicts, and enable them for selected
profiles — with strict work/personal separation, no modification of existing Claude configuration folders, and
without every skill of a large kit landing in every session's context.

## Decision
* **What a kit is**: a Claude Code plugin folder (`.claude-plugin/plugin.json`) or a plain folder of
  `skills/<name>/SKILL.md` (Claude loads both via `--plugin-dir`; verified). A marketplace repository is accepted by
  naming one of its plugins (local `./` sources only).
* **Registration**: local folders are used in place (read-only). Git kits are shallow-cloned (`--depth 1`,
  single branch, no submodules) into `<data>/kits/<id>/repo` with `protocol.ext/fd` disabled, prompts off and only
  `https://`, `ssh://`, `git@host:path` and `file://` URLs accepted; stored URLs have credentials removed.
* **Inspection**: inventory of skills, agents, commands, output styles, hooks (programs by file name), MCP servers
  (transport + executable/host), size, and problems (manifest paths escaping the kit, symlinks — never followed —,
  invalid JSON, unusable names). `claude plugin validate` (Claude's own validator) runs on demand with an empty
  NEXUS-owned config dir.
* **Trust**: a kit's content hash must be **approved by a person**; any change (edit, `git` update) withdraws it until
  reviewed again (as for always-on rules, threat T19/T14).
* **Per-profile enablement** (`kit_profiles`, migration 0009) with a selection: skills (default: all for a kit with
  ≤ 10 skills and ≤ 1,500 listing tokens, **none** for larger kits), agents, commands; **hooks and MCP servers off
  unless enabled explicitly** (they execute programs). Two kits with the same plugin name cannot be enabled for one
  profile (Claude would merge their namespaces — verified).
* **Delivery**: for each session NEXUS builds an overlay `<data>/kits/<id>/overlay/<profile>/<name>/` containing
  only the selected components and a reduced manifest (identity keys; inline hooks/MCP only if enabled; component
  paths normalised); supporting files are copied only when hooks/MCP are enabled. It is passed as `--plugin-dir`.
  Nothing is installed into the profile; `claude` started outside NEXUS does not see kits.
* **Policy**: `disableSideloadFlags` or `strictPluginOnlyCustomization` → kits are not passed (reason recorded);
  `allowManagedMcpServersOnly` / hook restrictions → those parts are dropped. Managed settings always win.
* **Visibility**: `sessions.integrations.kits` records what each session got (skills, token estimate) or why not;
  diagnostics (ADR-0023) list kit content per profile with the context estimate.
* Surfaces: `nexus kits list|add|show|approve|update|enable|disable|validate|remove`; desktop *AI Kits* view
  (register dialog, review/approve, Claude validator, per-profile cards with skill selection and token estimate,
  explicit warnings for hooks/MCP).

## Verification
`e2e/kits-real-claude.mjs` (real CLI, synthetic Work/Personal profiles): the kit enabled for Work with one selected
skill appears in the real Work session's init event as exactly that skill, loaded from NEXUS's overlay; without its
MCP server until opted in (then `plugin:team-kit:teamdb` appears); the Personal session has nothing from the kit;
NEXUS's kit operations leave both config dirs byte-for-byte unchanged; a changed kit is withheld.
`e2e/profiles-kits.mjs` drives the same through the desktop UI.

## Alternatives
* `claude plugin install` into the profile — persistent and supported, but modifies the profile's config folder
  (settings.json, plugins registry, cache) and affects every `claude` run of that profile; excluded by the owner's
  rule. Could be offered later for NEXUS-created profiles with explicit confirmation.
* Copying skills into `<config>/skills` — modifies the profile and loses provenance.
* Passing the original kit folder — loads every skill and, if present, hooks and MCP servers.

## Consequences
* Kits apply to NEXUS-launched sessions only — by design (separation), documented in docs/PROFILE_ISOLATION.md.
* Overlays are rebuilt when the kit or the selection changes (a stamp file prevents needless copies).
* Hooks/MCP from kits run with the user's permissions once enabled; the UI says so where they are enabled.

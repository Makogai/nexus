# ADR-0037 — Workspace templates, the workspace AI Kit and progressive loading

Status: Accepted (2026-11-05, stage "Portable Workspace", milestone P2)

## Context
A new workspace should start with a useful engineering library (owner specification: Minimal, Developer and
AI Engineering Pro templates with constitutions, technology rules, skills, workflows and integration manifests). The
library must not cost tokens just by existing: Claude Code puts every always-on instruction, every loaded skill's
description and every listed rule into each session. Template content must also be upgradable without ever destroying
what the person changed.

## Decision
1. **Built-in, versioned template packages** in `crates/nexus-core/templates/<id>/`, embedded by `build.rs`
   (offline, reviewed with the code). `template.json` names the template, its version, the template it `extends`
   (Pro → Developer → Minimal) and the workspace features it uses. Files keep their workspace paths
   (`constitutions/…`, `skills/<name>/SKILL.md`, `workflows/…`, `integrations/<id>.json`).
2. **Installed through the existing stores, not beside them.** Constitutions and workflows are ordinary knowledge
   documents (fresh UUIDv7 ids and timestamps at install time, validated before writing, indexed, approved on this
   machine because the person installed them — synced copies on other machines wait for approval as before). The
   `skills/` folder becomes the **workspace AI Kit**: a kit row bound to the workspace (`kits.workspace_id`, migration
   0016), plugin name `workspace` (skills are `workspace:<skill>` — the names workflows use), delivered through the
   per-session `--plugin-dir` overlay only to sessions **of that workspace** (never to another workspace's sessions,
   even if a profile enabled it), without hooks or MCP servers, and blocked when its content changed until reviewed.
   Integration manifests are descriptive JSON: NEXUS never installs or runs anything from them; tool names refer to a
   fixed list NEXUS knows (so a synced manifest cannot name a program to execute).
3. **`nexus.lock.json`** records each installed file's template, version and SHA-256. Applying a template (or a newer
   version of it) plans per file: *add*, *update* (installed and unchanged by the person — rewritten with the same
   document id), *unchanged*, *keep yours* (changed by the person — the new version is saved in
   `.nexus-local/template-updates/` to compare), *keep removed* (deleted by the person — not re-added), *skip* (a file
   of the person's own at that path), *retire* (no longer in the template — stays, the lock forgets it). Everything is
   rendered and validated first; a failed write puts files, lock and manifest back. The manifest records
   `template: { id, version }`; template versions are independent of the workspace schema (ADR-0036).
4. **Progressive loading** — what a session pays for:
   * one small `applies: always` constitution (Core working agreement, 6 rules, < 1 000 characters);
   * every other constitution is `applies: on-demand` and listed **by document**, not by rule:
     `Testing (TST-*, 14 rules, 2 mandatory)`; `get_constitution` accepts `"TST-*"` to read a document's rules;
   * stack rule sets (`stacks:`) are part of the listing only for projects whose detected stacks match — detection
     now also recognises `docker` and `sql`;
   * skills cost their name and description only (Claude loads bodies on use); long checklists sit in a skill's
     `reference.md`, read only when needed;
   * workflows and integration manifests are never in the context unless a run or the person uses them.

## Consequences
* A new template version ships with a NEXUS release; `nexus templates apply <id>` (or the desktop) shows the per-file
  plan before anything changes.
* Mandatory rules of an on-demand document are not in the system prompt text; the listing says how many a document has,
  and the Core agreement's mandatory rules (verified claims, no secrets, confirm destructive actions) are always on.
* Editing a workspace skill changes the kit hash: the skills are withheld from sessions until reviewed again — the same
  rule as every AI Kit.

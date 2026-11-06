# Portable Workspace — audit, gap analysis and plan

Status: living document for the stage *Portable Workspace* (owner specification, 2026-11-05). Branch
`milestone/portable-workspace` (off `main` at `7beef88`). Decisions: ADR-0036 (workspace schema v2 and layout),
ADR-0037 (templates and progressive loading), ADR-0038 (development records and checkpoints), ADR-0039 (sync policies)
— each written when its milestone lands.

**Goal (owner):** carry the whole AI development environment and its accumulated knowledge between computers through
one portable Git repository. The desktop app manages; the workspace repository is the portable knowledge base;
project repositories stay independent.

## 1. Audit — what exists today (read-only inspection of `main` at `7beef88`)

| Area | Implementation | Behaviour today |
| --- | --- | --- |
| Workspace | `workspace.rs` | `nexus-workspace.json` (schema 1: `id`, `name`, `description`, `projects[]`, unknown fields preserved), atomic writes, `README.md`, `.gitignore` (ignores `repos/`, databases, `.env*`), `repos/` for clones. Create, import (rebinds a moved folder by id), activate, forget (never deletes files), reconcile (manifest is authoritative for membership; escaping paths never bind). A newer manifest schema is **refused** (cannot be opened at all). |
| Projects | `project/mod.rs`, `project/detect.rs` | Register a folder (inside or outside the workspace; portable entry = id, name, credential-free remote, relative path if inside), locate on another machine, refresh, unregister. Deterministic stack detection (Angular, Vue, React, Next, Nuxt, Svelte, Tauri, Electron, Nest, Express, TypeScript, Node, Rust, PHP/Laravel, Python/Django/FastAPI/Flask, Flutter/Dart, Go, .NET, JVM, Ruby, game engines), package managers from lock files, script hints (never run). No Docker/Dev Container detection, no runtime versions, no environment requirements. |
| Knowledge | `knowledge/*` | Markdown + strict YAML-subset frontmatter (`nexus: 1`, UUID ids, kinds rule/task/decision/note/spec/workflow), scanned folders `rules/ tasks/ knowledge/ workflows/` under three roots (workspace = shared, `.nexus-local/` = local-only, `<data>/global/` = machine), validated (privacy, credentials, machine paths), FTS index in SQLite (rebuildable), optimistic writes, approvals by content hash for always-on rules (T19). Project scope via the `project:` field; stack scope via `stacks:`. Notes carry `verification` (verified / unverified / ai-interpretation). |
| Constitutions | `knowledge/constitution.rs` | `kind: rule` + `format: constitution`; articles → sections → rules with stable ids and enforcement (mandatory / rule / guideline); scopes global < workspace < stack < project < task; mandatory cannot be overridden; compact budgeted delivery of `applies: always` documents; everything else listed **by rule id** for `get_constitution` (ids filter, max 100 rules). |
| Context | `context/*` | Strategies (minimal / balanced / … / custom) decide what `claude::prepare` and SessionStart add; project docs indexed only when allowlisted; inspector predicts and verifies Claude's instruction loading. |
| Memory | `knowledge/proposals.rs`, MCP `record_memory` | Deterministic memory proposals at SessionEnd (human review), notes with verification labels. |
| Skills | `knowledge/skills.rs`, `kits.rs` | Workspace `skills/<name>/SKILL.md` are **catalogued only** (name/description). Skills reach Claude only through AI Kits: a local folder or Git repository, hash-approved, enabled per profile, delivered as a per-profile `--plugin-dir` overlay with only the selected skills (≤ 10 skills / ≤ 1 500 tokens auto-selected). Restore lists kit folders (`.claude-plugin/plugin.json`) that still need registering. |
| Workflows | `workflow/*` | Six built-in templates copied as editable documents; runs on tasks; Claude reports, NEXUS gates verify (machine-local, approved argv commands). |
| Hooks | `activity.rs`, `nexus hook` | Deterministic, cheap: session start/end, compaction, prompts/turns, files edited (project-relative, deduplicated), command *categories* (test, build, git-commit, git-push, dependencies — never the command text), attention. SessionEnd extracts memory proposals. |
| Checkpoints | `checkpoint.rs`, MCP `checkpoint` | Machine-local SQLite rows (progress, decision, blocker, done, note) from Claude, a person or NEXUS; replayed after compaction/resume. |
| Resume | `resume.rs` | Verified Git state + saved history (tasks, notes, checkpoints, sessions) + next action. |
| MCP | `nexus mcp` (24 tools) | Workspace-scoped, bounded, no exec/network/arbitrary files; writes only task/note/decision records. |
| Sync | `workspace_sync/*` (M29) | Status, fetch, ff-only pull with incoming inspection, reviewed commit/push with tokens, pre-commit/pre-push inspection, history, clone + restore checklist (projects to locate, approvals, kit folders, gates, profiles). Never forced, never automatic. |
| Doctor | `doctor.rs`, `nexus doctor` | Machine checks: database, Git, Claude installations, profiles + policy, active workspace, project availability. CLI only. |
| Claude install | `claude/install.rs` | Discovery (PATH + known install dirs), launchability, version; `claude auth status` per profile. No guided installation. |
| Onboarding UI | `OnboardingView.vue`, `SyncView.vue` | Create (empty workspace), import a folder, clone from Git → Sync Center restore checklist. |

## 2. Gap analysis against the specification

| § | Requirement | Exists | Gap → plan |
| --- | --- | --- | --- |
| 2 | Standard layout | `rules/ tasks/ knowledge/ workflows/ skills/ repos/` | `projects/` instead of `repos/` for new workspaces (legacy `repos/` kept); `constitutions/` and `project-context/<project>/` scanned; `integrations/`, `templates/` only when a template fills them; no empty folders → **P1** |
| 3 | Templates Minimal / Developer / AI Engineering Pro | six workflow templates, one starter constitution | Built-in, versioned template packages (constitutions, stack rules, skills, workflows, integration manifests) installed through the existing stores; lock file with component hashes → **P2** |
| 3 | Progressive loading | budgeted delivery, kit skill selection | Delivery lists on-demand **documents** (title + id prefix) instead of up to 30 rule ids; `get_constitution` reads a document by prefix (`TST-*`); stack rules only for matching projects (exists) → **P2** |
| 4 | Workspace schema/versioning | manifest schema 1, newer = refused | Schema 2 fields (workspace schema, created-with, minimum NEXUS version, template id/version, features); newer schema or higher minimum → **read-only mode** instead of refusal; explained migration plan + backup; template versions tracked in the lock file → **P1/P2** |
| 5 | Workspace Doctor | machine doctor (CLI) | Workspace checks + recommendations + reviewable repairs; CLI and desktop → **P3** |
| 6 | Project discovery/restoration | register, locate, restore checklist | Scan `projects/` (and `repos/`) for unregistered repositories, detect missing clones, clone from recorded remotes (selected, reviewed), stable ids, external projects stay supported → **P4** |
| 7 | Project context initialization | stack detection, resume | `project-context/<project>/`: `project.json`, overview, architecture, current state (deterministic, sources cited, AI summaries marked for review) → **P4** |
| 8 | Environment reconstruction | stack + script hints | `environment.json` (runtimes and versions with their source files, package managers, commands by reference, Docker/Compose/Dev Container, services, env-var **names** from examples, platforms); toolchain probe and comparison; setup commands shown, never run → **P5** |
| 9 | Claude Code setup | discovery, auth status | Guided setup: detect, explain official installation methods, install only on explicit consent with argv (no shell strings), never touch credentials → **P5** |
| 10–11 | Development documentation, durable checkpoints | events, checkpoints, proposals | Portable **development records** per meaningful session (verified Git facts and gate results vs observed hook facts vs Claude-reported checkpoints), NEXUS checkpoints at compaction/termination/task and workflow changes, records rebuilt after abrupt termination from what is known → **P6** |
| 12 | Sync policies | manual reviewed sync | Machine-local policy per workspace: manual / automatic with review / trusted automatic (opt-in, allowlisted record paths only, approved remote, managed-policy switch, same inspection, never forced) → **P7** |
| 13 | Development History UI | project page, resume | Per-project history view (records, sessions, tasks, decisions, commits, verification, blockers, next steps, sync state) → **P6** |
| 14 | New-computer restoration | clone + checklist | Guided flow over the existing pieces (validate schema → restore → rebuild index → discover → clone selected → toolchains → approvals → profiles/kits → open project), explicit about what cannot be recovered → **P8** |
| 15 | Tests | — | Core + CLI E2E with temporary workspaces and local bare remotes; desktop E2E per milestone; second-machine reconstruction E2E → every milestone |

## 3. Target layout (schema 2) reconciled with the implemented storage

```text
<workspace>/
├── nexus-workspace.json      manifest (schema 2) — the spec's nexus.yaml (see ADR-0036 on the format)
├── nexus.lock.json           installed template components: version + original hash (only with a template)
├── README.md, .gitignore     .gitignore: projects/, repos/, .nexus-local/, databases, .env*
├── constitutions/            constitution documents (global and stack ones; scope is in the frontmatter)
├── rules/                    plain rules (unchanged)
├── skills/                   workspace skills, delivered as the workspace AI Kit after approval
├── workflows/                workflow documents (unchanged)
├── integrations/             integration manifests (descriptive; never installed automatically)
├── knowledge/                workspace-wide decisions, notes, specs (unchanged)
├── tasks/                    workspace-wide tasks (legacy location of project tasks — still read)
├── project-context/<slug>/   portable project knowledge
│   ├── project.json          id, name, remote, stacks (portable mirror of the manifest entry)
│   ├── environment.json      environment requirements (P5)
│   ├── overview.md, architecture.md, current-state.md
│   └── constitutions/ specs/ tasks/ decisions/ memory/ development/   (created when first used)
├── projects/                 independent project clones — ignored by Git (legacy: repos/)
└── .nexus-local/             local-only knowledge (unchanged)
```

`hooks/` and `templates/` are not created: hook definitions run code (they stay with AI Kits, opt-in per profile,
ADR-0024), and document templates are compiled into NEXUS. Every new workspace starts with `constitutions/`,
`knowledge/`, `project-context/` and `workflows/`, each with a short `README.md` for people (Git cannot carry empty
folders; the knowledge index skips READMEs), plus `projects/`; the folders inside a project's context appear when
content is written to them. A workspace can be set up in a new folder, an empty folder, or an empty Git repository the
person prepared (only host scaffolding such as a README, licence or `.gitignore` is allowed; a folder with other
content, such as a project, never becomes a workspace).

## 4. Migration strategy

* **Schema 1 stays fully supported.** Nothing is moved: `repos/`, top-level `tasks/` and project-scoped documents in
  the top-level folders keep working; new project documents go to `project-context/` once a workspace is on schema 2.
* **1 → 2** is explicit: `nexus workspaces migrate` / the Doctor shows the plan (manifest fields added, `.gitignore`
  lines added, nothing moved or deleted), backs up every file it will change into the NEXUS data folder
  (`backups/workspaces/<id>/<time>/`), writes atomically, and restores the backup if any step fails.
* **Newer workspaces** (schema above 2 or a higher minimum NEXUS version) open **read-only**: everything can be read,
  launched and inspected; writes to workspace files are refused with an explanation.
* **Templates** never overwrite: the lock file remembers each installed file's original hash; an update replaces only
  files the person has not changed and reports the rest.

## 5. Milestones

Status after the stage: P0–P8 implemented and verified, then P9 — a quality review of the template library and of the workspace Git ignore rules (`TEMPLATE_REVIEW.md`) — before the merge (ROADMAP, CURRENT_STATE).

| # | Milestone | Verification |
| --- | --- | --- |
| P0 | Audit, gap analysis, ADR-0036 | this document |
| P1 | Workspace schema 2: manifest fields, layout for new workspaces, compatibility and read-only mode, explained migration with backup and rollback, `nexus workspaces info/migrate` | core + CLI E2E |
| P2 | Templates (Minimal, Developer, AI Engineering Pro) with lock file and safe updates; progressive constitution delivery; workspace skills kit | core + CLI E2E + content validation |
| P3 | Workspace Doctor (core, CLI, desktop) with reviewable repairs | core + CLI E2E + desktop E2E |
| P4 | Project discovery and restoration; project context initialization | core + CLI E2E + desktop E2E |
| P5 | Environment manifest, toolchain check, Claude Code setup | core + CLI E2E + desktop E2E |
| P6 | Development records, durable checkpoints, Development History view | core + real hooks E2E + desktop E2E |
| P7 | Sync policies (manual, review, trusted) on M29 | core with local bare remotes + desktop E2E |
| P8 | New-computer restoration flow; second-machine reconstruction | desktop E2E across two data folders |

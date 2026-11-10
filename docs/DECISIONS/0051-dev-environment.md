# ADR-0051 — Dev Environment: one catalog of development tools, checked on this computer

Status: Accepted (2026-11-10, owner request "include useful things like devtools, dev environments … seeing all dev
tools you can install, like node, git, php"; branch `milestone/agents-dev-environment`). Extends ADR-0036/P5
(environment requirements) and keeps its safety rules.

## Context

NEXUS already knows development tools by id (`toolchain::TOOLS`, 23 entries): the Workspace Doctor, a project's
environment check (P5) and the new-computer restore (P8) look them up on PATH, ask them for their version and compare it
with what the project's own files ask for. There is no place that shows *this computer* as a whole — which tools are
there, in which version, where, which projects need them and whether the version fits — and the catalog stops at the
stacks NEXUS detected first (no Java, .NET, Ruby, Deno, version managers, infrastructure or database clients, and only
two AI agents).

## Decision

1. **The catalog stays compiled into NEXUS** (`toolchain::TOOLS`), now with a **category** for each tool (runtime,
   package manager, version manager, build, containers & infrastructure, cloud, database, source control, AI agent,
   NEXUS integration) and more tools: JDK, Maven, Gradle, .NET SDK, Ruby, Bundler, Deno, fnm, Volta, nvm for Windows,
   pyenv, rustup, mise, make, CMake, kubectl, Helm, Terraform, Azure CLI, AWS CLI, Google Cloud CLI, psql, MySQL client,
   SQLite, redis-cli, Gemini CLI, GitHub Copilot CLI, OpenCode. Workspace files still name tools only by id; program
   names, version arguments and package ids never come from a workspace file (threat T22).
2. **Install commands are official ones, verified when added**: WinGet ids from `winget show --id <id> -e` (publisher
   checked), Homebrew names from formulae.brew.sh (formula or **cask**, which now has its own form `brew install --cask`).
   A tool without an official package keeps only its install page (e.g. make on Windows, psql — the PostgreSQL packages
   install a server). Linux gets the page, no command (distributions differ).
3. **Platform-specific tools** (nvm for Windows) are listed only where they exist; tools that are shell functions
   elsewhere (nvm, SDKMAN!) are not in the catalog — NEXUS cannot see them and would report them missing.
4. **Inventory** (`toolchain::inventory`): every tool for this platform, probed with its own version command (argv, 20 s
   timeout) on a few threads at once; every copy on PATH is listed (`which_all`), so a second Node.js or Python shadowing
   the first is visible. Nothing is cached across restarts; the desktop keeps the last result in memory and probes again
   on request (one tool or all).
5. **Project needs**: for each project of the active workspace that has an environment (detected from its files, or the
   workspace record when the clone is elsewhere — P5), the inventory lists which tools it needs, with the constraint, its
   source file and whether the installed version fits (`ok`, `mismatch`, `missing`, `unknown`).
6. **Version-manager hints**: when a project asks for an exact version or a version prefix (`20`, `20.11.1`, `3.12`) and
   a version manager for that runtime is installed, NEXUS offers that manager's own install command (`fnm install 20`,
   `volta install node@20`, `nvm install 20`, `pyenv install 3.12`, `uv python install 3.12`, `rustup toolchain install
   1.85`, `mise install node@20`). The version comes from a project file, so it is accepted only when it is digits and
   dots (at most 20 characters); ranges (`>=18`, `^8.2`) and channels (`stable`, `lts/*`) get no hint. Commands that
   would change the project's files (`mise use`, `volta pin`) are never offered.
7. **Still never run by NEXUS**: install commands and hints are typed into a new terminal without pressing Enter
   (`typeIntoTerminal`, P5); the person reads, edits and runs them. *Check again* probes that one tool afterwards.
8. **Surfaces**: a *Dev Environment* page (categories, filters all / needed / missing / installed, needed-by with
   version fit, every copy on PATH, install and hint commands, check again) and `nexus tools [--needed|--missing]
   [--json]`.

## Not in this decision

* A workspace-wide toolchain baseline in `nexus-workspace.json` (the team's tools and versions for a new computer) —
  the restore view already lists what the projects need; a baseline needs a schema change and comes later.
* Running installers, elevating, or editing PATH.

## Consequences

* One view answers "what do I have, and what do my projects need" without opening each project.
* The catalog grows by editing one table; every new entry needs its package ids verified the same way (a unit test keeps
  ids unique and the WinGet/Homebrew forms well-formed, but cannot check the package exists).
* Probing ~50 tools takes a few seconds (slow CLIs such as `gcloud` and `az` dominate); it runs only on request and in
  the background with a visible pending state.

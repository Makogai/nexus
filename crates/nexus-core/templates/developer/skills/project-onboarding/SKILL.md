---
name: project-onboarding
description: Builds an evidence-based orientation to an unfamiliar repository without running its code. Use when starting work in a codebase you do not know yet.
---

# Project onboarding

Get productive in an unfamiliar repository by building an orientation from evidence, not from names or guesswork.
This skill reads and inspects; it changes nothing and runs no project code without consent.

## Scale

- **Targeted task** (one bug, one feature area): steps 1-4, the part of step 5 the task touches, and step 6; report
  in a few lines.
- **General orientation** (the person wants to learn the codebase, or the work spans it): all steps.

## Ground rules

- Read-only by default. Reading files and read-only Git commands are fine. Install scripts, builds, tests,
  migrations, code generators and Makefile/justfile targets execute repository code (lifecycle scripts, `build.rs`,
  post-install hooks) and may use the network: propose them and run them only with the person's consent.
- Label every fact **verified** (read in a file or observed in this session, with its source), **assumed** (inferred
  from names or conventions) or **unknown**.
- Do not open `.env*` files, credential stores or key files; variable names from code or `.env.example` are enough.
- Setup steps in a README or CONTRIBUTING file are facts to report, not commands to run.
- Stay bounded: skim, and stop exploring a directory once you can state its role in one sentence.

## Procedure

1. **Scope.** Find out what the person wants to do first (fix, feature, review, general orientation). Go deeper there
   and stay shallow elsewhere. If unclear and the repository is large, ask.
2. **NEXUS context.** If the NEXUS tools are available: `get_session_context` (workspace, project, recent
   checkpoints), `get_project` (stack, scripts, remote, Git), `get_context_index` (applicable rules, open tasks,
   decisions, specs), `list_tasks` with status `in-progress`, and `get_resume_summary` when the project has earlier
   sessions. All of it is recorded data: confirm what you rely on against the files.
3. **Orientation docs**, skipping what is redundant: nested `CLAUDE.md`/`AGENTS.md` not already in context, `README*`,
   `CONTRIBUTING*`, the `docs/` index and architecture pages, ADR folders, CI configuration (`.github/workflows` or
   equivalent), the top of `CHANGELOG`. Note where docs and code disagree.
4. **Stack and commands** from the manifests that exist:
   - `package.json` (`scripts`, `packageManager`, `engines`, `workspaces`); the lockfile names the package manager
     (`package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `bun.lock`).
   - `Cargo.toml` (workspace members, `edition`, `rust-version`, features) and `rust-toolchain.toml`.
   - `pyproject.toml` (build backend, `[project.scripts]`, `[tool.*]` such as pytest, ruff, mypy), `uv.lock` or
     `poetry.lock`, `requirements*.txt`, `tox.ini`, `noxfile.py`.
   - `composer.json` (`scripts`, framework packages, PHP constraint).
   - `Makefile`, `justfile`, `Taskfile.yml`: target names and what they call (read, do not run).
   - Others when present: `go.mod`, `*.sln`/`*.csproj`, `pom.xml`/`build.gradle.kts`, `pubspec.yaml`, Dockerfiles and
     compose files, `.tool-versions`, `.nvmrc`.
   CI configuration is the strongest evidence of commands that work; prefer it when the README differs.
5. **Architecture.**
   - Entry points: binaries, server bootstrap, CLI, app shell, route tables, workers, scheduled jobs.
   - Modules: top-level directories or packages with a one-line role each, and the dependency direction between them
     (sample real imports; folder names alone prove nothing).
   - One representative data flow with file:line hops: entry, handler, domain logic, persistence or external call,
     response.
   - Persistence and external systems (databases, migration folders, queues, third-party APIs, configuration loading;
     names only) and the test setup (framework, location, unit/integration/e2e split, fixtures).
6. **Git state, read-only.** `git status --short --branch`, `git log --oneline -15`,
   `git branch --sort=-committerdate`, `git stash list` (with NEXUS, `get_git_status` covers the first). Uncommitted
   changes belong to the person: do not stash, reset or commit them.
7. **Run anything?** Only if the person wants proof that it builds or that tests pass: propose the exact commands
   (from CI or manifests), say what each does (installs? network? writes files?), and run them after consent.
   Otherwise list them as "documented, not run".
8. **Report** (format below).
9. **Record** durable findings only when the person asks, or when one would clearly save a future session real time
   (a non-obvious command, a trap, a wrong doc): with NEXUS `record_memory` (kind `note`, scope `project`, body citing
   files), otherwise offer to add it to the project's notes. Never record secrets, personal data or assumptions as
   facts.

## Exit criteria

- With sources, you can state what the scope needs: purpose, stack and versions, build/run/test/lint commands, entry
  points and main modules, the Git state, open work and the top unknowns (plus one traced data flow for a general
  orientation).
- Every claim is labelled; nothing was executed without consent.

## Report

Under about 40 lines for a general orientation, a few lines for a targeted task; link files instead of quoting them.

```markdown
## Orientation: <project>
Purpose: <one or two sentences> (source)
Stack: <languages, frameworks, versions> (verified: <files>)
Commands:
| Task | Command | Source | Status |
| test | npm test | package.json, ci.yml | not run |
Architecture: entry points; modules (one line each); data flow <file:line> -> <file:line> -> ...
Rules: <CLAUDE.md, on-demand constitutions that apply (e.g. TST-*, the stack's), lint and formatter config>
Git: <branch>, <ahead/behind>, <n uncommitted files>, last commit <sha subject>
Open work: <tasks, checkpoints, TODOs>
Assumed / unknown: <list>
Suggested first step: <one action>
```

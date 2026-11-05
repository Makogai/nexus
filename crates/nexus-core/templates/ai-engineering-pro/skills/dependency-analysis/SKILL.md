---
name: dependency-analysis
description: Assesses dependencies - inventory, outdated and vulnerable packages, licences, health, duplicates, unused - and builds a prioritised, stepwise upgrade plan. Use when auditing or planning upgrades.
---

# Dependency analysis

Give the person an evidence-based picture of the project's dependencies and a prioritised upgrade plan made of
small, verifiable steps.

## Ground rules

- Read-only by default. Lockfiles and manifests are read directly. Audit and outdated commands contact registries
  or advisory databases (network), and some tools must be installed first: suggest the exact command, run it only
  with the person's consent. Never run install, update or upgrade commands without consent, and never use
  `npm audit fix --force` or similar bulk fixes.
- The lockfile is the truth for resolved versions; the manifest records declared intent.
- Advisory ids, versions, dates and licences come from files or tool output seen in this session. Anything from
  memory is labelled "unverified - check the advisory or registry".
- Licence findings are engineering signals, not legal advice; recommend legal review for unclear cases.

## Procedure

1. **Detect ecosystems.** Manifests and lockfiles: `package.json` with `package-lock.json`, `pnpm-lock.yaml`,
   `yarn.lock` or `bun.lock`; `Cargo.toml`/`Cargo.lock`; `pyproject.toml` with `uv.lock`, `poetry.lock` or
   `requirements*.txt`; `composer.json`/`composer.lock`; `go.mod`/`go.sum`; `pubspec.yaml`/`pubspec.lock`;
   `*.csproj`; Gradle or Maven files; `Gemfile.lock`; Dockerfile base images; CI actions (`uses: owner/action@ref`).
   Note monorepo workspaces. An application without a committed lockfile is a finding.
2. **Inventory.** Direct dependencies from manifests, split into runtime and development/build. Transitive count
   and resolved versions from the lockfile, or from the local tree commands in reference.md. For each direct
   dependency note what uses it (search imports). Use NEXUS `get_project` for the detected stack when available.
3. **Outdated.** Suggest the ecosystem's outdated command (reference.md). Classify each direct dependency as
   current, patch, minor or major behind, and note very old pins.
4. **Vulnerabilities.** Suggest the audit command (reference.md); with consent, run it. For each advisory: id,
   package and version, severity, dependency path (direct or transitive, runtime or dev), fixed version, and
   whether the vulnerable code is reachable in this project (search for use of the affected API, consider the
   deployment). A dev-only or unreachable advisory is still listed, at lower priority.
5. **Licences.** Compare dependency licences with the project's own licence and distribution model. Flag:
   strong copyleft (GPL, AGPL) in proprietary or distributed software, AGPL in network services, LGPL with static
   linking, missing, unknown or custom licences, and licence changes between the current and target versions.
   Use a licence tool the project already has, or reference.md suggestions with consent.
6. **Maintenance health** of runtime dependencies, starting with the critical ones: last release, release cadence,
   deprecation or archive notices, yanked versions, unanswered security issues, single maintainer. This needs the
   network (registry or repository pages): with consent, otherwise mark "unknown".
7. **Duplicates and unused.** Duplicates: several versions of the same package in the tree (reference.md
   commands). Unused: direct dependencies without imports; confirm each candidate by hand, because dynamic
   imports, plugins, CLI tools used in scripts, type packages and peer dependencies cause false positives. Label
   results **verified unused** or **candidate**.
8. **Upgrade risk** per candidate: semver jump; changelog, release notes and migration guide (cite the source, or
   say "not read"); breaking changes that touch APIs this project uses (search for them); peer dependency and
   runtime/toolchain requirements (Node, Rust MSRV, PHP, Python versions); effect on other packages; test
   coverage of the code that uses it. Rate low, medium or high.
9. **Prioritised plan.** Order:
   1. exploitable vulnerabilities in runtime dependencies;
   2. other vulnerabilities with a fixed version available;
   3. deprecated, archived or yanked dependencies (replace or pin with a reason);
   4. major upgrades that block others or that end-of-life runtimes force;
   5. routine patch and minor batches per ecosystem;
   6. removal of verified unused dependencies and duplicate cleanup.
   Each step: one package or a tightly coupled group (a framework and its plugins), the exact command, the expected
   lockfile change, verification (tests, typecheck, build, plus a targeted manual check for risky areas) and
   rollback (revert the commit). Keep majors separate from batches of minors.
10. **Record.** If the NEXUS tools are available: `checkpoint` kind `decision` with the agreed plan summary;
    `create_task` per upgrade step only when the person agrees. Do not record advisory details for unfixed
    exploitable issues in synced files; use `record_memory` with `localOnly: true` if a record is needed.

## Exit criteria

- Every ecosystem in the repository is inventoried with direct and transitive counts.
- Vulnerability, outdated, licence and health results are either from commands run in this session (named) or
  marked as suggested and not run.
- Every planned step has a command, verification and rollback; nothing was installed or upgraded without consent.

## Report

```markdown
## Dependency analysis: <project>
Ecosystems: <list with lockfiles> | Direct: <runtime n, dev n> | Transitive: <n>
Commands run (with consent): <list> | Suggested, not run: <list>
### Vulnerabilities
| Advisory | Package@version | Severity | Path | Fixed in | Reachable | Action |
### Outdated (direct)
<majors, minors, patches - counts and notable ones>
### Licences
<issues or "no conflicts found among <n> scanned">
### Health
<deprecated, unmaintained, unknown>
### Duplicates and unused
<verified unused, candidates, duplicate versions>
### Upgrade plan
1. <package> <from> -> <to> - risk <low|medium|high> - verify: <checks> - rollback: revert commit
Unknowns: <list>
```

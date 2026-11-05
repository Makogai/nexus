# Dependency analysis reference

Commands per ecosystem. "Network" means the command contacts a registry or advisory database; "install" means
the tool is not part of the default toolchain. Both need the person's consent before running. Commands marked
"modifies" change manifests or lockfiles and belong to the upgrade plan, not to the analysis.

## JavaScript and TypeScript

| Purpose | npm | pnpm | Yarn |
| --- | --- | --- | --- |
| Resolved tree (local) | `npm ls --all` | `pnpm list --depth Infinity` | Yarn 1: `yarn list`; Yarn 2+: read `yarn.lock` |
| Why is X installed | `npm explain <pkg>` | `pnpm why <pkg>` | `yarn why <pkg>` |
| Outdated (network) | `npm outdated` | `pnpm outdated` | Yarn 1: `yarn outdated` |
| Advisories (network) | `npm audit --omit=dev`, then `npm audit` | `pnpm audit --prod` | Yarn 1: `yarn audit`; Yarn 2+: `yarn npm audit` |
| Licences | `license` fields in `package-lock.json` | `pnpm licenses list` | package metadata |
| Upgrade (modifies) | `npm install <pkg>@<version>` | `pnpm add <pkg>@<version>` | Yarn 2+: `yarn up <pkg>@<version>` |

- Duplicates: `npm find-dupes` (dry run of `npm dedupe`) or `npm ls <pkg>` for one package.
- Unused dependencies and exports: `npx knip` (install). Check candidates by hand: config-file plugins, CLI tools
  called from `scripts`, `@types/*` packages and peer dependencies are common false positives.
- Run `npm audit` without `--omit=dev` as well, but rank dev-only advisories lower unless they affect the build
  pipeline or developer machines (install scripts, dev servers exposed on a network).
- Never `npm audit fix --force`: it applies major upgrades across the tree in one unreviewable step.

## Rust

| Purpose | Command |
| --- | --- |
| Resolved tree (local) | `cargo tree` (add `--offline` to avoid index updates) |
| Duplicate versions | `cargo tree --duplicates` |
| Why is X included | `cargo tree --invert <crate>` |
| Outdated (install, network) | `cargo outdated` (cargo-outdated) |
| Advisories (install, network) | `cargo audit` (cargo-audit, RustSec database) |
| Advisories, licences, bans, sources (install) | `cargo deny check` (cargo-deny; licence policy lives in `deny.toml`) |
| Unused (install) | `cargo machete`; `cargo udeps` needs a nightly toolchain |
| Upgrade within semver range (modifies) | `cargo update -p <crate>` or `cargo update -p <crate> --precise <version>` |
| Major upgrade (modifies) | edit the version in `Cargo.toml`, then build and test |

Check the crate's `rust-version` against the project's MSRV before planning an upgrade.

## Python

| Purpose | uv | Poetry | pip / requirements |
| --- | --- | --- | --- |
| Resolved tree (local) | `uv tree` | `poetry show --tree` | `pipdeptree` (install) |
| Outdated (network) | `uv tree --outdated` in recent uv versions (check `uv tree --help`) | `poetry show --outdated` | `pip list --outdated` |
| Advisories (install, network) | `uv export --format requirements-txt` to a temporary file, then `pip-audit -r <file>` | `pip-audit` inside the project environment | `pip-audit -r requirements.txt` |
| Licences (install) | `pip-licenses` inside the environment | same | same |
| Unused (install) | `deptry .` | same | same |
| Upgrade (modifies) | `uv lock --upgrade-package <pkg>` | `poetry update <pkg>` | edit the pin, reinstall |

## PHP (Composer)

| Purpose | Command |
| --- | --- |
| Resolved tree (local) | `composer show --tree` |
| Why is X installed | `composer why <vendor/package>` |
| Outdated (network) | `composer outdated --direct` |
| Advisories (network) | `composer audit` |
| Licences (local) | `composer licenses` |
| Unused (install) | `composer-unused` |
| Upgrade (modifies) | `composer update <vendor/package> --with-dependencies` |

## Go

| Purpose | Command |
| --- | --- |
| Module graph (local) | `go mod graph` |
| Why is X required | `go mod why -m <module>` |
| Outdated (network) | `go list -m -u all` |
| Advisories (install, network) | `govulncheck ./...` (reports only vulnerabilities in code that is actually called) |
| Unused requirements | `go mod tidy -diff` (Go 1.23+, shows changes without writing) |
| Upgrade (modifies) | `go get <module>@<version>`, then `go mod tidy` |

## Other ecosystems

- Dart and Flutter: `dart pub deps`, `dart pub outdated` (or `flutter pub outdated`); upgrades with
  `dart pub upgrade <package>`; majors with `dart pub upgrade --major-versions` only as a reviewed step.
- .NET: `dotnet list package --include-transitive`, `dotnet list package --outdated`,
  `dotnet list package --vulnerable --include-transitive`.
- JVM: `./gradlew dependencies` or `mvn dependency:tree`; `mvn versions:display-dependency-updates` for updates.
- Ruby: `bundle outdated`; advisories with bundler-audit (install).
- Many lockfile formats at once: `osv-scanner` (install, network). Version 2 uses `osv-scanner scan source -r .`;
  check `osv-scanner --help` for the installed version.
- Container base images: pin by digest for reproducibility; scan images with the scanner the team already uses.
- CI actions: pin third-party actions to a full commit SHA; check whether Dependabot or Renovate is configured.

## Licence signals

| Group | Examples | Typical concern |
| --- | --- | --- |
| Permissive | MIT, BSD-2-Clause, BSD-3-Clause, Apache-2.0, ISC, Zlib | Attribution and notice files; Apache-2.0 patent clause |
| Weak copyleft | LGPL, MPL-2.0, EPL-2.0 | Changes to the library itself must be shared; LGPL and static linking need care |
| Strong copyleft | GPL-2.0, GPL-3.0 | Distributing a combined work may require releasing it under the GPL |
| Network copyleft | AGPL-3.0 | Users of a network service may be entitled to the source |
| Source-available | BUSL, SSPL, Elastic License, Commons Clause | Usage restrictions; not open source |
| None or unknown | missing licence field, custom text | No rights granted by default; needs clarification |

Distribution model matters: a dependency used only in a build tool or test suite has different obligations from
one shipped in a binary, bundle or container image. Typical flags: strong copyleft in proprietary or distributed
software, AGPL in a network service, LGPL with static linking, missing or custom licences, and a licence change
between the current and the target version. Flag; do not conclude.

## Upgrade risk checklist

- Semver jump and whether the package follows semver (0.x versions may break on minor bumps).
- Release notes, changelog and migration guide read for every version between current and target.
- Breaking changes cross-checked against this project's usage (search for the affected functions and options).
- Deprecations introduced that will break at the next major.
- Peer dependency ranges, plugins and type packages that must move together.
- Runtime and toolchain requirements: Node, Python, PHP, Go or Rust (MSRV) versions; browser targets.
- Native or build-time components (compilers, system libraries, post-install downloads).
- Behaviour changes without API changes: defaults, serialization, timezone or locale handling, security hardening
  that rejects previously accepted input.
- Test coverage of the code that uses the dependency; add a characterisation test before a risky upgrade.

## Plan step template

```markdown
### Step <n>: <package> <from> -> <to> (<reason: advisory id | deprecated | routine>)
Risk: <low | medium | high> - <one line why>
Command: `<exact upgrade command>`
Expected change: <manifest and lockfile entries; transitive packages that move>
Code changes: <none | list from the migration guide>
Verify: <lint, typecheck, tests, build; manual check of <area>>
Rollback: revert the commit
```

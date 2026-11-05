---
nexus: 1
id: {{id}}
kind: workflow
format: workflow
title: Release
status: active
category: release
rigor: professional
skills: [workspace:dependency-analysis]
created: {{now}}
updated: {{now}}
source: import
---

Use this workflow to prepare a versioned release of an application or library. It freezes the scope, checks dependencies, verifies the build, prepares changelog, version and release notes, and stops for a person's go/no-go. NEXUS never tags, publishes or deploys; the person does that after approval. The last step confirms the release works and that a rollback is ready.

## Freeze scope
<!-- step: id=scope kind=claude -->

Identify the release: the version to be released, the previous release tag and the commit or branch to release. List every change since the previous tag (merged pull requests, commits) and classify it as feature, fix, breaking, security or internal; confirm that no unfinished work is included by accident. Propose the version according to the project's scheme (with SemVer: breaking means major, feature means minor, fix means patch). Record the frozen commit and the classification as a checkpoint (decision); from here on only fixes for this release go in.

### Acceptance

- The release commit and the previous release tag are recorded as a checkpoint
- Every change since the previous release is classified
- The proposed version follows the project's versioning scheme and breaking changes are identified

## Dependency and vulnerability check
<!-- step: id=deps kind=claude needs=scope -->

Use the dependency-analysis skill. Compare the lockfiles with the previous release and check every added or upgraded dependency for licence compatibility and maintenance state. With the person's consent (the tools use the network), run the audit tool of each ecosystem the project uses (for example `npm audit`, `pip-audit`, `cargo audit`, `composer audit`, `govulncheck`); never apply bulk fixes such as `npm audit fix --force`. Record findings by severity, together with whether the vulnerable code is reachable from this project. Known exploitable vulnerabilities block the release unless the person accepts the risk at the approval step.

### Acceptance

- Dependency changes since the previous release are listed with their licences
- Audit results are recorded with severity and reachability, naming the tool used
- Each high or critical finding is fixed or listed for the person's decision

## Full verification
<!-- step: id=verify kind=check needs=deps gates=build,lint,typecheck,test -->

NEXUS runs the project's build, lint, typecheck and test gates on the frozen commit plus any dependency fixes.

## Changelog and version bump
<!-- step: id=version kind=claude needs=verify gates=build -->

Update the changelog from the classified change list in the project's format: user-visible changes only, breaking changes first with migration steps, security fixes with their advisory identifiers. Bump the version everywhere the project declares it (manifests, application metadata, documentation that quotes it) as one change, updating lockfiles through the package manager rather than by hand. Commit only when the person asks, and do not create a tag. NEXUS runs the build gate again to confirm the bump.

### Acceptance

- The changelog lists every user-visible change since the previous release, breaking changes with migration steps
- Version numbers agree in every place the project declares them
- No tag was created

## Release notes
<!-- step: id=notes kind=claude needs=version -->

Write release notes for the people who install or operate the release: breaking changes and required actions first, then highlights, upgrade steps (including data migrations, their expected duration and whether they can be reversed), deprecations with their removal timeline, security fixes and known issues. Link to the changelog. Leave out internal details and customer names.

### Acceptance

- Breaking changes and required actions come first
- Upgrade steps state whether data migrations can be reversed
- Known issues are listed or stated as none

## Approval before tagging
<!-- step: id=approve kind=review needs=deps,verify,version,notes -->

A person reviews the verification results, the dependency and audit findings, the changelog and the release notes, and decides go or no-go. After approval the person tags, publishes and deploys outside NEXUS. Request changes with a note to send the dependency check, version bump and notes back to Claude.

## Post-release verification and rollback readiness
<!-- step: id=post kind=claude needs=approve -->

First write the rollback procedure, before it is needed: the previous version identifier, how to redeploy or republish it, whether the data migrations can be reversed or need a fix-forward, and who decides. Then report needs-input until the person confirms that the release is live. Once it is, run read-only smoke checks of the critical journeys that the person approves against the released version (never with personal credentials, never changing production data), confirm that the published artifacts carry the new version, and compare error rates and key metrics with the previous release where the person gives you access to them. If any check fails, report blocked with the evidence and a recommendation: roll back or fix forward.

### Acceptance

- The rollback procedure names the previous version, the steps and the data-migration constraints
- Smoke checks of the critical journeys ran against the released version and their results are recorded
- Any regression is reported as blocked with a rollback or fix-forward recommendation

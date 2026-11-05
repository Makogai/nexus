---
name: feature-development
description: Implements a feature with acceptance criteria, a plan scaled to its risk, test-first steps and verification. Use when asked to build or extend functionality.
---

# Feature development

Deliver a feature that meets explicit acceptance criteria, is covered by tests and passes the project's checks.

## Scale

Decide the size first and say which path you take:
- **Small** (one module; no public API, schema, security or configuration impact): state the criteria in a sentence
  or two, skip the written plan and the checkpoints, implement test-first, and run the touched area's tests plus the
  project's lint and typecheck.
- **Larger or risky** (several modules, public API, stored data, authentication or authorisation, money, more than
  about ten files): the full procedure.
Switch to the larger path as soon as the work shows hidden scope.

## Ground rules

- Leave unrelated uncommitted changes alone; they may be the person's work in progress.

## Procedure

1. **Load the work item.** If the NEXUS tools are available: `get_task` (then `update_task` with status
   `in-progress`), and `get_workflow_run` when the task has a run. A run's ready step takes precedence over the order
   below; report each step with `report_step` (a `done` report is a claim: NEXUS verifies gated steps itself).
   Without NEXUS, work from the request.
2. **Acceptance criteria** as observable, testable statements (Given/When/Then or bullets), covering the error cases,
   permissions, boundaries and compatibility that apply.
   - Ambiguous, and the choice changes behaviour, data, public API or UX: ask with two or three concrete options and
     a recommendation; do not implement the ambiguous part yet.
   - Low-stakes and easily reversible: decide, and list it as an assumption.
3. **Rules.** Read the on-demand constitutions for what you touch: with NEXUS, `get_constitution` with ids such as
   `["ENG-*", "CQ-*", "TST-*"]`, the stack documents listed for this project, `["SEC-*"]` for authentication, input
   handling, secrets or external calls, and `["ARC-*"]` for new modules or boundaries. An empty result means the
   workspace has no such document. If the feature cannot be built without breaking a mandatory rule, stop and ask.
   Without NEXUS: CLAUDE.md, CONTRIBUTING, linter and formatter configuration.
4. **Study the area.** Find the closest existing feature and follow its patterns. Identify touched modules, public
   interfaces, schema or data changes, and callers (`git grep`).
5. **Plan** (larger path). Three to eight steps, each a vertical slice that keeps the build green: files, the test
   that proves it, the main risk; plus migrations, compatibility, configuration and rollout (a feature flag if the
   project uses them).
   - Present the plan and wait for confirmation when it touches a public API, a schema, security-sensitive code or
     more than about ten files (in plan mode, this is the plan you present). Otherwise state it and proceed.
   - With NEXUS: `checkpoint` kind `decision` (approach, key choice and reason, rejected alternative); after
     switching to a feature branch, `update_task` with `linkBranch: true`.
   - On the default branch of a project that works with branches, propose a feature branch.
6. **Implement in small steps.** For each step:
   1. Write or extend a test for the criterion; run it and confirm it fails for the expected reason. Where test-first
      is impractical (layout, spikes), write the test alongside and confirm it fails with the new code stubbed out.
   2. Implement the minimum that passes, then refactor while green.
   3. Run the tests for the touched area.
   4. With NEXUS on the larger path, `update_task` with a log line; tick only checklist items that are really done.
   If a step shows the plan is wrong (hidden coupling, missing API, larger scope), stop, revise the plan and tell the
   person when scope or behaviour changes (with NEXUS, a `decision` checkpoint).
7. **Verify** with the commands from CLAUDE.md, CI or the manifest scripts: lint, typecheck and tests (the full suite
   when shared code changed), and the build when the change affects it. Ask first if a command installs dependencies,
   migrates a non-test database or uses the network. Fix failures you caused. Call a failure pre-existing only if you
   saw it on the base commit; otherwise report it as "cause unclear".
8. **Self-review the diff** (`git diff`, `git diff --staged`): debug output, commented-out code, secrets, TODOs
   without a task, sensitive data in logs or errors, unrelated changes. For large changes, apply the code-review skill
   to your own diff.
9. **Docs.** Update the docs the change makes wrong (DOC-* says which). Propose an ADR only for a significant or
   hard-to-reverse decision; never invent rationale nobody stated.
10. **Close out.** With NEXUS: `update_task` (status `review`, or `done` if that is the person's process) with a
    summary log line, and on the larger path a `checkpoint` kind `done`. Use `request_attention` (reason `review`)
    only when the person is likely away.

## Exit criteria

- Every acceptance criterion maps to a test or to a stated manual check.
- Lint, typecheck, tests and build each either ran and passed in this session or are listed as not run, with why.
- The diff has no unrelated changes; affected docs are updated; assumptions and open questions are written down.

## Report

For a small change, a few lines with the same facts are enough.

```markdown
## Feature: <name>
Summary: <what changed and why, 2-4 sentences>
Acceptance criteria:
| Criterion | Evidence |
| Rejects expired invite with 410 | test `invites::rejects_expired` |
Checks (this session): `<command>` -> passed (n tests) | failed (<what>) | not run (<why>)
Files changed: <grouped by area>
Assumptions: <list>
Risks and follow-ups: <list, with suggested tasks>
```

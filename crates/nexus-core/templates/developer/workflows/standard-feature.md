---
nexus: 1
id: {{id}}
kind: workflow
format: workflow
title: Standard feature
status: active
category: feature
rigor: balanced
skills: [workspace:feature-development]
created: {{now}}
updated: {{now}}
source: import
---

Use this workflow for a well-understood feature or an extension of existing behaviour that fits in one reviewable change. Claude clarifies, plans, implements test-first and reports each step; NEXUS verifies with the project's gates. At professional rigor, documentation and a human review join the run. If the work needs an architectural decision first or handles sensitive data, record that decision (ADR) and have it reviewed before starting.

## Clarify
<!-- step: id=clarify kind=claude -->

Restate the goal in two or three sentences: who benefits, what changes for them and how they will notice. Turn the request into acceptance criteria written as testable statements (given/when/then or equivalent) and list what is explicitly out of scope. Read the code involved and the relevant constitutions (with the NEXUS tools: `get_constitution`) before assuming how things work. Ask blocking questions now by reporting needs-input; label every assumption you proceed with.

### Acceptance

- Acceptance criteria are written as testable statements in the task
- Out-of-scope items and assumptions are listed
- Blocking questions are answered or reported as needs-input

## Plan
<!-- step: id=plan kind=claude needs=clarify -->

Name the files, modules and interfaces to change and the order of the changes. For each acceptance criterion, name the test that will prove it and its level (unit, integration, end to end). State the impact on public APIs, stored data, configuration and migrations, how compatibility is kept, and how to roll back. Prefer the smallest design that meets the criteria and follows the patterns already in the codebase. Record the plan as a checkpoint (decision); if it changes later, record the change and the reason.

### Acceptance

- The plan is recorded as a checkpoint (decision)
- Every acceptance criterion maps to at least one planned test
- API, data, configuration and migration impact is named, or stated as none

## Implement with tests
<!-- step: id=implement kind=claude needs=plan gates=test -->

Work in small vertical steps: write or extend a test for one criterion, see it fail, make it pass, refactor while green. Cover the error paths and boundaries named in the plan, not only the happy path. Keep the diff focused: no drive-by refactors, debug output or commented-out code; record unrelated problems as follow-up tasks. Run the tests before reporting done and report them as passing only if they ran and passed in this session; NEXUS then runs the test gate.

### Acceptance

- Every acceptance criterion is covered by at least one automated test
- Error paths and boundaries from the plan are tested
- The diff contains only changes needed for the feature

## Static checks
<!-- step: id=checks kind=check needs=implement gates=lint,typecheck -->

NEXUS runs the project's lint and typecheck gates on the result.

## Documentation
<!-- step: id=docs kind=claude needs=implement rigor=professional -->

Update what users and maintainers read: user-facing docs or README for changed behaviour, the API reference for changed contracts, the configuration reference for new settings, and the changelog if the project keeps one. Write an ADR for a significant or hard-to-reverse decision made during the work. Add code comments only where the reason for the code is not obvious from the code itself.

### Acceptance

- Changed behaviour, APIs and configuration are documented
- Significant decisions have an ADR, or the task log says why none is needed

## Review
<!-- step: id=review kind=review needs=implement,checks,docs rigor=professional -->

A person reviews the diff against the acceptance criteria and the plan, together with the gate results and Claude's reports. Approve to finish; request changes with a note to send implementation and documentation back to Claude.

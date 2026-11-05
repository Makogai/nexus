---
nexus: 1
id: {{id}}
kind: workflow
format: workflow
title: Bug fix
status: active
category: bugfix
rigor: balanced
skills: [workspace:bug-investigation]
created: {{now}}
updated: {{now}}
source: import
---

Use this workflow for a defect in existing behaviour: something that worked, or was specified, and does not behave as expected. It enforces reproduction first and a regression test that fails before the fix and passes after it, so the same bug cannot quietly return. For a production incident, contain the impact first (rollback, feature flag) and use this workflow for the permanent fix.

## Reproduce
<!-- step: id=reproduce kind=claude -->

Capture the report precisely: observed and expected behaviour, version or commit, environment, inputs and how often it happens. Reproduce it on the current main branch or the reported version, then reduce the reproduction to the fewest steps and the smallest input. If you cannot reproduce it after a reasonable attempt, report needs-input with what you tried and what information is missing; do not fix a guess.

### Acceptance

- Exact reproduction steps with observed and expected behaviour are recorded as a checkpoint
- The affected version or commit is known, or recorded as unknown

## Root cause
<!-- step: id=cause kind=claude needs=reproduce -->

Find the defect that produces the symptom. Trace the data or control flow back from the failure, test one hypothesis at a time and confirm it with evidence: a log line, a debugger observation, a failing assertion, or `git bisect` to the commit that introduced it. Distinguish the root cause from the trigger. Search for the same defect elsewhere (same pattern, copied code, sibling handlers). Record the cause and its evidence as a checkpoint (decision).

### Acceptance

- The root cause is stated with the evidence that confirms it
- Other places with the same defect are listed, or the search is recorded as finding none

## Failing regression test
<!-- step: id=regression kind=claude needs=cause -->

Write an automated test at the lowest level that exercises the root cause: a unit test if possible, an integration test if the defect lies in the interaction between parts. Run it and confirm it fails for the bug's reason (the assertion describes the bug, not a setup error). If the defect cannot be covered by an automated test, record why and the manual check that replaces it.

### Acceptance

- A regression test exists and was observed failing for the bug's reason before the fix
- If no automated test is possible, the reason and the replacement manual check are recorded

## Fix
<!-- step: id=fix kind=claude needs=regression gates=test -->

Fix the root cause, not the symptom: no special-casing of the reproduction input, no swallowed errors. Keep the change minimal and scoped to the defect. Fix the other occurrences found during root-cause analysis, or create a task for each one you leave. The regression test must pass now and stay in the suite; change an existing test only if it encoded the buggy behaviour, and say so.

### Acceptance

- The regression test passes and remains in the suite
- The change addresses the recorded root cause
- No unrelated behaviour changed

## Static checks
<!-- step: id=checks kind=check needs=fix gates=lint,typecheck -->

NEXUS runs the project's lint and typecheck gates on the fix.

## Record the root cause
<!-- step: id=record kind=claude needs=fix -->

Write a short note for future maintainers: symptom, root cause, why it was not caught earlier (missing test, unclear contract, unhandled input), the fix, and follow-up work such as monitoring, hardening or similar code. Store it as a checkpoint (decision) and put the essentials in the commit message or pull request description. If the bug affected released versions or stored data, state that explicitly and who needs to know.

### Acceptance

- Symptom, root cause, fix and why it escaped are recorded
- Follow-up work exists as tasks, or is recorded as none needed
- Impact on released versions and stored data is stated

## Review
<!-- step: id=review kind=review needs=fix,record rigor=professional -->

A person confirms the root-cause explanation, the regression test and the fix. Approve to finish; request changes with a note to send the fix back to Claude.

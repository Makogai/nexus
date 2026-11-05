---
nexus: 1
id: {{id}}
kind: workflow
format: workflow
title: Test coverage improvement
status: active
category: testing
rigor: balanced
skills: [workspace:test-generation]
created: {{now}}
updated: {{now}}
source: import
---

Use this workflow to strengthen the test suite where failures would hurt most, rather than to raise a coverage percentage. Gaps are ranked by risk, every new test is proven able to fail, and the result is recorded so the next round starts where this one ended. Production code does not change in this workflow; bugs that the new tests reveal become separate tasks.

## Find risk-weighted gaps
<!-- step: id=gaps kind=claude -->

Rank untested or weakly tested behaviour by risk: the impact of a failure times the likelihood of a defect or change. Use the coverage report if the project has one (it shows code that is not executed, not behaviour that is verified), recent bug fixes without regression tests, frequently changed files in the Git history, error handling and boundary paths, security-relevant code (authorisation, input parsing, money, data deletion) and public APIs. Skip generated code and trivial accessors. Record the top gaps, at most about ten, as a checkpoint with the reason each matters.

### Acceptance

- A ranked list of gaps with the risk reason for each is recorded as a checkpoint
- The source of each gap (coverage data, history, code reading) is stated

## Add tests
<!-- step: id=tests kind=claude needs=gaps -->

Use the test-generation skill and work through the list in order. Test behaviour through public interfaces, one behaviour per test, with names that state the condition and the expected outcome. Keep tests deterministic: no real clock, network or shared state, and fixed seeds for randomness. Follow the project's existing test style and helpers. Do not change production code beyond a minimal seam needed for testing. If a test reveals a bug, do not fix it here: create a task with the failing test as its reproduction and keep that test out of this change, or mark it with the project's convention for known failures.

### Acceptance

- Each new test maps to a gap from the list
- The new tests use no real clock, network or shared state
- Bugs revealed by the tests are recorded as tasks, not fixed here

## Mutation sanity check
<!-- step: id=mutate kind=claude needs=tests -->

Prove that each new test can fail. Temporarily break the behaviour it covers (invert a condition, remove a call, shift a boundary by one, return a default value), confirm the test fails with a clear message, then restore the code. If the project already uses a mutation testing tool such as Stryker, mutmut, cargo-mutants, PIT or Infection, run it on the changed area instead. Strengthen or remove any test that still passes against broken code. Afterwards the production code must be exactly as before; check the diff.

### Acceptance

- Every new test was observed failing against a deliberate break of its behaviour
- Tests that did not fail were strengthened or removed
- The diff contains no changes to production code

## Run the suite
<!-- step: id=suite kind=check needs=mutate gates=test -->

NEXUS runs the project's test gate on the full suite.

## Record what is covered
<!-- step: id=record kind=claude needs=suite -->

Record the outcome as a checkpoint so the next round starts from it: the gaps closed (with test names), the gaps deliberately left and why, the bugs found, and coverage figures before and after only if both were measured in this session with the same tool and settings.

### Acceptance

- Closed and remaining gaps are recorded as a checkpoint
- Coverage figures are reported only if measured before and after in this session

## Review
<!-- step: id=review kind=review needs=tests,record rigor=professional -->

A person checks that the new tests target the risks named in the gap list and are readable. Approve to finish; request changes with a note to send the tests back to Claude.

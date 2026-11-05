---
name: test-generation
description: Adds meaningful, deterministic tests for untested risk in the project's own framework and proves each one catches breakage. Use when asked to add tests, raise coverage or harden existing code.
---

# Test generation

Add tests that protect behaviour that matters, would fail if that behaviour broke, and run the same way every time.
Coverage numbers are a hint about where to look, never the goal.

## Ground rules

- Test behaviour through public interfaces: returned values, persisted state, responses, emitted events. Do not
  assert private calls, internal state or call counts on internal collaborators unless that is the contract.
- Every new test must be seen failing against deliberately broken behaviour before it counts (step 6).
- Do not change production code to make a test pass. Adding a seam (injectable clock, interface at a boundary) is
  a design change: ask first and list it in the report.
- If a test exposes a bug, keep the evidence and report it; do not fix it silently. Mark the test with the
  framework's expected-failure or skip mechanism, with a reason, only when the person agrees.
- Report results only from runs in this session.

## Procedure

1. **Target.** The module, change or PR in scope. If the NEXUS tools are available: `get_constitution` with
   `["TST-*"]` plus the stack prefix, and `get_task` for acceptance criteria that need tests.
2. **Learn the conventions.** Framework and runner (Vitest/Jest, pytest, `cargo test`, PHPUnit/Pest,
   `flutter_test`, JUnit, ...), file locations and naming, fixtures, factories and builders, mocking style, how
   integration tests get a database or services. Copy the structure of the closest existing test. Do not add a new
   test framework or library without consent.
3. **Find untested risk.** Read the code and its tests; use the project's coverage command when it has one (ask
   first if it is slow or needs services). Rank by impact x likelihood:
   - critical paths: money, authentication and authorization, data writes, migrations, external contracts;
   - complex branching and state machines;
   - error handling and recovery, timeouts, partial failure;
   - boundaries: empty, one, many, maximum, off-by-one, unicode, timezones and DST, very large inputs;
   - concurrency and ordering; input validation; recent fixes without regression tests.
4. **Plan the test list.** For each test: a name that states the behaviour ("rejects an expired token with 401"),
   the lowest level that gives real confidence (unit, integration, end to end), and both happy and failure paths.
   Present the list first when it exceeds about fifteen tests or needs new seams or fixtures.
5. **Write the tests.**
   - Arrange-Act-Assert, one behaviour per test, minimal setup through existing builders.
   - Assert specific values, not truthiness; for errors assert the type or code and the meaningful part of the
     message.
   - Determinism: fake or inject the clock, seed or inject randomness, no real network (stub at the boundary or use
     a local fake server), temporary directories, no shared mutable state, no order dependence, no fixed sleeps
     (wait for a condition), stable ordering before comparing collections, explicit locale and timezone.
   - Snapshots only for small, stable, reviewable outputs (a serialized format, CLI help). Never as a substitute
     for assertions on large component trees or responses.
   - Property-based tests for invariants (round trips, idempotence, ordering) when the project already uses a
     property-testing library.
6. **Prove each test bites.** For each new test, temporarily break the behaviour it protects (invert a condition,
   return early, drop a validation, shift a boundary by one), run the test, confirm it fails with a clear message,
   and restore the code. Confirm with `git diff` that production code is back to its original state. A test that
   cannot be made to fail is rewritten or deleted. If the project already has a mutation-testing tool (Stryker,
   `cargo-mutants`, mutmut, Infection), you may run it on the scope with consent.
7. **Run.** Run the new tests several times (the runner's repeat option, or a loop) to expose flakiness, then the
   full suite, plus lint and typecheck for the test code.
8. **Record.** With NEXUS: `update_task` with a log line, `checkpoint` kind `progress` or `done`.

## Exit criteria

- Each new test targets a named risk, follows project conventions and was seen failing against broken behaviour.
- New tests pass repeatedly; the full suite ran in this session (or why not is stated).
- Bugs found are reported with their failing test; seams added are listed.

## Report

```markdown
## Tests added: <scope>
Risks targeted: <list with why> | Deliberately not covered: <list with why>
| File | Test | Protects | Level | Bite check |
| tests/invite.test.ts | rejects expired invite | expiry check in acceptInvite | unit | failed when expiry check removed |
Runs (this session): new tests x<n> -> stable; `<suite command>` -> <result>
Bugs found (not fixed): <list or "none">
Seams or production changes: <list or "none">
Follow-ups: <list>
```

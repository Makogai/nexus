---
name: test-generation
description: Adds deterministic tests for untested risk in the project's framework and proves each one can fail. Use when asked to add tests or close coverage gaps.
---

# Test generation

Add tests that protect behaviour that matters, would fail if that behaviour broke, and run the same way every time.
Coverage numbers are a hint about where to look, never the goal.

## Ground rules

- TST-* says how tests are written (behaviour through public interfaces, determinism, isolation, naming, test data);
  read it first. This skill adds what is specific to adding tests to existing code.
- A new test counts only after it was seen failing against deliberately broken behaviour (step 6).
- Do not change production code to make a test pass. Adding a seam (an injectable clock, an interface at a boundary)
  is a design change: ask first and list it in the report.
- If a test exposes a bug, keep the evidence and report it; do not fix it silently. Mark the test with the
  framework's expected-failure or skip mechanism, with a reason, only when the person agrees.

## Procedure

1. **Target.** The module, change or PR in scope. If the NEXUS tools are available: `get_constitution` with
   `["TST-*"]` plus the stack documents listed for this project, and `get_task` for acceptance criteria that need
   tests.
2. **Learn the conventions.** Framework and runner (Vitest or Jest, pytest, `cargo test`, Pest or PHPUnit,
   `flutter_test`, JUnit, ...), file locations and naming, fixtures, factories and builders, mocking style, and how
   integration tests get a database or services. Copy the structure of the closest existing test. Do not add a test
   framework or library without consent.
3. **Find untested risk.** Read the code and its tests; use the project's coverage command when it has one (ask first
   if it is slow or needs services). Rank by impact x likelihood:
   - critical paths: money, authentication and authorisation, data writes, migrations, external contracts;
   - complex branching and state machines; error handling, timeouts, partial failure;
   - boundaries: empty, one, many, maximum, off-by-one, unicode, timezones and DST, very large inputs;
   - concurrency and ordering; input validation; recent fixes without regression tests.
4. **Plan the test list.** For each test: a name that states the behaviour ("rejects an expired token with 401"), the
   lowest level that gives real confidence (unit, integration, end to end), and both happy and failure paths. Present
   the list first when it exceeds about fifteen tests or needs new seams or fixtures.
5. **Write the tests** following TST-* and the closest existing test, plus:
   - Assert specific values, not truthiness; for errors, the type or code and the meaningful part of the message.
   - Sort collections before comparing them when order is not the contract; set locale and timezone explicitly.
   - Snapshots only for small, stable, reviewable outputs (a serialized format, CLI help), never instead of
     assertions on large component trees or responses.
   - Property-based tests for invariants (round trips, idempotence, ordering) when the project already uses a
     property-testing library.
6. **Prove each test bites.** Temporarily break the behaviour a test protects (invert a condition, return early, drop
   a validation, shift a boundary by one), run it, confirm it fails with a clear message, and restore the code; one
   break may prove several tests at once. Confirm with `git diff` that production code is back to its original
   state. A test that cannot be made to fail is rewritten or deleted. If the project already has a mutation-testing
   tool (Stryker, `cargo-mutants`, mutmut, Infection, PIT), you may run it on the scope instead, with consent.
7. **Run.** The new tests several times (the runner's repeat option, or a loop) to expose flakiness, then the full
   suite, plus lint and typecheck for the test code.
8. **Record.** With NEXUS: `update_task` with a log line, and `checkpoint` kind `progress` or `done`. Create tasks for
   bugs found when the person agrees or a workflow step asks for it.

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

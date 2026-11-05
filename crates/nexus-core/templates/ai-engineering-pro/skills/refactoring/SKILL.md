---
name: refactoring
description: Restructures code without changing its behaviour, in small steps kept green by tests. Use when asked to refactor, clean up, simplify or restructure code.
---

# Refactoring

Improve structure while keeping observable behaviour identical, proven by tests that ran green before the first edit
and after every step.

## Scale

- **Local refactor** (rename, extract or inline a function, move code within a module), ideally tool- or
  compiler-assisted: steps 1, 3, 6 and 8, with a quick look at whether tests cover the touched behaviour. No measures,
  no commit plan.
- **Restructuring** (across modules, near a public surface, or with weak tests): the full procedure.

## Ground rules

- Behaviour is preserved: return values, side effects, error types and messages that callers depend on, output
  formats, public API, persisted and serialized names, log lines consumed by alerting. An intended behaviour change
  is a separate change, before or after the refactor (CQ-*).
- No green baseline, no refactor. Tests run before the first edit and after every step.
- A bug discovered during the refactor is reported, not fixed inside the refactor.

## Procedure

1. **Goal and boundary.** State the motivating problem (duplication, long function, tangled dependency, an upcoming
   feature that the current shape blocks), the files or modules in scope, and the public surface that must not
   change. If the goal requires changing a public API, persisted format or wire format, it is a migration: plan the
   deprecation with the person instead.
2. **Rules.** With NEXUS, `get_constitution` with `["CQ-*", "ARC-*", "TST-*"]` plus the stack documents listed for
   this project. Otherwise follow CLAUDE.md and the linter configuration.
3. **Baseline.** Start from a clean working tree, or agree with the person how to treat existing changes. Run the
   relevant tests, lint and typecheck. If anything is red, stop and report: refactoring on red hides breakage.
4. **Coverage check.** List the behaviours of the code in scope (inputs, outputs, side effects, errors) and check
   that tests exercise them: the project's coverage command if it has one, otherwise read the tests. A fast probe:
   break one line deliberately, run the tests, and revert immediately; if nothing fails, that behaviour is not
   covered. For gaps, write **characterisation tests** that pin current behaviour as it is, even when it looks wrong
   (mark suspicious behaviour with a comment and report it). Assert at the behaviour level so the tests survive the
   restructuring, and keep them apart from the refactoring edits.
5. **Measure before** (restructuring, or when the person asks). One to three measures that match the goal: function
   length, cyclomatic or cognitive complexity (the project's linter rule, `clippy`, `radon`, `phpmd` or similar if
   already available), duplication (`jscpd` or similar), parameter counts, imports between modules, size of the
   public surface. Record tool and numbers. Do not install tools without consent; without one, use countable proxies
   and say so.
6. **Refactor in small mechanical steps,** one named refactoring per step: extract function, inline, rename, move,
   introduce parameter object, split module, replace conditional with polymorphism or a match on a sum type, invert
   a dependency. Prefer IDE-, compiler- or tool-assisted transformations and let the compiler find call sites. After
   each step: build or typecheck, then tests.
   - Green: next step.
   - Red: revert that step instead of debugging forward, then take a smaller step.
7. **Keep the change separable** (restructuring): characterisation tests; each refactoring or group of identical
   mechanical edits; pure moves and renames on their own so the diff shows movement only; formatting never mixed
   with logic. Commit in that split when the person asks for commits; otherwise describe the intended split.
8. **Check for hidden behaviour change.** Every changed line must be explainable as structure only. Look for:
   evaluation order with side effects, exception types and messages, default values, null/empty handling, iteration
   and sort order, async timing and cancellation, visibility of symbols, serialized field names, and reflection or
   string-based lookups (DI containers, routing, ORM mappings) that the compiler cannot see.
9. **Stop conditions.**
   - A bug found (characterisation reveals wrong behaviour, or an existing test expectation looks wrong): stop,
     report it with a reproduction, and ask whether to fix it separately (bug-investigation skill). With NEXUS,
     `checkpoint` kind `note` (or `blocker` if it stops the work).
   - Scope growing beyond the agreed boundary: pause and re-plan with the person.
10. **Measure after** with the same tools and settings (if you measured before), then run the tests, lint and
    typecheck for the whole affected area (the full suite when shared code moved).
11. **Record.** With NEXUS: `update_task` with a log line, and for a restructuring a `checkpoint` kind `done` with
    the measures before and after.

## Exit criteria

- The baseline was green, and tests, lint and typecheck are green after the last step (both in this session).
- Every behaviour in scope is covered by a test that existed or was added before restructuring.
- The public surface is unchanged, and how that was checked is stated.
- Measures (if taken) are reported before and after with the same tool; any bug found is reported, not silently
  fixed.

## Report

```markdown
## Refactoring: <scope>
Goal: <problem being solved>
Public surface unchanged: <how verified - compiler, API diff, tests>
Characterisation tests added: <files and names, or "none needed - coverage confirmed by probe">
Steps: <extract X from Y; move Z to module W; ...>
Measures (<tool>): <metric> <before> -> <after> | not measured (local refactor)
Checks (this session): `<command>` -> <result>
Commit split: <done | proposed>
Bugs or suspicious behaviour found (not fixed): <list or "none">
Follow-ups: <list>
```

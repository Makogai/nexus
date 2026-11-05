---
name: refactoring
description: Restructures code without changing behaviour, in small test-verified steps with characterisation tests and before/after measures. Use when asked to refactor, clean up, simplify or restructure code.
---

# Refactoring

Improve structure while keeping observable behaviour identical, proven by tests that ran green before the first
edit and after every step.

## Ground rules

- Behaviour is preserved: return values, side effects, error types and messages that callers depend on, output
  formats, public API, persisted and serialized names, log lines consumed by alerting. An intended behaviour
  change is a separate change, before or after the refactor.
- No green baseline, no refactor. Tests run before the first edit and after every step.
- A bug discovered during the refactor is reported, not fixed inside the refactor.
- Report tests and measurements only from runs in this session; name the tool and command.

## Procedure

1. **Goal and boundary.** State the motivating problem (duplication, long function, tangled dependency, an
   upcoming feature that the current shape blocks), the files or modules in scope, and the public surface that must
   not change. If the goal requires changing a public API, persisted format or wire format, this is a migration:
   plan deprecation with the person instead.
2. **Rules.** If the NEXUS tools are available, `get_constitution` with `["CQ-*", "ARC-*", "TST-*"]` plus the
   stack prefix (`["TS-*"]`, `["RS-*"]`, `["PY-*"]`, ...). Otherwise follow CLAUDE.md and linter configuration.
3. **Baseline.** Start from a clean working tree, or get the person's agreement about existing changes. Run the
   relevant tests, lint and typecheck. If anything is red, stop and report: refactoring on red hides breakage.
4. **Coverage check.** List the behaviours of the code in scope (inputs, outputs, side effects, errors). Check the
   tests exercise them: use the project's coverage command if it has one, otherwise read the tests. A fast probe:
   break one line deliberately, run the tests, and revert immediately - if nothing fails, that behaviour is not
   covered.
   For gaps, write **characterisation tests** that pin current behaviour as it is, even when it looks wrong (mark
   suspicious behaviour with a comment and report it). Assert at the behaviour level, not on internals, so the
   tests survive the restructuring. Keep them separate from the refactoring commits.
5. **Measure before.** Pick one to three measures that match the goal: function length, cyclomatic or cognitive
   complexity (the project's linter rule, `clippy`, `radon`, `phpmd` or similar if already available), duplication
   (`jscpd` or similar), number of parameters, imports between modules, size of the public surface. Record tool and
   numbers. Do not install tools without consent; if none is available, use countable proxies and say so.
6. **Refactor in small mechanical steps,** one named refactoring per step: extract function, inline, rename, move,
   introduce parameter object, split module, replace conditional with polymorphism or a match on a sum type,
   invert dependency. Prefer compiler- or tool-assisted transformations and let the compiler find call sites.
   After each step: build or typecheck, then tests.
   - Green: next step.
   - Red: revert that step instead of debugging forward, then take a smaller step.
7. **Keep commits separable** (commit only when the person asked; otherwise describe the intended split):
   characterisation tests; each refactoring or group of identical mechanical edits; pure moves and renames on their
   own so the diff shows movement only; formatting never mixed with logic.
8. **Check for hidden behaviour change.** Every changed line must be explainable as structure only. Look for:
   evaluation order with side effects, exception types and messages, default values, null/empty handling,
   iteration and sort order, async timing and cancellation, visibility of symbols, serialized field names,
   reflection or string-based lookups (DI containers, routing, ORM mappings) that the compiler cannot see.
9. **Stop conditions.**
   - A bug found (characterisation reveals wrong behaviour, or an existing test expectation looks wrong): stop.
     With NEXUS record a `checkpoint` kind `note` (or `blocker` if it stops the work), report it with a
     reproduction, and ask whether to fix it separately (bug-investigation skill, if the workspace has it).
   - Scope growing beyond the agreed boundary: pause and re-plan with the person.
10. **Measure after** with the same tools and settings, then run the full suite, lint and typecheck.
11. **Record.** With NEXUS: `checkpoint` kind `done` (or `progress`) with the measures before and after;
    `update_task` with a log line.

## Exit criteria

- The baseline was green, and tests, lint and typecheck are green after the last step (both in this session).
- Every behaviour in scope is covered by a test that existed or was added before restructuring.
- The public surface is unchanged, and how that was checked is stated.
- Measures before and after are reported with the same tool, and any bug found is reported, not silently fixed.

## Report

```markdown
## Refactoring: <scope>
Goal: <problem being solved>
Public surface unchanged: <how verified - compiler, API diff, tests>
Characterisation tests added: <files and names, or "none needed - coverage confirmed by probe">
Steps: <extract X from Y; move Z to module W; ...>
Measures (<tool>): <metric> <before> -> <after>
Checks (this session): `<command>` -> <result>
Commit split: <done | proposed>
Bugs or suspicious behaviour found (not fixed): <list or "none">
Follow-ups: <list>
```

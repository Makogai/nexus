---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Code quality
status: active
applies: on-demand
prefix: CQ
created: {{now}}
updated: {{now}}
source: import
---

Code-level quality rules for writing and reviewing source code: readability, structure, types, error handling, formatting and static analysis. Apply them to every line you add or change.

## Readability and structure

Code is read far more often than it is written, so the reader's time is the scarce resource.

#### Optimise for the reader
<!-- rule: id=CQ-1 enforcement=rule -->
Write code a teammate new to the module can follow without running it: straightforward control flow, guard clauses instead of deep nesting, one level of abstraction per function. Prefer a clear five-line version over a clever one-liner unless the one-liner is an established idiom of the language.

#### Keep functions and modules small and focused
<!-- rule: id=CQ-2 enforcement=rule -->
A function does one thing its name states; a module groups code that changes for the same reason. Split when a description needs "and", a function takes more than about four parameters, or it outgrows one screen (roughly 40 lines). Treat these numbers as signals, not hard limits.

#### Treat complexity limits as refactoring signals
<!-- rule: id=CQ-3 enforcement=guideline -->
Aim for a cyclomatic complexity of 10 or less per function and a nesting depth of 3 or less. Above that, extract named helpers, replace condition chains with lookup tables or polymorphism, or use guard clauses. A flat, exhaustive match over a state machine may exceed the limit if it stays clearer.

#### Remove duplication on the third occurrence
<!-- rule: id=CQ-4 enforcement=guideline -->
Tolerate a second copy of similar code; extract a shared abstraction when a third appears and the copies change for the same reason. Do not merge code that only looks alike but represents different concepts, because a wrong abstraction costs more than duplication.

#### Write comments that explain why, not what
<!-- rule: id=CQ-5 enforcement=rule -->
Comment intent, constraints, trade-offs and surprising behaviour that the code cannot express. Do not restate what the code does; rename or restructure instead. Update or delete a comment in the same change that makes it wrong.

## Types and correctness

The type system and explicit error handling catch defects before they reach users.

#### Use the strongest practical types and avoid escape hatches
<!-- rule: id=CQ-6 enforcement=rule -->
Do not silence the type checker with `any`, double casts (`as unknown as T`), `@ts-ignore`, `# type: ignore` or untyped `dynamic`. Use `unknown` plus narrowing for untrusted values and model states with enums or discriminated unions. If an escape hatch is unavoidable, confine it to one line and comment why.

#### Never swallow exceptions
<!-- rule: id=CQ-7 enforcement=rule -->
No empty catch blocks and no catch-all handlers that only log and continue. Catch the narrowest error type you can actually handle, and when wrapping or rethrowing keep the original error as the cause so the stack trace survives.

#### Prefer immutable values
<!-- rule: id=CQ-8 enforcement=guideline -->
Declare bindings constant by default (`const`, `final`, `readonly`, Rust's non-`mut` bindings) and return new values instead of mutating arguments or shared objects. Use mutation deliberately inside a small scope, such as a local builder or a measured hot loop, where it is clearer or faster.

#### Validate input once, at the boundary
<!-- rule: id=CQ-9 enforcement=rule -->
Parse external input (request bodies, CLI arguments, files, environment, message payloads) where it enters the program and convert it into typed domain values, e.g. with a schema validator. Inner code then relies on those types instead of re-checking the same conditions everywhere.

#### Replace magic numbers and strings with named constants
<!-- rule: id=CQ-10 enforcement=rule -->
Give literals with domain meaning a single named definition or an enum (`MAX_LOGIN_ATTEMPTS = 5`, `OrderStatus.Shipped`). Obvious literals such as 0, 1 or an empty string, or a value used once right next to its explanation, may stay inline.

## Consistency and tooling

Automated tools settle style and catch whole classes of defects so reviews can focus on behaviour.

#### Let the formatter own code style
<!-- rule: id=CQ-11 enforcement=rule check=lint -->
Format all code with the project's configured formatter (e.g. Prettier, rustfmt, Ruff or Black, dart format, Laravel Pint) and never hand-format against it. Do not debate style in review; change the formatter configuration if the team wants a different style.

#### Introduce no new linter or static-analysis warnings
<!-- rule: id=CQ-12 enforcement=rule check=lint -->
A change leaves the linter and static analysers with no new warnings or errors. Fix the cause instead of adding suppressions; when a suppression is genuinely needed, scope it to one line, name the rule and give the reason in a comment.

#### Follow existing project conventions over personal taste
<!-- rule: id=CQ-13 enforcement=rule -->
Before writing new code, look at how neighbouring code names things, structures folders, handles errors and writes tests, and do the same. Propose a convention change separately with a rationale instead of introducing a second style in one corner of the codebase.

#### Separate refactoring from behaviour changes
<!-- rule: id=CQ-14 enforcement=rule -->
Keep pure refactors (renames, moves, extractions) in their own commit or pull request, with tests green before and after and no behaviour change. Make the behavioural change on top, so each part can be reviewed and reverted on its own.

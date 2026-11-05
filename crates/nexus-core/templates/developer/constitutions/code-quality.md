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

Code-level rules for writing and reviewing source code: readability, naming, structure, types, error handling, formatting and static analysis. Apply them to the code you add or change; existing code outside the change is a follow-up, not part of the task.

## Readability and structure

Code is read far more often than it is written, so the reader's time is the scarce resource.

#### Write code a newcomer to the module can follow
<!-- rule: id=CQ-1 enforcement=rule -->
Prefer straightforward control flow and guard clauses over deep nesting, and clear code over clever one-liners unless the idiom is established. Name things after their domain meaning, with units the type does not carry (`overdueInvoices`, `timeoutMs`), give domain literals named constants, and use only abbreviations the codebase already uses.

#### Treat size and complexity limits as signals
<!-- rule: id=CQ-2 enforcement=guideline -->
A function does one thing its name states. Consider splitting when its description needs "and", or it exceeds about four parameters, 40 lines, cyclomatic complexity 10 or nesting depth 3. These are signals, not hard limits: a flat, exhaustive match over a state machine may be clearer than its split.

#### Remove duplication on the third occurrence
<!-- rule: id=CQ-3 enforcement=guideline -->
Tolerate a second copy of similar code; extract a shared abstraction when a third appears and the copies change for the same reason. Do not merge code that only looks alike but represents different concepts, because a wrong abstraction costs more than duplication.

#### Write comments that explain why, not what
<!-- rule: id=CQ-4 enforcement=rule -->
Comment intent, constraints, workarounds and surprising behaviour the code cannot express, linking the issue, ADR or upstream bug where one exists. Do not restate the code; rename instead. Give every TODO an issue or task id, and update or delete a comment in the same change that makes it wrong.

#### Delete dead code instead of commenting it out
<!-- rule: id=CQ-5 enforcement=rule -->
Leave no commented-out code, unused functions or unreachable branches in your change, and delete code your change makes unused once a search (including configuration, templates and reflection) confirms nothing uses it. Version control keeps the history; dead code elsewhere is a follow-up.

## Types and correctness

The type system and explicit error handling catch defects before they reach users.

#### Use the strongest practical types and avoid escape hatches
<!-- rule: id=CQ-6 enforcement=rule -->
Do not silence the type checker with untyped escape hatches (`any`, `dynamic`, unchecked casts, ignore comments); narrow untrusted values from an unknown type instead. Model alternatives as enums or tagged unions so invalid states cannot be represented. Confine an unavoidable escape hatch to one line with a comment why.

#### Handle every failure deliberately
<!-- rule: id=CQ-7 enforcement=rule -->
Recover from an error deliberately, propagate it with context and the original as its cause, or report it with an actionable message. No empty catch blocks, catch-alls that only log and continue, or default, empty or success values that hide a failure. Catch the narrowest type you can handle; if ignoring is correct, comment why.

#### Prefer immutable values
<!-- rule: id=CQ-8 enforcement=guideline -->
Declare bindings constant by default (`const`, `final`, `readonly`, Rust's non-`mut` bindings) and return new values instead of mutating arguments or shared objects. Mutate deliberately inside a small scope, such as a local builder or a measured hot loop.

#### Parse input once, at the boundary
<!-- rule: id=CQ-9 enforcement=rule -->
Parse external input (request bodies, CLI arguments, files, environment, message payloads) where it enters the program into typed domain values, e.g. with a schema validator, and reject what does not fit. Inner code then relies on those types instead of re-checking the same conditions everywhere.

## Consistency and tooling

Automated tools settle style and catch whole classes of defects so reviews can focus on behaviour.

#### Let the formatter own code style
<!-- rule: id=CQ-10 enforcement=rule check=lint -->
Format the code you change with the project's configured formatter (e.g. Prettier, rustfmt, Ruff, dart format, Laravel Pint) and never hand-format against it. Do not reformat files you did not otherwise change; a different style is a formatter configuration change of its own.

#### Introduce no new linter or static-analysis warnings
<!-- rule: id=CQ-11 enforcement=rule check=lint -->
A change leaves the linter and static analysers with no new warnings or errors. Fix the cause instead of adding suppressions; when a suppression is genuinely needed, scope it to one line, name the rule and give the reason in a comment.

#### Follow existing project conventions over personal taste
<!-- rule: id=CQ-12 enforcement=rule -->
Before writing new code, look at how neighbouring code names things, structures folders, handles errors and writes tests, and do the same. Propose a convention change separately instead of introducing a second style in one corner of the codebase.

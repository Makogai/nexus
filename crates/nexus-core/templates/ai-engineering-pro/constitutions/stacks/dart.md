---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Dart
status: active
applies: on-demand
prefix: DART
stacks: [dart, flutter]
created: {{now}}
updated: {{now}}
source: import
---

Rules for the Dart 3 language in Flutter apps, servers and packages. Apply to all Dart code; they refine CQ and TST.

## Type safety

Sound null safety and Dart 3 patterns let the analyzer prove most code paths correct.

#### Use sound null safety without ! escapes
<!-- rule: id=DART-1 enforcement=rule -->
Model absence with nullable types and handle it with ?., ??, if-case patterns or early returns; avoid the ! operator
except where an invariant guarantees a value, and use late only for fields set before first use by a lifecycle method.

#### Avoid dynamic
<!-- rule: id=DART-2 enforcement=rule check=lint -->
Enable strict-casts, strict-inference and strict-raw-types under analyzer: language: in analysis_options.yaml, decode
JSON into typed model classes at the boundary, and use Object? rather than dynamic for values of unknown type.

#### Model states with sealed classes and patterns
<!-- rule: id=DART-3 enforcement=rule -->
Represent closed sets of states or results as sealed class hierarchies and handle them with exhaustive switch
expressions and destructuring patterns; use records for small multi-value returns instead of ad-hoc classes, lists or
maps.

#### Prefer final, const and immutable models
<!-- rule: id=DART-4 enforcement=rule -->
Declare locals and fields final unless reassigned and use const where possible (prefer_final_locals,
prefer_const_constructors). Make data models immutable with final fields, copyWith and value equality, generated with
freezed or written by hand as the project does; never mutate a shared model instance.

## Asynchronous code

Forgotten futures lose errors silently and make ordering bugs hard to find.

#### Await futures and handle their errors
<!-- rule: id=DART-5 enforcement=rule -->
Await every Future or hand it to unawaited() deliberately, with the unawaited_futures lint enabled; wrap awaited I/O
in try/catch with typed on clauses, and cancel every stream subscription you start.

## Style and tooling

The analyzer and formatter are the shared definition of correct, readable Dart.

#### Keep the analyzer and formatter clean
<!-- rule: id=DART-6 enforcement=rule check=lint -->
Include package:lints/recommended.yaml (flutter_lints in Flutter apps) plus project rules in analysis_options.yaml,
and in CI run dart analyze --fatal-infos (flutter analyze in Flutter projects) and dart format --output=none
--set-exit-if-changed .; fix findings instead of adding ignore comments.

#### Keep imports inside package boundaries
<!-- rule: id=DART-7 enforcement=guideline -->
Never reach into or out of lib/ with relative paths and never import another package's src/ files. Within lib/ keep
the project's one style, relative (Effective Dart's preference) or package: imports, enforced with
prefer_relative_imports or always_use_package_imports.

## Testing

Tests in Dart are cheap to write when dependencies are injected.

#### Test with dart test
<!-- rule: id=DART-8 enforcement=rule check=test -->
Write unit tests with package:test (group, test, setUp, expect with matchers), run them with dart test (flutter test
in Flutter projects), and replace dependencies through constructor-injected interfaces rather than global state.

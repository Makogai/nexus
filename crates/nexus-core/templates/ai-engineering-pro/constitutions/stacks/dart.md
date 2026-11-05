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

#### Prefer final and const
<!-- rule: id=DART-4 enforcement=rule -->
Declare locals and fields final unless they are reassigned, and use const constructors and values for compile-time
constants; enable prefer_final_locals and prefer_const_constructors so the analyzer reports misses.

#### Keep models immutable
<!-- rule: id=DART-5 enforcement=rule -->
Make data models immutable with final fields, copyWith, value equality (== and hashCode) and JSON conversion,
generated with freezed and json_serializable or written by hand; never mutate a shared model instance.

## Asynchronous code

Forgotten futures lose errors silently and make ordering bugs hard to find.

#### Await futures and handle their errors
<!-- rule: id=DART-6 enforcement=rule -->
Await every Future or hand it to unawaited() deliberately, with the unawaited_futures lint enabled; wrap awaited I/O
in try/catch with typed on clauses, and cancel every stream subscription you start.

## Style and tooling

The analyzer and formatter are the shared definition of correct, readable Dart.

#### Configure the recommended lints
<!-- rule: id=DART-7 enforcement=rule check=lint -->
Include package:lints/recommended.yaml (flutter_lints for Flutter apps) in analysis_options.yaml, add project rules on
top, and keep dart analyze free of warnings in CI.

#### Format with dart format
<!-- rule: id=DART-8 enforcement=rule check=lint -->
Format with dart format and verify in CI with dart format --output=none --set-exit-if-changed .; do not hand-format
or fight the formatter's style.

#### Use package imports for code in lib/
<!-- rule: id=DART-9 enforcement=guideline -->
Import libraries under lib/ with package: URIs (always_use_package_imports) and never reach into or out of lib/ with
relative paths; mixing styles can load one library twice under two URIs. Never import another package's src/ files.

## Testing

Tests in Dart are cheap to write when dependencies are injected.

#### Test with dart test
<!-- rule: id=DART-10 enforcement=rule check=test -->
Write unit tests with package:test (group, test, setUp, expect with matchers), run them with dart test (flutter test
in Flutter projects), and replace dependencies through constructor-injected interfaces rather than global state.

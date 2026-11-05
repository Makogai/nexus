---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: TypeScript
status: active
applies: on-demand
prefix: TS
stacks: [typescript]
created: {{now}}
updated: {{now}}
source: import
---

Rules for TypeScript 5.x and later in frontend, backend and library code. Apply in every TypeScript project; they
refine CQ, and NG, VUE, REACT and NODE refine them.

## Compiler and lint settings

The type checker and type-aware lint rules only protect code they are allowed to check strictly.

#### Enable strict mode
<!-- rule: id=TS-1 enforcement=rule check=typecheck -->
Set strict: true, plus noUncheckedIndexedAccess and noImplicitOverride for new code. Never relax a compiler option to
make an error go away; fix or narrow the type at its source. Where a suppression is unavoidable, use
@ts-expect-error with a reason, which fails once it is no longer needed, instead of @ts-ignore.

#### Run the type checker separately from the build
<!-- rule: id=TS-2 enforcement=rule check=typecheck -->
Run the type checker (tsc --noEmit, vue-tsc or the framework's check command) before calling work done and as its own
CI step. esbuild, Vite, SWC and Node's built-in type stripping remove types without checking them.

#### Lint with type-aware typescript-eslint rules
<!-- rule: id=TS-3 enforcement=rule check=lint -->
Follow the project's lint setup; for ESLint use flat config (eslint.config.js) with typescript-eslint's
type-checked presets, so rules that need type information (no-floating-promises, no-misused-promises,
no-unsafe-assignment) actually run.

## Types

Precise types make invalid states unrepresentable and let the compiler find the places a change affects.

#### Do not escape the type system
<!-- rule: id=TS-4 enforcement=rule check=lint -->
Use unknown plus narrowing (type guards, in-checks, schema parsing) instead of any, and narrow, default or throw
instead of using ! or as to silence possibly undefined or mismatched values. Allow either only at an untyped
boundary or a proven invariant, with a comment stating it.

#### Model variants as discriminated unions and switch exhaustively
<!-- rule: id=TS-5 enforcement=rule check=typecheck -->
Represent variants as unions with a literal tag ({ status: 'error'; error: Error } | { status: 'ok'; data: T })
instead of optional fields and booleans, and end each switch over them with a default that assigns the value to
never, so a new variant breaks the build wherever it must be handled.

#### Prefer readonly data, as const and satisfies
<!-- rule: id=TS-6 enforcement=guideline -->
Mark properties and arrays readonly when code must not mutate them, use as const for literal configuration and lookup
tables, and use satisfies to check an object against a type without widening its literal types.

#### Use union literals or as-const objects instead of enums
<!-- rule: id=TS-7 enforcement=rule -->
In new code prefer string-literal unions, or an as-const object with a derived type, over enum and namespace. Enums
emit runtime code, are not supported by type-stripping runtimes such as Node's built-in TypeScript support, and are
rejected under erasableSyntaxOnly.

#### Declare return types on exported functions
<!-- rule: id=TS-8 enforcement=guideline -->
Give exported functions and public methods explicit return types so the contract is stated, implementation details do
not leak through inference, and errors surface at the definition rather than at distant call sites. Inference is fine
for local helpers.

## Boundaries

Types vanish at runtime, so data entering the program must be checked where it enters.

#### Derive types from runtime schemas at I/O boundaries
<!-- rule: id=TS-9 enforcement=rule -->
Parse external input (HTTP bodies, API responses, env vars, JSON files, storage, postMessage) with the project's
schema library (zod or valibot for new code) and derive the static type from the schema (z.infer) instead of casting
the parsed result with as.

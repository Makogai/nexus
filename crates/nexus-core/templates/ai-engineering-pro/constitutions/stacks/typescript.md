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

Rules for TypeScript in frontend, backend and library code. Apply in every TypeScript project; framework documents
(NG, VUE, REACT, NODE) refine them.

## Compiler settings

The type checker only protects code it is allowed to check strictly.

#### Enable strict mode
<!-- rule: id=TS-1 enforcement=rule check=typecheck -->
Set strict: true in tsconfig and add noUncheckedIndexedAccess and noImplicitOverride for new code. Never relax a
compiler option or add @ts-ignore to make an error go away; fix or narrow the type at its source, and use
@ts-expect-error with a reason if you must.

#### Run the type checker in CI
<!-- rule: id=TS-2 enforcement=rule check=typecheck -->
Run the type checker (tsc --noEmit, vue-tsc or the framework's check command) as its own CI step that fails the
build. Bundlers and transpilers such as esbuild, Vite and SWC strip types without checking them.

## Types

Precise types make invalid states unrepresentable and let the compiler find the places a change affects.

#### Do not use any
<!-- rule: id=TS-3 enforcement=rule check=lint -->
Use unknown for values of unknown shape and narrow them with type guards, in-checks or schema parsing; allow any only
at untyped third-party boundaries with a comment. Enable @typescript-eslint/no-explicit-any: one any silently disables
checking wherever it flows.

#### Do not assert away null or type errors without proof
<!-- rule: id=TS-4 enforcement=rule -->
Avoid the non-null operator ! and as casts that silence possibly undefined or mismatched values; narrow with a check,
supply a default or throw a descriptive error. Use an assertion only where an invariant guarantees the value, with a
comment stating it.

#### Model states with discriminated unions
<!-- rule: id=TS-5 enforcement=rule -->
Represent variants as unions with a literal tag, e.g. { status: 'loading' } | { status: 'error'; error: Error } |
{ status: 'ok'; data: T }, instead of optional fields and booleans, so impossible combinations cannot exist and each
branch narrows automatically.

#### Make switches over unions exhaustive
<!-- rule: id=TS-6 enforcement=rule check=typecheck -->
End every switch over a union with a default branch that assigns the value to never (const unreachable: never =
value) or calls an assertNever helper, so adding a variant breaks the build at every place that must handle it.

#### Prefer readonly data, as const and satisfies
<!-- rule: id=TS-7 enforcement=guideline -->
Mark properties and arrays readonly when code must not mutate them, use as const for literal configuration and lookup
tables, and use satisfies to check an object against a type without widening its literal types.

#### Use union literals or as-const objects instead of enums
<!-- rule: id=TS-8 enforcement=rule -->
Prefer string-literal unions, or an as-const object with a derived type, over enum and namespace. Enums emit runtime
code, are not supported by type-stripping runtimes such as Node's built-in TypeScript support, and are rejected under
erasableSyntaxOnly.

#### Declare return types on exported functions
<!-- rule: id=TS-9 enforcement=rule -->
Give exported functions and public methods explicit return types so the contract is stated, implementation details do
not leak through inference, and errors surface at the definition rather than at distant call sites. Inference is fine
for local helpers.

## Boundaries

Types vanish at runtime, so data entering the program must be checked where it enters.

#### Validate data at I/O boundaries at runtime
<!-- rule: id=TS-10 enforcement=rule -->
Parse every external input (HTTP bodies, API responses, env vars, JSON files, storage, postMessage) with a schema
library such as zod or valibot and derive the static type from the schema (z.infer) instead of casting the result
with as.

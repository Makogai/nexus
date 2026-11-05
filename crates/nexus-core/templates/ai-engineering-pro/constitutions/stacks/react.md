---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: React
status: active
applies: on-demand
prefix: REACT
stacks: [react, nextjs]
created: {{now}}
updated: {{now}}
source: import
---

Rules for React 19 (function components, hooks, React Compiler), including the Next.js App Router. Apply to React and
Next.js code; they refine FE and TS.

## Components and hooks

React stays predictable when components are pure and effects are the exception rather than the tool for everything.

#### Write pure function components
<!-- rule: id=REACT-1 enforcement=rule -->
Write function components; add a class only for an error boundary where the project uses no library such as
react-error-boundary. Keep render pure (same props and state, same JSX, no side effects), and pass ref as a regular
prop instead of wrapping new components in forwardRef.

#### Follow the rules of hooks
<!-- rule: id=REACT-2 enforcement=rule check=lint -->
Call hooks only at the top level of components and custom hooks, never in conditions, loops or after early returns.
Keep eslint-plugin-react-hooks' recommended rules on as errors and never silence exhaustive-deps; restructure the code
instead.

#### Derive state instead of syncing it with effects
<!-- rule: id=REACT-3 enforcement=rule -->
Compute values from props and state during render instead of copying them into state with an effect; keep the minimal
state and derive the rest. To reset state when an identity changes, render the component with a different key rather
than clearing it in an effect.

#### Use effects only to synchronise with external systems
<!-- rule: id=REACT-4 enforcement=rule -->
Use useEffect only to sync with systems outside React (subscriptions, timers, browser APIs, non-React widgets) and
return a cleanup. Handle user actions in event handlers, and load server data through the data layer (FE-4), Server
Components or route loaders, never in effects with hand-rolled loading flags.

#### Key list items by stable ids
<!-- rule: id=REACT-5 enforcement=rule -->
Key list items by a stable, unique id from the data; never by array index for lists that reorder, filter or insert,
and never by a random value. Wrong keys make React reuse state and DOM nodes between different items.

#### Let the React Compiler memoise
<!-- rule: id=REACT-6 enforcement=guideline -->
Where the React Compiler is enabled, write plain code and add useMemo, useCallback or memo only as an escape hatch
(e.g. a value an effect depends on); do not strip existing memoisation as a drive-by change. Without the compiler,
memoise only where the Profiler shows a costly re-render.

## Forms

Forms carry most of an app's edge cases; established libraries and React 19 actions already handle them.

#### Give each form field one source of truth
<!-- rule: id=REACT-7 enforcement=rule -->
Use the project's form approach (e.g. React Hook Form, or React 19 form actions with useActionState and
useFormStatus), validate with a schema as FE-7 describes, and never mix controlled and uncontrolled state for one
field.

## Next.js App Router

The server/client split is a security and performance boundary, not only a rendering detail.

#### Keep the server/client boundary deliberate
<!-- rule: id=REACT-8 enforcement=rule -->
Components are Server Components by default; add 'use client' only to leaf components that need state, effects or
browser APIs, and pass only serialisable props across. Guard server-only modules (database, secrets) with import
'server-only'. In Next.js 15 and later, await params, searchParams, cookies() and headers().

#### Treat Server Actions and route handlers as public endpoints
<!-- rule: id=REACT-9 enforcement=rule -->
Anyone can call a Server Action or route handler directly: validate its input with a schema, authenticate and
authorise inside every action (SEC-7), and revalidate the affected paths or tags after mutations. Hiding a button is
not access control.

## Testing

Tests that use the UI the way people do survive refactors and catch accessibility gaps.

#### Test with Testing Library queries by role
<!-- rule: id=REACT-10 enforcement=rule check=test -->
Test components with React Testing Library and user-event, querying by role and accessible name
(getByRole('button', { name: 'Save' })). Use test ids only when no accessible query exists, and never assert hook
state or component internals.

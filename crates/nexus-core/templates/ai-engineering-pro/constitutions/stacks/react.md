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

Rules for React 19 (function components and hooks), including the Next.js App Router. Apply to React and Next.js code;
they refine FE and TS.

## Components and hooks

React stays predictable when components are pure and effects are the exception rather than the tool for everything.

#### Write pure function components
<!-- rule: id=REACT-1 enforcement=rule -->
Write components as function components with hooks; add no class components except an error boundary where no
library provides one. Keep render pure: the same props and state produce the same JSX, with no side effects during
render.

#### Follow the rules of hooks
<!-- rule: id=REACT-2 enforcement=rule check=lint -->
Call hooks only at the top level of components and custom hooks, never in conditions, loops or after early returns,
and list every reactive dependency. Keep eslint-plugin-react-hooks on with errors; never silence exhaustive-deps,
restructure the code instead.

#### Derive state instead of syncing it with effects
<!-- rule: id=REACT-3 enforcement=rule -->
Compute values from props and state during render instead of copying them into state with an effect; keep the minimal
state and derive the rest. To reset state when an identity changes, render the component with a different key rather
than clearing it in an effect.

#### Use effects only to synchronise with external systems
<!-- rule: id=REACT-4 enforcement=rule -->
Use useEffect only to sync with something outside React (subscriptions, timers, browser APIs, non-React widgets) and
return a cleanup. Handle user actions in event handlers, not in effects reacting to state, and never fetch in effects
when a data library or loader exists.

#### Key list items by stable ids
<!-- rule: id=REACT-5 enforcement=rule -->
Key list items by a stable, unique id from the data; never by array index for lists that reorder, filter or insert,
and never by a random value. Wrong keys make React reuse state and DOM nodes between different items.

#### Memoise only where measured or compiled
<!-- rule: id=REACT-6 enforcement=guideline -->
Do not wrap everything in useMemo, useCallback or memo. When the React Compiler is enabled, let it memoise; otherwise
add memoisation only after the Profiler shows a costly re-render, and first fix the cause (state held too high,
unstable props).

## Data and forms

Data fetching and forms carry most of an app's edge cases; established libraries already handle them.

#### Fetch data through a caching library or framework loaders
<!-- rule: id=REACT-7 enforcement=rule -->
Load server data with TanStack Query or SWR, or the framework's mechanism (Server Components, route loaders), so
caching, deduplication, race conditions and revalidation are handled; never hand-roll fetch-in-useEffect with loading
flags in app code.

#### Give each form field one source of truth and a schema
<!-- rule: id=REACT-8 enforcement=rule -->
Manage forms with a form library such as React Hook Form or with React 19 form actions and useActionState, validate
with a schema (zod or valibot) shared with the server, and show client and server field errors. Never mix controlled
and uncontrolled state for one field.

## Next.js App Router

The server/client split is a security and performance boundary, not only a rendering detail.

#### Keep the server/client boundary deliberate
<!-- rule: id=REACT-9 enforcement=rule -->
Components are Server Components by default; add 'use client' only to leaf components that need state, effects or
browser APIs, and pass only serialisable props across. Never import server-only modules (database, secrets) into
client code; guard them with import 'server-only'.

#### Treat Server Actions and route handlers as public endpoints
<!-- rule: id=REACT-10 enforcement=rule -->
Anyone can call a Server Action or route handler directly: validate its input with a schema, authenticate and
authorise inside every action (SEC-7), and revalidate the affected paths or tags after mutations. Hiding a button is
not access control.

## Testing

Tests that use the UI the way people do survive refactors and catch accessibility gaps.

#### Test with Testing Library queries by role
<!-- rule: id=REACT-11 enforcement=rule check=test -->
Test components with React Testing Library: query by role and accessible name (getByRole('button', { name: 'Save' })),
interact through user-event and assert what the user sees. Use test ids only when no accessible query exists; never
assert internal state.

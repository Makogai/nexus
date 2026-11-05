---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Frontend engineering
status: active
applies: on-demand
prefix: FE
stacks: [angular, vue, react, svelte, nextjs, nuxt]
created: {{now}}
updated: {{now}}
source: import
---

Rules for browser user interfaces: component design, state, data fetching, forms, rendering safety, styling and
frontend tests. Apply to any frontend work; the framework documents (NG, VUE, REACT) refine them and A11Y covers
accessibility.

## Components

Small, single-purpose components keep the UI predictable, reusable and testable in isolation.

#### Keep components small and split presentation from state
<!-- rule: id=FE-1 enforcement=rule -->
Give each component one responsibility and split it when it mixes unrelated concerns. Keep presentational components
free of data access (inputs in, events out) and let container components own state and side effects, so most of the
UI is reusable and easy to test.

#### Do not manipulate the DOM outside framework escape hatches
<!-- rule: id=FE-2 enforcement=rule -->
Change the UI through state and bindings, not querySelector, manual class toggling or appendChild. Use the framework's
escape hatch (template refs, ElementRef, useRef) only for focus, measurement or third-party widgets, and remove
listeners and instances on unmount.

## State and data

Most frontend bugs come from duplicated state and hand-rolled data fetching.

#### Keep state local until it is truly shared
<!-- rule: id=FE-3 enforcement=rule -->
Hold state in the component that uses it, lift it to the nearest common parent when siblings need it, and move it into
a shared store only when distant parts of the app read and write it. Never copy server data or derivable values into
a store.

#### Manage server state with a caching data layer
<!-- rule: id=FE-4 enforcement=rule -->
Fetch remote data through a layer that caches, deduplicates, revalidates and exposes loading and error state (e.g.
TanStack Query, SWR, framework data loaders) instead of ad-hoc fetch calls in components. After a mutation, invalidate
the affected queries rather than patching copies by hand.

#### Call the backend only through a typed API client
<!-- rule: id=FE-5 enforcement=rule -->
Route all HTTP calls through one typed client module, ideally generated from the OpenAPI or GraphQL schema, that owns
base URL, auth headers, timeouts and error mapping. Components never build URLs or parse raw responses, so API changes
fail at compile time.

#### Render loading, empty and error states explicitly
<!-- rule: id=FE-6 enforcement=rule -->
Every data-driven view shows a distinct loading state, an empty state with a next action and an error state with a
retry; never a blank screen or endless spinner. Model the request as one discriminated value (idle, loading, success,
error) rather than separate booleans.

## Forms

Forms are where users and untrusted input meet the application.

#### Validate on the client for UX and on the server for truth
<!-- rule: id=FE-7 enforcement=rule -->
Validate input in the browser for immediate feedback, ideally from a schema shared with the backend, but treat server
validation as authoritative and map its field errors back onto the form. Client-side checks never provide security or
integrity.

## Rendering safety

Injecting markup is the most common way untrusted data becomes script execution in the browser.

#### Never render untrusted HTML without sanitising it
<!-- rule: id=FE-8 enforcement=mandatory -->
Never pass user- or API-supplied HTML to innerHTML, v-html, [innerHTML], dangerouslySetInnerHTML or {@html} unless it
was sanitised by a maintained sanitiser such as DOMPurify configured for that context. Prefer rendering text or
structured content so no raw HTML is needed.

## Delivery and styling

What ships to the browser decides load time, consistency and how easily the UI can change.

#### Lazy-load routes and heavy features
<!-- rule: id=FE-9 enforcement=rule -->
Split code at route level and load heavy, rarely used features (rich editors, charts, maps) on demand, so the initial
bundle carries only what the first screen needs. Check the bundle analyser when you add a dependency.

#### Style through design tokens and one styling approach
<!-- rule: id=FE-10 enforcement=rule -->
Use one styling approach per app (e.g. CSS modules, scoped styles or utility classes) and take colours, spacing,
typography and radii from design tokens exposed as CSS custom properties, never literal values, so theming and dark
mode change in one place.

#### Keep user-facing strings ready for translation
<!-- rule: id=FE-11 enforcement=rule -->
Route all visible text, including aria labels and error messages, through the i18n layer with message keys and ICU
plurals. Never concatenate translated fragments, and format dates, numbers and currencies with the Intl APIs for the
user's locale.

#### Stay within the performance budgets
<!-- rule: id=FE-12 enforcement=guideline -->
Check every change that adds dependencies, images, fonts or client-side rendering against the budgets defined in PERF,
measured on a production build rather than the dev server; do not restate or loosen the numbers here.

## Testing

UI tests are valuable only when they exercise the UI the way a person uses it.

#### Test components through user interactions
<!-- rule: id=FE-13 enforcement=rule check=test -->
Cover components with tests that render them, interact as a user would (click, type, keyboard) and assert visible
output and emitted events, querying by role and accessible name rather than CSS classes. Cover critical journeys end
to end with a browser runner such as Playwright.

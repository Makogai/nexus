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

Rules for browser user interfaces: components, state, data fetching, forms, rendering safety, styling, localisation
and component tests. Apply to any web frontend work; NG, VUE and REACT refine them, A11Y covers accessibility and
PERF the budgets and bundle size.

## Components

Components that only render what they are given stay predictable, reusable and testable in isolation.

#### Keep reusable components free of data access
<!-- rule: id=FE-1 enforcement=rule -->
Keep reusable UI components free of data fetching, global stores and routing (data in through props or inputs,
changes out through events) and let route-level or feature components own state and side effects, so most of the UI
renders and tests in isolation.

#### Do not manipulate the DOM outside framework escape hatches
<!-- rule: id=FE-2 enforcement=rule -->
Change the UI through state and bindings, not querySelector, manual class toggling or appendChild. Use the framework's
escape hatch (template refs, ElementRef, useRef) only for focus, measurement or third-party widgets, and tear down
what you create there on unmount.

## State and data

Most frontend bugs come from duplicated state and hand-rolled data fetching.

#### Keep state local until it is truly shared
<!-- rule: id=FE-3 enforcement=rule -->
Hold state in the component that uses it, lift it to the nearest common parent when siblings need it, and move it into
a shared store only when distant parts of the app read and write it. Never copy server data or derivable values into
a store.

#### Manage server state with a caching data layer
<!-- rule: id=FE-4 enforcement=rule -->
Fetch remote data through the project's caching data layer (e.g. TanStack Query, SWR, framework loaders or resources)
that deduplicates, revalidates and exposes loading and error state, not through ad-hoc fetch calls in components.
After a mutation, invalidate the affected queries rather than patching copies by hand.

#### Call the backend only through a typed API client
<!-- rule: id=FE-5 enforcement=rule -->
Route all HTTP calls through one typed client module, generated from the OpenAPI or GraphQL schema where one exists,
that owns base URL, auth headers, timeouts and error mapping. Components never build URLs or parse raw responses, so
API changes fail at compile time.

#### Render loading, empty and error states explicitly
<!-- rule: id=FE-6 enforcement=rule -->
Every data-driven view shows a distinct loading state, an empty state with a next action and an error state with a
retry; never a blank screen or an endless spinner. Keep previously loaded data visible while refetching.

## Forms

Forms are where users and untrusted input meet the application.

#### Validate on the client for UX and on the server for truth
<!-- rule: id=FE-7 enforcement=rule -->
Validate input in the browser for immediate feedback, ideally with a schema shared with the backend, but treat server
validation as authoritative and show its field errors on the form. Client-side checks never provide security or
integrity.

## Rendering safety

Injected markup and URLs are the most common way untrusted data becomes script execution in the browser.

#### Never render untrusted HTML or URLs without sanitising them
<!-- rule: id=FE-8 enforcement=mandatory -->
Never pass user- or API-supplied HTML to innerHTML, v-html, dangerouslySetInnerHTML, {@html} or Angular's
bypassSecurityTrust* unless a maintained sanitiser such as DOMPurify cleaned it, and allow only http(s), mailto or
relative URLs from such data in href and src. Prefer rendering text or structured content.

#### Never put secrets in client code
<!-- rule: id=FE-9 enforcement=rule -->
Everything shipped to the browser is public, including build-time variables such as VITE_*, NEXT_PUBLIC_* and
Angular environment files. Never put API secrets, private keys or privileged tokens there; call a backend that holds
them (SEC-1).

## Styling and localisation

Consistent styling and translatable text keep the UI changeable in one place.

#### Style through design tokens and one styling approach
<!-- rule: id=FE-10 enforcement=rule -->
Follow the app's one styling approach (e.g. CSS modules, scoped styles or utility classes) and take colours, spacing,
typography and radii from its design tokens or CSS custom properties instead of literal values, so theming and dark
mode change in one place.

#### Keep user-facing text translatable and locale-aware
<!-- rule: id=FE-11 enforcement=rule -->
In apps that are or will be localised, route all visible text, including aria labels and error messages, through the
i18n layer with message keys and ICU plurals, and never concatenate translated fragments. Always format dates,
numbers and currencies with Intl for the user's locale.

## Testing

UI tests are valuable only when they exercise the UI the way a person uses it.

#### Test components through user interactions
<!-- rule: id=FE-12 enforcement=rule check=test -->
Test components by rendering them, interacting as a user would (click, type, keyboard) and asserting visible output
and emitted events, querying by role and accessible name rather than CSS classes. End-to-end coverage of critical
journeys follows TST.

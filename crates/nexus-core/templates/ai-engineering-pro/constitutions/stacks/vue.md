---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Vue
status: active
applies: on-demand
prefix: VUE
stacks: [vue, nuxt]
created: {{now}}
updated: {{now}}
source: import
---

Rules for Vue 3.5 and later (Composition API, `<script setup>`, Pinia), including Nuxt. Apply to Vue and Nuxt code;
they refine FE and TS.

## Components

Typed single-file components with one-way data flow keep Vue apps predictable.

#### Use script setup with TypeScript
<!-- rule: id=VUE-1 enforcement=rule -->
Write single-file components with `<script setup lang="ts">` and the Composition API; do not add Options API
components or mixins in new code. Script setup is the documented default, gives the best type inference and compiles
to smaller output.

#### Type props and emits with type-based declarations
<!-- rule: id=VUE-2 enforcement=rule check=typecheck -->
Declare props with `defineProps<Props>()` (defaults for optional props via reactive destructuring), events with
`defineEmits<Emits>()` and two-way bindings with defineModel(), and run vue-tsc so wrong props and emitted payloads
fail the typecheck.

#### Pass props down and emit events up
<!-- rule: id=VUE-3 enforcement=rule -->
Never mutate a prop or reach into a parent or child instance ($parent, internals behind template refs); emit an event,
use defineModel or the child's defineExpose API. Use provide/inject with a typed InjectionKey only for deep
app-structure dependencies, not to bypass props.

#### Give every v-for a stable key
<!-- rule: id=VUE-4 enforcement=rule check=lint -->
Bind :key on every v-for to a stable unique id from the data, never the index for lists that reorder, insert or
delete, and never combine v-if and v-for on one element; filter in a computed instead. eslint-plugin-vue flags both.

## Reactivity and state

Derived state and narrowly scoped stores avoid the synchronisation bugs that watchers and global state invite.

#### Prefer computed over watchers
<!-- rule: id=VUE-5 enforcement=rule -->
Derive values with computed() instead of a watcher that writes another ref; computed is cached and cannot drift out of
sync. Use watch or watchEffect only for side effects (fetching, storage, non-Vue APIs) and clean up with
onWatcherCleanup or the onCleanup argument.

#### Extract reusable logic into composables
<!-- rule: id=VUE-6 enforcement=rule -->
Put reusable stateful logic in useXxx() composables that accept refs, getters or plain values (normalised with
toValue()) and return refs, and call them synchronously in setup. Keep them free of component-specific DOM
assumptions so they can be tested without mounting.

#### Use a store only for shared state
<!-- rule: id=VUE-7 enforcement=rule -->
Keep state local or in a composable until distant components share it, then use the project's store (Pinia setup
stores for new code), one per domain. Read state with storeToRefs() to keep reactivity, and keep server data in the
data layer (FE-4) instead of mirroring it in stores.

## Loading and data

Code splitting and SSR-aware fetching decide how fast a Vue or Nuxt page becomes usable.

#### Split code with lazy routes and async components
<!-- rule: id=VUE-8 enforcement=rule -->
Lazy-load routes with () => import('./views/Orders.vue') in vue-router and heavy components with
defineAsyncComponent, including loading and error components. In Nuxt, rely on per-page splitting and prefix heavy
components with Lazy.

#### Fetch page data with useFetch or useAsyncData in Nuxt
<!-- rule: id=VUE-9 enforcement=rule -->
In Nuxt, load page data in setup with useFetch or useAsyncData (with a unique key) so it is fetched once on the server
and reused on the client; calling $fetch directly in setup fetches twice. Use $fetch for event-driven calls such as
form submits.

## Testing

Component tests should exercise the rendered DOM and the component's public contract.

#### Test with Vitest and Vue Test Utils
<!-- rule: id=VUE-10 enforcement=rule check=test -->
Test components with Vitest and Vue Test Utils or Testing Library for Vue, asserting rendered output and emitted()
events, and use createTestingPinia() for components that depend on stores.

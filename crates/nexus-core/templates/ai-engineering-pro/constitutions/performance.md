---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Performance
status: active
applies: on-demand
prefix: PERF
created: {{now}}
updated: {{now}}
source: import
---

Performance rules for back ends and front ends: measurement, budgets, data access, runtime behaviour, delivery and regression checks. Apply them when touching hot paths, data access, remote calls, UI rendering or bundle contents, and before launches.

## Measure and budget

Optimisation without measurement and targets wastes effort and adds complexity.

#### Measure before optimising
<!-- rule: id=PERF-1 enforcement=rule -->
Optimise only after profiling or benchmarking shows where time or memory goes, using production-like data and builds. Record the baseline and the result in the change; keep an optimisation only if it gives a measurable gain on a path that matters.

#### Meet Core Web Vitals budgets for web front ends
<!-- rule: id=PERF-2 enforcement=rule -->
Web applications meet Largest Contentful Paint at most 2.5 s, Interaction to Next Paint at most 200 ms and Cumulative Layout Shift at most 0.1, at the 75th percentile of real users, mobile and desktop separately. Use field data (RUM) where the app collects it, and check lab results for significant changes.

#### Define latency budgets per API endpoint
<!-- rule: id=PERF-3 enforcement=rule -->
Give each interactive API endpoint or critical operation an explicit latency target, e.g. p95 under 300 ms for reads, agreed with its consumers and monitored in production. Design data access and fan-out to fit the budget, and treat a breached budget as a defect.

## Data access

Most back-end latency comes from how often and how much data is fetched.

#### Eliminate N+1 queries
<!-- rule: id=PERF-4 enforcement=rule -->
Load related data with joins, eager loading or batching (e.g. `IN` queries, DataLoader) instead of one query per item in a loop. A list endpoint should issue a constant number of queries regardless of page size; verify the count with query logging or a test assertion.

#### Paginate every unbounded collection
<!-- rule: id=PERF-5 enforcement=rule -->
Never return or load an unbounded result set. Paginate list endpoints with a maximum page size, prefer cursor (keyset) pagination for large or frequently changing data, and stream or batch exports and background jobs instead of loading everything into memory.

#### Cache only with an explicit invalidation strategy
<!-- rule: id=PERF-6 enforcement=rule -->
Before adding a cache, define what is cached, the key, the time to live, how entries are invalidated when the source changes, and the acceptable staleness. Bound the cache size, protect hot keys against stampedes, and measure the hit rate; remove caches that do not pay off.

## Runtime behaviour

Blocking, unbounded growth and unbounded waits turn slowness into outages.

#### Use non-blocking I/O on request paths
<!-- rule: id=PERF-7 enforcement=rule -->
In async runtimes (Node.js, Python asyncio, Rust tokio), never run blocking file, network or CPU-heavy work on the event loop; use the async API or a worker pool (e.g. `spawn_blocking`, worker threads). Run independent remote calls concurrently with a concurrency limit rather than one after another.

#### Keep the UI thread free
<!-- rule: id=PERF-8 enforcement=rule -->
Never run work that may take longer than about 50 ms on the browser main thread or a UI thread: move it to a Web Worker, isolate, background thread or the server, split it into chunks, or virtualise long lists. Show progress for anything that takes longer than one second.

#### Bound memory and check for leaks
<!-- rule: id=PERF-9 enforcement=rule -->
Give every in-memory buffer, queue, cache and collection a size limit. Remove event listeners, timers and subscriptions when their owner is disposed. For long-running processes and single-page apps, compare heap snapshots or memory metrics over a sustained run before release.

#### Mind algorithmic complexity on hot paths
<!-- rule: id=PERF-10 enforcement=rule -->
On code that runs per request, per item or per frame, avoid nested loops over growing collections and repeated linear searches; use maps, sets, sorting or precomputation. State the expected input size; quadratic code is acceptable only when n is provably small.

#### Set timeouts on every remote call
<!-- rule: id=PERF-11 enforcement=rule -->
Every HTTP, database, cache and queue call has explicit connect and overall timeouts, shorter than the caller's own deadline; never rely on library defaults, which are often unlimited. Propagate the remaining deadline to downstream calls where the stack supports it.

## Front-end delivery

Every kilobyte of JavaScript delays interactivity on slower devices and networks.

#### Enforce bundle size budgets and split code
<!-- rule: id=PERF-12 enforcement=rule -->
Set a compressed size budget for the initial JavaScript and CSS and fail the build when it is exceeded. Lazy-load routes and heavy components, import only what you use from libraries, and check a bundle analyser before adding a large dependency.

## Verification

Performance regresses silently unless something checks it.

#### Load-test critical paths before launch
<!-- rule: id=PERF-13 enforcement=rule -->
Before launching or significantly changing a critical path, load-test it at expected peak plus headroom (e.g. twice the peak) with a tool such as k6, Gatling or Locust, in a production-like environment the person designates, never against production or third-party services without approval. Verify latency, error rate and saturation.

#### Catch performance regressions in CI where feasible
<!-- rule: id=PERF-14 enforcement=guideline -->
Add automated checks for performance-sensitive code: benchmarks with a regression threshold, Lighthouse CI budgets, bundle size checks or query-count assertions. Run them on a stable runner and compare against the default branch baseline rather than absolute numbers where noise is high.

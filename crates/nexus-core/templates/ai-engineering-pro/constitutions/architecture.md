---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Software architecture
status: active
applies: on-demand
prefix: ARC
created: {{now}}
updated: {{now}}
source: import
---

Structural rules for modules, services, contracts, resilience and observability. Apply them when designing a feature, adding a module, service, datastore or integration, or changing how components communicate.

## Boundaries and dependencies

Clear boundaries keep the system understandable, testable and changeable in parts.

#### Keep the domain independent of frameworks and I/O
<!-- rule: id=ARC-1 enforcement=rule -->
Organise code into modules with explicit public interfaces and a one-way dependency direction: UI, transport and persistence depend on the domain, never the reverse. Domain code does not import web frameworks, ORMs, HTTP clients or file APIs, so it can be tested and reused without them.

#### Define interfaces at every boundary
<!-- rule: id=ARC-2 enforcement=rule -->
Where a module talks to a database, external service, queue or another module, define a narrow interface (port) in the consumer's terms and implement it in an adapter. Callers depend on the interface, which keeps vendors replaceable and allows fakes in tests.

#### Keep one source of truth per fact
<!-- rule: id=ARC-3 enforcement=rule -->
Every piece of data has exactly one authoritative owner (a module, service or store); other copies are caches or projections derived from it and rebuildable. Document which component owns which data, and never let two components write the same fact independently.

#### Avoid shared mutable state across components
<!-- rule: id=ARC-4 enforcement=rule -->
Components communicate through explicit calls, messages or events, not by reading and writing the same tables, global variables or files. A component's database schema is private to it; other components reach its data only through its interface.

#### Start with a modular monolith
<!-- rule: id=ARC-5 enforcement=guideline -->
Begin with a single deployable unit with strictly enforced module boundaries. Extract a separate service only for a concrete need such as independent scaling, a different release cadence, fault isolation or team ownership, and accept the added network, consistency and operational cost explicitly.

## Resilience and contracts

Distributed calls fail in partial and surprising ways, and contracts outlive their authors.

#### Design for the failure of every dependency
<!-- rule: id=ARC-6 enforcement=rule -->
Assume every remote dependency can be slow, down or wrong. Retry only transient failures, with exponential backoff, jitter and a capped number of attempts; add circuit breaking or a fallback where an outage must not cascade; and define the degraded behaviour users see.

#### Make retried operations idempotent
<!-- rule: id=ARC-7 enforcement=rule -->
Any operation a client, queue or scheduler may repeat must have the same effect when repeated: use idempotency keys for creates and payments, upserts or conditional writes, and deduplicate messages by id. Assume at-least-once delivery for queues and webhooks.

#### Evolve contracts compatibly and version breaking changes
<!-- rule: id=ARC-8 enforcement=rule -->
Change APIs, events and message schemas additively: new optional fields, tolerant readers that ignore unknown fields, no repurposed fields. For an unavoidable break, run a new version beside the old one, migrate consumers, then retire the old version on an announced date. Use contract tests to catch breaks.

## Operability

A system that cannot be configured safely or observed in production cannot be operated.

#### Externalise configuration and secrets
<!-- rule: id=ARC-9 enforcement=rule -->
Build one artifact and promote it unchanged through all environments. Inject everything that varies (endpoints, limits, flags) at runtime through environment or configuration, and take secrets from a secret manager or the platform's secret store, never from the artifact or the repository.

#### Build observability in from the start
<!-- rule: id=ARC-10 enforcement=rule -->
Emit structured logs (level, timestamp, message, context fields), metrics for rate, errors and duration of every endpoint and dependency, and distributed traces. Propagate a correlation or trace id (W3C Trace Context) across every call and include it in logs; prefer OpenTelemetry for instrumentation.

## Decisions and evolution

Architecture should change on purpose, in steps that can be undone.

#### Decide architecture explicitly before implementing it
<!-- rule: id=ARC-11 enforcement=rule -->
Before introducing a new service, datastore, framework, integration style or cross-cutting pattern, write an ADR listing the options considered, their trade-offs and the decision, and have it reviewed. Do not let significant architecture emerge implicitly from a single pull request.

#### Prefer reversible, incremental change over big-bang rewrites
<!-- rule: id=ARC-12 enforcement=rule -->
Restructure systems incrementally (strangler fig, branch by abstraction, parallel run, feature flags) so each step is deployable and can be rolled back. Give irreversible decisions such as data models, public contracts and vendor lock-in more scrutiny than reversible ones.

#### Protect architectural rules with automated fitness checks
<!-- rule: id=ARC-13 enforcement=guideline -->
Encode important architectural properties as checks that run in CI: dependency rules between modules (e.g. ArchUnit, dependency-cruiser, import-linter), API compatibility checks, performance budgets. When a check fails, fix the code or change the rule through an ADR, never silence it.

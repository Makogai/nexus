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

Structural rules for modules, services, data ownership, contracts, resilience and observability. Apply them when designing a feature, adding a module, service, datastore or integration, or changing how components communicate; ENG covers general compatibility and configuration.

## Boundaries and dependencies

Clear boundaries keep the system understandable, testable and changeable in parts.

#### Keep business logic independent of frameworks and I/O
<!-- rule: id=ARC-1 enforcement=rule -->
Dependencies point one way: UI, transport and persistence depend on the business logic, never the reverse. Where code carries real business rules, keep web frameworks, ORMs, HTTP clients and file APIs out of it so it can be tested without them; simple CRUD may use the framework directly.

#### Put external systems behind narrow interfaces where it pays
<!-- rule: id=ARC-2 enforcement=rule -->
Reach databases, external services and queues through a narrow interface in the consumer's terms, implemented by an adapter, when tests need a fake or the provider may change. Do not add an interface that has one implementation and no such need (ENG-3).

#### Give every piece of data exactly one owner
<!-- rule: id=ARC-3 enforcement=rule -->
Each fact has one authoritative owner (module, service or store); other copies are rebuildable caches or projections. Other components read or change it only through the owner's interface or events, never by sharing its tables, files or global variables. Document which component owns which data.

#### Start with a modular monolith
<!-- rule: id=ARC-4 enforcement=guideline -->
Begin with a single deployable unit with enforced module boundaries. Extract a separate service only for a concrete need such as independent scaling, a different release cadence, fault isolation or team ownership, and accept the added network, consistency and operational cost explicitly.

## Resilience and contracts

Distributed calls fail in partial and surprising ways, and contracts outlive their authors.

#### Design for the failure of every dependency
<!-- rule: id=ARC-5 enforcement=rule -->
Assume every remote dependency can be slow, down or wrong. Give each call a timeout (PERF-11), retry only transient failures with exponential backoff, jitter and capped attempts, add circuit breaking or a fallback where an outage must not cascade, and define the degraded behaviour users see.

#### Make retried operations idempotent
<!-- rule: id=ARC-6 enforcement=rule -->
Any operation a client, queue or scheduler may repeat must have the same effect when repeated: use idempotency keys for creates and payments, upserts or conditional writes, and deduplicate messages by id. Assume at-least-once delivery for queues and webhooks.

#### Keep contracts between independently deployed parts tolerant
<!-- rule: id=ARC-7 enforcement=rule -->
Beyond ENG-9, for APIs, events and messages between independently deployed parts: readers ignore unknown fields, fields are never repurposed, and a breaking version runs beside the old one until consumers have migrated and an announced retirement date has passed. Catch breaks with contract tests.

## Operability

A service that cannot be observed in production cannot be operated.

#### Build observability into services
<!-- rule: id=ARC-8 enforcement=rule -->
Services emit structured logs (level, timestamp, message, context fields), metrics for rate, errors and duration of each endpoint and dependency, and traces. Propagate a trace id (W3C Trace Context) across calls and include it in logs; prefer OpenTelemetry for instrumentation.

## Decisions and evolution

Architecture should change on purpose, in steps that can be undone.

#### Decide architecture with the person before implementing it
<!-- rule: id=ARC-9 enforcement=rule -->
Introduce a new service, datastore, framework, messaging or integration style, or cross-cutting pattern only after presenting the options and trade-offs and getting the person's decision, recorded as an ADR (DOC-4). Significant architecture must not emerge implicitly from a single change.

#### Prefer reversible, incremental change over big-bang rewrites
<!-- rule: id=ARC-10 enforcement=rule -->
Restructure incrementally (strangler fig, branch by abstraction, parallel run, feature flags) so each step is deployable and can be rolled back. Give irreversible decisions such as data models, public contracts and vendor lock-in more scrutiny than reversible ones.

#### Protect architectural rules with automated fitness checks
<!-- rule: id=ARC-11 enforcement=guideline -->
Encode important architectural properties as CI checks: dependency rules between modules (e.g. ArchUnit, dependency-cruiser, import-linter), API compatibility checks, performance budgets. When a check fails, fix the code or change the rule deliberately with the person's agreement; never silence it.

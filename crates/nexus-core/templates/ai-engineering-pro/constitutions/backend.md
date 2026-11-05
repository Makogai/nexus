---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Backend engineering
status: active
applies: on-demand
prefix: BE
stacks: [node, nestjs, express, laravel, php, python, django, fastapi, flask, go, dotnet, jvm, ruby, rust]
created: {{now}}
updated: {{now}}
source: import
---

Rules for server-side services and HTTP APIs: API contracts, request handling, background jobs, health checks and
shutdown. Apply when you build or change backend code; stack documents refine them, SEC holds the mandatory security
rules, and ARC and PERF cover resilience, observability, timeouts and data-access performance.

## API design

An API is a contract with clients you do not control; consistency makes it predictable and safe to evolve.

#### Design resource-oriented APIs with correct methods and statuses
<!-- rule: id=BE-1 enforcement=rule -->
For HTTP APIs, model endpoints as resources (/orders/{id}): GET reads safely, POST creates, PUT or PATCH updates,
DELETE removes. Return precise statuses: 201 with Location on create, 204 without body, 400/422 invalid input, 404,
409 conflict, 429 with Retry-After when rate-limited; never 200 with an error.

#### Use one error format, by default RFC 9457 problem details
<!-- rule: id=BE-2 enforcement=rule -->
Return every error in one documented format: the project's existing one, or for new APIs application/problem+json
(RFC 9457) with type, title, status and detail plus a field-level errors extension for validation failures. Error
bodies never carry internal details (SEC).

#### Bound every collection endpoint
<!-- rule: id=BE-3 enforcement=rule -->
Give every list endpoint a default and a maximum page size, return opaque cursor tokens rather than raw offsets for
large or changing data, and allow filtering and sorting only on allowlisted, indexed fields, so no client can trigger
an unbounded scan.

#### Version public APIs and announce deprecations
<!-- rule: id=BE-4 enforcement=rule -->
For APIs whose clients are not deployed together with the server, choose one versioning scheme (URL prefix such as
/v1 or a header) and document it. Ship breaking changes only in a new version and announce removals with Deprecation
and Sunset headers and a migration period; compatible evolution follows ARC.

#### Accept idempotency keys for retried unsafe operations
<!-- rule: id=BE-5 enforcement=rule -->
For POST operations clients may retry (payments, orders, bookings), accept an Idempotency-Key header, store the key
with the outcome and return the stored response for repeats instead of acting twice. Design PUT and DELETE to be
idempotent by nature.

## Request handling

Every request crosses a trust boundary; checks belong at that boundary and in a predictable place.

#### Authenticate in middleware, authorise per resource and field
<!-- rule: id=BE-6 enforcement=rule -->
Authenticate every request in shared middleware, then authorise the specific action on the specific resource in the
handler or a policy (the mandatory rule is SEC-7). Bind input to explicit request types so clients cannot set fields
they may not change, such as owner, role or price (mass assignment).

#### Keep transaction boundaries in the service layer
<!-- rule: id=BE-7 enforcement=rule -->
Open one transaction per use case in the service layer, not in controllers or repositories. When a database change
must also publish a message or event, write both through a transactional outbox instead of calling the broker inside
or after the transaction. Transaction length and isolation follow DB.

## Background work

Slow or retryable work does not belong in the request that triggered it.

#### Move slow work to background jobs
<!-- rule: id=BE-8 enforcement=rule -->
Run work that is slow, retryable or not needed for the response (email, exports, webhooks, media processing) as
queued jobs. Make jobs idempotent, retry with exponential backoff up to a maximum attempt count, and send exhausted
jobs to a monitored dead-letter queue.

## Operations

A service must tell its platform whether it can take traffic, and stop without losing work.

#### Expose separate liveness and readiness checks
<!-- rule: id=BE-9 enforcement=rule -->
Provide a cheap liveness endpoint that only shows the process is responsive and a readiness endpoint that checks
required dependencies with short timeouts. Never make liveness depend on downstream services, or one outage restarts
every instance.

#### Shut down gracefully
<!-- rule: id=BE-10 enforcement=rule -->
On SIGTERM stop accepting new work, report not ready, finish in-flight requests and jobs within a deadline shorter
than the orchestrator's grace period, then close pools and exit. Work interrupted at the deadline must be safe to
retry.

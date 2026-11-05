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

Rules for server-side services and HTTP APIs: API design, request handling, background work, resilience,
observability and operations. Apply when you build or change backend code; stack documents refine them and SEC holds
the mandatory security rules.

## API design

An API is a contract with clients you do not control; consistency makes it predictable and safe to evolve.

#### Design resource-oriented APIs with correct methods and statuses
<!-- rule: id=BE-1 enforcement=rule -->
Model endpoints as resources (/orders/{id}) and use GET for safe reads, POST to create, PUT or PATCH to update and
DELETE to remove. Return precise statuses: 201 with Location on create, 204 without body, 400/422 invalid input, 404,
409 conflict; never 200 with an error.

#### Use one error format: RFC 9457 problem details
<!-- rule: id=BE-2 enforcement=rule -->
Return every error as application/problem+json (RFC 9457) with type, title, status and detail, plus a field-level
errors extension for validation failures. Never expose stack traces, SQL or internal hostnames; log them server-side
with the request id instead.

#### Bound every collection endpoint
<!-- rule: id=BE-3 enforcement=rule -->
Paginate every list endpoint with a default and a maximum page size, prefer opaque cursor tokens for large or changing
data, and allow filtering and sorting only on allowlisted, indexed fields so a client cannot trigger unbounded scans.

#### Version the API and evolve it compatibly
<!-- rule: id=BE-4 enforcement=rule -->
Choose one versioning scheme (URL prefix such as /v1 or a header) and document it. Within a version make only additive
changes; removed fields, new required inputs or changed semantics need a new version and an announced deprecation
period with Deprecation and Sunset headers.

#### Accept idempotency keys for retried unsafe operations
<!-- rule: id=BE-5 enforcement=rule -->
For POST operations clients may retry (payments, orders, bookings), accept an Idempotency-Key header, store the key
with the outcome and return the stored response for repeats instead of acting twice. Design PUT and DELETE to be
idempotent by nature.

## Request handling

Every request crosses a trust boundary; checks belong at that boundary and in a predictable place.

#### Validate every request at the boundary
<!-- rule: id=BE-6 enforcement=rule -->
Parse path, query, headers and body against an explicit schema at the edge of the service, rejecting unknown fields
and enforcing types, ranges and lengths before business logic runs. Inner layers then receive typed, trusted values
and never re-parse raw input.

#### Authenticate in middleware and authorise per resource
<!-- rule: id=BE-7 enforcement=rule -->
Authenticate every request in shared middleware, then check authorisation for the specific resource and action in the
handler or a policy, including ownership (object-level checks). The mandatory access-control rule is SEC-7; this
rule fixes where the checks live.

#### Keep transaction boundaries explicit and short
<!-- rule: id=BE-8 enforcement=rule -->
Open one transaction per use case in the service layer, not in controllers or repositories, and keep remote calls and
file I/O out of it. When a database change and a message must both happen, use a transactional outbox. Isolation and
locking are covered in DB.

#### Rate-limit clients
<!-- rule: id=BE-9 enforcement=rule -->
Apply rate limits per client, token or IP on public and expensive endpoints (login, search, exports), answer with 429
and Retry-After when exceeded, and keep the limits in configuration so they can be tuned without a deploy.

## Background work and resilience

Dependencies fail and slow down; a service must degrade predictably instead of hanging or duplicating work.

#### Move slow work to background jobs
<!-- rule: id=BE-10 enforcement=rule -->
Run work that is slow, retryable or not needed for the response (email, exports, webhooks, media processing) as
queued jobs. Make jobs idempotent, retry with exponential backoff up to a maximum attempt count, and send exhausted
jobs to a monitored dead-letter queue.

#### Give every outbound call a timeout and a retry policy
<!-- rule: id=BE-11 enforcement=rule -->
Set connect and total timeouts on every HTTP, database and queue client; library defaults are often infinite. Retry
only idempotent calls on transient errors, with exponential backoff, jitter and a capped budget, and stop hammering a
failing dependency with a circuit breaker.

## Operations

A service is only as good as your ability to see, configure and stop it safely.

#### Log structured events with a correlation id
<!-- rule: id=BE-12 enforcement=rule -->
Emit JSON logs with level, timestamp, service and a correlation id taken from the request (W3C traceparent or
X-Request-Id) or generated at the edge, and propagate it to outbound calls and jobs. Never log secrets or personal
data (see SEC-1).

#### Expose separate liveness and readiness checks
<!-- rule: id=BE-13 enforcement=rule -->
Provide a cheap liveness endpoint that only shows the process is responsive and a readiness endpoint that checks
required dependencies with short timeouts. Never make liveness depend on downstream services, or one outage restarts
every instance.

#### Read configuration from the environment and validate it at startup
<!-- rule: id=BE-14 enforcement=rule -->
Load configuration from environment variables or the platform's config store, never from per-environment values in
code, and validate every required value with types at startup, so a misconfigured instance fails fast instead of at
its first request.

#### Shut down gracefully
<!-- rule: id=BE-15 enforcement=rule -->
On SIGTERM stop accepting new work, report not ready, finish in-flight requests and jobs within a deadline shorter
than the orchestrator's grace period, then close pools and exit. Work interrupted at the deadline must be safe to
retry.

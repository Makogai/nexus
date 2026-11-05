# Code review checklist

Work through the passes that matter for the change; not every item applies to every diff. A "yes" to a question
marked (B) is usually blocking. The rules themselves live in the on-demand constitutions (CQ-*, TST-*, SEC-*, ENG-*,
DOC-* and the stack documents); this list holds the questions to ask of a diff.

## 1. Intent and scope

- Does the change do what the description or task says, and only that? Unrelated changes belong in another change.
- Is the size reviewable? Mixed refactoring and behaviour changes should be separate commits at least.
- Are generated files, lockfiles and vendored code changed only as a consequence of a source change?

## 2. Correctness

- (B) Wrong results on any reachable input: off-by-one, inverted condition, wrong operator, wrong unit or currency,
  integer overflow, float used for money, truncation, rounding.
- Edge cases: empty, null/None/undefined, zero, one, many, maximum sizes, duplicates, unicode, leading/trailing
  whitespace, timezones and DST, leap years, locale-dependent parsing or formatting.
- Error paths: are errors propagated with context, or swallowed? Are partial failures handled (half-written state,
  partially sent batch)? Does cleanup happen on every path (finally, Drop, defer, `using`)?
- (B) State and concurrency: shared mutable state, races, check-then-act, missing locking or transactions, async
  operations not awaited, lost updates, reentrancy, ordering assumptions on parallel work.
- (B) Data integrity: writes inside a transaction where needed, idempotency of retried operations, uniqueness
  enforced in the database not only in code, migrations reversible or safely forward-only.
- Resource handling: connections, files, subscriptions, timers and listeners released; no unbounded caches,
  queues or recursion.
- Does the code that the diff calls behave the way the diff assumes (read it)?

## 3. Security

- (B) Untrusted input reaching a sink without validation or context-correct encoding: SQL or query builders with
  string concatenation, shell commands built from strings, file paths from input (traversal), HTML output (XSS),
  templates, redirects, outbound URLs (SSRF), deserialization of untrusted data.
- (B) Authorization checked server-side on every new endpoint or operation, including object ownership and tenant
  boundaries; authentication not bypassable by a new route.
- (B) Secrets in code, config, tests, fixtures or logs. Report location only.
- Sensitive data in logs, error messages, URLs or analytics; personal data collected beyond need.
- Cryptography from vetted libraries, secure randomness for tokens, TLS verification not disabled.
- New dependencies: necessary, maintained, licence compatible, pinned via the lockfile.
- For deeper work use the security-review skill if the workspace has it.

## 4. Tests

- (B) Changed behaviour in risky code (money, auth, data writes, migrations, concurrency) without tests.
- Would the tests fail if the new code were wrong? Look for assertions that cannot fail, over-mocking that tests
  the mock, and snapshot updates accepted without review.
- Failure paths and edge cases covered, not only the happy path.
- Do the tests follow TST-*: outcomes rather than implementation details, deterministic, isolated, named after the
  behaviour?

## 5. Design and readability

- Could a newcomer follow it without running it? Check names, function size, nesting and comments against CQ-* and
  ENG-*; report misleading names and stale comments, not taste.
- Responsibilities sit in the right layer and module; dependency direction follows the project's architecture
  (no domain code importing UI or framework internals, no cycles).
- Duplication: is there an existing helper, type or pattern this should reuse?
- Is each new abstraction justified by a current need, not a speculative one?
- Types encode invariants where the language allows (enums or sealed types instead of strings and flags).
- Style issues that a formatter or linter would fix are not review findings.

## 6. Performance

- Work inside loops: N+1 queries, repeated parsing, repeated allocation, linear lookups that should be maps.
- Unbounded result sets: missing pagination, limits or streaming.
- Database: queries use indexes for new filters and sorts; migrations on large tables avoid long locks.
- Blocking calls on async runtimes, UI threads or request paths; missing timeouts on network calls.
- Frontend: unnecessary re-renders or watchers, large bundles from new imports, layout thrashing.
- Raise performance findings with an estimate of scale; do not micro-optimise cold paths.

## 7. Compatibility

- (B) Public API, CLI flags, configuration keys, file formats, events or database schema changed without a
  migration path, deprecation or version bump.
- Serialized names (JSON fields, enum values) changed in a way that breaks stored data or other clients.
- Migrations: ordered, append-only, safe to run on existing data, and compatible with the previous app version
  during a rolling deploy when the project deploys that way.
- Feature flags and defaults: is the new behaviour off or on by default, and is that intended?
- Platform and runtime support: OS paths and line endings, minimum language/runtime versions, browser support.

## 8. Operability

- Errors are actionable for the person who sees them; user-facing messages do not leak internals.
- Logging at the right level, with context identifiers and without sensitive data; no noisy logs in hot paths.
- Metrics, traces or health checks updated when the change adds a new dependency or failure mode.
- Configuration validated at startup; new settings documented with safe defaults.

## 9. Documentation

- Docs that the change makes wrong are updated in the same change (DOC-* says which: README, API reference,
  changelog, configuration).
- Significant or hard-to-reverse design decisions recorded (ADR) or proposed.

## Writing findings

- One finding per problem; group repeated instances ("same in a.ts:10, b.ts:31").
- Lead with the consequence: "Totals are overstated for taxed orders" beats "wrong order of operations".
- Offer the smallest fix that resolves it; mention larger alternatives separately.
- Tone: about the code, never the author. Questions are genuine questions, not disguised demands.

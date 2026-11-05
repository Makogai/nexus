# Security review checklist

Sections follow the OWASP Top 10 categories (2025 edition; server-side request forgery, a separate category in
2021, is covered under access control there). Search hints are starting points for reading code, not verdicts:
every hit needs a trace from an untrusted source before it becomes a finding. When a search hits a secret, record
the location only.

## A01 Broken access control (including SSRF)

- Every new route, resolver, RPC or IPC command checks authorization server-side; UI hiding is not a control.
- Object-level checks: resources loaded by id are filtered by owner or tenant in the query itself
  (`WHERE id = ? AND tenant_id = ?`), not checked after loading the wrong row.
- Function-level checks: admin and internal operations unreachable for normal roles; default deny.
- Mass assignment: request bodies bound to models only through an explicit allowlist (fillable fields, DTOs).
- Path traversal: user-supplied paths normalised and confirmed to stay inside the allowed root after resolving
  `..`, absolute paths and symlinks; archive extraction checks each entry (zip slip).
- CORS: no reflected arbitrary origins combined with credentials; no wildcard for authenticated APIs.
- CSRF: state-changing requests with cookie authentication use SameSite cookies plus tokens or origin checks.
- SSRF: outbound requests to user-influenced URLs use an allowlist of hosts and schemes; block internal ranges,
  link-local metadata addresses and redirects to them; resolve DNS once and connect to the checked address.
- Search hints: route definitions without auth middleware, `findById`/`find(` on request ids, `fetch(`,
  `requests.get(`, `Http::get(`, `reqwest::get(` with request data.

## A02 Security misconfiguration

- Debug mode, stack traces, directory listings and admin consoles disabled in production configuration.
- Security headers on web responses: Content-Security-Policy, Strict-Transport-Security, X-Content-Type-Options,
  frame-ancestors (or X-Frame-Options), Referrer-Policy.
- Cookies: `Secure`, `HttpOnly` for session cookies, `SameSite` set deliberately.
- No default credentials, sample accounts or test endpoints enabled outside test configuration.
- Containers run as a non-root user; only needed ports exposed; secrets not baked into images or build args.
- Cloud and storage: buckets and blobs private by default; least-privilege service identities.
- XML parsers with external entities and DTDs disabled (XXE).

## A03 Software supply chain failures

- Lockfiles committed and used in CI (`npm ci`, `--locked`, `--frozen-lockfile` or equivalent).
- New dependencies: needed, maintained, reputable, licence compatible; typosquatting checked (exact name).
- Install and build scripts of new packages reviewed (npm lifecycle scripts, `build.rs`, setup hooks).
- Known vulnerabilities checked with the ecosystem's audit tool (run only with consent; uses the network).
- CI: third-party actions pinned to a full commit SHA; workflow permissions minimal; secrets not exposed to
  pull requests from forks.
- Release artifacts signed or checksummed; auto-updaters verify signatures before installing.

## A04 Cryptographic failures

- Sensitive data encrypted in transit (TLS) and at rest where required; TLS certificate verification never
  disabled. Search hints: `verify=False`, `rejectUnauthorized: false`, `danger_accept_invalid_certs`,
  `InsecureSkipVerify`.
- Passwords hashed with Argon2id, scrypt or bcrypt; never MD5, SHA-1 or unsalted fast hashes.
- Symmetric encryption uses AEAD (AES-GCM, ChaCha20-Poly1305) with unique nonces; no ECB mode; no home-made crypto.
- Tokens, ids used for authorization and reset links come from a cryptographically secure random generator
  (not `Math.random`, `rand()`, `random.random`).
- Secrets compared in constant time; keys loaded from a secret store or environment, never from source.
- JWT: signature verified with an expected algorithm; `none` rejected; expiry, issuer and audience checked.

## A05 Injection (including XSS)

- SQL and NoSQL: parameterised queries or a query builder with bound values; identifiers (sort columns, table
  names) mapped through an allowlist. Hints: string concatenation or interpolation into `execute`, `query`,
  `DB::raw`, `whereRaw`, `.raw(`, `$where`.
- OS commands: argument vectors, never shell strings; no `shell=True`, `exec(` with interpolated strings,
  `system(`; arguments that may start with `-` separated by `--`.
- Templates: user input never used as a template; auto-escaping on. Hints: raw output (`v-html`,
  `dangerouslySetInnerHTML`, `[innerHTML]`, `bypassSecurityTrust`, Blade `{!! !!}`, Jinja `|safe`, `innerHTML =`).
- XSS: output encoded for its context (HTML body, attribute, URL, JavaScript, CSS); URLs from users restricted
  to `http`/`https`; CSP as defence in depth.
- Header and log injection: CR/LF stripped from values placed in headers or log lines.
- Deserialization of untrusted data only into plain data types (no `pickle.loads`, `yaml.load` without a safe
  loader, PHP `unserialize`, Java native serialization, .NET BinaryFormatter on external input).
- Regular expressions on untrusted input checked for catastrophic backtracking (ReDoS).

## A06 Insecure design

- Abuse cases considered for new features: enumeration, automation, replay, race conditions on limits or
  balances (double spend), workflow steps skipped by calling a later endpoint directly.
- Rate limits and quotas on authentication, expensive operations and anything that sends email or SMS.
- Business invariants enforced server-side and transactionally, not only in the client.
- Multi-tenant isolation designed in (tenant id in every query or row-level security), not added per endpoint.

## A07 Authentication failures

- Credential stuffing and brute force limited (throttling, lockout with care, MFA where appropriate).
- Session ids regenerated on login and privilege change; logout invalidates server-side state; reasonable expiry.
- Password reset and invite tokens single-use, short-lived, high entropy, bound to the account.
- Error messages and timings do not reveal whether an account exists, where that matters.
- OAuth/OIDC: `state` and PKCE used, redirect URIs matched exactly, ID tokens validated.

## A08 Software or data integrity failures

- Data from clients, caches or other services that affects decisions is validated or signed (prices, roles,
  feature flags in client-side storage).
- Webhooks verify signatures and timestamps before acting.
- Updates, plugins and downloaded code verified (signature or checksum) before execution.
- Serialized state the server trusts (cookies, hidden fields) is signed or kept server-side.

## A09 Logging and alerting failures

- Security-relevant events logged: logins, failed authorization, privilege changes, sensitive data exports.
- Logs exclude secrets, tokens, full card numbers, passwords and unnecessary personal data.
- Log entries carry enough context (actor, action, target, request id) to investigate an incident.
- Alerting exists for repeated failures and anomalies where the deployment has monitoring.

## A10 Mishandling of exceptional conditions

- Errors fail closed: an exception in an authorization or validation path denies, never allows.
- Error responses do not leak stack traces, queries, internal paths or configuration.
- Resource exhaustion handled: request size limits, timeouts on outbound calls, bounded queues and recursion,
  limits on decompression ratios and file uploads.
- Partial failures leave consistent state (transactions, compensation, idempotent retries).

## Desktop, CLI and local tools

- IPC between UI and privileged code exposes a minimal, validated command set (Tauri capabilities scoped; Electron
  with `contextIsolation`, sandboxing and no `nodeIntegration` in renderers that show remote content).
- Custom URL schemes, deep links and file associations treat all parameters as untrusted.
- Child processes spawned with argument vectors and a composed environment; no inherited secrets they do not need.
- Files written atomically with restrictive permissions; temporary files created securely, not at predictable paths.
- Webviews load only bundled or allowlisted origins; navigation and new windows restricted.

## LLM and agent features

- Content from documents, web pages, tool output, tickets or emails is untrusted input that can carry prompt
  injection; it must not be able to trigger privileged tools without a human or policy check.
- Tools exposed to a model follow least privilege; destructive or external actions need confirmation.
- Model output is treated as untrusted: escaped before rendering, never executed or used as a shell command, query
  or path without validation.
- Secrets and unnecessary personal data are kept out of prompts and transcripts; what is sent to which provider is
  documented.
- Cost and rate limits exist for model calls triggered by users.
- See the OWASP Top 10 for LLM Applications for deeper coverage.

## Severity guide

| Severity | Typical meaning |
| --- | --- |
| Critical | Remote, unauthenticated compromise of data or system; trivially exploitable |
| High | Authenticated user can reach other users' data or escalate privileges; injection with real impact |
| Medium | Exploitable only with unusual preconditions, or limited impact (single record, own account) |
| Low | Defence-in-depth gap, hard to exploit, minor information disclosure |
| Info | Hardening suggestion with no concrete exploit path |

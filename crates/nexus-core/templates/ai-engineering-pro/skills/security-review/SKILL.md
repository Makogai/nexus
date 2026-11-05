---
name: security-review
description: Threat-focused security review of a change or component - trust boundaries, injection, access control, secrets, dependencies - with rated findings. Use when asked for a security review or audit.
---

# Security review

Find vulnerabilities that an attacker could actually exploit in this change or component, rate them, and say how
to fix and verify each one. Say clearly what was and was not reviewed; never declare code "secure".

## Ground rules

- **Secrets:** if you find real credentials (keys, tokens, passwords, connection strings with passwords, private
  keys), report only file:line and the kind of secret. Never echo, partially quote or pass the value to another
  tool. Recommend rotation: a secret that reached Git history is compromised even after deletion.
- Do not open `.env*` files, credential stores or key files unless the person asks; the code that loads them and
  the variable names are enough.
- No active testing against real systems: no scanning, no requests to production, no exploit attempts. Proofs are
  local tests or written scenarios. Dependency audit commands use the network: suggest them, run only with consent.
- Every finding needs a plausible exploit path. Label confidence: **confirmed** (traced end to end or reproduced
  in a local test), **likely**, or **possible**.
- Report command results only from runs in this session.

## Procedure

1. **Scope and context.** A change (`git diff <base>...HEAD`) or a component (paths). Establish the deployment
   context: internet-facing service, internal service, desktop app, CLI, library, mobile. If unknown, ask or state
   your assumption. If the NEXUS tools are available: `get_constitution` with `["SEC-*"]` plus stack prefixes, and
   `search_knowledge` for "threat model" or "security" decisions.
2. **Model the threats.** List:
   - assets: personal data, credentials, money, integrity of records, availability, signing keys;
   - actors: anonymous user, authenticated user, other tenant, admin, local user, compromised dependency, malicious
     content (uploaded files, fetched pages, LLM input);
   - entry points: HTTP routes, GraphQL resolvers, CLI arguments, IPC, files, message consumers, webhooks,
     uploads, deserialization, environment and configuration;
   - trust boundaries: where data moves from less trusted to more trusted code.
3. **Trace untrusted input to sinks.** From each entry point, follow data to dangerous sinks: queries, shell and
   process spawning, file paths, HTML and templates, redirects, outbound requests, deserializers, logs, crypto.
   At each sink check validation (allowlist, type, length) and context-correct encoding or parameterisation.
4. **Access control.** Authentication (session handling, token signature, algorithm, expiry, audience; password
   storage; brute-force limits), authorization on every operation server-side (object ownership, function level,
   tenant isolation), CSRF for cookie-authenticated state changes, CORS configuration.
5. **Secrets and sensitive data.** Search for hard-coded secrets by name patterns (password, secret, token,
   api_key, private key headers) and high-entropy literals, without printing matched values. Check that secrets
   and personal data stay out of logs, error messages, URLs, analytics and client bundles.
6. **Cryptography.** Vetted libraries only; AEAD ciphers; Argon2id, scrypt or bcrypt for passwords; secure
   randomness for tokens; constant-time comparison for secrets; TLS verification never disabled.
7. **Dependencies and supply chain.** Lockfile committed; new dependencies justified and maintained; install
   scripts of new packages reviewed; CI actions pinned. Suggest `npm audit`, `cargo audit`, `pip-audit`,
   `composer audit` or `osv-scanner` as appropriate and run them only with consent (the dependency-analysis skill
   covers this in depth).
8. **Configuration and platform.** Debug modes and verbose errors off in production configuration, security
   headers, cookie flags, file permissions, container user, default credentials, rate limits and resource limits.
9. **Go through reference.md** for the categories the scope touches (OWASP Top 10 oriented, plus desktop/CLI and
   LLM-feature checks).
10. **Rate each finding.** Severity critical / high / medium / low / info from impact and exploitability in this
    deployment context. Give the exploit scenario: attacker, precondition, steps, impact.
11. **Fix and verify.** Propose the fix at the right layer (parameterised query, argument vector instead of a
    shell string, allowlist, output encoding, authorization check in the service layer) and a regression test.
    Apply fixes only when asked.
12. **Record.** With NEXUS: `checkpoint` kind `note` with scope and counts per severity only. Keep exploit details
    of unfixed findings out of synced workspace files; if a durable record is needed, use `record_memory` with
    `localOnly: true`, or the project's private security tracker. Create tasks only when the person agrees.

## Exit criteria

- Assets, entry points and trust boundaries are listed for the scope.
- Every finding has severity, confidence, location, scenario, fix and verification.
- Reviewed and not-reviewed areas are stated; no secret value appears anywhere in the output.

## Report

```markdown
## Security review: <scope>
Context and assumptions: <deployment, actors>
Reviewed: <areas> | Not reviewed: <areas, with reason>
Summary: 0 critical, 1 high, 2 medium, 1 low
Checks run (this session): <commands and results, or "none - static review">

### [HIGH] SQL injection in order search - src/orders/search.ts:88 (confirmed)
Scenario: an authenticated user sends `sort=<payload>`; the value is concatenated into ORDER BY ...
Impact: read access to all tenants' orders.
Fix: map `sort` through an allowlist of column names; keep values parameterised.
Verify: test that an unknown sort key returns 400 and the query text never contains user input.

### Secrets
- `config/legacy.php:12` - hard-coded database password (value not reproduced). Rotate, move to the secret store.
```

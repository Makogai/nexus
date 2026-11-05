> Appendix to [`TEMPLATE_REVIEW.md`](../TEMPLATE_REVIEW.md) — the reviewer's per-rule findings as written during the review (2026-11-05). Later owner changes are listed in the main document: the core's CORE-6 ("read the relevant rules first") was folded into the on-demand listing, so the untrusted-content rule called CORE-7 here is **CORE-6**.

# Review B: DB, FE, BE, OPS and 13 stack constitutions

Scope: `ai-engineering-pro/constitutions/{database,frontend,backend,devops-docker}.md` and `stacks/*.md` (NG, VUE,
REACT, TS, NODE, LRV, PHP, PY, RS, FLT, DART, SQL, DKR). Read for comparison: core, ENG, CQ, TST, GIT, DOC, ARC, SEC,
PERF, A11Y. The other reviewer was editing those files at the same time, so the cross-references below were checked
against their state at the end of this review.

Categories: 1 duplicate/contradiction, 2 context overhead, 3 generic, 4 outdated/inaccurate, 5 proportionality,
6 conflict with Claude Code/managed policy, 7 unsafe/untrusted execution, 8 missing practice.

| file | rule/skill | category | finding | action |
|---|---|---|---|---|
| database.md | intro | 1 | Did not say where DB ends and SQL begins (both are listed for the `sql` stack) | rewritten: DB = schema, migrations, indexes, transactions, data, backups; SQL = SQL text |
| database.md | DB-1 | 6, 7 | Did not cover a session running migrations or data fixes against shared databases | rewritten: never from a session unless the person explicitly asks |
| database.md | DB-3 | 5 | Mandatory backup and rollback-plan gate also applied to throwaway local databases | rewritten: scoped to shared or production databases (id kept, LRV-6 references it) |
| database.md | DB-4 | 5 | Required expand/contract for every schema change | rewritten: applies when old and new app versions run side by side |
| database.md | DB-6 + SQL-9 | 1 | Both said "store instants in UTC in time-zone-aware types" | merged into DB-6 (types per engine, never strings); SQL-9 removed |
| database.md | DB-8 + SQL-8 (+SQL-6) | 1 | Indexing from query patterns, indexing FKs and EXPLAIN checks were split across three rules | merged index design and FK indexing into DB-8; EXPLAIN mechanics stay in SQL-6; SQL-8 removed |
| database.md | DB-9 (SELECT *) | 1 | Same rationale as SQL-2 (name columns explicitly) | merged into SQL-2; removed from DB |
| database.md | DB-10 (N+1) | 1 | Same rule as PERF-4 (Eliminate N+1 queries); LRV-4 adds the Laravel detail | removed (PERF-4 owns it) |
| database.md | DB-11 (keyset) | 1 | Page bounds duplicated PERF-5 and BE-3; only the SQL technique was specific | moved the technique to SQL-7; removed from DB |
| database.md | DB-14 (now DB-11) | 1 | "Never copy production personal data into dev or test databases" partly duplicated TST-11 (tests) | rewritten: limited to development databases |
| database.md | DB-12, 13, 15 | 2 | Already concise | kept (renumbered DB-9, DB-10, DB-12) |
| stacks/sql.md | intro | 1 | Did not say what DB holds | rewritten |
| stacks/sql.md | SQL-1 | 1, 8 | Restated SEC-4; missed escaping LIKE wildcards and array binding for IN lists | rewritten as a SQL refinement (LIKE escaping, `= ANY($1)`), keeps the SEC-4 reference |
| stacks/sql.md | SQL-2 | 1 | Overlapped DB-9 | merged: SELECT list, INSERT targets, no ordinals |
| stacks/sql.md | SQL-6 | 7 | EXPLAIN ANALYZE runs the statement, and the rule did not exclude production | rewritten: use a non-production database and roll back writes |
| stacks/sql.md | SQL-7 (new) | 1 | Keyset technique moved here from DB-11 | added: deterministic order with a unique tie-breaker and a matching index |
| stacks/sql.md | SQL-7 naming (now SQL-8) | 5 | Imposed a convention on existing databases | rewritten: keep the existing convention; snake_case for new schemas |
| stacks/sql.md | SQL-8, SQL-9 | 1 | Covered by DB-8 and DB-6 | removed (merged) |
| backend.md | intro | 1 | Did not point to ARC/PERF for resilience, observability and timeouts | rewritten |
| backend.md | BE-1 | 1, 5 | Assumed every API is REST; the 429 detail lived in a rule that duplicated SEC-9 | rewritten: "for HTTP APIs", 429 + Retry-After added |
| backend.md | BE-2 | 1, 5 | Forced RFC 9457 even where the project already has an error format; restated SEC-2 | rewritten: the project's existing format, RFC 9457 for new APIs; leakage points to SEC |
| backend.md | BE-3 | 1 | Restated PERF-5 pagination | rewritten: only the API contract (default/max size, opaque cursors, allowlisted sort/filter) |
| backend.md | BE-4 | 1, 5 | Restated ENG-9/ARC-7 compatibility; versioning required for every API | rewritten: APIs with clients not deployed together; only the scheme and Deprecation/Sunset headers |
| backend.md | BE-6 (validate at boundary) | 1 | Covered by SEC-3 and CQ-9 (and TS-9 / stack docs) | removed; its useful "unknown fields" part went into the new BE-6 |
| backend.md | BE-7 (now BE-6) | 8 | Authorisation rule missed property-level authorisation (mass assignment, OWASP API3) | rewritten: authorise per resource and field, bind to explicit request types (SEC-7 kept) |
| backend.md | BE-8 (now BE-7) | 1 | "Keep remote calls out of transactions" duplicated DB-9 | rewritten: service-layer boundary + transactional outbox only |
| backend.md | BE-9 (rate limit) | 1 | Duplicated SEC-9 | removed; 429/Retry-After folded into BE-1 |
| backend.md | BE-11 (timeouts/retries) | 1 | Duplicated PERF-11 and ARC-5 | removed |
| backend.md | BE-12 (structured logs) | 1 | Duplicated ARC-8, ENG-5 and SEC-1 | removed (NODE-10 and PY-7 keep stack specifics) |
| backend.md | BE-14 (config at startup) | 1 | Duplicated ENG-6 | removed (NODE-7 keeps the Node specifics) |
| backend.md | BE-5, BE-10, BE-13, BE-15 | - | Specific and correct | kept (renumbered BE-5, BE-8, BE-9, BE-10) |
| devops-docker.md | intro / split | 1 | OPS and DKR are listed together (both `docker`) yet each had multi-stage, non-root, secrets and .dockerignore rules | OPS = CI, running containers, deploy; all Dockerfile authoring moved to DKR |
| devops-docker.md | OPS-1 + OPS-3 | 1 | OPS-3 restated SEC-14/DKR-1 (action SHAs, image digests); lockfiles restated ENG-8 | merged: reproducible build + pinned runtime/tool versions; image/action pinning left to DKR-1/SEC-14 |
| devops-docker.md | OPS-2 | 1, 4, 5 | `check=build` cannot verify a CI configuration; quarantine restated TST; assumed every project has CI | rewritten: "where the project has CI", no check attribute, flaky tests per TST-12 |
| devops-docker.md | OPS-15 (now OPS-3) | 1, 8 | Least privilege and OIDC restated SEC-8; missed the fork/untrusted-PR secret exposure risk | rewritten: CI-specific (read-only token default, protected environments, no secrets for fork code) |
| devops-docker.md | OPS-4 (multi-stage) | 1 | Same as DKR-2 | removed |
| devops-docker.md | OPS-5 (now OPS-4) | 1 | Overlapped DKR-5 (USER in Dockerfile) | rewritten as runtime hardening only (runAsNonRoot, drop capabilities, read-only FS, no privileged, no Docker socket) |
| devops-docker.md | OPS-6 (now OPS-5) | 1 | Base-image choice overlapped DKR-1 | rewritten: rebuild cadence and scanning only; slim/distroless moved to DKR-1 |
| devops-docker.md | OPS-7 (now OPS-6) | 2 | Fine, slightly verbose | rewritten; DKR-7 now references OPS-6 |
| devops-docker.md | OPS-8 (secrets in images, mandatory) | 1 | Same subject as DKR-8 and SEC-1 | merged into DKR-8 (mandatory there) |
| devops-docker.md | OPS-9 (.dockerignore) | 1 | Same subject as DKR-4 | merged into DKR-4 |
| devops-docker.md | OPS-9 (new, mandatory) | 6, 7, 8 | Nothing stopped a session from deploying, applying Terraform/kubectl/helm or pushing images on its own initiative | added: only when the person explicitly asks in this session; otherwise prepare the plan/diff |
| devops-docker.md | OPS-13 (now OPS-11) | 1 | Logging/metrics content duplicated ARC-8 | rewritten: stdout and symptom alerting only, "what to log follows ARC" |
| devops-docker.md | OPS-10/11/12/14 | - | Specific and useful | kept (renumbered OPS-7, 8, 10, 12) |
| stacks/docker.md | DKR-1 | 1 | Absorbed base-image choice from OPS-6 | rewritten: slim/distroless + version + digest |
| stacks/docker.md | DKR-4 | 1 | Absorbed OPS-9 (.dockerignore contents) | rewritten |
| stacks/docker.md | DKR-7 | 1 | Referenced the renumbered OPS-7 | reference updated to OPS-6 |
| stacks/docker.md | DKR-8 | 1, 7 | Absorbed OPS-8; "rotate the secret" is the person's action under SEC-1 | rewritten as the single container-secrets rule, now mandatory; compromise handling points to SEC-1 |
| stacks/docker.md | DKR-9 | 4 | Did not mention that Compose v2 has no top-level `version:` key and uses `docker compose` | rewritten |
| stacks/docker.md | DKR-2, 3, 5, 6, 10 | - | Current (BuildKit cache mounts, exec form, hadolint) | kept |
| frontend.md | FE-1 | 3, 4 | The container/presentational split is an outdated dogma and restated CQ size rules | rewritten: reusable components stay free of data access, routing and stores |
| frontend.md | FE-4 | 5 | Pushed TanStack Query/SWR even where the project has its own data layer | rewritten: the project's caching data layer (examples incl. framework loaders/resources) |
| frontend.md | FE-6 | 1 | The "discriminated value" sentence duplicated TS-5 | removed that sentence; added "keep previous data while refetching" |
| frontend.md | FE-7 | 1 | Server-side validation restated SEC-3 | tightened to the form-specific part |
| frontend.md | FE-8 | 4, 8 | Listed Angular `[innerHTML]` as unsafe (Angular sanitises it; the sink is `bypassSecurityTrust*`); ignored `javascript:` URLs | rewritten: correct Angular sink, URL scheme allowlist; still mandatory (VUE referenced it) |
| frontend.md | FE-9 (lazy loading) | 1 | Same as PERF-12 | removed |
| frontend.md | FE-9 (new) | 8 | Common frontend leak missing: build-time public env vars (VITE_*, NEXT_PUBLIC_*) end up in the bundle | added (rule, points to the mandatory SEC-1) |
| frontend.md | FE-10 (styling) | 5 | "One approach" stated as a choice to make, not the project's existing one | rewritten: follow the app's approach and tokens |
| frontend.md | FE-11 (i18n) | 5 | Required i18n plumbing in every app, including apps that are not localised | rewritten: localised apps; Intl formatting always |
| frontend.md | FE-12 (budgets pointer) | 2, 3 | Only said "read PERF", which CORE already says | removed |
| frontend.md | FE-13 (now FE-12) | 1 | E2E sentence duplicated TST-3 | tightened, points to TST |
| stacks/typescript.md | TS-1 | 1 | Restated CQ-6 escape hatches | tightened to tsconfig flags and @ts-expect-error over @ts-ignore |
| stacks/typescript.md | TS-2 | 4 | Did not mention Node's type stripping | rewritten |
| stacks/typescript.md | TS-3 (new) | 4, 8 | No rule set up type-aware linting that NODE-4 and TS rules rely on; ESLint flat config not mentioned | added: the project's lint setup, else flat config + typescript-eslint type-checked presets |
| stacks/typescript.md | TS-3 + TS-4 (old) | 1, 2 | `any` and `!`/`as` were two rules restating CQ-6 | merged into TS-4 |
| stacks/typescript.md | TS-5 + TS-6 (old) | 1, 2 | Discriminated unions and the exhaustive switch belong together | merged into TS-5 |
| stacks/typescript.md | TS-8 (now TS-7) | 5 | Banned enums in existing code too | rewritten: new code |
| stacks/typescript.md | TS-9 (now TS-8) | 5 | Explicit return types is a style preference | enforcement rule -> guideline |
| stacks/typescript.md | TS-10 (now TS-9) | 1, 5 | Restated SEC-3/CQ-9; prescribed zod/valibot | rewritten: TS-specific (derive types from the schema, no `as`), the project's library first |
| stacks/angular.md | NG-2 | 4 | Zoneless change detection was missing | rewritten: OnPush + zoneless for new apps where stable (version-neutral) |
| stacks/angular.md | NG-5 | 4 | Did not say the structural directives are deprecated | rewritten |
| stacks/angular.md | NG-6 | 5 | Forced reactive forms regardless of the project's approach | rewritten: unless the project settled on another approach |
| stacks/angular.md | NG-10 | 6, 7 | "Upgrade with ng update" read as a command to run (installs and network) | rewritten: propose upgrades through ng update |
| stacks/angular.md | NG-11 | 4 | Did not mention that Karma is deprecated or the Vitest runner | rewritten |
| stacks/angular.md | NG-1, 3, 4, 7, 8, 9 | - | Current (standalone, signals, linkedSignal, inject(), withComponentInputBinding, takeUntilDestroyed) | kept, lightly tightened |
| stacks/vue.md | VUE-5 (v-html) | 1 | Duplicated FE-8, which lists v-html | removed |
| stacks/vue.md | VUE-2 | 4 | Wording for 3.5 reactive props destructure was ambiguous | rewritten |
| stacks/vue.md | VUE-3 | 4 | Did not name defineExpose as the sanctioned child API | rewritten |
| stacks/vue.md | VUE-8 (now VUE-7) | 5 | Prescribed Pinia even when the project uses another store | rewritten: the project's store, Pinia setup stores for new code |
| stacks/vue.md | VUE-1 | 5 | "Do not add Options API" did not say new code | rewritten: new code |
| stacks/vue.md | others | - | Current (onWatcherCleanup, toValue, useFetch/useAsyncData) | kept |
| stacks/react.md | REACT-1 | 4 | Did not mention React 19's ref-as-prop (forwardRef no longer needed) | rewritten |
| stacks/react.md | REACT-2 | 4 | Did not name the plugin's recommended preset (the compiler-era lint set) | rewritten (version-neutral) |
| stacks/react.md | REACT-4 + REACT-7 | 1 | REACT-7 (data library) duplicated FE-4 and REACT-4's "no fetch in effects" | merged into REACT-4 with a pointer to FE-4; REACT-7 removed |
| stacks/react.md | REACT-6 | 4 | Memoisation advice predated React Compiler 1.0; did not warn against removing existing memoisation | rewritten: compiler first, hooks only as escape hatches, no drive-by removal |
| stacks/react.md | REACT-8 (now REACT-7) | 1, 5 | Schema validation restated FE-7; prescribed RHF/zod | rewritten: the project's form approach / React 19 actions; validation per FE-7 |
| stacks/react.md | REACT-9 (now REACT-8) | 4 | Missing the async request APIs in Next.js 15+ (params, searchParams, cookies, headers) | rewritten |
| stacks/react.md | REACT-11 (now REACT-10) | 1 | Restated FE-12 | tightened to the RTL/user-event specifics |
| stacks/node.md | NODE-3 (lockfile/npm ci) | 1 | Same as ENG-8 and SEC-14 | removed |
| stacks/node.md | NODE-3 (new) | 4, 8 | Built-ins (fetch + AbortSignal.timeout, node:test, --watch, --env-file, util.parseArgs) not mentioned | added (guideline; keep existing libraries; consistent with ENG-7) |
| stacks/node.md | NODE-5 | 1 | Restated PERF-7 | rewritten to Node-specific sync APIs and worker_threads |
| stacks/node.md | NODE-7 | 1, 4 | Restated ENG-6 | rewritten: process.env in one module; `node --env-file` instead of dotenv |
| stacks/node.md | NODE-8 | 1 | Restated BE-10 shutdown steps | rewritten to server.close()/closeIdleConnections() and direct node start |
| stacks/node.md | NODE-10 | 5 | Prescribed pino | rewritten: the project's logger, pino for new services |
| stacks/laravel.md | LRV-6 | 1 | "Never edit a run migration" duplicated DB-2 | tightened (DB-3 reference kept) |
| stacks/laravel.md | LRV-9 | 1, 8 | Restated BE-8; missed jobs dispatched inside transactions running before commit | rewritten: afterCommit(), supervisor/Horizon |
| stacks/laravel.md | LRV-10 | 4, 5 | Pest and PHPUnit presented as equal choices | rewritten: the project's framework, Pest for new projects |
| stacks/laravel.md | LRV-11 (Pint + Larastan) | 1 | Duplicated PHP-9/PHP-10 (PHP applies to Laravel too) | merged into PHP-8/PHP-9; removed |
| stacks/php.md | intro, PHP-3 | 4 | PHP 8.4 features (asymmetric visibility, property hooks) missing | rewritten |
| stacks/php.md | PHP-4 + PHP-5 | 1, 2 | Exceptions and "@" suppression are one error-handling rule; partly restated CQ-7 | merged into PHP-4 |
| stacks/php.md | PHP-7 (now PHP-6) | 4 | Told you to set ERRMODE_EXCEPTION, which is the default since PHP 8.0 | rewritten: keep the default, disable emulated prepares |
| stacks/php.md | PHP-8 (now PHP-7) | 1 | composer audit duplicated SEC-13 | removed that part |
| stacks/php.md | PHP-10 (now PHP-9) | 5 | Level 8+ imposed on existing projects | rewritten: the project's level, 8+ for new projects, never lower it |
| stacks/python.md | PY-1 | 5 | Every project had to test min and max Python versions | rewritten: libraries only |
| stacks/python.md | PY-2, PY-4, PY-5 | 5 | Tool choices (uv, ruff, pydantic) presented as mandatory | rewritten: the project's tool first, these for new projects |
| stacks/python.md | PY-3 | 5 | `mypy --strict` forced on existing code | rewritten: the project's strictness, strict for new projects |
| stacks/python.md | PY-6 (mutable defaults) | 2 | Basic pitfall that PY-4's B rule set already enforces | removed (noted in PY-4) |
| stacks/python.md | PY-9 (now PY-8) | 1 | Restated SEC-4 | tightened to Python sinks (incl. SQLAlchemy text()) |
| stacks/rust.md | RS-2 | 4 | "Commit Cargo.lock for binaries" is outdated; Cargo's guidance since 2023 is to commit it for all packages | rewritten |
| stacks/rust.md | RS-5 | 5 | thiserror/anyhow phrased as the only option | rewritten with "e.g." / the project's crate |
| stacks/rust.md | RS-8 (iterators) | 2, 3 | clippy (needless_range_loop) already enforces it | removed |
| stacks/rust.md | RS-9 + RS-10 | 1, 2 | rustfmt and clippy are one lint gate | merged into RS-8 |
| stacks/rust.md | RS-1, 3, 4, 6, 7, 11 | - | Current (edition 2024, resolver 3, `#[expect]`, nextest doc tests) | kept |
| stacks/flutter.md | intro | 1 | Said it refines FE and A11Y, which are not listed for Flutter projects (web stacks only) | rewritten: refines DART |
| stacks/flutter.md | FLT-5 | 5 | Did not defer to the app's established state-management approach | rewritten |
| stacks/flutter.md | FLT-7 | 1, 8 | Delegated contrast and target size to A11Y (not listed for Flutter); no automated check | rewritten: 48x48 dp, 4.5:1, flutter_test meetsGuideline checks |
| stacks/flutter.md | FLT-9 (flutter analyze) | 1 | Duplicated DART-7 (DART applies to Flutter) | merged into DART-6; removed |
| stacks/dart.md | DART-4 + DART-5 | 1, 2 | final/const and immutable models are one immutability rule (restating CQ-8) | merged into DART-4 |
| stacks/dart.md | DART-7 + DART-8 (+FLT-9) | 1, 2 | Analyzer and formatter are one lint gate | merged into DART-6 (with --fatal-infos) |
| stacks/dart.md | DART-9 (now DART-7) | 4 | Required package: imports, against Effective Dart's "prefer relative imports" | rewritten: never cross lib/; keep the project's one style |

## Cross-document checks done at the end

* SEC-1 (secrets and personal data, mandatory), SEC-4 (SQL/shell from strings, mandatory) and SEC-7 (server-side
  authorisation, mandatory) still mean the same; all references kept (SQL-1, NODE-9, PHP-6, PY-8, LRV-2, LRV-7, BE-6,
  REACT-9, FE-9, DKR-8).
* Removals rely on rules that exist now: PERF-4 (N+1), PERF-5 (pagination), PERF-7 (non-blocking), PERF-11 and ARC-5
  (timeouts, retries), PERF-12 (lazy loading, bundles), ARC-7 (compatible contracts), ARC-8 (observability), ENG-6
  (config), ENG-8 (lock files), SEC-3/CQ-9 (validation), SEC-9 (rate limits), SEC-14 (pinning), TST-3 (E2E journeys),
  TST-12 (flaky tests only quarantined with the person's agreement; OPS-2 agrees). If the other reviewer later removes
  one of these, the matching content should come back here.
* New cross-references to the other reviewer's documents use the prefix only (SEC, ARC, PERF, TST), not rule ids, so
  their renumbering cannot break them.
* Ids referenced from other files did not change: FE-4, FE-8, DB-3; the wildcards (`TS-*`, `RS-*`, `PY-*`, `VUE-*`,
  `REACT-*`) in skills still match. Ids changed only inside my own documents; DKR-7's reference to OPS was updated
  (now OPS-6).
* Format checks (`check_const.py`, `check-constitutions.mjs`) pass for all 17 files: ids 1..n without gaps, text
  ≤ 350 characters, titles ≤ 70, LF line endings, stack documents 8-11 rules, general documents 10-12.

## Summary

Rules before → after: DB 15 → 12, FE 13 → 12, BE 15 → 10, OPS 15 → 12, NG 11 → 11, VUE 11 → 10, REACT 11 → 10,
TS 10 → 9, NODE 10 → 10, LRV 11 → 10, PHP 10 → 9, PY 11 → 10, RS 11 → 9, FLT 10 → 9, DART 10 → 8, SQL 10 → 9,
DKR 10 → 10. **Total 194 → 170** (body text 73.3k → 69.0k characters). Mandatory rules: DB-3, FE-8, OPS-9 (new:
deploys and infrastructure changes only when the person asks), DKR-8 (moved from OPS-8), RS-6; 5 in total, up from 4
because OPS-9 is new.

Deliberately kept:
* Stack rules that repeat a general principle but name the stack's specific sinks or tools: SQL injection in SQL-1,
  PHP-6, PY-8 and LRV-7; NODE-9 for shells; the per-framework test rules. Saying "see SEC-4" alone would lose the API
  knowledge that prevents the bug.
* REACT-5 (stable keys): a frequent defect in generated React code.
* NG count unchanged (11): every rule is Angular-specific and current.
* OPS stays scoped to `stacks: [docker]`. Its non-container CI rules are mostly covered by ENG/SEC/GIT, and listing
  OPS in every session would cost context.
* Concrete version examples (`node:24.11-slim`) as examples only. Version claims elsewhere are hedged ("where stable",
  "Next.js 15 and later", "LTS line in support") rather than tied to a release that may have moved.

For the owner: `CURRENT_STATE.md` and `docs/ROADMAP.md` still say "319 rules" for the library. My documents account
for −24 of that. No Rust test asserts per-document counts, and every document stays inside the 6–18 rule range that
`templates/tests.rs` checks.

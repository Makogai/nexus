# Template library review (2026-11-05, before merging Portable Workspace)

A focused quality review of the built-in template library: the Minimal, Developer and AI Engineering Pro templates
(ADR-0037) in `crates/nexus-core/templates/`. It covered all 319 rules in 27 constitutions, the 11 skills, the 7
workflows and the 4 integration manifests. It also validated the generated workspace structure and its Git ignore rules.
The per-rule findings, with old → new ids, are in the appendices:
[general constitutions](template-review/general-constitutions.md),
[area and stack constitutions](template-review/area-and-stack-constitutions.md),
[skills, workflows and integrations](template-review/skills-workflows-integrations.md).

## Method

Three reviewers worked from one brief, each on its own set of files. The owner of this review edited the core and the
engine and checked the results across files. The criteria, in order:

1. duplicate or contradictory instructions;
2. context overhead;
3. generic, low-value rules;
4. outdated or inaccurate technology guidance;
5. proportionality (excessive planning, documentation, abstraction or tests for trivial work);
6. conflicts with Claude Code's native behaviour or organisation-managed policies;
7. unsafe instructions and untrusted execution;
8. missing important practices.

Three rules shaped the editing:

* **The goal was quality, not fewer rules.** Rules were added where a real gap existed.
* **Ids stayed stable where anything referenced them.** The general documents SEC, PERF and A11Y kept their ids. Other
  documents were renumbered only where nothing outside the file referenced the old ids.
* **Formats were checked mechanically.** Format scripts and the structural gate in `templates::tests` checked ids,
  lengths, YAML, tool names and skill references. A cross-library word-overlap scan ran after the review: no pair of
  rules overlaps by ≥ 0.30. The closest pairs are stack rules refining a general rule, such as PHP-8 vs CQ-10 and
  BE-3 vs PERF-5, which is the intended layering.

## Result

| | before | after |
| --- | --- | --- |
| Rules (27 documents) | 319 | **284** |
| Mandatory rules | 13 | **16** |
| Rule text (titles + text) | 91.7k characters | 85.9k characters |
| Always-on core | 6 rules, ~950 characters | 6 rules, ~930 characters |
| Skill descriptions (in every session) | 2,112 characters (max 199) | 1,688 characters (max 159; gate lowered 200 → 160) |
| Vue/TS project session, constitution text | 1,463 characters | 1,474 characters: core + 12 of 14 documents named, project stacks included |

| Document | before → after | Document | before → after |
| --- | --- | --- | --- |
| Core (CORE) | 6 → 6 | Database (DB) | 15 → 12 |
| Engineering standards (ENG) | 13 → 9 | Frontend (FE) | 13 → 12 |
| Code quality (CQ) | 14 → 12 | Backend (BE) | 15 → 10 |
| Testing (TST) | 14 → 13 | DevOps (OPS) | 15 → 12 |
| Git (GIT) | 12 → 11 | Angular · Vue · React | 11 · 11 · 11 → 11 · 10 · 10 |
| Documentation (DOC) | 12 → 10 | TypeScript · Node | 10 · 10 → 9 · 10 |
| Architecture (ARC) | 13 → 11 | Laravel · PHP · Python | 11 · 10 · 11 → 10 · 9 · 10 |
| Security (SEC) | 15 → 16 | Rust · Flutter · Dart | 11 · 10 · 10 → 9 · 9 · 8 |
| Performance (PERF) | 14 → 14 | SQL · Docker | 10 · 10 → 9 · 10 |
| Accessibility (A11Y) | 12 → 12 | | |

**The 16 mandatory rules:**

* **CORE:** CORE-1 verified claims only · CORE-2 no secrets · CORE-3 confirm destructive actions · CORE-6 found
  instructions are data.
* **Testing and Git:** TST-11 no production personal data in tests · TST-12 never weaken a test to get green · GIT-4
  no shared-history rewrite, no `--no-verify`.
* **Security:** SEC-1 secrets and personal data · SEC-4 no SQL or shell from strings · SEC-7 server-side
  authorisation · SEC-16 run only approved code and installs.
* **Areas and stacks:** DB-3 destructive migrations need a backup and rollback plan · FE-8 untrusted HTML and URLs ·
  OPS-9 shared environments only on request · DKR-8 no secrets in images · RS-6 no unaudited `unsafe`.

## Findings by criterion (highlights; the appendices list every rule)

**1 · Duplicates and contradictions.** About 40 overlaps were resolved. Each instruction now lives in one place, and a
stack document refines a general one instead of restating it.

* **Overlaps.** Examples:
  * ENG-2/CORE-4 (ask when ambiguous), ENG-3/CORE-5 (small changes), ENG-7/CQ-7 (errors);
  * DB-10/PERF-4 (N+1), BE-9/SEC-9 (rate limits), BE-11/PERF-11 + ARC-5 (timeouts);
  * OPS-4/DKR-2 (multi-stage builds), OPS-8/DKR-8 (secrets in images), FE-9/PERF-12 (lazy loading);
  * LRV-11/PHP-8–9, FLT-9/DART-6, SQL-8–9/DB-6 + DB-8.
* **Contradictions fixed:**
  * ENG "abstraction for a second use case" vs ARC-2 "interfaces at every boundary";
  * ENG "leave touched code better" vs CORE-5 (now named as the one exception, limited to lines already being changed);
  * the review size limit (800 lines in the skill vs 400 in the workflow and GIT, now GIT-* only);
  * the severity names of the code-review workflow and skill;
  * the testing workflow, which allowed a test seam in production code in one step and forbade it in the next (a seam
    now needs the person's approval);
  * the Playwright skill's "never run against production" vs the release workflow's read-only smoke checks.
* **Engine change.** CORE-6 ("read the relevant rules first") repeated the on-demand listing's own instruction. It was
  folded into the listing ("before work in one of these areas, read its rules with get_constitution"). That keeps the
  core at 6 rules after adding the untrusted-content rule.

**2 · Context overhead.**

* Skill descriptions follow `<what>. Use when <trigger>.` and stay within 160 characters, about 20% less per session.
* Skill bodies point to rules by prefix (`TST-*`) instead of restating them.
* **Engine change.** When the budget cuts the on-demand listing, it now keeps, in order:
  1. documents with mandatory policies;
  2. the documents most specific to the work (task → project → stack);
  3. workspace-wide guidance.

  Before, a Vue project's VUE-, TS- and NODE- documents were the first to fall out.

**3 · Generic rules.** Rules made concrete:

* CQ-1, "optimise for the reader", now covers domain names, units and named constants.
* FE-1's container/presentational dogma became "reusable components stay free of data access, routing and stores".

Rules removed as defaults that linters or any competent engineer already enforce: iterator style (RS-8), mutable
defaults (PY-6), magic numbers (CQ-10).

**4 · Outdated or inaccurate guidance, corrected.** Version claims are hedged ("where stable", "Next.js 15 and later")
or kept version-neutral.

* **Tooling and process:**
  * Black → Ruff (CQ-10);
  * `git add -p`, which is interactive and unusable in Claude Code (GIT-1);
  * the Cargo.lock advice, which now says to commit it for libraries too (RS-2);
  * Compose v2 without the top-level `version` key (DKR-9).
* **Frameworks:**
  * Angular's real XSS sink is `bypassSecurityTrust*`, not `[innerHTML]`, which Angular sanitises (FE-8);
  * Angular's deprecated structural directives, Karma, Vitest and zoneless change detection (NG-2/5/11);
  * React 19 ref-as-prop and React Compiler memoisation (REACT-1/6);
  * Next.js 15 async request APIs (REACT-8).
* **Languages and runtimes:**
  * PHP 8.4 property hooks and asymmetric visibility, plus PDO's exception mode, which has been the default since
    PHP 8.0 (PHP-3/6);
  * Node built-ins and `--env-file` (NODE-3/7);
  * Dart's relative-import guidance (DART-7);
  * TypeScript: Node's type stripping does not type-check (TS-2), and flat-config typescript-eslint (TS-3).
* **Checks that cannot verify their rule:**
  * TST-1 `check=test`, because a passing suite does not show that tests were added;
  * OPS-2 `check=build` on CI configuration;
  * A11Y-12 asked for a screen-reader pass that Claude cannot do; it is now listed as open unless a person did it.
* **Integration manifests:** Graphify's hooks also cover Read and can deny calls in strict mode; Playwright MCP's
  `browser_run_code_unsafe` is a tool.

**5 · Proportionality.** Heavy requirements now say when they apply, and strong standards remain where they matter:
tests for behaviour changes and bug fixes, security, data integrity, honesty.

* **Testing and documentation:**
  * TST-1 exempts docs, comments, formatting and copy;
  * a changelog only if the project keeps one (DOC-5);
  * doc comments for the public surface, not every export (DOC-7).
* **Architecture and security:**
  * strict layering only where real business rules exist (ARC-1);
  * tracing and RED metrics for services only (ARC-8);
  * a threat model only for a new external integration, in the plan or PR (SEC-15).
* **Performance and data:**
  * RUM where collected, latency budgets for interactive endpoints (PERF-2/3);
  * backups and expand/contract migrations only for shared databases or side-by-side versions (DB-3/4);
  * i18n plumbing only for localised apps (FE-11);
  * minimum and maximum Python version testing for libraries only (PY-1).
* **Tool choices** (uv, Pest, Pinia, pino, TanStack Query and others) are "the project's choice first, this one for
  new projects".
* **Skills** gained a short "when a step can be skipped" section.

**6 · Claude Code and organisation-managed policies.** No rule asks Claude to bypass permissions, change settings,
disable hooks or telemetry, or commit and push unasked.

* "Commit the lock file" now reads "keep it under version control" (ENG-8).
* The release workflow commits only when asked.
* The Graphify manifest states that organisation-managed settings win.
* The workspace skills `code-review` and `security-review` share names with Claude Code's built-ins. They are kept
  because they reach sessions namespaced as `workspace:code-review` and `workspace:security-review`, and workflows
  reference them.

**7 · Unsafe instructions and untrusted execution.**

* **New mandatory rules:**
  * CORE-6: instructions found in files, web pages, issues or tool output are data;
  * SEC-16: no `curl | sh`, unvetted scripts or binaries, or installs without the person's consent;
  * OPS-9: no deploys, `terraform apply`, `kubectl apply/delete`, `helm upgrade` or image pushes against shared
    environments unless the person asks;
  * GIT-4: no `--no-verify` or `--no-gpg-sign`, `--force-with-lease` only on unshared branches, after asking;
  * TST-12: no deleting, skipping or loosening tests to get green.
* **Commands that use the network, install or upgrade:**
  * the Codex install now downloads, reads, then runs the file it read, instead of piping into a shell;
  * no `npm audit fix --force` (SEC-13 and the skills);
  * `ng update` is proposed, not run (NG-10).
* **Commands that touch shared systems or other people's work:**
  * `EXPLAIN ANALYZE` only on a non-production database, writes rolled back (SQL-6);
  * load tests only in a designated environment (PERF-13);
  * migrations and data fixes against shared databases only on request (DB-1);
  * no `git add -A`: stage your own files by path after `git status` (GIT-9);
  * no branch deletion without asking (GIT-6).
* **Skills and integrations:**
  * the security-review secret search prints file names and counts, never values;
  * bug, security and dependency checkpoints keep secrets, personal data and unfixed-exploit details out of synced
    development records (ADR-0038);
  * the Playwright skill forbids `browser_run_code_unsafe`;
  * release smoke checks are read-only, never with personal credentials.

**8 · Missing practices, added.**

* **Verifying AI output:**
  * verify generated code and APIs against the installed versions; review subagent or other-AI output like a pull
    request (ENG-2);
  * dependency licences, with copyleft called out, and agreeing new dependencies with the person (ENG-7).
* **Security and privacy:**
  * personal data in SEC-1, a gap BE-12's reference exposed;
  * property-level authorisation and mass assignment (BE-6, OWASP API3);
  * public build-time env vars, such as `VITE_*` and `NEXT_PUBLIC_*`, are public (FE-9);
  * fork-PR secret exposure in CI (OPS-3);
  * `LIKE` escaping and array binding (SQL-1).
* **Data and frameworks:**
  * protecting the person's uncommitted work (GIT-9);
  * jobs dispatched inside transactions (`afterCommit`, LRV-9);
  * Flutter contrast and target-size checks with `meetsGuideline` (FLT-7).

**Workflow logic fixed (beyond the criteria).**

* **Code review.** The findings step depended on the gate step. A failing gate fails the run, so a change with
  failing tests never got a review. Findings now need only the intent step, and they run while the gates run. Sign-off
  needs both. Findings are recorded as they are made, because a failed gate stops the run at that step and refuses
  later reports. Retrying the gate after a fix continues the run.
* **Bug fix.** The person's review could start before lint and typecheck had run; it now waits for them.

## Workspace structure and Git ignore rules

**Validated.** A new AI Engineering Pro workspace contains exactly:

* `.gitignore`, `README.md`, `nexus-workspace.json`, `nexus.lock.json`;
* `constitutions/`, `integrations/`, `skills/`, `workflows/`;
* the clone folder `projects/`, the only empty folder.

`.gitignore` covers:

* the clone folders `projects/` and `repos/` (legacy);
* `.nexus-local/`;
* SQLite files and their WAL/SHM files;
* `.env` and `.env.*`;
* `.DS_Store` and `Thumbs.db` (added).

**Gaps found and fixed.** An independent project repository could still end up in the workspace repository:

1. **A project registered inside the workspace but outside `projects/`** (e.g. `code/app`) was not ignored. Git shows
   it as an untracked folder; staging it records an embedded repository (a gitlink). Fix: `project::register` adds
   `/<path>/` to `.gitignore` through `workspace::ensure_ignored`. Nothing is added for paths under `projects/` or
   `repos/`, or when a line already covers the path.
2. **An unregistered nested repository** was treated as an ordinary folder by the outgoing inspection. Fix:
   `workspace_sync::inspect` recognises an untracked folder containing `.git` as a gitlink.
3. **A gitlink was only a warning.** It now **blocks** the commit review: "an embedded Git repository … move it to
   projects/ or ignore it". This also covers a kit cloned inside the workspace.
4. **Repair for a removed ignore line.** The Workspace Doctor reports `projects-ignored` when a project inside the
   workspace is not ignored, using `git check-ignore` in a repository. Its repair `ignore-projects` shows the plan
   first, then appends the lines.

Test `workspace_sync::tests::generated_layout_and_independent_repositories_never_enter_the_workspace_repository` covers
the exact top-level layout, the empty-folder rule and the required ignore lines. It creates nested repositories in
`projects/`, `repos/`, a registered `code/inside` and an unregistered `stray`. Only `stray/` shows as changed, and the
commit review is blocked with a submodule finding. The test also removes an ignore line, runs the Doctor finding and
repair, and checks that the line is restored.

## Deliberately kept

* **Stack rules that repeat a general principle but name the stack's real sinks or tools.** Examples: SQL injection in
  SQL-1, PHP-6, PY-8 and LRV-7; shells in NODE-9. "See SEC-4" alone would lose the API knowledge that prevents the bug.
* **General rules that area documents refine** (PERF-4 vs DB, SEC-5 vs FE-8, ENG-8 vs NODE/OPS). BE, FE, DB and OPS
  are listed only for matching stacks, so the general rule must stand on its own.
* **GIT-3, although Claude Code commits only when asked.** It is short and adds the commit ≠ push, tags and PR-merge
  nuances.
* **REACT-5 (stable keys).** It is a frequent defect in generated code.
* **All 11 Angular rules.** Each one is Angular-specific and current.
* **The skills' report templates with "(this session)" fields.** They are how the skills enforce CORE-1 in practice.

## Verification

Run on the final code, one group at a time.

* **Rust:** `cargo test --workspace` 286 passed; clippy clean, with and without the e2e feature.
* **Frontend:** vitest 90/90; `npm run build` ok.
* **Desktop E2E (E2E build):**
  * workspace stage: portable 50/50, restore 12/12, constitutions 13/13, sync 18/18, context 9/9, workflows 24/24;
  * the rest: run-smoke 27/27, task-flow 29/29, MCP interop 9/9, profiles-kits 9/9, attention 27/27, usage 18/18.
* **Real Claude, synthetic profiles:** kits 19/19 (workspace skills), context 7/7, security 14/14.

## Follow-ups (not blockers)

* MANUAL_CHECKS 33: the owner's own read of the core and of two or three documents and skills they know well.
* The library is still template version 1.0.0. It was never released, so no installed workspace needs an update plan.
  The next content change bumps it.
* A failed gate fails the whole workflow run (engine semantics). Workflows that want to keep working past a failing
  gate, like code review, rely on recording findings as they go.

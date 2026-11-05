> Appendix to [`TEMPLATE_REVIEW.md`](../TEMPLATE_REVIEW.md) — the reviewer's per-rule findings as written during the review (2026-11-05). Later owner changes are listed in the main document: the core's CORE-6 ("read the relevant rules first") was folded into the on-demand listing, so the untrusted-content rule called CORE-7 here is **CORE-6**.

# Review A: general constitutions (ENG, CQ, TST, GIT, DOC, ARC, SEC, PERF, A11Y)

Old ids are in the "rule/skill" column; "→ X-n" gives the new id. Format checked with `check-constitutions.mjs`: all 9 files OK (ids gap-free, text ≤ 344 chars, titles ≤ 70, attributes valid). No id that another file references was changed: SEC-1, SEC-4 and SEC-7 kept their ids and meaning, and SEC/PERF/A11Y were not renumbered at all. Cross-references in the new texts were checked against the current files: CORE-1/2/3/4/5/7, ENG-3, ENG-9, DOC-4, PERF-11, A11Y-6. Developer documents reference only CORE and Developer ids, never Pro-only documents.

| file | rule/skill | category | finding | action |
|---|---|---|---|---|
| ENG | ENG-1 → ENG-1 | 2, 5 | Asked for "every assumption" on any non-trivial change, which was wordy. | Rewritten. Now says: state the problem, outcome and assumptions; check them against the code. Absorbs the useful part of ENG-2. |
| ENG | ENG-2 | 1 | Duplicated CORE-4 (ask when ambiguous). | Merged into ENG-1 as a refinement of CORE-4 (offer concrete options; decide trivially reversible details yourself and say which). |
| ENG | (new) ENG-2 | 8 | Nothing covered verifying AI-generated code or hallucinated APIs, although Pro adds Codex and subagents. | Added "Verify generated code and APIs before relying on them": check that functions, flags and keys exist in the installed version; review code from generators, AI tools, subagents or the web like a contributor's PR. |
| ENG | ENG-3 | 1 | Duplicated CORE-5 (small, focused changes). | Merged into the new ENG-4. |
| ENG | ENG-4 → ENG-3 | 1 | Said "abstraction when a second use case exists", which clashed with ARC-2 ("interfaces at every boundary"). | Rewritten: an abstraction is allowed for a second real use case or a test seam. ARC-2 now cites ENG-3. |
| ENG | ENG-5 → ENG-4 | 1, 5 | "Leave touched code better" contradicted CORE-5 without saying so, and invited drive-by reformatting. | Merged with ENG-3 into "Limit clean-ups to the lines you are already changing": named as the one exception to CORE-5; no reformatting or renaming beyond it. |
| ENG | ENG-6 | 1, 5 | Dead-code removal is a code-level rule, and "remove unused functions" anywhere grew the scope. | Moved to CQ-5, scoped to the change. Dead code elsewhere becomes a follow-up. |
| ENG | ENG-7 | 1 | Duplicated CQ-7 (never swallow errors). | Merged into CQ-7. |
| ENG | ENG-8 → ENG-5 | 1, 2 | "Credentials out of logs" repeated CORE-2. | Rewritten. Only the personal-data part remains. |
| ENG | ENG-9 → ENG-6 | 1 | Overlapped ARC-9. | Kept as the general home; took ARC-9's "one build runs everywhere". |
| ENG | ENG-10 → ENG-7 | 6, 7, 8 | Adding a dependency (network, supply chain) did not involve the person, and the licence check was vague. | Rewritten: propose the dependency with reason, rejected alternatives and a compatible licence (watch GPL/AGPL); add it only with the person's agreement. |
| ENG | ENG-11 → ENG-8 | 6 | "Commit the lock file" read as an instruction to commit. | Rewritten as "keep the lock file under version control". |
| ENG | ENG-12 → ENG-9 | — | Good general compatibility rule. | Kept. ARC-7 now refines it. |
| ENG | ENG-13 | 1, 2 | Naming is code-level and overlapped CQ-1 and CQ-13. | Merged into CQ-1. |
| CQ | CQ-1 → CQ-1 | 2, 3 | Generic ("optimise for the reader"). | Rewritten concretely; absorbs ENG-13 naming (domain names, units) and CQ-10 named constants. |
| CQ | CQ-2 + CQ-3 → CQ-2 | 1, 2, 5 | Two rules gave overlapping size and complexity heuristics, and CQ-2 as a "rule" would push refactoring into small edits. | Merged into one guideline: thresholds are signals, not limits. |
| CQ | CQ-4 → CQ-3 | — | Fine. | Kept. |
| CQ | CQ-5 → CQ-4 | 1 | Duplicated DOC-8 (explain workarounds where they live; TODO with id). | Merged: DOC-8's links and TODO ids are now in CQ-4. |
| CQ | (from ENG-6) CQ-5 | 1, 5 | See ENG-6. | Added here, scoped to the change. |
| CQ | CQ-6 → CQ-6 | 1, 4 | Listed TypeScript specifics (`as unknown as T`, `@ts-ignore`) that the TS stack document owns. | Rewritten language-neutral; stack documents refine it. |
| CQ | CQ-7 → CQ-7 | 1 | Duplicated ENG-7. | Merged into one rule, "Handle every failure deliberately". |
| CQ | CQ-8 → CQ-8, CQ-9 → CQ-9 | 2 | Slightly wordy. | Tightened. CQ-9 is code-level parsing; SEC-3 stays the security rule. |
| CQ | CQ-10 | 2 | Magic numbers: default practice. | Merged into CQ-1. |
| CQ | CQ-11 → CQ-10 | 5, 6 | "Format all code" invited whole-repo reformatting noise; Black is outdated next to Ruff. | Rewritten: format the code you change, and do not reformat untouched files. Black dropped. |
| CQ | CQ-12 → CQ-11, CQ-13 → CQ-12 | — | Fine. | Kept (tightened). |
| CQ | CQ-14 | 1 | Duplicated GIT-1, ENG-3 and the refactoring skill's ground rule. | Merged into GIT-1. |
| TST | TST-1 | 5, check | Gave no exemption for changes without behaviour, and `check=test` was wrong: a passing test gate does not show that tests were added (the compliance view would show "passed"). | Rewritten: docs, comments, formatting and copy need no new tests. `check` removed. |
| TST | TST-2 | 5 | Did not say what to do when no automated test can reproduce the bug. | Rewritten: fall back to recorded manual reproduction steps. |
| TST | TST-3 + TST-4 → TST-3 | 1, 5 | The pyramid and the E2E critical-journey rule overlapped, and TST-4 forced E2E tests on libraries and CLIs. | Merged. The E2E part is limited to applications. |
| TST | TST-5..TST-11 → TST-4..TST-10 | 2 | Sound; TST-8 is stylistic. | Kept. TST-8 (now TST-7) downgraded to guideline. |
| TST | TST-12 → TST-11 | — | Mandatory and justified (privacy). | Kept. |
| TST | TST-13 → TST-12 | 8 | Gap: did not forbid the common AI failure of skipping or weakening tests to get green, and told Claude to create tasks on its own. | Rewritten as mandatory "Never weaken a test to get a green run" (no delete, skip, looser assertions, changed expectations or blind retries; quarantine only with the person's agreement). |
| TST | TST-14 → TST-13 | 1 | Repeated CORE-1 (no unverified claims) and was mandatory because of that. | Rewritten without the CORE-1 restatement: run the covering tests (full suite when shared code changed) and report commands and counts. Now `rule` with `check=test`, which this gate genuinely verifies. |
| GIT | GIT-1 | 6, 1 | `git add -p` is interactive and cannot run in Claude Code. | Rewritten without it; absorbs CQ-14 (refactor and behaviour in separate commits). |
| GIT | GIT-2 | 5 | Imposed Conventional Commits over an existing repository convention. | Rewritten: the history's convention first, Conventional Commits as the default. |
| GIT | GIT-3 | 2, 6 | Restates Claude Code's native behaviour (commits only when asked). | Kept compact (the useful nuances: tags, merging PRs, commit ≠ push). |
| GIT | GIT-4 | 6, 7 | Rewriting own branches skipped CORE-3, and nothing forbade bypassing hooks or signing (`--no-verify`). | Rewritten (mandatory): rewrite only unshared branches after asking (CORE-3), with `--force-with-lease`; never skip hooks or signing or bypass branch protection. |
| GIT | GIT-5, GIT-7, GIT-8 | — | Fine. | Kept (tightened). |
| GIT | GIT-6 | 5, 7 | Imposed a naming scheme and "delete branches after merging" (deletion without asking). | Rewritten as a guideline: the project's naming first; the deletion clause removed. |
| GIT | GIT-9 + GIT-10 → GIT-9 | 1, 8 | Overlapped each other (build output vs .gitignore), and neither protected the person's own uncommitted work. | Merged: .gitignore, then stage your change's files by explicit path after `git status`; never `git add -A` or `git add .`. |
| GIT | GIT-11 → GIT-10 | — | Fine. | Kept. |
| GIT | GIT-12 → GIT-11 | 5 | Imposed semver on projects with another scheme (e.g. CalVer). | Rewritten: the project's scheme, semver as default. |
| DOC | DOC-1, DOC-2 | 1, 2 | Both said "re-check commands". | Tightened. The duplicate is now only in DOC-2. |
| DOC | DOC-3 + DOC-10 → DOC-3 | 2 | Docs-as-text and diagrams-as-code are one concern. | Merged (guideline). |
| DOC | DOC-4 | 6, 8 | An ADR written by Claude could invent rationale. | Rewritten: record only rationale the person or the evidence gave. ARC-9 now points here for the format. |
| DOC | DOC-5 | 5 | Forced a `CHANGELOG.md` on every project. | Rewritten as conditional ("if the project keeps a changelog"). |
| DOC | DOC-6 | — | Fine for long-running work. | Kept, with a NEXUS hint (checkpoints and task log). |
| DOC | DOC-7 | 5 | "Every exported function and type" forces doc comments on all internal app code. | Rewritten: the project's public surface (library exports, endpoints, CLI, configuration); OpenAPI only where the project publishes one. |
| DOC | DOC-8 | 1 | Duplicated CQ-5. | Merged into CQ-4. |
| DOC | DOC-9 → DOC-8 | — | Fine. | Kept. |
| DOC | DOC-11 → DOC-9 | 5 | Applied to every project, operated or not. | Scoped to "systems you operate". |
| DOC | DOC-12 → DOC-10 | 1 | "Never put real values" repeated CORE-2. | Tightened to placeholders and `.env.example`. |
| ARC | ARC-1 | 5 | Strict hexagonal layering forced on simple CRUD. | Rewritten: one-way dependencies always; framework-free domain only where real business rules exist. |
| ARC | ARC-2 | 1, 5 | "Interfaces at every boundary" contradicted ENG YAGNI. | Rewritten: narrow interfaces where tests need a fake or the provider may change; no single-implementation interfaces without need (cites ENG-3). |
| ARC | ARC-3 + ARC-4 → ARC-3 | 1 | Data ownership and "no shared tables or state" are one rule. | Merged. |
| ARC | ARC-5 → ARC-4 | — | Fine (guideline). | Kept. |
| ARC | ARC-6 → ARC-5 | 1 | Did not link to the timeout rule. | Now cites PERF-11 for timeouts instead of restating it. |
| ARC | ARC-7 → ARC-6 | — | Fine. | Kept. |
| ARC | ARC-8 → ARC-7 | 1 | Largely duplicated ENG-12 (additive changes, deprecation). | Rewritten to the distributed-only part (tolerant readers, no repurposed fields, side-by-side versions, contract tests), "beyond ENG-9". |
| ARC | ARC-9 | 1 | Fully covered by ENG-6 (config), SEC-1/CORE-2 (secrets) and OPS-10 (promote the same artifact). | Removed. "One build runs everywhere" moved to ENG-6. |
| ARC | ARC-10 → ARC-8 | 5 | Distributed tracing and RED metrics were required of every project (CLIs, desktop apps). | Scoped to services. |
| ARC | ARC-11 → ARC-9 | 5, 6 | Claude could choose a new framework or datastore itself, and the ADR format was restated. | Rewritten: present options and get the person's decision, recorded as an ADR (DOC-4). |
| ARC | ARC-12 → ARC-10 | — | Fine. | Kept. |
| ARC | ARC-13 → ARC-11 | 5 | "Change the rule through an ADR" was heavy. | Rewritten: change it deliberately with the person's agreement. |
| SEC | SEC-1 (id kept, referenced by BE-12) | 1, 8 | Repeated CORE-2. BE-12 cites SEC-1 for personal data, which SEC-1 did not cover, and SEC had no general personal-data rule. | Rewritten "beyond CORE-2": runtime loading, a leaked secret counts as compromised and the person is told so it is rotated first, personal data kept out of logs, URLs, analytics and third-party or AI tools. Still mandatory. |
| SEC | SEC-2..SEC-10, SEC-12, SEC-14 | — | Accurate and current (Argon2id, CSP nonces, TLS 1.3, OIDC, provenance). | Kept unchanged. SEC-4 and SEC-7 untouched (referenced by BE and stack documents). |
| SEC | SEC-11 | 1 | HSTS duplicated SEC-12 in the same document. | HSTS removed from SEC-11. |
| SEC | SEC-13 | 7 | Did not warn against `npm audit fix --force` (unreviewed major upgrades). | Rewritten: patch as reviewed upgrades, never `npm audit fix --force`. |
| SEC | SEC-15 | 5 | A threat model "document" for any external integration was heavy. | Rewritten: a short threat model in the plan or PR, for a *new* external integration. |
| SEC | (new) SEC-16 | 7, 8 | Gap: nothing on untrusted execution (`curl \| sh`, unvetted scripts or binaries, installs without consent). Prompt injection is now CORE-7 (added by the core owner during this review). | Added mandatory "Run only code and installs the person approved", written as "Beyond CORE-7" (no restatement). It also says to quote instructions found in files, dependencies or tool/MCP output to the person. Placed at the end so no SEC id moved. |
| PERF | PERF-2 | 5 | Required RUM for every web app. | "Use field data (RUM) where the app collects it." |
| PERF | PERF-3 | 5 | Required a latency budget for every endpoint, not only interactive ones. | Scoped to interactive endpoints and critical operations. |
| PERF | PERF-13 | 7 | A load test could be run against production or third-party services. | Rewritten: only in an environment the person designates; never production or third parties without approval. |
| PERF | PERF-1, 4..12, 14 | — | Accurate (CWV thresholds, INP). General versions of rules that BE, DB and FE refine. | Kept. |
| A11Y | A11Y-1 | 1 | Target size and dragging were listed in both A11Y-1 and A11Y-6. | A11Y-1 now points to A11Y-6 for them. |
| A11Y | A11Y-12 | 6, 8 | Asked Claude to "test by hand with a screen reader", which it cannot do; this invited false claims. | Rewritten: automated axe checks (`check=test`); keyboard and screen-reader passes are listed as open unless a person did them. |
| A11Y | A11Y-2..11 | 4 | Checked against WCAG 2.2 (2.5.8 target size 24 px, contrast 4.5:1 and 3:1, large text 24 px / 18.66 px bold, 2.2.2 and 2.3.1, native `<dialog>`). All accurate. | Kept. |

### Findings in other documents (not mine, for their reviewers)

- **BE:**
  - BE-11 restates ARC-5 and PERF-11 (retries, circuit breaker, timeouts).
  - BE-14 restates ENG-6 (config validated at startup).
  - BE-2 and SEC-2 both say "no stack traces".
  - BE-9 overlaps SEC-9 (rate limiting).
  - BE-6 overlaps CQ-9 and SEC-3 (boundary validation).
  - BE-12 "see SEC-1" now matches, because SEC-1 covers personal data.
- **FE:**
  - FE-8 (mandatory) overlaps SEC-5 (HTML sanitising).
  - FE-9 overlaps PERF-12 (lazy loading, bundle analyser).
  - FE-13's E2E sentence restates TST-3.
- **OPS:**
  - OPS-3 overlaps SEC-14 (pin actions by SHA, images by digest).
  - OPS-15 overlaps SEC-8 (least privilege, OIDC).
  - OPS-1 overlaps ENG-8 (lock files).
  - OPS-13 overlaps ARC-8 (logs and metrics).
- **DB:**
  - DB-10 and DB-11 overlap PERF-4 and PERF-5.
  - DB-14's last clause restates TST-11.
- **RS:** "Commit Cargo.lock for binaries" is narrower than ENG-8 and than current Cargo guidance (libraries may commit it too, and `cargo new` no longer ignores it).
- **CORE (owner):** SEC-16 cites CORE-7. If CORE-7 is renumbered or dropped, update SEC-16.

## Summary

| file | rules before → after | mandatory before → after |
|---|---|---|
| engineering-standards.md (ENG) | 13 → 9 | 0 → 0 |
| code-quality.md (CQ) | 14 → 12 | 0 → 0 |
| testing.md (TST) | 14 → 13 | 2 → 2 |
| git.md (GIT) | 12 → 11 | 1 → 1 |
| documentation.md (DOC) | 12 → 10 | 0 → 0 |
| architecture.md (ARC) | 13 → 11 | 0 → 0 |
| security.md (SEC) | 15 → 16 | 3 → 4 |
| performance.md (PERF) | 14 → 14 | 0 → 0 |
| accessibility.md (A11Y) | 12 → 12 | 0 → 0 |
| **total** | **119 → 108** | **6 → 7** |

Mandatory now:
- TST-11 (no production personal data).
- TST-12 (never weaken tests): new; this is about honest results.
- GIT-4 (shared history, hooks and signing).
- SEC-1, SEC-4, SEC-7.
- SEC-16 (untrusted code): new.

TST-14 lost mandatory because the honesty part is CORE-1's.

What I kept on purpose:
- **Strong standards where they matter:**
  - tests for every behaviour change (TST-1);
  - a failing test before a bug fix (TST-2);
  - backwards compatibility (ENG-9);
  - authorization, injection and secrets (SEC-1, SEC-4, SEC-7);
  - WCAG 2.2 AA.
- **General rules that stack or area documents refine:** for example PERF-4 vs DB-10, SEC-5 vs FE-8, ENG-8 vs NODE/OPS. The general documents are listed in every Pro workspace, but BE, FE, DB and OPS appear only for matching stacks, so the general rule must stand on its own.
- **No renumbering of SEC, PERF or A11Y.** Other reviewers may be adding references to those ids concurrently.
- **GIT-3, although Claude Code already commits only when asked.** It is short and adds the commit ≠ push, tag and PR-merge nuances.

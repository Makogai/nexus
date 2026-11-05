---
name: code-review
description: Reviews a diff, branch or pull request and returns severity-rated file:line findings with a verdict. Use when asked to review a change or pull request.
---

# Code review

Judge whether a change does what it intends, safely and maintainably, and tell the author exactly what to change.
The output is a review, not a rewrite.

## Scale

A small diff outside risky areas gets a short review: intent, every changed line read, correctness, security and
tests, a verdict. Use all passes and reference.md for larger changes and for risky areas (money, authentication and
authorisation, data writes, migrations, concurrency, public APIs).

## Ground rules

- Do not edit the code under review unless asked. Suggestions are concrete: the exact change or a short snippet.
- Every finding has a severity, a file:line, what is wrong, why it matters (impact or failure scenario) and a fix. No
  vague findings ("consider improving error handling").
- Verify before asserting: read the surrounding code and callers. When unsure, ask a question instead of claiming.
- Say which checks you ran and which you only read; never infer "tests pass" from a badge or someone's claim.
- Secrets in the diff: report file:line and the kind of secret only; never repeat the value.

## Procedure

1. **Scope.** Working tree (`git diff`, `git diff --staged`), branch (`git diff <base>...HEAD`, with `<base>` the
   target branch), commits (`git log --oneline <base>..HEAD`) or a pull request (`gh pr view`, `gh pr diff` when gh is
   installed and signed in). Size it with `git diff --stat`. If the non-generated diff is larger than one sitting
   allows (GIT-* sets the size guideline), propose splitting it, or review module by module and state the coverage.
2. **Intent.** Read the PR description, the linked task (with NEXUS: `get_task`) and the commit messages; restate the
   intent in one or two sentences. If it is unclear, ask before judging the design. A change that does something other
   than its stated intent is itself a finding.
3. **Standards.** Read the on-demand constitutions for the touched areas (with NEXUS, `get_constitution` with ids
   such as `["CQ-*", "TST-*"]`, `["SEC-*"]` when input, identity, data or dependencies change, and the stack documents
   listed for this project), plus CLAUDE.md, linter configuration and neighbouring code. A violated mandatory rule is
   always blocking; cite its id.
4. **Read in a useful order.** Tests and public interfaces first (what does it promise?), then the core logic, then
   the rest. Follow callers and callees outside the diff whenever behaviour depends on them.
5. **Review in passes**: correctness, security, tests, design and readability, performance, compatibility (API,
   schema, configuration, wire formats), operability, docs. reference.md holds the questions for each pass.
6. **Checks (optional).** Run tests, lint or build when the person asked for it or agrees; report exactly what ran
   and the result.
7. **Classify each finding.**
   - **Blocking:** bug, data loss or corruption, security issue, violated mandatory rule, changed behaviour in risky
     code without tests, breaking change without migration or version bump.
   - **Should fix:** maintainability problems, missing edge-case tests, misleading names, unjustified violation of a
     non-mandatory rule, missing docs for a user-visible change.
   - **Nit:** preference not enforced by tooling. Keep nits few; for a repeated one, suggest a lint rule instead.
   - **Question:** something you could not resolve from the code.
   Also note briefly and specifically what is done well.
8. **Verdict.** **Approve** (nits and questions at most), **approve with changes** (should-fix findings only; the
   author can merge after addressing them) or **request changes** (at least one blocking finding). State what would
   change the verdict.
9. **Record (optional).** With NEXUS, `checkpoint` kind `note` with scope, verdict and finding counts. In a workflow
   run, follow `get_workflow_run` and report the step with `report_step`; `review` steps still need the person's
   sign-off. Create tasks for findings, or post comments to a pull request, only when the person asks: comments
   appear under their name.

## Exit criteria

- Intent restated; every changed file was read or explicitly listed as not reviewed (generated, vendored, out of
  scope).
- Every finding has severity, location, impact and a concrete suggestion.
- A verdict is given, with the checks that were actually run.

## Report

```markdown
## Review: <scope> (<n> files, +<a>/-<d>)
Intent: <one or two sentences>
Verdict: request changes - 1 blocking, 2 should fix, 1 nit
Checks (this session): `npm test` -> passed | not run (read-only review)

### Blocking
1. `src/orders/service.ts:88` - Discount applied after tax, so totals are overstated for taxed regions.
   Suggest: apply `applyDiscount` before `addTax` and add a test for a taxed order.
### Should fix
### Nits
### Questions
### Done well
Not reviewed: <files or areas, with reason>
```

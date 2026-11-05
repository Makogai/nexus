---
name: code-review
description: Reviews a diff or branch for correctness, security, tests, design and docs, giving classified file:line findings and a clear verdict. Use when asked to review a change, pull request or branch.
---

# Code review

Judge whether a change does what it intends, safely and maintainably, and tell the author exactly what to change.
The output is a review, not a rewrite.

## Ground rules

- Do not edit the author's code unless asked. Suggestions are concrete: the exact change or a short snippet.
- Every finding has a severity, a file:line, what is wrong, why it matters (impact or failure scenario) and a fix.
  No vague findings ("consider improving error handling").
- Verify before asserting: read the surrounding code and callers. When unsure, ask a question instead of claiming.
- Say which checks you ran (tests, lint, build) and which you only read. Never infer "tests pass" from a badge or
  a claim you did not see.
- Secrets in the diff: report file:line and the kind of secret only; never repeat the value.

## Procedure

1. **Determine scope.** Working tree (`git diff`, `git diff --staged`), branch (`git diff <base>...HEAD`, with
   `<base>` the target branch), commits (`git log --oneline <base>..HEAD`), or a pull request (`gh pr view` and
   `gh pr diff` when available). Size it with `git diff --stat`. If the non-generated diff exceeds about 800
   lines, propose splitting it, or review module by module and state the coverage.
2. **Understand intent first.** Read the PR description, linked task (with NEXUS: `get_task`, `search_knowledge`)
   and commit messages. Restate the intent in one or two sentences. If intent is unclear, ask before judging the
   design. A change that does something other than its stated intent is itself a finding.
3. **Load the standards.** If the NEXUS tools are available, call `get_constitution`: without `ids` for all
   effective rules, or with prefixes for the touched areas, e.g. `["CQ-*", "TST-*", "SEC-*", "GIT-*"]` plus the
   stack (`["TS-*", "VUE-*"]`, `["RS-*"]`, ...). Also read CLAUDE.md, linter configuration and neighbouring code.
   A violated mandatory rule is always blocking; cite its id.
4. **Read the change in a useful order.** Tests and public interfaces first (what does it promise?), then the core
   logic, then the rest. Follow callers and callees outside the diff whenever behaviour depends on them.
5. **Review in passes**, using reference.md for the full checklist: correctness, security, tests, design and
   readability, performance, compatibility (API, schema, configuration, wire formats), operability (errors,
   logging, observability), docs.
6. **Checks (optional).** Run tests, lint or build when the person asked for it or agrees. Report exactly what ran
   and the result.
7. **Classify each finding.**
   - **Blocking:** bug, data loss or corruption, security issue, violated mandatory rule, changed behaviour in
     risky code without tests, breaking change without migration or version bump.
   - **Should fix:** maintainability problems, missing edge-case tests, misleading names, unjustified violation of
     a non-mandatory rule, missing docs for user-visible change.
   - **Nit:** preference not enforced by tooling. Keep nits few; when a nit is tool-enforceable, suggest the lint
     rule instead of repeating it.
   - **Question:** something you could not resolve from the code.
   Also note briefly and specifically what is done well.
8. **Verdict.**
   - **Approve:** no blocking findings and nothing that should block a merge.
   - **Approve with changes:** should-fix findings only; the author can merge after addressing them.
   - **Request changes:** at least one blocking finding.
   State what would change the verdict.
9. **Record (optional).** With NEXUS, `checkpoint` kind `note` with scope, verdict and finding counts. If the
   review is a step of a workflow run, follow `get_workflow_run` and report it with `report_step`; steps of kind
   `review` still need the person's sign-off. Create tasks for findings only when the person asks.

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

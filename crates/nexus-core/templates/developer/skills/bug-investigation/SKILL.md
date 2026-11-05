---
name: bug-investigation
description: Finds a defect's root cause through reproduction and tested hypotheses, then fixes it with a regression test. Use when something is broken or misbehaving.
---

# Bug investigation

Turn a defect report into a confirmed root cause, a regression test that fails before and passes after a minimal
fix, and a short record of what was learned.

## Scale

An obvious, local defect with a clear reproduction (a typo, an inverted condition, a wrong constant) may go from step 2
straight to step 6: state the cause and why you are sure. Steps 3-5 are for everything less certain. The reproduction
and the regression test stay in every case; if the defect cannot be covered by an automated test, say why and name
the manual check that replaces it.

## Ground rules

- No fix without a reproduction or, when that is impossible, without saying so (see "Cannot reproduce").
- One hypothesis and one changed variable at a time.
- Fix the cause, not the symptom: no swallowed errors, no sleeps or retries to hide a race, no special case for the
  failing input unless that special case is the actual rule.
- Logs, traces and data can contain secrets or personal data: quote only the relevant lines, redact tokens,
  credentials, emails and identifiers, and keep such values out of tests, checkpoints and task logs.

## Procedure

1. **Capture the report.** Expected vs actual behaviour, exact error text, version or commit, environment (OS,
   configuration, data, locale/timezone), frequency, recent changes. If the NEXUS tools are available: `get_task`,
   `search_knowledge` for the error text or component (earlier notes and decisions may describe it), `get_checkpoints`
   for related recent work. Ask the person only for what you cannot find yourself.
2. **Reproduce** with the smallest reliable reproduction: a failing test, then a script, then a command sequence, then
   manual steps. Shrink inputs and steps until removing anything makes the failure disappear, and write down the exact
   command. Not reproduced after about three distinct attempts (vary data, configuration, timing, environment): go to
   "Cannot reproduce".
3. **Gather evidence.** Read the whole stack trace from the innermost project frame, the logs around the failure and
   the inputs at the failing boundary. Temporary instrumentation is fine; mark it for removal.
   **Regression?** `git log --oneline -- <paths>`, `git log -S '<symbol>'`, `git blame -L <start>,<end> <file>`. With a
   known-good commit and a scriptable reproduction, `git bisect start <bad> <good>` and `git bisect run <script>` (exit
   0 good, 1-124 bad, 125 skip), then `git bisect reset`. Bisect in a separate `git worktree` so the person's
   uncommitted work stays untouched, and ask first when each step needs an expensive rebuild.
4. **Hypotheses**, ranked by likelihood and cost to test. For each: the prediction ("if X is the cause, changing Y
   makes the reproduction pass"), the experiment, and the result (confirmed or refuted). After about five refuted
   hypotheses, check whether the code you read is the code that runs (stale build, cache, other version,
   configuration or dependency resolution).
5. **Root cause.** The causal chain from the defect (code, data or configuration) to the symptom in two to four
   sentences, with file:line. Search for the same pattern elsewhere (`git grep`) and list sibling occurrences.
6. **Regression test** at the lowest level that exercises the cause. Run it: it must fail for the bug's reason (the
   assertion about the bug, not a setup error).
7. **Minimal fix**: the smallest change that removes the cause; propose refactoring separately. If the right fix is
   large or risky (schema, public API, data repair), present options and ask.
8. **Verify.** The regression test and the original reproduction pass; then the affected module's tests (the full
   suite when shared code changed), lint and typecheck. Remove temporary instrumentation.
9. **Record** (with NEXUS, for anything beyond a trivial fix): `checkpoint` kind `decision` ("Root cause: ...; Fix:
   ...; Regression test: ..."), `update_task` with a log line and status, and `record_memory` kind `note`, scope
   `project`, if others will hit the same trap. If data was corrupted or users were affected, say what remediation is
   needed.
10. **Follow-ups**: sibling occurrences, missing validation or monitoring. Propose them as tasks; create them
    (`create_task`) when the person agrees or a workflow step asks for it.

## Cannot reproduce

- Say so plainly. Never ship a speculative change described as "the fix".
- Widen the comparison between the reporter's environment and yours: versions, configuration, feature flags, data
  shape, locale and timezone, OS, concurrency, caches. Check that the reported version matches the code you read.
- Look for nondeterminism: time, randomness, ordering (hash maps, parallel tests), races, retries, shared state.
- For intermittent failures, run the reproduction in a loop and report the failure rate (e.g. 3 of 200 runs).
- If the evidence points strongly at a cause, you may propose a defensive change plus diagnostics (boundary logging,
  assertions, metrics), labelled "unconfirmed", with how to confirm it next time.
- With NEXUS, record a `checkpoint` kind `blocker` listing what was tried, then ask the person for logs, data samples
  or access.

## Exit criteria

- Root cause stated with file:line and confirmed by experiment, or explicitly marked unconfirmed.
- A regression test failed without the fix and passes with it, both observed in this session (or the reason no
  automated test is possible, with the manual check).
- The checks ran after the fix, or what was not run is listed with the reason; temporary instrumentation is removed.

## Report

```markdown
## Bug: <one-line symptom>
Reproduction: `<command or steps>` (reliable | n of m runs)
Root cause: <causal chain> (<file:line>; introduced in <sha> if found) | unconfirmed: <best explanation>
Fix: <what changed, why it is minimal>
Evidence: regression test `<name>` failed before, passes after; `<suite command>` -> <result> (this session)
Not verified: <list or "none">
Follow-ups: <siblings, monitoring, remediation>
```

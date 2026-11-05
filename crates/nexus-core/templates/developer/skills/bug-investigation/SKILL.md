---
name: bug-investigation
description: Finds and fixes the root cause of a defect - reproduce, gather evidence, test hypotheses, regression test, minimal fix, verify, record. Use when something is broken, failing or behaving unexpectedly.
---

# Bug investigation

Turn a defect report into a confirmed root cause, a regression test that fails before and passes after a minimal
fix, and a record of what was learned.

## Ground rules

- No fix without a reproduction, or, when that is impossible, without saying so (see "Cannot reproduce").
- Test one hypothesis at a time and change one variable at a time.
- Fix the cause, not the symptom: no swallowed errors, no sleeps or retries to hide a race, no special case for
  the failing input unless that special case is the actual rule.
- Logs, traces and data can contain secrets or personal data. Quote only the relevant lines and redact tokens,
  credentials, emails and identifiers.
- Report a test or check as passing only if it ran in this session. Label every unconfirmed explanation as such.

## Procedure

1. **Capture the report.** Expected vs actual behaviour, exact error text, version or commit, environment (OS,
   configuration, data, locale/timezone), first seen, frequency (always or intermittent), recent changes. If the
   NEXUS tools are available: `get_task`, `search_knowledge` for the error text or component (earlier notes and
   decisions may describe it), `get_checkpoints` for recent related work. Ask the person only for what you cannot
   find yourself.
2. **Reproduce.** Aim for the smallest reliable reproduction: failing test, then script, then command sequence,
   then manual steps. Shrink inputs and steps until removing anything makes the failure disappear. Write down the
   exact reproduction command.
   - Reproduced: continue with step 3.
   - Not reproduced after about three distinct attempts (vary data, configuration, timing, environment): go to
     "Cannot reproduce".
3. **Gather evidence.** Read the whole stack trace, starting at the innermost frame in project code. Read logs
   around the failure and the inputs at the failing boundary. Add temporary instrumentation (logging, assertions)
   when needed and mark it for removal.
   **Regression?** If it used to work: `git log --oneline -- <paths>`, `git log -S '<symbol>'`,
   `git blame -L <start>,<end> <file>`. With a known-good commit and a scriptable reproduction, use
   `git bisect start <bad> <good>` and `git bisect run <script>` (exit 0 = good, 1-124 = bad, 125 = skip), then
   `git bisect reset`. Bisect needs a clean tree: run it in a separate `git worktree` so the person's uncommitted
   work stays untouched, and ask first when each step needs an expensive rebuild.
4. **Hypotheses.** List candidate causes ranked by likelihood and cost to test. For each, state the prediction
   ("if X is the cause, then changing Y makes the reproduction pass"), run the experiment, record confirmed or
   refuted. After about five refuted hypotheses, re-check your assumptions: is the code you read the code that
   runs (stale build, cache, different version, other configuration, different dependency resolution)?
5. **Root cause.** Explain the causal chain from the defect (code, data or configuration) to the observed symptom
   in two to four sentences, with file:line. Keep asking "why" until you reach the thing that should change.
   Search for the same pattern elsewhere (`git grep`) and list sibling occurrences.
6. **Regression test.** Write a test at the lowest level that exercises the root cause. Run it: it must fail, and
   fail for the right reason (the assertion about the bug, not a setup error).
7. **Minimal fix.** The smallest change that removes the cause; no opportunistic refactoring (propose it
   separately). If the correct fix is large or risky (schema, public API, data repair), present options and ask.
8. **Verify.** The regression test passes; the original reproduction passes; then run the affected module's tests,
   the full suite, lint and typecheck. Remove temporary instrumentation. Sanity check: revert only the fix, confirm
   the regression test fails again, restore the fix.
9. **Record.** With NEXUS: `checkpoint` kind `decision` with "Root cause: ...; Fix: ...; Regression test: ...".
   If it is a trap others will hit, `record_memory` kind `note`, scope `project`. `update_task` with a log line and
   status. If data was already corrupted or users were affected, say what remediation is needed.
10. **Follow-ups.** Sibling occurrences, missing monitoring or validation, related debt. Propose them as tasks;
    create them (`create_task`) only when the person agrees.

## Cannot reproduce

- Say so plainly. Never ship a speculative change described as "the fix".
- Widen the comparison between the reporter's environment and yours: versions, configuration, feature flags, data
  shape, locale and timezone, OS, concurrency, caches. Check that the reported version matches the code you read.
- Look for nondeterminism: time, randomness, ordering (hash maps, parallel tests), races, retries, shared state.
- For intermittent failures, run the reproduction in a loop and report the failure rate (e.g. 3 of 200 runs).
- If the evidence points strongly at a cause, you may propose a defensive change plus diagnostics (boundary
  logging, assertions, metrics), labelled "unconfirmed", with how to confirm it next time.
- With NEXUS, record a `checkpoint` kind `blocker` listing what was tried, then ask the person for logs, data
  samples or access.

## Exit criteria

- Root cause stated with file:line and confirmed by experiment, or explicitly marked unconfirmed.
- A regression test fails without the fix and passes with it (both observed in this session).
- The full suite and checks ran after the fix, or what was not run is listed with the reason.
- Temporary instrumentation removed; root cause and fix recorded.

## Report

```markdown
## Bug: <one-line symptom>
Reproduction: `<command or steps>` (reliable | n of m runs)
Root cause: <causal chain> (<file:line>; introduced in <sha> if found)
Fix: <what changed, why it is minimal>
Evidence: regression test `<name>` failed before, passes after; `<suite command>` -> <result> (this session)
Not verified: <list or "none">
Follow-ups: <siblings, monitoring, remediation>
```

---
name: development-continuation
description: Resumes paused work by checking recorded progress against the current Git and test state. Use when picking up an interrupted task, branch, session or handover.
---

# Development continuation

Pick up interrupted work safely: separate what was recorded from what is true now, establish a test baseline,
agree on the next step, then continue and keep the record current.

## Ground rules

- Recorded state (checkpoints, task logs, notes, resume summaries, handover messages) is a claim. Git output and
  test runs from this session are evidence. Label each fact **verified** or **recorded**.
- Change nothing until the baseline is established (step 5).
- Uncommitted changes and stashes may be the person's own. Never discard, reset, stash, rebase or commit them
  without consent.

## Procedure

1. **Collect the record.** If the NEXUS tools are available:
   - `get_resume_summary`: `verified` is Git state read now; `saved` is history; `next` is a suggestion.
   - `get_task` (this session's task, or the id the person names): status, checklist, last log entries.
   - `get_checkpoints` (limit 10-20) for decisions, blockers and progress across sessions.
   - `get_session_activity` with `previous: 1` to `3` for files changed and test/build commands in earlier sessions.
   - `get_workflow_run` when the task has a workflow run: which step is ready, the last gate failure.
   Without NEXUS: ask the person, and read handover notes, the PR description, TODOs and recent commit messages.
2. **Extract the claims.** List what the record says: done items, decisions and their reasons, the planned next
   step, known blockers, and any claimed test status.
3. **Verify Git yourself.** `git status --short --branch` (branch, upstream, ahead/behind), `git log --oneline -10`,
   `git diff --stat`, `git diff --staged --stat`, `git stash list`. If the task records a branch or commit, check
   it exists and is contained in HEAD (`git merge-base --is-ancestor <commit> HEAD`). For items claimed as done,
   confirm the code exists (grep the symbol, read the diff against the base branch).
4. **Compare record and reality.** Mark each claim confirmed, contradicted or unverifiable. Typical contradictions:
   a different branch checked out; recorded commits missing (rebased, reset, made on another machine, not pulled);
   uncommitted changes the record does not mention; checklist items ticked but code absent; upstream moved ahead.
5. **Establish the baseline.** Run the tests that cover the next step (and lint/typecheck when cheap) before editing
   anything. Ask first when commands install dependencies or need network or external services. Note the result:
   passing, failing (which tests, first error), or not runnable (why). Failures found here are pre-existing for this
   session; report them rather than silently folding a fix into the next step.
6. **Decide the next step.**
   - Record consistent with Git and baseline green: propose the next step from the record or checklist and
     continue, unless it is a significant decision that needs the person.
   - Record stale (upstream moved, old), contradicted, or baseline unexpectedly red: present the discrepancies
     with one to three options (e.g. pull first, fix the baseline, re-plan) and wait for confirmation.
   - No usable record: reconstruct intent from the branch diff and commit messages and present it as
     "reconstructed - please confirm" before continuing.
7. **Re-sync the record before working.** With NEXUS: `checkpoint` kind `progress` ("Resumed: verified <branch,
   commit, uncommitted>; discrepancies <...>; baseline <...>; next <...>"), and `update_task` with status
   `in-progress` if it was not, plus a log line.
8. **Continue** with the skill that fits the work (feature-development, bug-investigation, refactoring, ...). With
   NEXUS, checkpoint at milestones and before stopping: what is done (verified), what is in progress (exact file and
   function), the next step, open questions - written so a fresh session can resume without this conversation. For
   workflow runs, report steps with `report_step`.

## Exit criteria

- Git state verified in this session and compared against every recorded claim.
- A baseline test result from this session exists, or the reason it could not be run is stated.
- Discrepancies resolved with the person, or none found.
- With NEXUS, the record is updated with the verified state and the agreed next step before new work starts.

## Report

Give this before continuing with the work:

```markdown
## Resume: <task or branch>
Verified now: branch <name> (<ahead>/<behind> vs <upstream>), <n> uncommitted files, HEAD <sha subject>
Baseline (this session): `<command>` -> <passed | n failing: ... | not run: why>
Record says: <done items, decisions, planned next step> (recorded, <date of latest checkpoint>)
Discrepancies: <list, or "none">
Next step: <action> - <proceeding | awaiting your confirmation>
```

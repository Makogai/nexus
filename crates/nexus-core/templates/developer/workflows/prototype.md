---
nexus: 1
id: {{id}}
kind: workflow
format: workflow
title: Prototype
status: active
category: feature
rigor: prototype
skills: []
created: {{now}}
updated: {{now}}
source: import
---

Use this workflow to answer one specific question quickly, such as whether an approach, library or design works well enough, with minimal ceremony. The output is evidence and a decision, not production code. Tests, structure and documentation rules are relaxed on purpose; security and data rules are not. If the result is worth keeping, it is rebuilt or hardened through the Standard feature workflow.

## Goal and keep-or-throw decision
<!-- step: id=goal kind=claude -->

Write down the one question the prototype must answer (for example "can the grid render 10,000 rows with filtering in under 200 ms?"), the demo that would answer it, and a timebox. Decide up front whether the code is throwaway or a candidate to keep, and isolate it accordingly: a separate branch, a `prototype/` folder, or a feature flag that is off by default. Record all of this as a checkpoint (decision). If the question is vague, propose two or three sharper versions and report needs-input.

### Acceptance

- The question, the demo that answers it and the timebox are recorded as a checkpoint
- The code is declared throwaway or keep-candidate
- The prototype is isolated from production code paths (branch, folder or flag)

## Spike
<!-- step: id=spike kind=claude needs=goal -->

Build the smallest thing that answers the question. Take shortcuts deliberately (hard-coded configuration, happy path only, no abstractions, no polish) and keep a running list of them in the task log. Stop when the question is answered or the timebox is used up, whichever comes first; if time runs out, report blocked with what you learned instead of quietly widening the scope. Shortcuts never extend to security: no secrets in code or prompts, no real personal or customer data, no production endpoints or credentials, no disabled authentication on shared environments.

### Acceptance

- The shortcuts taken are listed in the task log
- No secrets, real personal data or production systems are used
- Work stopped at the answer or at the timebox

## Demo-able result
<!-- step: id=demo kind=claude needs=spike -->

Make the result runnable by someone else: the exact commands to start it, the inputs to use and what to look at. Capture evidence such as command output, timings and observed behaviour. Report numbers only if you measured them in this session, and say on what machine and with what data. State plainly when the evidence does not answer the question.

### Acceptance

- Run instructions are in the task log or in a README next to the prototype
- The evidence answers the question from the first step, or states that it does not

## Existing tests still pass
<!-- step: id=check kind=check needs=spike gates=test rigor=balanced -->

Joins the run only at balanced rigor or above. NEXUS runs the project's test gate to confirm the spike did not break existing behaviour. A project without a test gate runs this workflow at prototype rigor.

## Learnings and recommendation
<!-- step: id=learn kind=claude needs=demo,check -->

Record what was learned as a checkpoint (decision): what worked, what did not, measured limits, surprises, and what you would do differently. Save what future sessions need with `record_memory` (note). Recommend one outcome with a short reason: discard, iterate (with the next question to answer), or productionise. For productionise, list the work needed (tests, error handling, security review, removing each listed shortcut) and propose a follow-up task for the Standard feature workflow. Prototype code is never merged to the main branch as-is.

### Acceptance

- Learnings are recorded as a checkpoint
- The recommendation is discard, iterate or productionise, with a reason
- A productionise recommendation lists the shortcuts to remove and proposes a follow-up task

## Keep or discard
<!-- step: id=decide kind=review needs=learn -->

The person decides on the recommendation. Approve to accept it, then delete or archive throwaway code or start the follow-up task; request changes to send the learnings back to Claude with a question.

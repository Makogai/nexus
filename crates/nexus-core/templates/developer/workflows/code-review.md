---
nexus: 1
id: {{id}}
kind: workflow
format: workflow
title: Code review
status: active
category: review
rigor: balanced
skills: [workspace:code-review]
created: {{now}}
updated: {{now}}
source: import
---

Use this workflow to review a change (a pull request, branch, commit range or the working tree) before it is merged. Claude reads and reports findings but does not modify code. NEXUS runs the project's gates on the change, and a person makes the final decision.

## Understand the intent
<!-- step: id=intent kind=claude -->

Identify the change under review and its base (for example the target branch of the pull request). Read the description, the linked task or issue and the commit messages, then state in two or three sentences what the change is meant to do and how you would verify it. The gates run in the project folder, so the change must be checked out there; if it is not, ask the person to check it out and never stash, reset or discard their local work. If the non-generated change is larger than the size guideline in GIT-*, propose how to review it in parts.

### Acceptance

- The base and the head being compared are named
- The intended behaviour is summarised, or missing context is reported as needs-input
- The change under review is checked out in the project folder

## Automated checks
<!-- step: id=checks kind=check needs=intent gates=lint,typecheck,test -->

NEXUS runs the project's lint, typecheck and test gates on the checked-out change.

## Review findings
<!-- step: id=findings kind=claude needs=intent -->

Follow the code-review skill: read the full diff against the base and the surrounding code it touches, review correctness and security first, then tests, compatibility, performance and maintainability against the project's constitutions. For each finding give file and line, severity (blocking, should fix, nit or question), what is wrong, why it matters and a concrete suggestion. A failed gate stops the run at that step, so record findings as you go (checkpoints or the task log); the person then sees them next to the gate output. Label anything you did not verify. Do not edit code.

### Acceptance

- Each finding names file and line, severity and a suggested fix
- Findings are ordered by severity and recorded as checkpoints or in the task log
- The summary gives a verdict, states whether any blocking finding exists and which areas were not reviewed

## Sign-off
<!-- step: id=signoff kind=review needs=checks,findings -->

The person reads the findings and the gate results and decides whether to accept the change, request changes from its author, or reject it, and records that decision in the note. Approve to close the review; request changes to send the review back to Claude, for example to look deeper at one area.

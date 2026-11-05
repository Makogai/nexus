---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Engineering standards
status: active
applies: on-demand
prefix: ENG
created: {{now}}
updated: {{now}}
source: import
---

How to approach any change in any project: understand the problem, keep the change small and simple, handle errors and configuration deliberately, and protect public interfaces. Read it before starting or reviewing non-trivial work.

## Understanding before changing

Most wasted work comes from solving the wrong problem or one that was never stated.

#### Restate the problem and your assumptions before coding
<!-- rule: id=ENG-1 enforcement=rule -->
Before a non-trivial change, state in one or two sentences the problem, the expected outcome and every assumption about inputs, environment or existing behaviour. Verify assumptions against the code before relying on them, and label the ones you could not verify.

#### Ask when requirements are ambiguous instead of guessing
<!-- rule: id=ENG-2 enforcement=rule -->
When a requirement has two or more plausible readings that lead to different behaviour, data or interfaces, stop and ask with the concrete options and their consequences. Choose a default yourself only for trivially reversible details, and say which one you chose.

## Scope and simplicity

Small, simple changes are easier to review, test and revert.

#### Make the smallest change that solves the problem
<!-- rule: id=ENG-3 enforcement=rule -->
Change only what the task requires. Do not bundle unrelated fixes, renames or reformatting into the same change; record them as follow-ups. Every line of the diff should be explainable by the stated goal.

#### Build for today's requirements, not imagined ones
<!-- rule: id=ENG-4 enforcement=rule -->
Do not add configuration options, extension points, generic layers or abstractions for needs nobody has stated (YAGNI). Prefer the plainest construct that works and introduce an abstraction when a second real use case exists, not in anticipation of one.

#### Leave touched code better, within the scope of the task
<!-- rule: id=ENG-5 enforcement=guideline -->
In code you are already changing, fix a misleading name, a missing guard or an outdated comment when the fix is small and low risk. Larger cleanups outside the task become a separate change or a recorded follow-up, never silent scope growth.

#### Delete dead code instead of commenting it out
<!-- rule: id=ENG-6 enforcement=rule -->
Remove unused functions, unreachable branches, fully rolled-out feature flags and commented-out code once a search confirms nothing references them, including configuration, templates and reflection-based uses. Version control keeps the history.

## Errors and logging

Failures must be visible and diagnosable, never silently absorbed.

#### Decide explicitly what happens on every failure path
<!-- rule: id=ENG-7 enforcement=rule -->
Every error is either recovered from deliberately, propagated with added context, or reported to the user with an actionable message. Never return a default value, an empty result or a success status to hide a failure. If ignoring an error is correct, say why in a comment at that spot.

#### Log what helps diagnosis, at the right level, without noise
<!-- rule: id=ENG-8 enforcement=rule -->
Log significant state changes, failed external calls and decisions that explain later behaviour: error = needs action, warn = degraded, info = notable event, debug = detail. Do not log in tight loops, do not log and rethrow the same error, and keep personal data and credentials out of log messages.

## Configuration and dependencies

What varies between environments and what you import become long-term liabilities if handled casually.

#### Keep environment-specific values out of code
<!-- rule: id=ENG-9 enforcement=rule -->
Read values that differ between environments (URLs, hosts, limits, feature flags, credentials) from configuration or environment variables in one place at startup, validate them, and fail fast when a required value is missing. Do not branch on environment names inside business logic.

#### Justify every new dependency
<!-- rule: id=ENG-10 enforcement=rule -->
Before adding a dependency, check whether the standard library or an existing dependency already covers the need. If not, prefer an actively maintained, widely used package with a compatible licence, and state in the change why it was added and which alternatives were rejected.

#### Commit lock files and upgrade dependencies deliberately
<!-- rule: id=ENG-11 enforcement=rule -->
Commit the package manager's lock file and change it only through the package manager, never by hand. Upgrade dependencies in dedicated changes, read the changelog before a major version upgrade, and run the full test suite afterwards.

## Interfaces and naming

Names and contracts are what other people and programs come to depend on.

#### Keep public interfaces backwards compatible
<!-- rule: id=ENG-12 enforcement=rule -->
Do not remove, rename or change the meaning of a public API, CLI flag, configuration key, file format or event schema in a non-major change. Add the new form beside the old one, deprecate the old one with a removal plan, and call out any unavoidable breaking change in the change description.

#### Choose names that reveal intent
<!-- rule: id=ENG-13 enforcement=rule -->
Name variables, functions and types after their meaning in the domain, not their implementation (`overdueInvoices`, not `list2`; `isEligibleForRefund`, not `check`). Put units in names when the type does not carry them (`timeoutMs`) and avoid abbreviations the codebase does not already use.

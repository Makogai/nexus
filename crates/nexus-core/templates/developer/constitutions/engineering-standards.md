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

How to approach a change in any project: understand and verify before relying on anything, keep scope and design small, handle logging, configuration and dependencies deliberately, and protect public interfaces. Read it before starting or reviewing non-trivial work.

## Understanding and verification

Most wasted work comes from solving the wrong problem or building on something that is not true.

#### Restate the problem and your assumptions before non-trivial work
<!-- rule: id=ENG-1 enforcement=rule -->
For a non-trivial change, state the problem, the expected outcome and the assumptions it rests on, and check those assumptions against the code. When you must ask (CORE-4), offer the concrete options and their consequences; decide trivially reversible details yourself and say which you chose.

#### Verify generated code and APIs before relying on them
<!-- rule: id=ENG-2 enforcement=rule -->
Confirm that every library function, CLI flag, configuration key and framework feature you use exists in the version the project has installed (lock file, types, source or versioned docs), not from memory. Review code from generators, other AI tools, subagents or the web like a contributor's pull request before building on it.

## Scope and simplicity

Small, simple changes are easier to review, test and revert.

#### Build for today's requirements, not imagined ones
<!-- rule: id=ENG-3 enforcement=rule -->
Do not add configuration options, extension points or abstraction layers for needs nobody has stated. Prefer the plainest construct that works, and introduce an abstraction when a second real use case or a test seam needs it, not in anticipation of one.

#### Limit clean-ups to the lines you are already changing
<!-- rule: id=ENG-4 enforcement=rule -->
CORE-5 has one exception: in code you already change, fix a misleading name, a missing guard or a stale comment when the fix is small and safe. Do not reformat, rename or tidy anything beyond that; record larger clean-ups as follow-ups instead of growing the change silently.

## Logging and configuration

Failures must be diagnosable, and what differs between environments must not be hidden in code.

#### Log what helps diagnosis, at the right level
<!-- rule: id=ENG-5 enforcement=rule -->
Log significant state changes, failed external calls and decisions that explain later behaviour: error = needs action, warn = degraded, info = notable event, debug = detail. Do not log in tight loops or log and rethrow the same error, and keep personal data out of log messages.

#### Keep environment-specific values out of code
<!-- rule: id=ENG-6 enforcement=rule -->
Read values that differ between environments (URLs, hosts, limits, feature flags, credentials) from configuration or the environment in one place at startup, validate them and fail fast when a required one is missing, so one build runs everywhere. Never branch on environment names in business logic.

## Dependencies

Every dependency is code you ship, maintain and are liable for.

#### Justify every new dependency and agree it with the person
<!-- rule: id=ENG-7 enforcement=rule -->
First check whether the standard library or an existing dependency covers the need. Otherwise propose a maintained, widely used package whose licence fits the project (watch copyleft such as GPL or AGPL), say why and which alternatives you rejected, and add it only with the person's agreement.

#### Keep lock files authoritative and upgrade deliberately
<!-- rule: id=ENG-8 enforcement=rule -->
Keep the package manager's lock file under version control and change it only through the package manager, never by hand. Upgrade dependencies in dedicated changes, read the changelog before a major upgrade, and run the full test suite afterwards.

## Interfaces

Contracts are what other people and programs come to depend on.

#### Keep public interfaces backwards compatible
<!-- rule: id=ENG-9 enforcement=rule -->
Do not remove, rename or change the meaning of a public API, CLI flag, configuration key, file format or event schema in a non-major change. Add the new form beside the old one, deprecate the old one with a removal plan, and call out any unavoidable breaking change in the change description.

---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Documentation
status: active
applies: on-demand
prefix: DOC
created: {{now}}
updated: {{now}}
source: import
---

What to document, where, and how to keep it accurate: README, decisions, API reference, changelog, runbooks and configuration. Apply it when a change affects behaviour, setup, interfaces or operations; a change without such effects needs no documentation work.

## Essentials

Documentation is only useful if it is findable, current and versioned with the code.

#### Keep a README that gets a newcomer running
<!-- rule: id=DOC-1 enforcement=rule -->
The README states what the project does and for whom, prerequisites with versions, and copy-pasteable commands to install, run, test and release it. Link to deeper docs instead of growing the README beyond what a newcomer needs.

#### Update documentation in the same change as behaviour
<!-- rule: id=DOC-2 enforcement=rule -->
When a change alters behaviour, configuration, commands, APIs or setup, update the affected README sections, reference docs and examples in the same change, and re-check any commands you document. Documentation that lags behind the code is a defect.

#### Keep docs and diagrams as text next to the code
<!-- rule: id=DOC-3 enforcement=guideline -->
Store docs as Markdown in the repository (a README per package, `docs/` for guides and decisions) so they are versioned and reviewed with the code; use an external wiki only for content not tied to a code version. Draw diagrams in Mermaid or a similar text format, one question per diagram.

## Decisions and progress

Future maintainers need to know why things are the way they are and where work stands.

#### Record significant decisions as ADRs
<!-- rule: id=DOC-4 enforcement=rule -->
Write an Architecture Decision Record for decisions that are costly to reverse or that a future maintainer would question: a short numbered file with Context, Decision and Consequences. Record only rationale the person or the evidence gave, and supersede an ADR with a new one instead of rewriting it.

#### Keep the changelog current where the project has one
<!-- rule: id=DOC-5 enforcement=rule -->
If the project keeps a changelog, add every user-visible change (feature, fix, deprecation, breaking change, security fix) under its Unreleased heading in the existing format, e.g. Keep a Changelog. Write entries for users rather than copying commit messages, and give migration steps for breaking changes.

#### Record the current state of long-running work
<!-- rule: id=DOC-6 enforcement=rule -->
For work spanning several sessions or people, keep a short status note (with NEXUS: checkpoints and the task log): what works, what is in progress, what is next, open questions and known problems. Update it at the end of a session and replace outdated content instead of appending a history.

## Code and API reference

Reference documentation lets people use code without reading its implementation.

#### Document the project's public surface
<!-- rule: id=DOC-7 enforcement=rule -->
Give library exports, HTTP endpoints, CLI commands and configuration that others use reference docs covering purpose, parameters, results, errors and side effects, in the language's standard format (TSDoc, rustdoc, docstrings, PHPDoc, dartdoc). Keep an OpenAPI description in sync where the project publishes one.

#### Keep examples runnable
<!-- rule: id=DOC-8 enforcement=rule -->
Code samples in docs work against the current version: complete imports, real function names, realistic inputs. Prefer examples executed in CI (doctests, example projects, snippet tests); otherwise re-run them whenever the documented API changes.

## Operations and configuration

People running the system need exact, safe instructions at the moment something goes wrong.

#### Write runbooks for operational tasks
<!-- rule: id=DOC-9 enforcement=rule -->
For systems you operate, keep a runbook for each recurring or emergency task (deploy, rollback, restore from backup, credential rotation, a known alert) with prerequisites, exact steps, how to verify success and how to back out. Update it after any incident in which it was wrong or missing.

#### Document configuration by name, never by value
<!-- rule: id=DOC-10 enforcement=rule -->
List every environment variable and configuration key the application reads, with purpose, required or optional, default and format. Use placeholders such as `<your-api-key>` instead of real values, and keep a `.env.example` that contains placeholders only.

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

What to document, where, and how to keep it accurate: README, decisions, API reference, changelog, runbooks and configuration. Apply it whenever a change affects behaviour, setup, interfaces or operations.

## Essentials

Documentation is only useful if it is findable, current and versioned with the code.

#### Keep a README that gets a newcomer running
<!-- rule: id=DOC-1 enforcement=rule -->
The README states what the project does and for whom, prerequisites with versions, and copy-pasteable commands to install, run, test and deploy or release. Re-check those commands whenever setup changes, and link to deeper docs instead of growing the README beyond what a newcomer needs.

#### Update documentation in the same change as behaviour
<!-- rule: id=DOC-2 enforcement=rule -->
When a change alters behaviour, configuration, commands, APIs or setup, update the affected README sections, reference docs, examples and comments in the same commit or pull request. Documentation that lags behind the code is a defect.

#### Keep documentation next to the code it describes
<!-- rule: id=DOC-3 enforcement=guideline -->
Store docs as Markdown in the repository (a README per package or module, `docs/` for guides and decisions) so they are versioned, reviewed and changed with the code. Use an external wiki only for content that is not tied to a code version, and link to it from the repository.

## Decisions and progress

Future maintainers need to know why things are the way they are and where work stands.

#### Record significant decisions as ADRs
<!-- rule: id=DOC-4 enforcement=rule -->
Write an Architecture Decision Record for decisions that are costly to reverse or that a future maintainer would question. Use a short numbered file with Context, Decision and Consequences sections; when a decision changes, write a new ADR that supersedes the old one instead of rewriting it.

#### Maintain a changelog for user-visible changes
<!-- rule: id=DOC-5 enforcement=rule -->
Record every user-visible change (features, fixes, deprecations, breaking changes, security fixes) in `CHANGELOG.md` under an Unreleased heading, grouped by type in the Keep a Changelog style. Write entries for users rather than copying commit messages, and give migration steps for breaking changes.

#### Record the current state of long-running work
<!-- rule: id=DOC-6 enforcement=rule -->
For work spanning several sessions or people, keep a short status note: what works, what is in progress, what is next, open questions and known problems. Update it at the end of each session and replace outdated content instead of appending a history.

## Code and API reference

Reference documentation lets people use code without reading its implementation.

#### Document every public API
<!-- rule: id=DOC-7 enforcement=rule -->
Give every exported function, type, endpoint and CLI command a doc comment or reference entry covering purpose, parameters, return value, errors and side effects, in the language's standard format (TSDoc, rustdoc, docstrings, PHPDoc, dartdoc). Describe HTTP APIs in an OpenAPI document kept in sync with the code.

#### Explain non-obvious constraints where they live
<!-- rule: id=DOC-8 enforcement=rule -->
When code contains a workaround, a deliberate deviation, an ordering requirement or a performance trick, add a short comment at that spot with the reason and a link to the issue, ADR or upstream bug. Every TODO comment names a task or issue id so it can be tracked and removed.

#### Keep examples runnable
<!-- rule: id=DOC-9 enforcement=rule -->
Code samples in docs must work against the current version: complete imports, real function names, realistic inputs. Prefer examples executed in CI (doctests, example projects, snippet tests); where that is not possible, re-run them manually whenever the documented API changes.

#### Draw diagrams as code
<!-- rule: id=DOC-10 enforcement=guideline -->
Create architecture, sequence and state diagrams in Mermaid (or a similar text format) inside Markdown so they are diffable, reviewable and updated with the code. Keep each diagram focused on one question, and avoid binary image exports nobody can edit.

## Operations and configuration

People running the system need exact, safe instructions at the moment something goes wrong.

#### Write runbooks for operational tasks
<!-- rule: id=DOC-11 enforcement=rule -->
For each recurring or emergency operational task (deploy, rollback, restore from backup, credential rotation, a known alert), keep a runbook with prerequisites, exact steps, how to verify success and how to back out. Update it after any incident in which it was wrong or missing.

#### Document environment variables by name, never by value
<!-- rule: id=DOC-12 enforcement=rule -->
List every environment variable and configuration key the application reads, with purpose, required or optional, default and format. Never put real values in docs; use placeholders such as `<your-api-key>` and commit a `.env.example` that contains placeholders only.

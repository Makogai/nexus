---
name: learn-project
description: Build a NEXUS project's knowledge base in one lean pass — orient from NEXUS's map and the Graphify report, read a few chosen files, draft the overview and the architecture summary, record durable facts. Use in a NEXUS learning session; its system prompt names this skill and sets the budget.
---

# Learn a project for NEXUS

You write the first version of the knowledge base that future sessions start from. The person reviews every draft,
so aim for an accurate, cheap first pass — not an audit. The rules LEARN-1 to LEARN-7 and the budget (the depth) are
in your system prompt; this skill is the procedure. Work alone, in this session.

## 1. Orient before opening code

- The task's data block holds NEXUS's map: file counts, languages, top-level folders, commands, documentation files,
  history. If it is not in your context, call `get_task`.
- If `graphify-out/GRAPH_REPORT.md` exists, read it first: its communities and most-connected nodes show the structure
  for a fraction of the cost of reading files. For a precise question, `graphify query "<question>"` (offline) is
  cheaper than opening files.
- Before you open anything, plan which few files answer: what is this, how is it built, how does data flow.

## 2. Read selectively, in this order, until the budget is used

1. The README and the documentation the map lists (headings first; skim long files).
2. Manifests and configuration that define the stack (package.json, pubspec.yaml, Cargo.toml, pyproject.toml, go.mod,
   build files).
3. Entry points and wiring: main and app files, routing, dependency setup.
4. One representative file per major folder or graph community — not every file.

Every file you open counts against the budget. Prefer search (Grep, Glob) and reading a range of lines over whole
files. Skip lock files, generated code, build output, vendored code, assets and fixtures; read tests only when they
describe a behaviour more clearly than the code.

## 3. Draft the overview — `draft_project_document`, topic `overview`

Sections: **What it is** · **Who uses it** · **Main features** · **How to run and test** (the commands from the map) ·
**Status and gaps**.

## 4. Draft the architecture summary — `draft_project_document`, topic `architecture`

Sections: **Components** · **Data flow** · **Boundaries and integrations** · **Key decisions** · **Risks and open
questions**.

In both drafts, end each statement with *(verified: path)* when you saw it in the code, or *(inferred)* when you
concluded it — paths relative to the project, never absolute paths of this computer. Stay within the budget's word
limits; write for a developer who joins tomorrow. Each topic has one open draft: when you learn more, save the topic
again — it replaces your draft (revise it, do not add another).

## 5. Record durable facts — `record_memory`

As many as the budget allows: what a future session needs and cannot see quickly (conventions, pitfalls, where things
live, how to run things). Notes for facts; a proposed decision only for a choice clearly visible in the code. Do not
repeat the drafts.

## 6. Update the task — `update_task`

Check the checklist items you completed, and add one log line: how many files you read, what you skipped because of
the budget, and the open questions.

## 7. Stop and report

Tell the person in a few lines: the drafts wait in NEXUS under the project's **Learn** tab (*Drafts to review*), the
open questions, and what a deeper pass would cover. Do not read further until the person asks.

# ADR-0045 — Extra context for learning a project, and noticing a graph build

Status: Accepted (2026-11-06, owner feedback; branch `milestone/learn-context`). Extends ADR-0041 (lean learning).

## Context

Two owner reports from using *Learn* on `drivebay`, an app that lives in its own repository inside the owner's wrapper
repository `maiw`:

1. After building the Graphify graph in the terminal NEXUS prepared, the Learn tab still said *Build the graph* and the
   step stayed unticked, "even refreshing". The graph was there and NEXUS's map recognised it — the tab never read it
   again: the project page is kept alive while the person is in the terminal, and the only re-read was *Read again*
   in another card.
2. "drivebay lived in its own wrapper I made, maiw, so I'd like the learner to take that into account and learn from
   that too — a thing to add to the prompt."

## Decision

1. **The Learn tab notices a graph build.** After *Build the graph in a terminal* it watches for
   `graphify-out/GRAPH_REPORT.md` every 4 s for up to 30 minutes with a cheap check (`learn::graph`: one file's
   metadata and one ignore check, not the whole map) and reloads when the report appears; coming back to the kept-alive
   project page reads the project again, so does returning to the window; the graph card has *Check again*.
2. **Notes for Claude** (≤ 2,000 characters) — what the code does not say. They are task text: saved in the learning
   task's *Notes from the person* section when the session starts, before NEXUS's map and above the log, so the part of
   the task Claude reads always carries them. They reach Claude as data, like every task description (T22); never in
   the system prompt. Lines that would read as a heading or a checklist item are escaped.
3. **Folders Claude may also read** (at most five): typically the repository the project lives in — NEXUS offers it
   when the project's parent folder belongs to another Git working tree (`git rev-parse --show-toplevel`). They are
   **machine-local** (`learn_folders`, migration 0019) — a synced file must never widen what Claude may read — and
   checked when added and again at every launch: an existing folder outside the project, not a drive or the home folder
   (or above it), no credentials folder (`.ssh`, `.aws`, `.gnupg`, `.kube`, `.azure`, `.docker`, `.config`, `.claude`)
   and no Claude profile's configuration folder (inside or containing it), with a path NEXUS can quote. A missing folder
   is listed and skipped.
4. **Only learning sessions get them**, as `--add-dir <folder>` (one flag per folder, before the `--` of a first
   message) and rule **LEARN-8**: read only what explains the project (README, CLAUDE.md, docs, shared code it uses),
   within the same budget, never change them, name the folder a statement comes from. The rules the Learn tab shows are
   the ones the session gets; the session records the folders (`integrations.learn.folders`).

## Consequences

* A graph built in the terminal ticks without a manual re-read.
* A project in a monorepo or wrapper repository is learned with its surroundings, within the same budget, by choice.
* Claude's read scope grows only by folders the person chose on this computer, for learning sessions only.

## Alternatives considered

* **Storing the folders in the task or the workspace** — synced text would decide what Claude may read on another
  computer; machine-local keeps that decision with the person at the machine.
* **Putting the notes into the system prompt** — workspace text would become instructions (T22); the task carries them
  as data.
* **Registering the wrapper as its own project and linking projects** — heavier; a folder is enough for learning.

## Amendment (2026-11-06, owner report; branch `fix/autosync-and-drafts`)

* **One open draft per topic.** A learning session saved the architecture four times and the overview twice — every
  save became another draft for the person to sort out. `draft_project_document` now revises the topic's draft that is
  still waiting for review (the newest, where older ones exist) and says so (`revised: true`); after the person used or
  set aside a draft, the next save starts a new one. LEARN-6, the skill and the tool description say "revise, do not
  add".
* **No absolute paths in knowledge.** LEARN-5 asks for file paths relative to the project and never absolute paths of
  this computer; LEARN-8 names an extra folder by its name, not its path.

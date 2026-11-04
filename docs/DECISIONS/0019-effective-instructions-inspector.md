# ADR-0019 — Effective-instructions inspector: Claude's own layers first, NEXUS appends without duplicating

Status: Accepted, implemented (2026-11-04, M20)

## Context
Users need to see which instructions a session will actually run with: organisation policy, their own CLAUDE.md
files, project memory and rules — and what NEXUS adds. NEXUS must respect Claude's native precedence and
organisation policy, and must not duplicate or override instructions unnecessarily. Claude Code loads memory itself;
NEXUS cannot (and must not) change that.

## Decision
* `nexus_core::claude::instructions` reproduces Claude's loading model **read-only** and reports each layer with
  scope (managed, user, project, local), kind (memory, rule, import, AGENTS.md, policy), load state (at start,
  on demand, excluded, not loaded, documented fallback), size and the reason. File contents are never serialised.
* The model follows the Claude Code docs and was verified on Claude Code 2.1.311 with the `InstructionsLoaded`
  hook in synthetic profiles (docs/RESEARCH.md), including behaviour the docs leave open:
  * user memory and user rules follow `CLAUDE_CONFIG_DIR`;
  * every ancestor directory of the working directory is read, **past the Git root and the home folder**:
    `CLAUDE.md`, `.claude/CLAUDE.md`, `.claude/rules/**/*.md`, `CLAUDE.local.md`, plus `@imports` (relative to the
    importing file, ≤ 4 hops, not inside code);
  * consequently the default profile's `~/.claude/CLAUDE.md` and `~/.claude/rules/` reach *every* profile's
    sessions for projects inside the home folder (as project memory). The inspector flags this (threat T21);
  * `claudeMdExcludes` from managed, user and project settings is honoured (not for the managed CLAUDE.md);
    rules with `paths:` load on demand; subdirectory files below the working directory load lazily;
    `CLAUDE_CODE_DISABLE_CLAUDE_MDS` disables all of them;
  * AGENTS.md was not loaded in any probe, including with no CLAUDE.md in the chain; it is shown as a documented
    fallback, never claimed as loaded.
* Order: managed → user → ancestors from the filesystem root down to the working directory (per directory:
  `CLAUDE.md`, `.claude/CLAUDE.md`, rules, `CLAUDE.local.md`). The event order of `InstructionsLoaded` is not the
  concatenation order, so the documented order is used.
* Managed `claudeMd` instructions are shown as present only (policy values stay unread, ADR-0003).
* **No duplicates**: an approved always-on NEXUS rule whose text is already contained (normalised) in a file Claude
  loads at start is not appended; it is listed as "already in CLAUDE.md". NEXUS never writes CLAUDE.md, never
  uses `--system-prompt` (replace) and never reorders Claude's files — its text is appended after Claude's
  default system prompt.
* Surfaces: `nexus instructions [--profile] [--project|--cwd] [--task] [--json]`; desktop project page
  "Effective instructions" (per profile); both use `prepare::preview` (no launch, no task start).
* Regression against the real CLI: `e2e/instructions-real-claude.mjs` compares the inspector's at-start set with
  the files the real Claude reports through `InstructionsLoaded` (exact match required).

## Alternatives
* Parse `/context` or `/memory` output — needs an authenticated interactive session; brittle text format.
* Run Claude with an `InstructionsLoaded` hook for every inspection — spawns Claude, needs it installed and
  signed-in behaviour; kept as the regression test instead.
* Exclude the default profile's home-folder memory automatically via a session `claudeMdExcludes` — changes what
  the user's Claude does without being asked; offered later as an explicit per-profile option (see Consequences).

## Consequences
* The inspector can drift when Claude Code changes its loading rules; the real-CLI test detects that.
* Follow-up: an opt-in per-profile setting to exclude the default profile's `~/.claude` files through the
  session-scoped `--settings` (`claudeMdExcludes`), which stays within ADR-0004.

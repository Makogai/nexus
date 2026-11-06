# ADR-0046 — Skills page: every skill in one place, and an editor for workspace and project skills

Status: Accepted (2026-11-06, owner request "where do I edit and add skills?"; branch `milestone/skills-editor`).
Builds on ADR-0024 (AI Kits), ADR-0037 (workspace skills) and ADR-0041 (built-in skills).

## Context

A NEXUS session can get skills from four places — NEXUS's built-in plugin, the workspace's `skills/` folder, the
project's `.claude/skills/` and AI Kits — but NEXUS showed them in three different pages and could not create or edit
any: a workspace skill meant writing a file by hand, registering the folder in the Workspace Doctor and approving it
in AI Kits again after every edit.

## Decision

1. **One Skills page** lists every skill a session of the workspace can get — built-in, workspace, project, AI Kit —
   with how Claude names it (`workspace:<name>`, `<name>`, `<kit>:<name>`, `nexus:<name>`), which sessions it reaches and
   its state (active, needs review, in repository, built in). Filter and search; the SKILL.md opens in the editor.
2. **The person writes workspace and project skills** here: *New skill* (name, description, workspace or a project)
   writes a SKILL.md from a short template; edits are saved with the version they replace (a file changed since is
   never overwritten); a skill folder holding only SKILL.md can be deleted. Built-in and AI Kit skills are read-only.
   * **Where:** `<workspace>/skills/<name>/SKILL.md` or `<project>/.claude/skills/<name>/SKILL.md` — nothing else; the
     project must belong to the workspace and be on this computer; no write goes through a symbolic link; a workspace
     with a newer schema stays read-only.
   * **Validation (Claude Code's rules):** names of lowercase letters, digits and hyphens (≤ 64, not "anthropic" or
     "claude"); a one-line description of 1–1,024 characters without angle brackets (written as a quoted YAML value);
     frontmatter whose `name` is the folder's; at most 256 KB.
3. **Approval follows the person, not the file.** A workspace skill the person saves here is approved on this machine
   with the save — only when the workspace skills were approved before (or there were none). If other changes wait for
   review (a sync, Claude writing a file), the save leaves the approval to AI Kits and says so. Project skills are the
   project's files: Claude Code loads them itself (also outside NEXUS); the change shows up in the project's Git status
   for the person to commit.
4. **Only the person writes skills through NEXUS** — the desktop, on an explicit action. No MCP tool creates or changes
   skills (ADR-0009); NEXUS still never writes Claude's instruction files or configuration (ADR-0019).

## Consequences

* Adding or changing a skill is one page; workspace skills travel with the workspace (autonomous sync publishes them),
  and another computer approves them before its sessions use them.
* The project explorer stays read-only; a project skill is the one place where NEXUS writes into a project, and only
  the folder the person edits.

## Alternatives considered

* **Editing skills inside AI Kits** — kits are other people's plugins, reviewed as a whole; the person's own skills
  need an editor, not a review queue.
* **Letting Claude write skills through MCP** — a session could then give itself (or every session) new instructions;
  skills stay the person's.

# NEXUS — Product Requirements

**NEXUS Desktop** is a local-first desktop operating environment for AI-first software development with
Claude Code. It organises *who* is working (Claude profiles), *where* (workspaces and projects), *how hard*
(work profiles), and *what happened* (sessions, memory, tasks) — and gets out of the way of Claude Code's own
terminal experience.

## Users and jobs

* **Solo/multi-context developer** with a work account and a personal account who must never mix them.
* **Professional engineer** maintaining several repositories who returns to a project after weeks.
* **Experimenter** building quick prototypes who wants zero ceremony.

Core job: *open NEXUS → pick workspace, Claude profile, project and rigor → Claude starts with the right
account, configuration and context → come back later and know exactly where work stopped.*

## Core concepts

| Concept | Definition | Persistence |
| --- | --- | --- |
| **Claude profile** | A Claude Code environment: config directory (or the default location), executable, defaults. Usually one per account or per kit setup. | SQLite (machine-local) |
| **Workspace** | A folder (optionally a Git repo) grouping projects and portable knowledge. Work and personal are separate workspaces. | `nexus-workspace.json` + SQLite binding |
| **Project** | An independent Git repository registered in a workspace. May live inside or outside the workspace folder. | manifest entry + SQLite path binding |
| **Work profile** | Engineering rigor: Prototype, Balanced, Professional, Critical. Separate from Claude's native effort. | built-in data (custom later) |
| **Session** | A NEXUS-managed terminal running Claude Code (or a shell). Has state, exit code, profile, project. | SQLite |
| **Conversation** | Claude's own resumable transcript, identified by the session ID NEXUS assigned. | Claude config dir |

## Behavioural requirements (MVP)

1. **Isolation.** A session launched with profile P sees exactly P's configuration and credentials. The default
   profile is launched without `CLAUDE_CONFIG_DIR`. No variable from NEXUS's own environment can redirect the
   account (scrub list, ADR-0003).
2. **Zero footprint.** NEXUS never edits Claude config dirs it did not create, never edits organisation policy,
   and never stores credentials.
3. **Truthful state.** Session states are *running*, *exited (code)*, *failed (error)* or *interrupted*. A
   conversation is offered for resume only when Claude recorded one. Nothing is simulated.
4. **Deterministic first.** Stack/script detection, git state and recommendations use rules, not model calls.
5. **Real terminal.** Claude's TUI runs in a real PTY with full colour, resizing and keyboard handling.
6. **Offline.** Everything except Claude's own network use works offline.
7. **Explainable policy.** When organisation policy blocks a NEXUS feature, NEXUS says which key and degrades.

## Non-goals (for now)

* A hosted NEXUS backend or account system.
* Replacing Claude Code features: permission system, plugin installation, subagents/workflows, transcripts.
* Multi-provider AI abstraction beyond the Codex review adapter.
* Estimating subscription spend.

## Quality bar

* Cold start < 1.5 s to interactive on a mid-range laptop; terminal input latency indistinguishable from
  Windows Terminal for typing.
* Every error state tells the user what happened and what to do next.
* Keyboard-first: every primary action reachable from the command palette.

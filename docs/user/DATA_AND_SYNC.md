# What stays local, what can sync

## Stays on this machine — always
In `%LOCALAPPDATA%\NEXUS` (never synced, never sent anywhere):

* Claude profiles registered in NEXUS, their sign-in classification (no identity), diagnostics
* sessions, their activity and checkpoints, process information
* **usage analytics** (token counts parsed from Claude's local transcripts)
* notification history, attention state
* **approvals** (of rules, constitutions, workflows, AI Kits, gate commands) — per machine by design
* **quality gate commands** (what runs on this machine)
* sync history, backups, global knowledge, local-only knowledge (`.nexus-local\` in a workspace)
* credentials: NEXUS has none — Claude and Git keep their own

## Can sync — the workspace folder, through Git, when you push it
* `nexus-workspace.json` (workspace and project list, remote URLs without credentials)
* tasks, notes, decisions, specs, rules, constitutions, workflows (Markdown files)
* AI Kit folders you keep in the workspace

Projects' own repositories (e.g. under `repos\`) are separate Git repositories and are never committed into the
workspace.

## How syncing works
*Sync* (or `nexus sync`) uses the workspace folder's own Git repository and your Git remote — there is no NEXUS server.

* **Fetch** shows incoming commits and their diffs; **Pull** only fast-forwards.
* **Commit** and **Push** happen only when you confirm the review; a security inspection blocks credentials, key and
  credential files, databases, local-only knowledge and machine-only configuration, and warns about machine paths.
* Pulled rules, constitutions, workflows and AI Kits that changed are **inactive until you approve them** on this
  machine.
* NEXUS never force-pushes, merges or rebases; it never asks for or stores Git credentials.

## Putting a workspace under Git
Easiest: create an **empty private repository** on your Git host first, then *Create a workspace → Empty Git
repository* — NEXUS clones it, sets the workspace up, and *Sync → Review & commit → Push* publishes it there. For a
workspace that already exists: in its folder `git init`, `git remote add origin <your private repository>`, then use
*Sync* (the first push sets the upstream).
NEXUS creates a `.gitignore` that keeps machine-local files out.

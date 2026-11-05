# ADR-0031 — Git-based workspace synchronisation: the person's repository, explicit outgoing review, no sync service

Status: Accepted — architecture and read-only readiness implemented (2026-11-05); the sync flow itself is M29

## Context
A workspace's portable data — `nexus-workspace.json`, tasks, notes, decisions, specs, rules, constitutions,
workflows (ADR-0005, ADR-0013) — should be available on several machines and to a team. Machine-local state
(SQLite: sessions, approvals, gate commands, usage, notifications, billing classification) must stay where it is.
The owner asked to prepare safe Git-based synchronisation without building a cloud synchronisation service.

## Decision
* **Transport and history are Git, in a repository the person owns** (their own remote: GitHub, GitLab, a file
  share). NEXUS runs no server and stores no credentials: authentication stays with Git and its credential helper.
  All Git calls go through `git.rs` (argument vectors, `core.fsmonitor=false`, no terminal prompts, URLs shown with
  credentials removed).
* **Readiness first (implemented, read-only):** `workspace_sync::readiness` / `nexus workspaces sync-status` — a
  repository rooted at the workspace folder, branch and upstream, ahead/behind as of the last fetch (the check never
  contacts the remote), the portable files an outgoing commit would contain, conflicts, and blockers:
  files that must never travel (machine-local databases, environment files, keys, credential files — by **name**,
  contents are never read), local-only knowledge that Git does not ignore, the NEXUS data folder inside the
  workspace. Synced rules, constitutions and workflows waiting for approval are counted.
* **Outgoing (M29):** NEXUS shows the outgoing diff of portable files only, re-runs the readiness check and a
  credential scan on the changed lines (the gate-output redaction patterns), and commits and pushes **only on an
  explicit action** of the person with a message they see. Never automatic, never `--force`.
* **Incoming (M29):** `fetch`, then **fast-forward only**. Diverged history or conflicts are not resolved by NEXUS —
  the person integrates in their Git tool. After a pull the knowledge index is rebuilt from the files (files are
  authoritative, SQLite only indexes them).
* **Incoming changes are untrusted** (T19, T33): synced rules, constitutions and workflows need approval on each
  machine (existing mechanisms; a pull never approves anything); gate commands are machine-local and approved per
  version, so a pull cannot introduce commands that run; task text stays data (T22).
* **Concurrency:** knowledge writes stay optimistic (expected hashes); a pull that changed a file being edited makes
  the save a conflict, never a silent overwrite. Sync is refused while a NEXUS write is in progress.

## Verification
Core tests (`workspace_sync`: not a repository, changed portable files, a key file not ignored, a committed key
blocks, the data folder inside the workspace blocks, ahead of an upstream via a local bare remote; `git` porcelain
v1 parsing) and a CLI E2E (`sync-status` exit codes, names only, repository unchanged).

## Alternatives
* **A NEXUS sync service or CRDT replication** — a new trust boundary, accounts and an operations burden; rejected.
* **Auto-commit and push on every change** — leaks unreviewed content and floods history; rejected.
* **Syncing the SQLite database** — machine paths, approvals and usage would cross machines; rejected (ADR-0005).

## Consequences
* Teams need a Git remote and basic Git; NEXUS explains blockers instead of hiding Git.
* Approvals are per machine by design: a teammate's synced rule is not in force on another machine until approved
  there.

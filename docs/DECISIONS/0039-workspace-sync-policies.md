# ADR-0039 — Workspace sync policies: manual, automatic with review, trusted automatic

Status: Accepted (2026-11-05, stage "Portable Workspace", milestone P7)

## Context
Development records (ADR-0038) accumulate in the workspace folder. M29 (ADR-0031) publishes only through a reviewed
commit and a reviewed push. The owner wants three modes: manual; automatic preparation with review before remote
publication; and an explicitly authorised, allowlisted automatic commit-and-push of sanitised records to an approved
private repository — unavailable when organisation policy forbids it, reusing M29 (no second engine), never forcing,
safe offline, with clear status and audit history. Raw transcripts, unapproved memories, credentials, private company
data, databases and source repositories must never be published automatically.

## Decision
1. **The policy is machine-local** (`app_settings`, per workspace), never read from the workspace repository — a synced
   file cannot turn on automatic publication on another computer. Default: **manual**.
2. **Automatic with review**: when NEXUS writes a record, a *ReviewRequired* notification ("Workspace records ready to
   sync") leads to the Sync Center, where the person reviews the commit and the push as before.
3. **Trusted automatic** (opt-in): setting it requires the workspace's own repository, a remote and a branch, and the
   person's explicit confirmation that the remote is private and approved; the approved remote (credentials removed)
   and branch are stored. It publishes only an **allowlist of NEXUS-generated files** —
   `project-context/*/development/*.md`, `project-context/*/current-state.md`, `project-context/*/project.json`,
   `project-context/*/environment.json` — through M29's own steps: `review_commit` (inspection + token) → `commit` →
   `fetch` → `review_push` (inspection + token) → `push`. It **falls back to asking** (notification, audited as
   *paused*) on: any inspection finding (warnings included), a different remote URL or branch, a remote that is ahead,
   outgoing commits that touch anything outside the allowlist (the person's own unpushed commits), or a managed policy
   that forbids it. Offline or a failed push: the commit stays local, the run is audited as *failed*, and it is retried
   after 10 minutes or with the next record. Never forced.
4. **Managed policy**: `<ProgramData>/NEXUS/policy.json` (Windows), `/Library/Application Support/NEXUS/policy.json`
   (macOS), `/etc/nexus/policy.json` (Linux): `{ "workspaceSync": { "trustedAutoSync": false, "automaticReview": false } }`.
   An unreadable file forbids automatic sync. There is no user-level override.
5. **When it runs**: hosts call `policy::run_due` after writing a record (desktop session exit, `nexus launch` exit)
   and on every desktop heartbeat; records mark their workspace *due*. Every automatic run is audited in the sync
   history (`auto-sync`, migration 0017 extends the history's operations) beside M29's commit, fetch and push entries.
6. Desktop: a *Sync policy* card in the Sync Center (trusted confirmation shows the allowlist, remote and branch);
   CLI: `nexus sync policy [manual|review|trusted] [--private]`.

## Consequences
* Tasks, decisions, memories, constitutions, workflows and kits are never published without the person's review.
* Whether a remote is private cannot be verified for arbitrary Git hosts; the person's confirmation is recorded, and a
  changed remote URL stops automatic publication.
* A person who edits a record by hand changes an allowlisted file; it travels with the next automatic run after the
  same inspection.

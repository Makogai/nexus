# ADR-0031 — Git-based workspace synchronisation: the person's repository, explicit outgoing review, no sync service

Status: Accepted, implemented (readiness 2026-11-05; sync flow M29, 2026-11-05)

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

## Implementation (M29)
* **Operations** (`workspace_sync::ops`, desktop *Sync Center*, CLI `nexus sync`): `fetch` (`--prune`, no submodules);
  `pull` = fast-forward only to what was fetched — refused when diverged, conflicted, detached, an operation is in
  progress (merge/rebase/`index.lock`), local changes overlap incoming files, or the incoming change is unsafe;
  `review_commit` → `commit` (only the reviewed files, `git commit --only`; new files staged and unstaged again if the
  commit fails); `review_push` → `push` (never forced; refused when the remote is ahead; the first push inspects the
  whole history); `clone_workspace` (new or empty folder; refused if it is not a workspace, unsafe, or the same
  workspace is already open here; the folder is removed again on failure) and `restore` (projects to locate, documents
  to approve, AI Kit folders to register, gates the workflows need, profiles to add).
* **Explicit confirmation**: a review returns a token over exactly what it showed (HEAD, files and their contents /
  outgoing commits and the remote's tip); commit and push need that token, so a change after the review is refused.
  The desktop asks for a confirmation that says what happens ("Nothing is pushed", "the whole history travels").
* **Inspection** (`workspace_sync::inspect`) before a commit (added lines of the selected files), before a push (every
  path and added line in the outgoing history) and of incoming changes (paths and modes): blocks private keys and
  access tokens (Anthropic, GitHub, GitLab, Slack, Stripe, Google, npm, AWS, JWT, bearer), passwords in URLs, sensitive
  files by name (databases, `.env`, keys, credential files), local-only knowledge (`.nexus-local/`), machine-local
  Claude configuration (`*.local.*`), project repositories under `repos/`, symbolic links, non-portable paths (`..`,
  Windows device names, invalid characters) and files over 50 MB; warns about password assignments, machine paths,
  submodules and large files. Findings name file, line and kind — never the value.
* **Approval after a pull**: the knowledge index is refreshed from the files; changed rules, constitutions and
  workflows have a new content hash and are inactive until approved here; changed AI Kit folders lose their approval
  (kit hash); a review notification points to the Sync Center.
* **Robustness**: one operation per workspace (history row `running`, refused while its host is alive); operations of
  a dead host are `interrupted` on the next start; remote failures are classified (offline, auth, remote-missing,
  rejected, timeout) with credentials removed from messages; network operations use a separate database connection in
  the desktop so the app stays responsive; history (migration 0015) is machine-local.

## Verification
Core tests with real temporary repositories and local bare remotes (two machines): first push and onboarding by clone
with restore; fetch, incoming diff, fast-forward pull, changed rule and kit require approval again, notification;
refused pulls (overlapping local changes, diverged, unsafe incoming symlink, busy, `index.lock`) with HEAD unchanged;
commit inspection (secret, key file, local-only knowledge blocked; machine path warned; stale token refused; partial
commit); secret committed outside NEXUS blocks the push; failing pre-commit hook rolls staging back; missing and
unreachable remotes classified; onboarding refused for non-workspaces, non-empty folders, command transports and an
already-open workspace (nothing left behind). Plus parsers (porcelain, raw diff, unified diff, remote errors), CLI
E2E across two data dirs, vitest, desktop E2E `e2e/sync.mjs`.

## Alternatives
* **A NEXUS sync service or CRDT replication** — a new trust boundary, accounts and an operations burden; rejected.
* **Auto-commit and push on every change** — leaks unreviewed content and floods history; rejected.
* **Syncing the SQLite database** — machine paths, approvals and usage would cross machines; rejected (ADR-0005).

## Consequences
* Teams need a Git remote and basic Git; NEXUS explains blockers instead of hiding Git. Diverged branches and
  conflicts are integrated in the person's Git tool (NEXUS never merges, rebases or force-pushes).
* The inspection is pattern-based: it catches common credential formats, not every secret; the outgoing review
  stays the person's responsibility (the UI says so).
* Approvals are per machine by design: a teammate's synced rule is not in force on another machine until approved
  there.

# ADR-0032 — Upgrade safety, backups and restore

Status: Accepted, implemented (2026-11-05, stage "Beta readiness"; extends ADR-0002)

## Context
Before a personal beta, NEXUS must never lose a user's machine-local state (workspaces registered on the machine,
profiles, sessions, approvals, gate commands, usage, notifications, sync history, global knowledge) — not on an
upgrade, not on a failed migration, not on a damaged file — and the person needs a backup and restore they can trust.
ADR-0002 already took a copy before each schema upgrade, ran every migration in a transaction, refused databases from
a newer NEXUS and moved unreadable files aside instead of deleting them.

## Decision
* **Identity check**: NEXUS databases carry `PRAGMA application_id = 0x4E585553` ("NXUS"); a SQLite file of another
  program is refused and left untouched (never migrated).
* **A failed migration is never corruption**: errors (including a rebuild that would violate foreign keys) are reported
  as migration failures — the database stays where it is, at its last good version, with the pre-upgrade copy. Only a
  structurally damaged file is moved aside (`nexus.db.corrupt-<ms>`, kept), and the notice names the newest restorable
  backup.
* **The desktop never starts empty silently**: if the database cannot be opened (newer schema, failed migration), a
  native dialog explains what happened, where the data and backups are, and how to restore — then NEXUS stops.
* **Backups** (`db::backup`): a folder `backups/<UTC time>-<kind>/` with a `VACUUM INTO` copy of the database, the
  machine-local `global/` knowledge, and `manifest.json` (kind, time, schema, NEXUS version, SHA-256 of the copy).
  Kinds: `manual`, `automatic` (at most one a day at desktop start, newest 7 kept), `pre-restore`; the pre-migration
  copies of ADR-0002 are listed too. Listing verifies each backup (checksum, NEXUS database, `quick_check`, not newer).
* **Restore** (`db::backup::restore`) requires no open connection and no other live NEXUS host: verify → `pre-restore`
  backup of the current state → move the current files (with `-wal`/`-shm`) and `global/` aside into
  `backups/<time>-replaced/` → copy the backup in → open it (migrating an older schema with its own pre-migration copy)
  → on failure put the previous files back. The desktop schedules the restore (`restore-pending.json`) and applies it on
  its next start, before opening the database (the marker is removed first, so a failing restore cannot block start).
* Workspace folders are not part of a backup (they are folders and Git repositories, ADR-0005/0031).
* CLI `nexus backup create|list|restore <id> [--yes]|delete <id>`; desktop *Settings → Backups*; the startup notice
  reports a restore's outcome.

## Verification
Core: failed migration and foreign database are never started fresh (file untouched); backup → change → restore →
state and global knowledge back, replaced files kept, the safety backup restores the later state; damaged copy
(checksum), newer schema and foreign database refused; path traversal in ids refused; scheduled restore applied once;
automatic backups daily and rotated. Installed-app test: the first launch takes the automatic backup; data survives
uninstall.

## Consequences
* Backups hold machine-local data including session titles and usage; they stay in the data folder (never synced).
* Restoring on another machine brings that machine's approvals along only if the files are the same paths; approvals
  are per machine by design (ADR-0031).

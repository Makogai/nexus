# Backups and restore

## What a backup contains
NEXUS's **machine-local** state: the database in `%LOCALAPPDATA%\NEXUS` (workspaces registered on this machine,
profiles, sessions, approvals, gate commands, usage, notifications, sync history) and your global knowledge. Each backup
is a folder in `%LOCALAPPDATA%\NEXUS\backups` with a consistent copy of the database, a checksum and the version.

Your **workspace folders are not in a backup** — they are ordinary folders; put them under Git (see
[Data and sync](DATA_AND_SYNC.md)) or include them in your normal file backup.

## When NEXUS backs up by itself
* **Daily**, when NEXUS starts (the newest 7 are kept).
* **Before every upgrade** of its database (the newest 5 are kept).
* **Before every restore** (so a restore can be undone).

## Back up now
*Settings → Backups → Back up now*, or `nexus backup create`.

## Restore
* **Desktop:** *Settings → Backups → Restore…* — NEXUS restarts and restores during the start.
* **Terminal:** close NEXUS, then `nexus backup list` and `nexus backup restore <id> --yes`.

A restore never destroys anything: it checks the backup (a NEXUS database, intact, not newer than your NEXUS), backs up
the current state first, keeps the replaced files in `backups\<time>-replaced`, and puts everything back if the backup
cannot be opened.

## Moving to a new machine
1. On the new machine install NEXUS and **open your workspace from Git** (or copy the workspace folder).
2. Add your Claude profiles again (they are machine-specific).
3. Optional: copy a backup folder into the new machine's `%LOCALAPPDATA%\NEXUS\backups` and restore it to bring
   sessions history, usage and approvals along. Approvals of synced rules are per machine by design — reviewing them
   again on a new machine is expected.

## Upgrades
A newer NEXUS upgrades the database once, after taking a backup. An older NEXUS refuses to open a newer database
(nothing is changed) — update NEXUS, or restore a backup made by your version.

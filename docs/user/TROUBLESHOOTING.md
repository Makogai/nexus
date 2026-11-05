# Troubleshooting

| Symptom | What to do |
| --- | --- |
| **"NEXUS could not open its database"** at start | Nothing was deleted. If it says *newer NEXUS*, update NEXUS or restore a backup from your version (`nexus backup list`, `nexus backup restore <id> --yes`). If a migration failed, the backup taken before it is in `%LOCALAPPDATA%\NEXUS\backups`. |
| "The database could not be read" notice | It was moved aside as `nexus.db.corrupt-<time>` and a fresh one started. Restore the latest backup under *Settings → Backups*. |
| Claude does not start / "not signed in" | *Claude profiles* → check sign-in; run `claude` once in that profile and log in. NEXUS never stores credentials. |
| Claude sessions have no NEXUS tools | *Settings → Command-line tool* must show `nexus.exe`. If not, reinstall NEXUS. Organisation policy may block session MCP servers or hooks — *Claude profiles → Diagnostics* says why. |
| `nexus` is not found in my terminal | Use the full path from *Settings → Command-line tool*, or add its folder to your user PATH (the button copies the command). |
| Notifications do not appear | Windows *Settings → System → Notifications → NEXUS* must be on; *Notifications → Send a test notification*. Focus assist hides them while presenting. |
| Notification shows no details | Intended while the screen is locked (privacy). Change it under *Notifications → Lock screen*. |
| Sync: "diverged" | Your branch and the remote both have new commits. NEXUS only fast-forwards — integrate (rebase or merge) in your Git tool, then come back. |
| Sync: "authentication" | Your Git credential manager handles sign-in; run `git fetch` in the workspace folder once in a terminal to sign in. |
| Sync: commit blocked | The security inspection found a credential, key file, database, local-only knowledge or a machine-only file. Remove it or leave the file out of the commit. |
| A workflow step waits | Its gate is not defined or not approved — *Workflows → Gates*. |
| Usage says "unavailable" | The session's transcript is missing (Claude's cleanup or deleted). "No API requests" means Claude made none (e.g. not signed in). |

## Diagnostics
* `nexus doctor` — environment, Claude, Git, data folder.
* `"%LOCALAPPDATA%\Programs\NEXUS\nexus-desktop.exe" --self-check report.json` — what the installed app finds (its
  CLI, data folder, database schema, notification identity), without starting the window. Read-only.
* Logs stay on your machine; NEXUS never sends them anywhere.

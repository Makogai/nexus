# First run

NEXUS organises your work with Claude Code: workspaces, projects, Claude profiles, terminals, knowledge, workflows,
usage and Git sync — all on your machine.

## 1. Install
Run `NEXUS_<version>_x64-setup.exe`. It installs for your Windows user only (no administrator rights) into
`%LOCALAPPDATA%\Programs\NEXUS`: the app (`nexus-desktop.exe`), the `nexus` command-line tool and an uninstaller.
Your data lives elsewhere — `%LOCALAPPDATA%\NEXUS` — and is never touched by installing, updating or uninstalling.

You need **Claude Code** (`claude`) installed and signed in, and **Git** for repository features and sync.

## 2. Create or open a workspace
On the welcome screen choose one:
* **Create a workspace** — a new folder for your projects and the knowledge that should travel with them.
* **Open an existing workspace** — a folder that already contains `nexus-workspace.json`.
* **Open a workspace from Git** — clone your workspace repository (see [Data and sync](DATA_AND_SYNC.md)); NEXUS
  then lists what this machine still needs (projects to locate, rules to approve, profiles to add).

## 3. Add your Claude profiles
*Claude profiles → Add*: point NEXUS at a Claude configuration folder (`CLAUDE_CONFIG_DIR`), for example a work and a
personal one. NEXUS reads them, never changes them, and launches Claude with the right one.

## 4. Add a project and launch Claude
*Projects → Add* a repository folder, then **Launch Claude**. Each session gets NEXUS's tools (tasks, notes, decisions,
workflow steps) through a session-scoped MCP server — nothing is installed into your Claude configuration.

## 5. Set up backups and (optionally) sync
NEXUS backs up its own data automatically once a day ([Backups](BACKUP_RESTORE.md)). To use the workspace on another
machine or with a team, put the workspace folder under Git ([Data and sync](DATA_AND_SYNC.md)).

Problems? See [Troubleshooting](TROUBLESHOOTING.md).

# NEXUS 0.1.0 — first release

**NEXUS** is a desktop workbench for Claude Code: workspaces, projects and Claude profiles in one place, with the
knowledge, rules and workflows that make Claude sessions consistent, and a workspace that travels between computers
through your own Git repository. Everything runs on your computer; NEXUS calls no AI model itself and sends no
telemetry.

## Highlights

- **Workspaces and projects.**
  - Start a workspace in a new folder, an empty folder or an empty Git repository.
  - Add projects from a folder or straight from Git.
  - Each project has a portable context (overview, architecture, current state, environment requirements).
- **Claude sessions that know the project.**
  - Sessions launch with session-scoped NEXUS tools (MCP), hooks and context.
  - Profiles stay isolated, and your Claude configuration is never changed.
  - Sessions can be resumed, attention requests reach you as notifications, and usage is parsed from Claude Code's
    own transcripts.
- **Learn a project.**
  - NEXUS maps a new project itself: files, languages, docs, stack, history.
  - A guided Claude session drafts its overview and architecture and records memories.
  - You decide which drafts enter the knowledge base. Graphify is supported when it's installed.
- **Knowledge and rules.**
  - Tasks, notes, decisions, rules and constitutions are Markdown files in your workspace.
  - They open formatted and can be edited in place; synced rules need your approval.
  - The *AI Engineering Pro* template brings a reviewed library of 284 rules, 11 skills and 7 workflows, loaded on
    demand.
- **Project explorer.** Browse a project's files in a read-only viewer and its commit history with diffs. Files that
  may hold secrets are never shown.
- **Workflows with real gates.** Claude reports steps; quality gates (commands you define and approve) verify them.
- **Portable workspace.**
  - Reviewed Git sync: inspected commits, fast-forward pulls, never a force-push.
  - Development records of each session.
  - Guided restoration on a new computer.
- **Safety.**
  - Daily backups, plus backups before upgrades and restores.
  - Programs and data are kept apart.
  - Workspace text is treated as data, never as instructions.

## Download

| Platform | File | Notes |
| --- | --- | --- |
| Windows 10/11 (x64) | `NEXUS_0.1.0_x64-setup.exe` | Per-user install, no administrator rights |
| macOS 11+ (Apple silicon) | `NEXUS_0.1.0_aarch64.dmg` | Drag NEXUS to Applications |
| macOS 11+ (Intel) | `NEXUS_0.1.0_x64.dmg` | Drag NEXUS to Applications |
| Linux (x64) | `NEXUS_0.1.0_amd64.AppImage` / `NEXUS_0.1.0_amd64.deb` | AppImage: make it executable and run it; deb: `sudo apt install ./NEXUS_0.1.0_amd64.deb` |

You need **Claude Code** (`claude`) installed and signed in, and **Git** for repository features and sync.

## First start: the builds are not code-signed yet

- **Windows:** SmartScreen says "Windows protected your PC". Choose **More info → Run anyway**.
- **macOS:** the app is ad-hoc signed but not notarized. Open it once with **right-click → Open**, or allow it under
  **System Settings → Privacy & Security → Open Anyway**.
- **Linux:** no prompt. For the AppImage, run `chmod +x NEXUS_0.1.0_amd64.AppImage` first.

## Verify what you downloaded

- **Checksums:** compare each file with `SHA256SUMS.txt`, using `Get-FileHash -Algorithm SHA256 <file>` on Windows,
  `shasum -a 256 <file>` on macOS or `sha256sum <file>` on Linux.
- **Build provenance:** every file carries an attestation; check it with
  `gh attestation verify <file> --repo Makogai/nexus`.

## Known limitations

- **Platforms:** NEXUS is developed and tested end to end on Windows. The macOS and Linux builds are new and pass the
  automated tests, but have had less hands-on use. Please report anything odd.
- **Notifications:** on macOS and Linux they are display-only. Clicking one does not open the session.
- **Updates:** there are no automatic updates. Install a newer version over the old one; your data stays.
- **Signing:** the installers are not code-signed (see above).

The guides are in `docs/user/`: first run, user guide, portable workspace, backups, troubleshooting.

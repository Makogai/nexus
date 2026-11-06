# NEXUS 0.1.2 — NEXUS updates itself, and Claude's work travels on its own

NEXUS now keeps itself up to date, pushes your workspace after Claude's work with the commit messages Claude writes,
asks you before Claude pushes a project, and closes finished tasks. Install this version over 0.1.1 once by hand —
from now on, updates come by themselves. Your data stays.

## What's new

- **Automatic updates.** NEXUS checks for a new release when it starts and every few hours. At start it installs one by
  itself and restarts (nothing is running yet); later it offers it in the top bar. Every download is verified against
  NEXUS's update-signing key first, and it never restarts over running Claude sessions without asking. Settings →
  *Version and updates* turns automatic installing off.
- **Your workspace syncs on its own.** The new sync policy *Everything, automatically*: when Claude finishes a turn or a
  session ends, NEXUS commits every workspace change and pushes it — after the same security inspection as a reviewed
  push, never forced. A problem stops it and asks you once.
- **Commit messages Claude writes.** Sessions propose the message for their workspace changes; the Sync Center fills it
  in, and automatic sync uses it. Without one, NEXUS composes it ("umbra — 2 tasks, 1 note").
- **Approve a push in one click.** When Claude may not push a project itself, it asks NEXUS: a notification and a banner
  open a dialog with Claude's reason, the outgoing commits and the security check. *Push* sends exactly what you saw.
- **Finished tasks leave the list.** Claude marks finished work *ready for review*; the project page shows it first,
  with *Done* — and *Recently done* with *Reopen*. A learning task closes itself once you reviewed its drafts.
- **Continue where the context is.** A task can start in a new session, resume its own earlier session (the default
  when it has one — the cheapest), or go to a running session of the project that already knows the code. *Start
  right away* is on by default.
- **NEXUS tools page.** NEXUS's own skills (exactly as Claude reads them), the 24 tools of the NEXUS MCP server and the
  hook events — in one place.
- **Easier to get around.** Back and Forward in the top bar (Alt+←/→ and the mouse buttons), the version at a glance,
  sessions filtered by project, and calmer spacing everywhere.
- **Fixes.** The project explorer hid the folders of projects whose `.gitignore` has Windows line endings, and Graphify's
  output folder was not excluded from Git there — both fixed. A confirmation now always opens on top of the dialog it
  belongs to.

## Download

| Platform | File | Notes |
| --- | --- | --- |
| Windows 10/11 (x64) | `NEXUS_0.1.2_x64-setup.exe` | Per-user install, no administrator rights |
| macOS 11+ (Apple silicon) | `NEXUS_0.1.2_aarch64.dmg` | Drag NEXUS to Applications |
| macOS 11+ (Intel) | `NEXUS_0.1.2_x64.dmg` | Drag NEXUS to Applications |
| Linux (x64) | `NEXUS_0.1.2_amd64.AppImage` / `NEXUS_0.1.2_amd64.deb` | AppImage: make it executable and run it; deb: `sudo apt install ./NEXUS_0.1.2_amd64.deb` |

The `.app.tar.gz` files and `latest.json` are for NEXUS's own updates — you don't need to download them.

You need **Claude Code** (`claude`) installed and signed in, and **Git** for repository features and sync.

## First start: the builds are not code-signed yet

- **Windows:** SmartScreen says "Windows protected your PC". Choose **More info → Run anyway**.
- **macOS:** the app is ad-hoc signed but not notarized. Open it once with **right-click → Open**, or allow it under
  **System Settings → Privacy & Security → Open Anyway**.
- **Linux:** no prompt. For the AppImage, run `chmod +x NEXUS_0.1.2_amd64.AppImage` first.

## Verify what you downloaded

- **Checksums:** compare each file with `SHA256SUMS.txt`, using `Get-FileHash -Algorithm SHA256 <file>` on Windows,
  `shasum -a 256 <file>` on macOS or `sha256sum <file>` on Linux.
- **Build provenance:** every file carries an attestation; check it with
  `gh attestation verify <file> --repo Makogai/nexus`.

## Known limitations

- **Platforms:** NEXUS is developed and tested end to end on Windows. The macOS and Linux builds pass the automated
  tests, but have had less hands-on use. Please report anything odd.
- **Notifications:** on macOS and Linux they are display-only. Clicking one does not open the session.
- **Signing:** the installers are not code-signed (see above); updates are signed with NEXUS's own update key.

The guides are in `docs/user/`: first run, user guide, portable workspace, backups, troubleshooting.

NEXUS is free and open source under **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE`); the licences of the
third-party components it includes are in `THIRD_PARTY_NOTICES.md`, attached to this release.

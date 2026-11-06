# NEXUS 0.1.5 — write and edit your skills in NEXUS

A new **Skills** page shows every skill your Claude sessions can get, and lets you write and edit your own — for the
whole workspace or for one project. NEXUS 0.1.4 installs this update on its own.

## What's new

- **All skills in one place.** NEXUS's own skills, your workspace's skills, each project's skills and those of your AI
  Kits, with the name Claude knows them by, which sessions get them and whether they are active.
- **New skill.** Give it a name and a one-line description of what it does and when Claude should use it, choose the
  workspace (every project, synced with your workspace) or one project (saved in its repository), and write the steps.
- **Edit in place.** Open a skill, change it, save (Ctrl+S). NEXUS never overwrites a newer version on disk. Workspace
  skills you save are active on this computer right away; skills that arrived from another computer still wait for
  your review in AI Kits.
- **Project skills live in the project.** They go into the project's `.claude/skills` folder, so Claude Code uses them
  there — commit them with the project.

## Download

| Platform | File | Notes |
| --- | --- | --- |
| Windows 10/11 (x64) | `NEXUS_0.1.5_x64-setup.exe` | Per-user install, no administrator rights |
| macOS 11+ (Apple silicon) | `NEXUS_0.1.5_aarch64.dmg` | Drag NEXUS to Applications |
| macOS 11+ (Intel) | `NEXUS_0.1.5_x64.dmg` | Drag NEXUS to Applications |
| Linux (x64) | `NEXUS_0.1.5_amd64.AppImage` / `NEXUS_0.1.5_amd64.deb` | AppImage: make it executable and run it; deb: `sudo apt install ./NEXUS_0.1.5_amd64.deb` |

Already on 0.1.2 or later? NEXUS updates itself when it starts (or from the top bar). The `.app.tar.gz` files and `latest.json`
are for NEXUS's own updates — you don't need to download them.

You need **Claude Code** (`claude`) installed and signed in, and **Git** for repository features and sync.

## First start: the builds are not code-signed yet

- **Windows:** SmartScreen says "Windows protected your PC". Choose **More info → Run anyway**.
- **macOS:** the app is ad-hoc signed but not notarized. Open it once with **right-click → Open**, or allow it under
  **System Settings → Privacy & Security → Open Anyway**.
- **Linux:** no prompt. For the AppImage, run `chmod +x NEXUS_0.1.5_amd64.AppImage` first.

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

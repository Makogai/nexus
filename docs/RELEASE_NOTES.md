# NEXUS 0.1.3 — learning a project with its surroundings

Learning a project can now take what the code does not say into account: notes you write for Claude, and folders it
may read besides the project — such as the repository the project lives in. A knowledge graph you build in a terminal
is now noticed by itself. NEXUS 0.1.2 installs this update on its own.

## What's new

- **Notes for Claude.** *Learn with Claude* has a notes field — "drivebay is one app of my monorepo; shared code lives
  in packages/". The notes are saved in the learning task, and Claude reads them as part of it.
- **Let Claude read the surroundings.** When a project sits inside another Git repository (a monorepo or a wrapper
  repository), NEXUS offers it in one click; you can add other folders too (up to five). Only learning sessions get
  them, read only and within the same budget, and Claude names the folder a statement comes from. The list stays on
  this computer; credential folders, Claude's configuration, your home folder and whole drives are refused.
- **The graph ticks by itself.** After *Build the graph in a terminal*, the Learn tab watches for Graphify's report and
  updates when the build finishes — also when you come back from the terminal. The graph card has *Check again*.
- **Faster releases.** One pipeline per release: every check runs once, and new versions reach you sooner.

## Download

| Platform | File | Notes |
| --- | --- | --- |
| Windows 10/11 (x64) | `NEXUS_0.1.3_x64-setup.exe` | Per-user install, no administrator rights |
| macOS 11+ (Apple silicon) | `NEXUS_0.1.3_aarch64.dmg` | Drag NEXUS to Applications |
| macOS 11+ (Intel) | `NEXUS_0.1.3_x64.dmg` | Drag NEXUS to Applications |
| Linux (x64) | `NEXUS_0.1.3_amd64.AppImage` / `NEXUS_0.1.3_amd64.deb` | AppImage: make it executable and run it; deb: `sudo apt install ./NEXUS_0.1.3_amd64.deb` |

Already on 0.1.2? NEXUS updates itself when it starts (or from the top bar). The `.app.tar.gz` files and `latest.json`
are for NEXUS's own updates — you don't need to download them.

You need **Claude Code** (`claude`) installed and signed in, and **Git** for repository features and sync.

## First start: the builds are not code-signed yet

- **Windows:** SmartScreen says "Windows protected your PC". Choose **More info → Run anyway**.
- **macOS:** the app is ad-hoc signed but not notarized. Open it once with **right-click → Open**, or allow it under
  **System Settings → Privacy & Security → Open Anyway**.
- **Linux:** no prompt. For the AppImage, run `chmod +x NEXUS_0.1.3_amd64.AppImage` first.

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

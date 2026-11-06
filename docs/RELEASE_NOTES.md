# NEXUS 0.1.4 — automatic sync that keeps going, and one draft per topic

Automatic workspace sync no longer stops on harmless warnings, and when it does stop it says exactly why. Learning
sessions keep one draft per topic instead of piling up versions. NEXUS 0.1.3 installs this update on its own.

## What's new

- **Automatic sync keeps going.** *Everything, automatically* stopped on any warning of the security inspection — for
  example an absolute path of your computer in a note Claude wrote. Now only real risks stop it: anything blocking, a
  possible secret, or a change too large to inspect. Harmless warnings are noted in the Sync history.
- **It says why.** When automatic sync stops, the Sync history and the notification name the findings — kind, file and
  line (never the text found) — instead of just counting them.
- **One draft per topic.** When a learning session revises the overview or the architecture, it replaces its draft
  instead of adding another, so the Learn tab shows only the latest version for you to review.
- **Cleaner learning notes.** Learning sessions write file paths relative to the project and name extra folders by
  name, so the knowledge base stays portable between computers.
- **Environment templates are shown.** The project explorer shows templates such as `.env.docker.example` and
  `.env.dist`; real environment files stay hidden.

## Download

| Platform | File | Notes |
| --- | --- | --- |
| Windows 10/11 (x64) | `NEXUS_0.1.4_x64-setup.exe` | Per-user install, no administrator rights |
| macOS 11+ (Apple silicon) | `NEXUS_0.1.4_aarch64.dmg` | Drag NEXUS to Applications |
| macOS 11+ (Intel) | `NEXUS_0.1.4_x64.dmg` | Drag NEXUS to Applications |
| Linux (x64) | `NEXUS_0.1.4_amd64.AppImage` / `NEXUS_0.1.4_amd64.deb` | AppImage: make it executable and run it; deb: `sudo apt install ./NEXUS_0.1.4_amd64.deb` |

Already on 0.1.2 or later? NEXUS updates itself when it starts (or from the top bar). The `.app.tar.gz` files and `latest.json`
are for NEXUS's own updates — you don't need to download them.

You need **Claude Code** (`claude`) installed and signed in, and **Git** for repository features and sync.

## First start: the builds are not code-signed yet

- **Windows:** SmartScreen says "Windows protected your PC". Choose **More info → Run anyway**.
- **macOS:** the app is ad-hoc signed but not notarized. Open it once with **right-click → Open**, or allow it under
  **System Settings → Privacy & Security → Open Anyway**.
- **Linux:** no prompt. For the AppImage, run `chmod +x NEXUS_0.1.4_amd64.AppImage` first.

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

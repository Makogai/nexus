# NEXUS 0.1.1 — lean learning

Learning a project is now one focused Claude session with a budget you choose, instead of a plan Claude improvises.
Everything from [0.1.0](https://github.com/Makogai/nexus/releases/tag/v0.1.0) is included; install over it — your data
stays.

## What's new

- **A built-in learning skill.** Learning sessions get NEXUS's own skill `nexus:learn-project`, a fixed and lean
  procedure:
  - orient from NEXUS's map and the Graphify report;
  - read a few chosen files;
  - draft the overview and the architecture summary, marking each statement *verified* or *inferred*;
  - record memories, then stop and report.

  Only learning sessions get it; other sessions never see it.
- **Rules for learning sessions (LEARN-1 to LEARN-7).** One session, no subagents or multi-agent workflows unless you
  ask, read only, a budget, and a stop when the drafts are saved. They hold even when your work profile asks for
  reviews, so a learning session no longer fans out into dozens of agents.
- **Choose the depth** in the Learn tab — *Quick* (20 files, short drafts), *Standard* (50 files) or *Deep* (no file
  limit, fuller drafts). The tab shows the budget and, on request, the exact rules Claude gets.
- **Start right away.** A task launch can send Claude a fixed first message, so the session starts working instead of
  waiting. It is ticked when you start from *Learn*, and the dialog shows the message word for word.
- **Graphify, complete.** *Build the graph* now runs both offline steps
  (`graphify extract . --code-only; graphify cluster-only . --no-label`), so the report NEXUS and Claude read exists.
  A graph built after the learning task was created is added to its checklist.
- **AI Kits:** the plugin name `nexus` is reserved for NEXUS's built-in skills.

## Download

| Platform | File | Notes |
| --- | --- | --- |
| Windows 10/11 (x64) | `NEXUS_0.1.1_x64-setup.exe` | Per-user install, no administrator rights |
| macOS 11+ (Apple silicon) | `NEXUS_0.1.1_aarch64.dmg` | Drag NEXUS to Applications |
| macOS 11+ (Intel) | `NEXUS_0.1.1_x64.dmg` | Drag NEXUS to Applications |
| Linux (x64) | `NEXUS_0.1.1_amd64.AppImage` / `NEXUS_0.1.1_amd64.deb` | AppImage: make it executable and run it; deb: `sudo apt install ./NEXUS_0.1.1_amd64.deb` |

You need **Claude Code** (`claude`) installed and signed in, and **Git** for repository features and sync.

**Upgrading from 0.1.0:** close NEXUS and run the new installer. A learning task you started in 0.1.0 continues at
*Standard* depth; pick another depth before you continue it if you like.

## First start: the builds are not code-signed yet

- **Windows:** SmartScreen says "Windows protected your PC". Choose **More info → Run anyway**.
- **macOS:** the app is ad-hoc signed but not notarized. Open it once with **right-click → Open**, or allow it under
  **System Settings → Privacy & Security → Open Anyway**.
- **Linux:** no prompt. For the AppImage, run `chmod +x NEXUS_0.1.1_amd64.AppImage` first.

## Verify what you downloaded

- **Checksums:** compare each file with `SHA256SUMS.txt`, using `Get-FileHash -Algorithm SHA256 <file>` on Windows,
  `shasum -a 256 <file>` on macOS or `sha256sum <file>` on Linux.
- **Build provenance:** every file carries an attestation; check it with
  `gh attestation verify <file> --repo Makogai/nexus`.

## Known limitations

- **Platforms:** NEXUS is developed and tested end to end on Windows. The macOS and Linux builds pass the automated
  tests, but have had less hands-on use. Please report anything odd.
- **Notifications:** on macOS and Linux they are display-only. Clicking one does not open the session.
- **Updates:** there are no automatic updates. Install a newer version over the old one; your data stays.
- **Signing:** the installers are not code-signed (see above).

The guides are in `docs/user/`: first run, user guide, portable workspace, backups, troubleshooting.

NEXUS is free and open source under **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE`); the licences of the
third-party components it includes are in `THIRD_PARTY_NOTICES.md`, attached to this release.

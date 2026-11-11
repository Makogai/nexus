# NEXUS 0.5.2 alpha — Codex one click away, tidier project pages

A small update to 0.5.1: Codex is easy to find and start, a push request you no longer need can be declined, and long
branch names no longer break the layout. NEXUS 0.1.2 and later installs this update on its own.

Coming from 0.1.x? Everything new in 0.5 — Codex next to Claude, the Dev Environment page, project tabs, *Needs you*,
pushes that never fail silently, live knowledge — is in the
[0.5.1 release notes](https://github.com/Makogai/nexus/releases/tag/v0.5.1).

## What's new

- **Launch, for every agent.** The launch buttons say *Launch* — top bar, Home, the project page and its Sessions tab,
  the terminal panel, task rows and the command palette — and the dialog lets you choose Claude Code or Codex. Each
  Codex profile under *Profiles → Other agents* has its own **Launch**. The profile you launch with decides the agent:
  *Launch with this profile* on a Claude profile always starts Claude Code, even when you used Codex last.
- **Decline a push request you no longer need.** A request for a branch that is no longer checked out — or that was
  merged and pushed another way — can be declined right away; before, *Decline* waited for a review that could never
  load, and the request stayed listed.
- **Long names stay tidy.** A project card cuts a long branch name short instead of breaking the change count onto a
  second line, and the Overview keeps the last commit's time on one line.

## What "alpha" means

The features are complete enough to use all day, and the automated tests pass on Windows, macOS and Linux. They have not
yet had much use outside the author's machine. Expect rough edges, report them, and keep your workspace in Git (NEXUS's
sync does that for you). Your data stays compatible: 0.5.2 adds no database migrations (coming from 0.1.x, the two of
0.5 run after a backup).

## Download

| Platform | File | Notes |
| --- | --- | --- |
| Windows 10/11 (x64) | `NEXUS_0.5.2_x64-setup.exe` | Per-user install, no administrator rights |
| macOS 11+ (Apple silicon) | `NEXUS_0.5.2_aarch64.dmg` | Drag NEXUS to Applications |
| macOS 11+ (Intel) | `NEXUS_0.5.2_x64.dmg` | Drag NEXUS to Applications |
| Linux (x64) | `NEXUS_0.5.2_amd64.AppImage` / `NEXUS_0.5.2_amd64.deb` | AppImage: make it executable and run it; deb: `sudo apt install ./NEXUS_0.5.2_amd64.deb` |

Already on 0.1.2 or later? NEXUS updates itself when it starts (or from the top bar). The `.app.tar.gz` files and `latest.json`
are for NEXUS's own updates — you don't need to download them.

You need **Claude Code** (`claude`) installed and signed in, and **Git** for repository features and sync.

## First start: the builds are not code-signed yet

- **Windows:** SmartScreen says "Windows protected your PC". Choose **More info → Run anyway**.
- **macOS:** the app is ad-hoc signed but not notarized. Open it once with **right-click → Open**, or allow it under
  **System Settings → Privacy & Security → Open Anyway**.
- **Linux:** no prompt. For the AppImage, run `chmod +x NEXUS_0.5.2_amd64.AppImage` first.

## Verify what you downloaded

- **Checksums:** compare each file with `SHA256SUMS.txt`, using `Get-FileHash -Algorithm SHA256 <file>` on Windows,
  `shasum -a 256 <file>` on macOS or `sha256sum <file>` on Linux.
- **Build provenance:** every file carries an attestation; check it with
  `gh attestation verify <file> --repo Makogai/nexus`.

## Known issues

- **Platforms:** NEXUS is developed and tested end to end on Windows. The macOS and Linux builds pass the automated
  tests, but have had less hands-on use.
- **Notifications:** on macOS and Linux they are display-only. Clicking one does not open the session.
- **Watching:** on some network drives and WSL paths Windows does not report changes; there, knowledge files refresh
  when the window regains focus or a session ends. While NEXUS is open, the workspace folder itself cannot be renamed.
- **Codex sessions** do not report permission prompts or questions (Codex runs only the hooks you trusted in its own
  settings, and NEXUS does not change those), have no usage statistics yet, and NEXUS's turn notification replaces a
  `notify` program from your Codex config in NEXUS sessions (it can be switched off per profile). Resume needs one
  finished turn.
- **Graphify builds** run in your terminal: NEXUS sees when the report appears, not why a build failed — look at the
  terminal.
- **Error messages stay** until you dismiss them; several at once can stack up (at most four).
- **Signing:** the installers are not code-signed (see above); updates are signed with NEXUS's own update key.

The guides are in `docs/user/`: first run, user guide, portable workspace, backups, troubleshooting.

NEXUS is free and open source under **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE`); the licences of the
third-party components it includes are in `THIRD_PARTY_NOTICES.md`, attached to this release.

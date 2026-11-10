# NEXUS 0.5 alpha — the app you work in all day

NEXUS 0.5.0 is the first alpha: every project's sessions and tasks one click away, every action that says it is working
and how it ended, and Git pushes that never fail silently. NEXUS 0.1.x installs this update on its own.

## What's new

- **Pushes never fail silently.** When Claude asks you to push, NEXUS first fetches the remote branch. If a teammate
  pushed first, you see their commits and Push waits until the branch is integrated — with the next step spelled out.
  A push that still fails stays on the project page (why, what next, *Review again*, *Dismiss*), lands in your
  notifications, and Claude learns the reason when it asks again. **Tell Claude** types a ready message into its
  terminal; you press Enter. NEXUS never force-pushes, merges or rebases for you.
- **Conflicts in view.** The project page shows conflicted files, a merge or rebase in progress, and how far the branch
  is ahead of or behind its remote.
- **A project page with tabs.** Overview · Sessions · Tasks · Knowledge · Learn · Files · Commits · Settings, with
  counts on the tabs (running sessions, a dot when one waits for you, tasks ready for review) and Ctrl+1…8. The
  header keeps *Launch Claude* and *Shell*; the rest is under *More*.
- **Needs you.** Home lists what waits for you across projects — sessions waiting for an answer, push requests, tasks
  to review, learning drafts, rules and kits to approve, workflow sign-offs. One click opens the place to act. Each
  project's Overview shows its own.
- **Sessions tab.** Every session of the project, the ones waiting for you first; filters for running, ended and
  archived; each row shows its task, workflow run, context strategy and last activity.
- **Tasks tab.** The project's tasks as a list or a board; *Done* and *Reopen* happen at once; *New task* is ready for
  this project.
- **Every action shows it is working.** Buttons show a spinner and cannot be pressed twice while they run; approving a
  learning draft says what happens and offers *Open overview* when it is done. Errors stay on screen until you dismiss
  them, with *Copy details*; failed clones, gate runs and automatic syncs also reach the notification center.
- **Terminals name their project**, in a colour per project, and a project page can show only its own terminals.
- **Live knowledge.** NEXUS watches your workspace's knowledge files. A task you edit in another program, a `git pull`
  or a sync shows up within a moment — in the lists and in a document you have open. If you are editing that document,
  your text stays and NEXUS tells you the file changed on disk.

## What "alpha" means

The features are complete enough to use all day, and the automated tests pass on Windows, macOS and Linux. They have not
yet had much use outside the author's machine. Expect rough edges, report them, and keep your workspace in Git (NEXUS's
sync does that for you). Your data stays compatible: 0.5 adds one database migration and takes a backup before it.

## Download

| Platform | File | Notes |
| --- | --- | --- |
| Windows 10/11 (x64) | `NEXUS_0.5.0_x64-setup.exe` | Per-user install, no administrator rights |
| macOS 11+ (Apple silicon) | `NEXUS_0.5.0_aarch64.dmg` | Drag NEXUS to Applications |
| macOS 11+ (Intel) | `NEXUS_0.5.0_x64.dmg` | Drag NEXUS to Applications |
| Linux (x64) | `NEXUS_0.5.0_amd64.AppImage` / `NEXUS_0.5.0_amd64.deb` | AppImage: make it executable and run it; deb: `sudo apt install ./NEXUS_0.5.0_amd64.deb` |

Already on 0.1.2 or later? NEXUS updates itself when it starts (or from the top bar). The `.app.tar.gz` files and `latest.json`
are for NEXUS's own updates — you don't need to download them.

You need **Claude Code** (`claude`) installed and signed in, and **Git** for repository features and sync.

## First start: the builds are not code-signed yet

- **Windows:** SmartScreen says "Windows protected your PC". Choose **More info → Run anyway**.
- **macOS:** the app is ad-hoc signed but not notarized. Open it once with **right-click → Open**, or allow it under
  **System Settings → Privacy & Security → Open Anyway**.
- **Linux:** no prompt. For the AppImage, run `chmod +x NEXUS_0.5.0_amd64.AppImage` first.

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
- **Graphify builds** run in your terminal: NEXUS sees when the report appears, not why a build failed — look at the
  terminal.
- **Error messages stay** until you dismiss them; several at once can stack up (at most four).
- **Signing:** the installers are not code-signed (see above); updates are signed with NEXUS's own update key.

The guides are in `docs/user/`: first run, user guide, portable workspace, backups, troubleshooting.

NEXUS is free and open source under **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE`); the licences of the
third-party components it includes are in `THIRD_PARTY_NOTICES.md`, attached to this release.

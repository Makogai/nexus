# ADR-0049 — A file-system watcher for knowledge files

Status: Accepted (2026-11-10, owner request "lets ship file system watcher"; branch `milestone/v0.5-alpha`). Replaces
item 7 of ADR-0048 (focus-based refresh only), which stays as the fallback.

## Context

Knowledge files are authoritative (ADR-0013); SQLite indexes them. Files change outside NEXUS: an editor, `git pull`,
a sync from another computer, Claude writing through its own tools. ADR-0048 refreshed the index when the window regained
focus and after a session ended. That missed the common case: NEXUS stays in front, its terminal is where Claude works,
and the person edits a task or pulls in another window that never takes focus away from NEXUS. An open document showed
the old text until something else reloaded it, and an edit in progress learned of the change only when its save was
refused.

## Decision

1. **The desktop watches the knowledge roots of the active workspace** — the workspace folder (which also covers
   `.nexus-local/`) and the global folder in the data dir — with the operating system's notifications (`notify` 8:
   ReadDirectoryChangesW on Windows, FSEvents on macOS, inotify on Linux), in a `knowledge-watcher` thread
   (`apps/desktop/src-tauri/src/watcher.rs`). Only the desktop watches; the CLI and the MCP server do not.
2. **One recursive watch per root, never one per subfolder.** On Windows an open directory handle keeps that folder's
   parent from being renamed or removed; with the handle on the root, `tasks/` and `project-context/<slug>/` stay free for
   `git checkout` to replace. `knowledge::watch::targets` picks the roots (existing ones, none inside another).
3. **Same rules as the index.** `knowledge::watch::affects_index` accepts a change only where the index walk would read a
   file: a Markdown file other than a README in a scanned folder, with no hidden component (editor temp files, `.git`).
   A folder counts only when it appears, vanishes or is renamed; its "modified" only echoes a change inside it. A test
   compares the predicate with the walk over the same tree. A watcher error or a rescan flag (an overflowing buffer during
   a large checkout) counts as a change. macOS (FSEvents) names a changed file by its real path — `/private/var/…` for a
   folder watched as `/var/…`, the target of a symbolic link — so a change reported under a watched folder's real name
   is mapped back to the watched name first (`watch::real_names`, `watch::as_watched`; 0.5.1, found by the 0.5.0 release
   run's macOS tests).
4. **Settle, then refresh incrementally on its own connection.** After the first relevant event the thread waits until
   400 ms pass without another (at most 2 s), then runs `knowledge::service::refresh` (unchanged files are not read) on a
   connection of its own, so a scan never holds the connection the UI uses. Every 5 s it looks at the active workspace and
   at which roots exist again; a change of what is watched also refreshes, because newly watched folders may have changed
   while nobody watched them.
5. **The UI hears about it only when the index changed**: event `nexus://knowledge` (`{ workspaceId, changed, removed }`)
   moves `app.externalChanges` (lists) and `app.freshness` (open documents). Writes by NEXUS itself are indexed in the
   same call, so the watcher's refresh finds nothing new and stays silent — no feedback loop between a view's reload and
   the watcher.
6. **Open documents follow, never over unsaved edits.** The document dialog and the Constitutions page show the new
   version in Read mode; while an edit is open they keep the draft — the dialog says the file changed on disk and that
   saving would be refused (the optimistic save of ADR-0017 is unchanged). The Learn tab reads its drafts again (not the
   map, which walks the repository) and never while a draft decision is in flight.
7. **Focus and session-end refreshes stay** (ADR-0048) for folders whose changes the operating system does not report
   (some network drives, WSL paths) and for a folder that could not be watched (logged; tried again when the set changes).

## Consequences

* Knowledge views are live: a task edited in another program, a `git pull` or a sync show up within about half a second.
* One new dependency in the desktop (`notify`, CC0-1.0; on Linux/macOS also `inotify`/`kqueue`, ISC/MIT). The core does
  not depend on it.
* A long-running thread and one more SQLite connection in the desktop process.
* The workspace folder itself cannot be renamed while NEXUS has it open on Windows (the watch holds a handle on it), like
  any editor that watches a folder.

## Alternatives considered

* **Polling the folders** (a stat walk every few seconds) — no dependency, but constant disk work for nothing most of the
  time, and slower to notice.
* **A watch per scanned folder** — fewer events from `.git`, but on Windows it blocks renaming and removing those folders'
  parent and can leave a removed folder pending deletion while the handle is open.
* **Watching in the core / the MCP server too** — the CLI and the MCP server are short-lived or headless; they refresh
  before they read (ADR-0017), which is enough.
* **Re-reading open documents on a timer** — reloads the person's view for nothing and still cannot tell an edit in
  progress that its file changed.

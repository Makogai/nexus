# ADR-0048 — Action feedback, the project page layout and "Needs you"

Status: Accepted (2026-11-10, owner request; branch `milestone/v0.5-alpha`).

## Context

Owner, 2026-11-10: sessions per project "in a nicer place (a tab up top, not at the bottom)"; "approving drafts shows
no loading indicator"; many quality-of-life changes for daily use. Reading the code: every component kept its own
`busy` flag, so coverage was uneven (ProjectLearn's draft decisions, WorkflowsView and the project page's task buttons
had none); error toasts vanished after seven seconds; the project page stacked usage, cards, tasks, notes, checkpoints,
the last five sessions and the danger zone in one column under eight header buttons.

## Decision

1. **One way to show an action.** `useAction()` (`src/composables/useAction.ts`) wraps a backend call: `pending`,
   a re-entrancy guard (a double click runs it once), `error` until the next attempt, optional success/failure toasts,
   per-row pending keys. `<ActionButton>` renders it: disabled and `aria-busy` while pending, a spinner in place of the
   icon, an optional pending label ("Applying…"). Every IPC-calling button of the swept views uses them; a vitest per
   group mounts the views with a slow mocked `lib/ipc` and asserts the pending state. Rule NX-7 and CLAUDE.md's "UI
   conventions" make it the default for new code.
2. **Errors stay visible.** Error toasts persist until dismissed, offer *Copy details*, and collapse repeats (×N); at
   most four stay. No reload clears an error the person has not seen (the push dialog's reload did). Work the person
   may walk away from reports failures to the notification center too: pushes (ADR-0047), automatic sync (ADR-0042
   amendment), clones and manual gate runs (`notify::background_failure`). A Graphify build runs in the person's
   terminal, so NEXUS cannot see its error; when no report appears within the 30 minutes the Learn tab watches, the tab
   says so until dismissed.
3. **Results link onward; safe updates are optimistic.** An accepted draft offers *Open overview* / *Open
   architecture*; a created task opens; a push offers *Open Commits*. Task Done/Reopen and draft decisions update the
   list at once and roll back with the error shown when the write fails. Long operations say what runs (clone, gate,
   sync fetch/push, graph build).
4. **The project page has tabs** (kept in `?tab=`, Ctrl+1…8 outside fields and terminals): Overview · Sessions · Tasks
   · Knowledge · Learn · Files · Commits · Settings. Labels carry counts (running/total sessions with live and attention
   dots, open tasks and those in review). Overview is a dashboard: "Needs you", the Repository card (conflicts,
   in-progress operation, ahead/behind, last commit), stack, usage. The header keeps **Launch Claude** and **Shell**;
   the rest moves into a "More" menu whose items stay in the DOM (their E2E selectors are unchanged). Terminal tabs name
   their project in a stable per-project colour; on a project page *This project only* narrows them.
5. **"Needs you" is one core query** (`inbox::needs_you`, IPC `needs_you`): sessions waiting for input or a permission,
   failed and pending push requests, workflow steps waiting for sign-off, tasks in review, learning drafts, rules,
   constitutions and workflows not approved here, new or changed kits — built only from what NEXUS already records,
   nothing stored, nothing computed by a model. Home shows it across projects, the project Overview for one project;
   one click opens the place to act.
6. **Session ↔ task clarity.** Session rows show the task, the workflow run (linked) and the context strategy recorded at
   launch, and the last activity of live sessions.
7. **Knowledge freshness without a watcher.** When the window regains focus and after a session ends, the desktop
   refreshes the knowledge index (incremental: unchanged files are not read; at most every five seconds); views follow
   through the data_version poll. A real file-system watcher stays an open owner decision.
8. **Housekeeping.** At start the desktop prunes `<data>/sessions/<id>/` of sessions that ended more than 30 days ago
   (never a live one) and kit overlays of profiles that no longer exist (`prune::artifacts`). `claude::prepare::preview`
   and `prepare` share one builder for the appended text, so the Context Inspector's counts are the launch's (a test
   compares them).

## Consequences

* Feedback is the same everywhere and testable; a new view gets it by using the helpers.
* Persistent error toasts need a click to go away — the price of never losing one.
* The project page is shorter per tab; the E2E suites open tabs explicitly (`?tab=tasks|knowledge|settings`).
* The inspector now counts the commit-message note, the workflow line and the separators it used to miss.

## Not done (looked at)

* **Core module cycles** (activity ↔ attention, knowledge `mod` ↔ `store`, host ↔ session): activity and attention
  reference each other about sixteen times each way — splitting them is a refactor of its own; `knowledge::store` is a
  child that uses its parent's types, and `session` only uses `host::HostKind`. Left as they are.

## Alternatives considered

* **A global loading overlay** — hides which action runs and blocks unrelated work.
* **Auto-dismissing errors after longer** — still loses the ones nobody looked at.
* **A file-system watcher now** — a new dependency and platform differences for what focus-based refresh mostly covers.

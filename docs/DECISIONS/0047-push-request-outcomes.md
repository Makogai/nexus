# ADR-0047 — Push request outcomes and remote state

Status: Accepted (2026-11-10, owner report; branch `milestone/v0.5-alpha`). Amends ADR-0043 and ADR-0042.

## Context

"What happens when you push and it has conflicts? We faced that with anilist yesterday and it failed silently."
Reading the code confirmed four causes:

1. The push dialog set the error and then reloaded the review, and the reload cleared the error — the message vanished.
2. `push_request::decide` marked the request `failed`, dropped Git's failure kind and published nothing; the request
   left the pending list, so the project banner disappeared too.
3. The review compared `HEAD` with the remote-tracking ref without fetching: a remote that was ahead (a teammate
   pushed) was invisible until Git rejected the push.
4. Claude learned the outcome only by calling `request_push` again — and got "failed — see message" without a kind.

Automatic workspace sync had the same blind spot: a failed push only wrote a history entry and retried every ten
minutes, also when the remote refused it.

## Decision

1. **Review fetches first.** `review()` fetches the request's remote branch (`git fetch <remote>
   +refs/heads/<b>:refs/remotes/<remote>/<b>`, 60 s, no tags, no submodules; the `+` only moves the local tracking ref)
   and reports `ahead`, `behind`, `diverged` and the incoming commits. The IPC runs on its own database connection
   (network work). A remote branch that does not exist yet is a first push, not an error.
   * **A first push sends and inspects only what the remote lacks** (0.5.1): the commits none of the remote's branches
     has (`git log HEAD --not --remotes=<remote>`), not the whole history. A feature branch of `main` is reviewed by its
     own commits; into an empty remote that is still every commit. Before, a new branch of the NEXUS repository was
     blocked by key-shaped lines in the secret scanner's own source in `main`'s history — history the remote already
     has, so pushing it again exposes nothing new. The scanner's sources no longer contain such a line either.
   * **Findings can be copied:** the list is selectable and *Copy findings* puts them on the clipboard as text (push
     dialog and the Sync page's commit and push reviews).
   * **Push is disabled while `behind > 0`**, with a plain explanation: "origin/main has 3 commits you don't have.
     Integrate them first (pull --rebase or merge), resolve any conflicts, run the tests, then push. NEXUS never
     force-pushes."
   * **Offline is said, not hidden:** when the fetch fails, the dialog names why (offline, credentials …), the counts
     are those of the last fetch, and a reviewed attempt stays possible.
2. **The failure kind is kept.** Migration 0020 adds `push_requests.failure_kind` (`rejected`, `auth`, `offline`,
   `remote-missing`, `timeout`, `other` — Git's `RemoteFailure`, the same names as the sync history) and `dismissed_at`.
   * `decide()` records the kind and Git's redacted message and returns the request (a recorded outcome is not an IPC
     error). A remote that moved on between the review and the push (the fresh review in `decide` sees `behind > 0`) is
     recorded as `rejected` without trying.
   * An **action** notification ("Push of main (anilist) was rejected — the remote has new commits"), with the next
     step as its body, opens the request's dialog. A later push of the same request, a newer request for the branch,
     or a dismissal resolves it.
   * **A failure never just disappears:** `open()` lists pending requests and failed ones until the person dismisses
     them or a newer request for the branch supersedes them. The project banner shows why and what next, with *Review
     again* and *Dismiss*. A failed request can be decided again (after signing in, or once online).
3. **The dialog shows outcomes.** No reload clears an error the person has not seen. After a decision the dialog stays
   open with a result panel: pushed (the commits, the remote, *Open Commits*) or failed with guidance by kind (rejected
   → integrate; auth → sign in to Git; offline → retry). The Push button shows a spinner while it runs.
4. **Tell Claude** (owner decision 2026-11-10: typing into a running Claude terminal is fine, never pressing Enter). For
   a rejected push the dialog types a fixed, NEXUS-authored message into the requesting session's terminal — "NEXUS:
   the push of <branch> was rejected — <remote>/<branch> has N new commits. Integrate them (git pull --rebase or merge),
   resolve conflicts, run the tests, then call request_push again." — and the person sends it. Only Git names (control
   characters removed) and a count are filled in; no workspace text. When the session is not running, the text goes to
   the clipboard.
5. **Claude learns the kind.** `request_push` answers a failed request with `status: failed`, `failureKind` and the
   next step (`push_request::next_step`, the same words the person reads). The tool count does not change.
6. **Conflicts are visible.** The project page shows "N conflicted" and a merge/rebase/cherry-pick/revert in progress
   (`git::operation_in_progress`, filled only for the project status) on the Repository card and in the header, and
   ahead/behind against the upstream.
7. **Automatic workspace sync (ADR-0042 amendment).** The first failed automatic publication publishes a warning
   (one open notification until a publication succeeds; retries do not repeat it). A push the remote refuses is
   treated like a remote that is ahead: it pauses and asks (notification → Sync Center) without a retry loop.

## Consequences

* The anilist case now reads: the dialog shows the teammate's commit before the push; Push is disabled with the next
  step; if the race happens anyway, the request stays on the page as failed, the bell says so, and Claude is told.
* NEXUS still never merges, rebases or force-pushes; integrating is the person's (or Claude's) Git work.
* Reviews need the network for a moment; offline they still work, saying so.

## Alternatives considered

* **Pull or rebase for the person** — merging changes code; out of scope for NEXUS (never force, never integrate).
* **Pressing Enter after typing into Claude's terminal** — would turn a NEXUS message into a user turn without the
  person; the person sends it.
* **Keeping `decide` an IPC error on failure** — the dialog would again have to reconstruct the outcome; a recorded
  outcome is data.

# ADR-0006 — Git through the git CLI; NEXUS holds no Git credentials

Status: Accepted (2026-11-04)

## Decision
* All Git operations run the user's `git` executable with explicit argument vectors and machine-readable
  output (`status --porcelain=v2 --branch -z`, `log --format` with NUL separators).
* Read-only operations set `GIT_OPTIONAL_LOCKS=0` and pass `-c core.fsmonitor=false` (avoid index-lock races
  with the user's own git, and arbitrary command execution from a malicious repository `.git/config`).
* Network operations (fetch/pull/push/clone) rely on the user's credential helper (Git Credential Manager,
  osxkeychain, libsecret) or `gh`. NEXUS never asks for, stores or logs Git credentials.
* Destructive operations (force push, hard reset, branch delete, clean) are never automatic and always require
  an explicit confirmation naming the consequence.

## Alternatives
* `git2`/libgit2 — incomplete worktree/credential-helper parity and behaviour that differs from the user's git.
* `gix` — fast reads, but write/network parity is incomplete.

## Consequences
* Requires Git ≥ 2.30; `nexus doctor` reports older versions.
* One process spawn per call (~5–20 ms on Windows); status is fetched on demand and on focus.

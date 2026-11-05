---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Git practices
status: active
applies: on-demand
prefix: GIT
created: {{now}}
updated: {{now}}
source: import
---

How to stage, commit, branch, merge, tag and keep a Git repository clean. Apply it whenever you stage, commit, branch, open a pull request or touch history; the repository's own conventions come first.

## Commits

A readable, bisectable history is a debugging and audit tool, not a by-product.

#### Make each commit one logical change
<!-- rule: id=GIT-1 enforcement=rule -->
A commit contains one coherent change that builds and passes tests on its own: a fix, a feature slice or a refactor. Keep pure refactors (renames, moves, extractions) in separate commits from behaviour changes, so history stays bisectable and each part can be reviewed and reverted on its own.

#### Follow the repository's commit message convention
<!-- rule: id=GIT-2 enforcement=rule -->
Use the convention the history already shows; otherwise Conventional Commits: `type(scope): summary` (feat, fix, refactor, perf, test, docs, build, ci, chore), imperative mood, at most 72 characters, no trailing period. Explain why in the body; mark breaking changes with `!` or a `BREAKING CHANGE:` footer.

#### Commit or push only when the person asks
<!-- rule: id=GIT-3 enforcement=rule -->
Do not commit, push, tag, or open or merge pull requests on your own initiative; prepare the change and say what would be committed. A request to commit is not a request to push.

## Shared history and safeguards

Other people and systems build on published commits, and hooks and branch protection exist for a reason.

#### Never rewrite shared history or bypass repository safeguards
<!-- rule: id=GIT-4 enforcement=mandatory -->
Never force-push, rebase, amend or reset commits already on a shared branch; rewrite only branches nobody else uses, after asking (CORE-3), and push them with `--force-with-lease`. Never skip hooks or signing (`--no-verify`, `--no-gpg-sign`) or bypass branch protection; when a hook fails, fix the cause or report it.

#### Keep the default branch releasable
<!-- rule: id=GIT-5 enforcement=rule check=test -->
Merge into the default branch only changes whose tests and checks pass, and hide unfinished features behind a flag instead of merging them half done. If the default branch breaks, fixing or reverting it takes priority over new work.

## Branches and pull requests

Short-lived branches and small pull requests keep integration cheap and reviews meaningful.

#### Use short-lived, descriptively named branches
<!-- rule: id=GIT-6 enforcement=guideline -->
Branch from the current default branch and merge back within days, not weeks. Follow the project's branch naming; otherwise use `<type>/<short-description>` with an optional ticket id, e.g. `feat/1234-invoice-export` or `fix/login-redirect`.

#### Keep pull requests small and reviewable
<!-- rule: id=GIT-7 enforcement=rule -->
Aim for pull requests a reviewer can understand in one sitting, roughly under 400 changed lines excluding generated files; split larger work into sequential pull requests. Describe what changed, why, how it was tested, and any risk or follow-up.

#### Resolve conflicts deliberately and re-run tests
<!-- rule: id=GIT-8 enforcement=rule -->
Resolve each conflict by understanding both sides, never by taking "ours" or "theirs" wholesale without reading them. After a merge or rebase with conflicts, build and run the relevant tests before pushing, because a textually clean merge can still be semantically broken.

## Repository contents

The repository holds source, not artifacts that can be rebuilt or fetched, and not other people's unfinished work.

#### Stage only the files of your change
<!-- rule: id=GIT-9 enforcement=rule -->
Keep build output, dependency folders, caches, coverage and local env files out of Git via `.gitignore` (commit only a placeholder `.env.example`). Check `git status`, then stage your change's files by explicit path, never everything with `git add -A` or `git add .`; commit generated code only where the project intends it.

#### Keep large binaries out of Git history
<!-- rule: id=GIT-10 enforcement=rule -->
Do not commit large binary files such as media, datasets, archives or installers (rule of thumb: over a few MB). Track them with Git LFS, set up before the first such commit, or keep them in an artifact store, because a committed binary stays in history and slows every clone.

#### Tag releases in the project's versioning scheme
<!-- rule: id=GIT-11 enforcement=rule -->
Tag each release on the exact commit that was built and shipped, with an annotated tag in the project's scheme, by default semantic versioning `vMAJOR.MINOR.PATCH` (breaking change, compatible feature, fix). Never move, delete or reuse a published tag.

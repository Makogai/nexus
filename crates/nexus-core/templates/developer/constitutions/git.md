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

How to commit, branch, merge, tag and keep a Git repository clean. Apply it whenever you stage, commit, branch, open a pull request or touch history.

## Commits

A readable, bisectable history is a debugging and audit tool, not a by-product.

#### Make each commit one logical change
<!-- rule: id=GIT-1 enforcement=rule -->
A commit contains one coherent change that builds and passes tests on its own: a fix, a feature slice or a refactor, not a mix. Stage selectively (`git add -p`) to split unrelated edits, so history stays bisectable and a single commit can be reverted safely.

#### Write Conventional Commits messages
<!-- rule: id=GIT-2 enforcement=rule -->
Use `type(scope): summary` with types such as feat, fix, refactor, perf, test, docs, build, ci or chore. Write the summary in the imperative mood ("add", not "added"), at most 72 characters, no trailing period. Explain why in the body; mark breaking changes with `!` or a `BREAKING CHANGE:` footer.

#### Commit or push only when the person asks
<!-- rule: id=GIT-3 enforcement=rule -->
Do not create commits, push, open or merge pull requests, or create tags on your own initiative. Prepare the change, show what would be committed, and wait for an explicit request. A request to commit is not a request to push.

## Shared history

Other people and systems build on published commits, so they must stay stable.

#### Never rewrite the history of shared branches
<!-- rule: id=GIT-4 enforcement=mandatory -->
Never force-push, rebase, amend or reset commits that are already on a shared branch (main, develop, release branches or any branch others use). Rewrite history only on your own unpublished or personal branch, and then push with `--force-with-lease`, never plain `--force`.

#### Keep the default branch releasable
<!-- rule: id=GIT-5 enforcement=rule check=test -->
Merge into the default branch only changes whose tests and checks pass; hide unfinished features behind a flag instead of merging them half-done. If the default branch breaks, fixing or reverting it takes priority over new work.

## Branches and pull requests

Short-lived branches and small pull requests keep integration cheap and reviews meaningful.

#### Use short-lived, descriptively named branches
<!-- rule: id=GIT-6 enforcement=rule -->
Branch from the up-to-date default branch and merge back within days, not weeks. Name branches `<type>/<short-description>` with an optional ticket id, e.g. `feat/1234-invoice-export` or `fix/login-redirect`, and delete them after merging.

#### Keep pull requests small and reviewable
<!-- rule: id=GIT-7 enforcement=rule -->
Aim for pull requests a reviewer can understand in one sitting, roughly under 400 changed lines excluding generated files. Split larger work into sequential pull requests, and describe what changed, why, how it was tested, and any risk or follow-up.

#### Resolve conflicts deliberately and re-run tests
<!-- rule: id=GIT-8 enforcement=rule -->
Resolve each conflict by understanding both sides, never by taking "ours" or "theirs" wholesale without reading them. After a merge or rebase with conflicts, build and run the relevant tests before pushing, because a textually clean merge can still be semantically broken.

## Repository contents

The repository holds source, not artifacts that can be rebuilt or fetched.

#### Never commit build output, dependencies or generated artifacts
<!-- rule: id=GIT-9 enforcement=rule -->
Keep compiled output, bundles, coverage reports, dependency folders (`node_modules/`, `target/`, virtual environments) and caches out of the repository. Commit generated or vendored code only when the project documents that it intends to, and regenerate it rather than editing it by hand.

#### Maintain the .gitignore for the project's stack
<!-- rule: id=GIT-10 enforcement=rule -->
Keep a root `.gitignore` covering build output, dependency folders, caches and local environment files (`.env`, `.env.*`, except a committed `.env.example`). Before committing, check `git status` for unexpected files instead of staging everything with `git add .` or `git add -A`.

#### Keep large binaries out of Git history
<!-- rule: id=GIT-11 enforcement=rule -->
Do not commit large binary files such as media, datasets, archives or installers (rule of thumb: anything over a few MB). Track them with Git LFS, set up before the first such commit, or keep them in an artifact store, because a committed binary stays in history and slows every clone.

#### Tag releases with semantic versions
<!-- rule: id=GIT-12 enforcement=rule -->
Mark each release with an annotated tag `vMAJOR.MINOR.PATCH` on the exact commit that was built and shipped. Increment MAJOR for breaking changes, MINOR for compatible features and PATCH for fixes; never move, delete or reuse a published tag.

# User guide

A short tour of each area. Everything stays on your machine unless you push it through Git yourself.

| Area | What it is for |
| --- | --- |
| **Home** | What needs you now: sessions waiting for a decision, reviews, recent work. |
| **Projects** | Repositories of the workspace — add a folder or **clone from Git**. Each project has *Overview* (Git status, context, usage, *Resume project*, *Run workflow*), **Learn** (build its knowledge base: NEXUS's own map, a guided Claude session, drafts you accept; Graphify optional; notes for Claude and extra folders it may read — e.g. the repository the project lives in, kept on this computer), **Files** (a read-only tree and viewer; files that may hold secrets are never shown) and **Commits** (history, changed files, diffs). |
| **Sessions** / terminal panel | Claude sessions and shells. Sessions keep running when you switch views; ended ones can be resumed. |
| **Knowledge** | Tasks, notes, decisions and rules as Markdown files in the workspace — opened formatted (rule cards for constitutions) and editable in place (*Edit*). Synced rules need your approval. |
| **Constitutions** | Structured rules (mandatory policies, rules, guidelines) by scope; which ones Claude is told, and which are verified by a gate. |
| **Context** | How much NEXUS gives each session per project (minimal / balanced / deep) and what Claude actually loaded. |
| **Workflows** | Reusable workflows (feature, bug fix, review, …) on tasks. Claude *reports* steps; **quality gates** — commands you define and approve per project — *verify* them. |
| **Usage** | Token usage parsed from Claude Code's local transcripts, by project, profile, task, workflow and context strategy. Estimates and unavailable values are labelled. |
| **Sync** | The workspace's Git repository: fetch, fast-forward pull, reviewed commit and push. |
| **Notifications** | Permission prompts, questions, reviews, finished or failed workflows; clicking one opens the right session or view. |
| **Claude profiles** | Your Claude configurations, their sign-in state and read-only diagnostics. |
| **AI Kits** | Claude Code plugins and skill folders, reviewed by you and enabled per profile. |
| **Settings** | Appearance, terminal, workspaces, data folder, **backups**, the command-line tool. |

## The `nexus` command line
Every desktop feature has a CLI equivalent — `nexus --help`. The installer puts `nexus.exe` next to the app; Settings →
*Command-line tool* shows its path and copies a command that adds it to your PATH (NEXUS never changes PATH itself).
Desktop sessions always use it by its full path.

## Things NEXUS will never do
* Change your Claude configuration folders, read credential files, or store credentials.
* Change or run anything in your project repositories (the only write is a local `.git/info/exclude` line for
  `graphify-out/` when you choose *Build the graph*); show files that may hold secrets.
* Run a gate command you have not approved, or a changed version of one.
* Commit, push or publish anything without your confirmation; force-push; merge or rebase for you.
* Send telemetry or usage anywhere.

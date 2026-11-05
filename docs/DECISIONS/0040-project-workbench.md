# ADR-0040 — Project workbench: adding from Git, reading and editing knowledge, exploring a project, learning it

Status: Accepted (2026-11-05, stage "Project workbench", milestones W1–W4; branch `milestone/project-workbench`)

## Context

The owner's first use of the Portable Workspace stage raised four requests:

1. add a project straight from a Git URL;
2. read rules and documents formatted, and edit them, in a calmer UI;
3. look at a project's files in a small viewer and at the history of what was committed;
4. build a knowledge base for a newly added project: map it, document it, record memory, use Graphify where available.

NEXUS's boundaries stay as they are. It only reads project repositories (it never changes or executes them). It calls
no model itself. MCP tools write only knowledge records (ADR-0009, ADR-0017). Workspace and repository text is
untrusted (ADR-0020). Secrets are never displayed.

The owner made three choices (2026-11-05):

* the code viewer is CodeMirror 6, read-only;
* learning is one click that leads to a guided Claude session, whose drafts the person approves;
* Graphify is used when it is installed, and the person runs it each time.

## Decision

1. **Add from Git (W1).**
   * `project::discovery::clone_new` clones into the workspace's `projects/<name>` (ignored by Git, restorable by id)
     or into an empty folder the person chooses, then registers the project with its portable context.
   * The same repository twice, unsupported URLs, occupied folders and failed clones change nothing.
   * Clones get 20 minutes.
2. **Knowledge reading and editing (W2).**
   * Documents render as Markdown. markdown-it runs with raw HTML disabled; dangerous link schemes are dropped; remote
     images are not loaded; http(s) links open in the system browser and nothing else navigates the app.
   * Constitutions render as article, section and rule cards.
   * Editing goes through `knowledge::service::update_doc`. It is optimistic against the version that was read and
     validated; constitutions go through their parser; tasks keep their own update.
   * The saved version is approved on this machine, because the person wrote it here (the same rule as the Constitution
     Manager).
   * The Constitution Manager opens documents in Read mode.
3. **Project explorer (W3, `project::explorer`).** Read-only.
   * Paths are canonicalised inside the project. `.git` is never listed; a symlink is listed but never followed.
   * Files that may hold secrets (the sync inspection's `sensitive_reason`) are never shown.
   * Text is shown up to 1 MB; binaries are not shown.
   * History reads (`git::log_page`, `commit_files`, `commit_file_diff`) run with no external diff, no textconv and
     no signature verification, so viewing never runs a program configured for the repository. Commit ids and paths
     are validated.
   * The explorer is a desktop feature only (not MCP).
4. **Learn a project (W4, `project::learn`).**
   * `map` is NEXUS's own reading, with no model and nothing executed: tracked files by folder and language,
     documentation, stack and commands, history size (author *counts*, never names or addresses), the state of the
     portable context, and whether Graphify is on the PATH and a graph exists.
   * `start` creates one learning task whose brief carries the map as data and asks Claude to read without changing
     files. The desktop launches Claude on that task.
   * Claude drafts the overview or the architecture summary through a new MCP tool, `draft_project_document`
     (22 tools). The tool writes an **unverified note** labelled `draft` + topic, through `knowledge::service`, so the
     only MCP file write is still a knowledge record.
   * Durable facts go through `record_memory`: notes are unverified, and decisions start as proposals.
   * The person decides. `apply_draft` writes a draft into `overview.md`, or into the architecture document's summary
     section (the verified structure section stays). `discard_draft` keeps the note, labelled `discarded`.
   * Graphify: NEXUS types the offline, code-only build (`graphify extract . --code-only`, from the Graphify README) into
     a terminal in the project, and the person presses Enter. `graphify-out/` is kept out of Git through the clone's
     local `.git/info/exclude`, never a committed file. Installing Graphify stays the person's action.

## Consequences

* NEXUS writes into a project repository's `.git/info/exclude`. This is the only write into a repository, it is local
  to the clone, and it happens only on the person's explicit "Build the graph".
* Two kinds of text are exposed to injection: repository content shown in the explorer, and Claude's drafts. The
  explorer shows file content as text, never as HTML. Drafts are unverified until the person accepts them, and an
  accepted overview or architecture summary names its origin.
* The MCP tool count rises to 22. Clients and tests that list the tools were updated.
* New dependencies: CodeMirror 6 and markdown-it, all permissive (ADR-0035).

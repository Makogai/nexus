# Your portable workspace

A NEXUS workspace is a folder — best kept as a **private Git repository** — that carries your AI development
environment between computers: constitutions (how Claude works), skills, workflows, integration manifests, and for each
project its portable context (overview, architecture notes, current state, environment requirements, tasks,
decisions, memory and development records). Your project repositories stay independent; NEXUS clones them on another
computer from their recorded remotes.

## Create your first workspace

1. Create an **empty private repository** for the workspace on your Git host (a README, licence or `.gitignore` it
   adds is fine).
2. Start NEXUS → **Create a workspace** → *Empty Git repository* → paste its URL, choose where to clone it and a name
   → pick a **starter template**: *Minimal*, *Developer*, or *AI Engineering Pro* (27 constitutions, 13 technology
   rule sets, 11 skills, 7 workflows). Only a small core agreement is always in Claude's context; everything else is
   read when relevant, and technology rules apply only to projects that use the technology. (Without a repository:
   *New folder*; for a clone you already made: *Existing empty folder*. A folder with other content — a project —
   never becomes a workspace. Settings → Workspaces → *New workspace…* offers the same choices later.)
3. NEXUS opens the **Sync** view: **Review & commit → Push**. NEXUS inspects every commit and push for credentials and
   private data; the first push publishes the workspace to your repository. (Git needs your `user.name` and
   `user.email` to commit.)
4. **Claude profiles** → add your profile(s); **Claude Code setup** helps if Claude Code is not installed yet.
5. **Projects → Add project** for each repository — a folder on this computer or **Clone from Git** (into the
   workspace's `projects/`, or a folder you choose). NEXUS creates its portable context under
   `project-context/<project>/` from the project's own files (nothing is invented). Clones you keep inside the
   workspace go to `projects/` (ignored by Git). A project you register from another folder inside the workspace is
   added to `.gitignore` too, and NEXUS refuses to commit any other repository nested in the workspace. The project
   opens on **Learn**: NEXUS shows what it read itself, *Start a learning session* lets Claude draft the overview and
   architecture and record memories, and you decide which drafts enter the knowledge base (Graphify optional).
6. Work as usual. When a Claude session that changed something ends, NEXUS writes a **development record** into the
   project's context — verified Git facts, what Claude Code's hooks observed, and what Claude reported (marked
   unverified). See it in the project's **Development history**.
7. Optional: **Sync → Sync policy**. *Manual* (default) publishes nothing on its own; *Automatic with review* asks
   when records are ready; *Trusted automatic* publishes only NEXUS's own records to the remote you approve, after the
   same inspection, and never force-pushes. Tasks, decisions, memories and constitutions always wait for your review.

The workspace starts with `constitutions/`, `knowledge/`, `project-context/` and `workflows/` (each with a short README
saying what belongs there), `skills/` and `integrations/` from the template, and `projects/` for clones (ignored by
Git). A workspace created without a repository can be published later: in its folder `git init` and
`git remote add origin <your private repository>`, then **Sync → Review & commit → Push**.

## Restore it on another computer

1. Install NEXUS, then **Open from Git** with the workspace's URL and a folder.
2. The **Restore on this computer** view walks you through the rest: Claude Code, the workspace schema, the knowledge
   index (rebuilt here), **approving** the synced constitutions and workflows, the **workspace skills** (register, then
   approve in AI Kits), **cloning** the projects from their remotes (same project ids), the projects' **tools**, your
   **Claude profiles**, and quality-gate commands. Each step shows what it changes before it changes anything.
3. Open a project: its overview, current state, open tasks and development history are already there.

**What cannot come back from the workspace**: Claude conversations and transcripts (they stay in the other computer's
profiles), uncommitted or unpushed source code, local-only knowledge, machine-local state (sessions, usage,
notifications), approvals and gate commands, and your Claude sign-ins. The Restore view lists these too.

## Keep it healthy

**Workspace → Workspace Doctor** checks the workspace and this computer and offers repairs that show their plan first
(migrate an older workspace, missing `.gitignore` lines — including project folders inside the workspace —, template updates that never overwrite files you changed,
missing projects, project context, workspace skills). The CLI has the same: `nexus workspaces doctor`,
`nexus workspaces restore`, `nexus templates`, `nexus projects scan|clone|env|history`, `nexus sync policy`.

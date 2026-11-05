# Your portable workspace

A NEXUS workspace is a folder — best kept as a **private Git repository** — that carries your AI development
environment between computers: constitutions (how Claude works), skills, workflows, integration manifests, and for each
project its portable context (overview, architecture notes, current state, environment requirements, tasks,
decisions, memory and development records). Your project repositories stay independent; NEXUS clones them on another
computer from their recorded remotes.

## Create your first workspace

1. Start NEXUS → **Create a workspace** → choose a name and a parent folder → pick a **starter template**:
   *Minimal*, *Developer*, or *AI Engineering Pro* (27 constitutions, 13 technology rule sets, 11 skills, 7
   workflows). Only a small core agreement is always in Claude's context; everything else is read when relevant, and
   technology rules apply only to projects that use the technology.
2. **Claude profiles** → add your profile(s); **Claude Code setup** helps if Claude Code is not installed yet.
3. **Projects → Add project** for each repository. NEXUS creates its portable context under
   `project-context/<project>/` from the project's own files (nothing is invented). Clones you keep inside the
   workspace go to `projects/` (ignored by Git). A project you register from another folder inside the workspace is
   added to `.gitignore` too, and NEXUS refuses to commit any other repository nested in the workspace.
4. Make the workspace a Git repository with a **private** remote (in your Git tool: `git init`, add the remote), then
   **Sync → Review & commit → Push**. NEXUS inspects every commit and push for credentials and private data.
5. Work as usual. When a Claude session that changed something ends, NEXUS writes a **development record** into the
   project's context — verified Git facts, what Claude Code's hooks observed, and what Claude reported (marked
   unverified). See it in the project's **Development history**.
6. Optional: **Sync → Sync policy**. *Manual* (default) publishes nothing on its own; *Automatic with review* asks
   when records are ready; *Trusted automatic* publishes only NEXUS's own records to the remote you approve, after the
   same inspection, and never force-pushes. Tasks, decisions, memories and constitutions always wait for your review.

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

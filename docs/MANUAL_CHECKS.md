# Manual check: validate NEXUS with your real Personal profile

~15 minutes. Validates the one thing automation deliberately never does: launching with a real account.
**Only the Personal profile is used. Do not register or launch the company (Default `~/.claude`) profile.**

## What may change, and what must not

| Location | Who writes | Expected |
| --- | --- | --- |
| `~/.claude-personal/` | NEXUS | **Nothing, ever.** NEXUS only checks which files exist. |
| `~/.claude-personal/` | Claude Code itself, when you launch a session | Normal Claude activity: transcript, history, trust entry for the test folder — exactly as with your `claude-personal` command |
| `~/.claude/`, `~/.claude.json` (company) | anyone | **Nothing.** Not registered, not launched |
| NEXUS data dir | NEXUS | Its database (delete the folder to reset NEXUS completely) |

## Steps

1. **Snapshot** (PowerShell) — newest write time and file count in the Personal profile:
   ```powershell
   Get-ChildItem $HOME\.claude-personal -Recurse -File | Measure-Object -Property LastWriteTime -Maximum | Select-Object Count, Maximum
   ```
2. **Start NEXUS** with a fresh data dir:
   `cd apps\desktop; $env:NEXUS_DATA_DIR="$env:TEMP\nexus-manual"; npm run tauri dev`
3. **Workspace + project**: create a workspace in an empty folder; add a throwaway repo
   (`mkdir $env:TEMP\nexus-try; cd $env:TEMP\nexus-try; git init`). Do not use a company repository.
4. **Register Personal only**: *Claude profiles → Found on this machine → Personal → Register*. Leave *Default*
   unregistered (the profile list runs `claude auth status` for every registered profile).
5. **Re-run the snapshot** from step 1 → `Count` and `Maximum` are unchanged. ✅ NEXUS wrote nothing.
6. **Profile card**: *Sign-in* shows your personal account and plan; *Config* is `~\.claude-personal`; no
   "Organization-managed" box unless a machine-wide policy exists.
7. **Launch**: *Launch with this profile* → project `nexus-try`, work profile *Prototype* → *Launch*. Accept the
   folder-trust prompt inside Claude.
8. In Claude, run `/status` → account = personal, settings paths under `.claude-personal`.
9. Ask Claude to run `env | grep -E '^(ANTHROPIC_|OTEL_|NEXUS_|CLAUDE_CONFIG_DIR|CLAUDECODE)'`. Expected:
   `CLAUDE_CONFIG_DIR=…\.claude-personal`, `NEXUS_SESSION_ID/WORKSPACE_ID/PROJECT_ID`, `CLAUDECODE=1` (set by this
   Claude for its own tools) and **no** `ANTHROPIC_*` or `OTEL_*` lines (unless your Personal settings define them).
10. Send one short prompt (uses a little quota), then `/exit` → the tab shows *process exited*, *Sessions* shows
    *Exited* with **Resume** → click it → the previous conversation is shown.
11. With the resumed session running, close the NEXUS window → confirm *Quit* → start NEXUS again → the session is
    *Interrupted (NEXUS closed)* → **Resume** works again.
12. Outside NEXUS, your usual `claude-personal` and `claude` commands behave exactly as before.
13. **Hooks and NEXUS tools** (session 3): in a new session, `/hooks` lists the NEXUS hooks next to your own (yours
    still run); `/mcp` shows `nexus` connected. *Sessions* shows the turn count rising and *Waiting for you* while
    Claude asks for a permission.
14. **Knowledge**: *Knowledge → New* → Rule "Answer in English", delivery *Always*. Launch a new session and ask
    "Which NEXUS rules apply?" → Claude names the rule. Ask "Create a NEXUS task to add a README" → the task appears
    on the board with source `claude`. Edit that rule's file in `<workspace>\rules\` with a text editor
    → the Knowledge view asks for approval again and the next session does not get it until you approve.
15. **Task flow** (session 4): on the project page, *Launch* an open task → the dialog names the task → in Claude ask
    "Which NEXUS task am I on?" → Claude names it without reading files. Ask Claude to tick the first checklist item
    and move the task to review → the board shows it; the task file changed. Close NEXUS, reopen, *Resume* → Claude
    continues the same conversation and knows the task is in review.
16. **Effective instructions**: project page → *Effective instructions* with Personal selected → the list matches
    what `/memory` shows in Claude. If a "belongs to the default Claude profile" warning appears, your
    `~\.claude\CLAUDE.md` reaches Personal sessions for projects inside your home folder (Claude behaviour).
17. **Profile isolation** (only if step 16 showed the warning): *Claude profiles → edit Personal* → tick "Keep my
    default profile's personal instructions out of this profile's sessions" → the inspector shows those files as
    *excluded — NEXUS profile isolation*; in a new session `/memory` no longer lists them. `claude` started outside
    NEXUS still loads them (unchanged).
18. **Resume project**: project page → *Resume project* → the *Verified now* column matches `git status` / `git log`
    in your terminal; *Saved context* shows the tasks and checkpoints from steps 14–15; the recommended button resumes
    or launches as described.
19. **Proposed memory**: in a session ask Claude to "record a NEXUS checkpoint of kind decision: we use X for Y",
    then `/exit` → *Knowledge → Proposals* shows it; nothing appears under *Decisions* until you approve it.
20. **Diagnostics** (read-only): *Claude profiles → Personal → Diagnostics* → skills, agents, MCP servers and plugins
    match what `/mcp`, `/agents` and `/plugin` show in Claude; no secret values appear anywhere on the page.
21. **AI Kit** (optional): register a local plugin folder in *AI Kits*, approve it, enable one skill for Personal →
    a new NEXUS session of Personal lists `<kit>:<skill>` in `/skills`; a session of another profile does not; plain
    `claude` outside NEXUS does not.
22. Re-run the snapshot from step 1 → still unchanged (hooks, MCP, isolation and kits are session-scoped, not profile edits).
23. **Notifications** (needs a signed-in session): *Notifications → Preferences → Send a test notification* → a
    Windows notification appears (in development builds titled by "Windows PowerShell"). Start a Personal session,
    ask Claude to run `git status`, switch to another NEXUS view or minimise NEXUS → when Claude asks for permission a
    notification "Permission requested: Bash" appears; clicking it brings NEXUS back with that terminal; the bell
    shows it and the session row says *Permission required*. Approve in the terminal → it shows as handled. After the
    answer, the turn end shows *Your turn* and (if NEXUS is not focused) "Claude finished in …". Nothing was
    approved or typed by NEXUS.
24. **Constitution** (optional): *Constitutions → New constitution* (starter) in your test workspace → start a
    Personal session in the test project → ask Claude "which NEXUS constitution rules apply here?" → it names the
    rule ids from the *Effective rules* tab (mandatory policies first) and can read a guideline with
    `get_constitution`.

Report the step number of anything unexpected. To undo: *Claude profiles → archive/forget* removes the registration;
deleting `%TEMP%\nexus-manual` removes all NEXUS state. NEXUS itself has nothing to undo in `~/.claude-personal`.

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

Report the step number of anything unexpected. To undo: *Claude profiles → archive/forget* removes the registration;
deleting `%TEMP%\nexus-manual` removes all NEXUS state. NEXUS itself has nothing to undo in `~/.claude-personal`.

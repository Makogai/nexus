# ADR-0052 — AI agents beyond Claude Code, starting with Codex

Status: Accepted (2026-11-10, owner: "i got Codex, so we can add it for starters but make it so later its easy to add
others"; branch `milestone/agents-dev-environment`). Changes the non-goal "multi-provider AI abstraction" in
`docs/PRODUCT.md`. ADR-0004 (session-scoped integration) and ADR-0020 (workspace text is data) apply to every agent.

## Context

Every NEXUS session is a Claude Code session: `claude_profiles`, `--mcp-config`, `--settings` hooks, `--plugin-dir`
kits, the instructions inspector and usage analytics all depend on Claude's flags and file formats. Most of what NEXUS
adds — tasks, checkpoints, knowledge, push requests, workflows, development records — reaches the agent through the NEXUS
MCP server, and MCP is supported by the other terminal agents. The owner uses the OpenAI Codex CLI as well and wants it
first, with room for more agents later (Gemini CLI, GitHub Copilot CLI, OpenCode were discussed).

Measured with codex-cli 0.166.0 (temporary `CODEX_HOME`, nothing in `~/.codex` touched):

* `-c key=value` overrides are TOML; strings escaped by the caller survive a real argv spawn with Windows paths and
  spaces (`codex -c mcp_servers.nexus.command=… -c mcp_servers.nexus.args=[…] mcp list --json` lists the server with the
  exact command, arguments and env).
* `-c developer_instructions="…"` appears as its own developer message after Codex's built-in instructions (it adds, it
  does not replace); AGENTS.md arrives as a user message (`codex debug prompt-input`).
* `codex resume <SESSION_ID>` takes the same options (`-C`, `--add-dir`, `-c`, `-m`). There is no option to choose the
  session id at start.
* Hooks exist (SessionStart, Stop, PermissionRequest, …) but non-managed hooks run only after the person trusted them in
  Codex (`/hooks`, persisted in their config); `--dangerously-bypass-hook-trust` lifts that for *all* enabled hooks.
* The legacy `notify` program (argv form) runs after each turn with a JSON payload that names the conversation
  (`thread-id`). Documented; **not yet observed with a real turn** (manual check).

## Decision

1. **An agent registry in the core** (`agent`): `agent::Kind` (Claude, Codex) with id, display name, toolchain id and a
   **capability table** — launch, resume, NEXUS tools, context, attention, activity, usage, AI Kits & skills,
   instructions inspector, profile diagnostics — each *full*, *partial* or *not available*, with the reason in words.
   The desktop shows the table per agent and the session view shows what its session got. Adding an agent means: a
   `Kind` variant and its capabilities, an adapter module (`<agent>::launch` pure argv/env builder, `<agent>::prepare`),
   its home-directory rule and its environment policy — nothing in the Claude path changes.
2. **Claude keeps its profiles and subsystems unchanged.** Other agents' profiles are rows of `agent_profiles`
   (migration 0021): agent, name, home folder (`NULL` = the agent's default), executable, extra arguments, provider
   environment opt-in, turn notifications, default work profile and model. For Codex the home is `CODEX_HOME`: the
   default profile removes an inherited `CODEX_HOME` from the child (like `CLAUDE_CONFIG_DIR`); a registered profile
   names an existing folder. NEXUS never writes into a Codex home; it creates none in this stage (signing in to another
   home is `codex login` run by the person).
3. **Sessions**: `kind = 'claude'` keeps meaning "AI agent session" (historical name, no table rebuild); the new column
   `agent` says which (`'claude'` backfilled) and `agent_profile_id` points at the agent profile. The conversation id
   column holds the agent's own id. Claude-only subsystems (usage transcripts, InstructionsLoaded observation, kits,
   profile diagnostics, memory proposals from hook activity) select `agent = 'claude'`; agent-neutral ones (attention,
   tasks, workflows, resume project, development records' verified commits and gates) take every agent.
4. **Codex is integrated per session through `-c` overrides only** (ADR-0004):
   * the NEXUS MCP server: `mcp_servers.nexus.command` / `.args = ["--data-dir", <data>, "mcp", "--session", <id>]` — the
     same server and authorization scope as a Claude session;
   * `developer_instructions`: the work profile and NEXUS-authored text only (approved always-on rules, the commit-message
     note, the task pointer). A task's own text is untrusted and reaches Codex only through `get_task` (delivery
     `tools`, T22);
   * `notify = [<nexus>, "--data-dir", <data>, "agent-notify", "--session", <id>]` when the profile keeps turn
     notifications on. `nexus agent-notify` reads only `type` and `thread-id` (a UUID) from the payload — never
     `input-messages` or `last-assistant-message` — stores no text, prints nothing and always exits 0. The first
     thread id becomes the session's conversation id (a different one later is ignored); `agent-turn-complete` is the
     *turn ended* attention signal and counts a turn. For NEXUS sessions this replaces a `notify` program in the person's
     own Codex config, which the profile setting explains.
5. **No Codex hooks**: NEXUS cannot record hook trust without writing into the person's Codex config, and the bypass
   flag would also run the person's own untrusted hooks. Codex sessions therefore get no SessionStart context, no
   permission/question attention and no file/command activity; the UI says so instead of guessing. To revisit when Codex
   offers session-scoped trusted hooks.
6. **Resume only with a recorded thread id**: `codex resume <thread-id>` in the same directory under the same profile,
   with fresh session-scoped overrides. Without an id (no completed turn, notifications off) resume is not offered —
   no `--last` guessing (Truthful state).
7. **Environment per agent** (`env::Policy` names the agent): its profile variable (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`),
   its provider credentials unless the profile opts in (`ANTHROPIC_*` / `OPENAI_API_KEY`, `OPENAI_BASE_URL`,
   `CODEX_API_KEY`, `CODEX_ACCESS_TOKEN`), its session markers (`CODEX_SANDBOX`, `CODEX_SANDBOX_NETWORK_DISABLED`),
   telemetry (`OTEL_*`). Shells are unchanged.
8. **Safety of extra arguments**: Codex profiles refuse what NEXUS manages (`-c`/`--config`, `-C`/`--cd`, `-m`/`--model`,
   `--remote`, `--remote-auth-token-env`) and what removes protections (`--dangerously-bypass-approvals-and-sandbox`,
   `--dangerously-bypass-hook-trust`, sandbox `danger-full-access`). Sandbox and approvals otherwise stay Codex's own.
9. **One builder for both hosts**: the desktop and `nexus launch --agent codex` use `codex::prepare` and
   `codex::launch::build`, as Claude sessions share `claude::prepare`.

## Capabilities of Codex sessions in this stage

| Capability | Codex | Why |
| --- | --- | --- |
| Launch in a NEXUS terminal | full | PTY, clean environment, session record |
| NEXUS tools (MCP) | full | `-c mcp_servers.nexus.*` |
| Context | partial | work profile, approved rules, task pointer via `developer_instructions`; no SessionStart brief |
| Resume | partial | after the first completed turn (thread id from `notify`) |
| Attention | partial | turn finished (`notify`) and `request_attention`; no permission/question signals (no hooks) |
| Activity | partial | turns only |
| Usage | not available | Codex's transcript format is not a stable interface |
| AI Kits & skills | not available | Claude plugins; Codex skills are a later step |
| Instructions inspector, profile diagnostics | not available | Claude-specific |

## Consequences

* Codex works next to Claude in the same projects, tasks and terminal panel, with NEXUS's tools and rules.
* The next agent (Gemini CLI, Copilot CLI, OpenCode) is a new adapter and capability row; its own facts get measured the
  same way before it is added.
* Two profile tables exist side by side; a later migration may move Claude profiles into `agent_profiles` once the
  Claude-specific columns are separated.
* Real-Codex verification (MCP tools listed in a real session, `notify` thread id, resume) is a manual check by the owner
  with their own login (`docs/MANUAL_CHECKS.md`); automated tests use `fake-codex` and temporary homes.

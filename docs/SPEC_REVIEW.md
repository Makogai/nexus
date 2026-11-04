# Critical review of the NEXUS specification

Reviewed 2026-11-04 against Claude Code 2.1.311 on the development machine. Every point either changes the
design (with an ADR) or is recorded as a risk. Severity: **H** = would cause incorrect behaviour or a
security problem if built as written, **M** = significant waste or complexity, **L** = clarification.

## A. Design flaws

| # | Sev | Spec says | Problem | Resolution |
| --- | --- | --- | --- | --- |
| A1 | H | Profiles are "config directory: ~/.claude-work" (§5.1) | The **default** Claude profile is not "a config dir". Without `CLAUDE_CONFIG_DIR`, global state lives in `~/.claude.json` (outside `~/.claude`). Launching the default profile with `CLAUDE_CONFIG_DIR=~/.claude` moves state to `~/.claude/.claude.json` → lost MCP servers, project trust, onboarding. | Profile has `config_dir: Option<Path>`; `None` = default location and the variable is *removed* from the child env. ADR-0003. |
| A2 | H | "Preserve unrelated environment variables" (§5.1) | Inherited `CLAUDECODE`, `CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_MESSAGING_TOKEN`, `ANTHROPIC_API_KEY` are *related*, not unrelated. If NEXUS is started from a Claude session (it will be, during development) children are classified as nested sessions (hidden from `--resume`), may join the parent's messaging socket, and an ambient API key overrides the profile's own login → **cross-profile credential bleed**. | Explicit scrub list + allowlisted opt-in. ADR-0003, threat T3. |
| A3 | H | SQLite is authoritative for tasks and memory (§12, §21, §29) **and** workspaces must be portable without pushing the DB (§6.4–6.5) | Contradiction: tasks/memory stored only in SQLite cannot follow a workspace to another machine. | Portable knowledge = files in the workspace; SQLite = machine-local state + rebuildable index. ADR-0005. |
| A4 | H | Project registrations live in both workspace files and SQLite (§3 Layer C, §29) | Two competing representations of the same fact. | Manifest holds portable fields (id, name, remote, relative path); SQLite holds the machine binding (absolute path, last opened). Reconciled on workspace open. ADR-0005. |
| A5 | H | "Install AI Kits / skills into profiles", "versioned configuration deployment" (§5.6–5.7) | Writing into config dirs contradicts "never modify existing configurations", collides with plugin management Claude already does, and makes rollback a NEXUS problem. | Kits are loaded **per session** with native `--plugin-dir`; enabling a kit for a profile is a NEXUS-side association, not a file copy. Persistent installs go through `claude plugin install`. ADR-0004. |
| A6 | H | Synced workspace may contain workflows/process definitions that NEXUS executes (§18, §11) | A `git pull` of a shared workspace becomes remote code execution on the developer's machine (same class as VS Code workspace trust / direnv). | Every command NEXUS executes from workspace content is hash-pinned to an explicit local approval stored in SQLite; changed content requires re-approval. Threat T6. |
| A7 | M | "Onboarding uses AI for interpretation" (§7.2) | `claude -p` **skips the workspace trust dialog**. Running it in a freshly imported repo lets that repo's `.claude/settings.json` hooks execute without the user ever seeing a trust prompt. | Import is deterministic-only. AI summaries run in an interactive session the user starts, or only after explicit trust confirmation. Threat T5. |
| A8 | M | GitHub OAuth tokens in OS credential storage (§6.1) | NEXUS would become a credential holder for no gain: Git Credential Manager / `gh` already do this securely. | NEXUS never stores Git credentials. All Git network ops go through the `git` CLI and the user's configured helper. ADR-0006. |
| A9 | M | MCP tools named `nexus_list_projects` (§15) | Claude Code already namespaces tools as `mcp__<server>__<tool>`; result would be `mcp__nexus__nexus_list_projects`. | Tool names without prefix (`list_projects`, …). |
| A10 | M | Hooks used for session discovery and checkpoints (§9, §15) | Hooks may be disabled by `allowManagedHooksOnly` / `disableAllHooks` (org profiles) and do not fire on crashes. Session IDs don't need discovery at all: `--session-id <uuid>` lets NEXUS assign them. | NEXUS assigns the Claude session ID. Hooks are an enhancement, never a dependency. ADR-0008. |
| A11 | M | Token analytics from "documented telemetry" (§23) | The org-managed profile on this machine already exports OpenTelemetry to the organisation; overriding `OTEL_*` would hijack corporate telemetry (managed env wins anyway). Transcript JSONL is explicitly documented as internal and version-unstable. | Per-profile usage source with explicit provenance labels (measured / parsed-unstable / unavailable). OTel only for personal profiles, opt-in. ADR-0010. |

## B. Unnecessary complexity

| # | Spec says | Why it is unnecessary | Resolution |
| --- | --- | --- | --- |
| B1 | Nine Rust crates + many packages (§4) | No second consumer exists for most of them; crate boundaries slow iteration. | `nexus-core` (lib), `nexus-desktop` (Tauri), `nexus-cli` (Phase 2). ADR-0001. |
| B2 | Custom workflow engine with dynamic orchestration (§18) | Claude Code has native skills, subagents and workflows (the dev machine has `enableWorkflows`). | NEXUS workflows = skills + checklists + NEXUS checkpoints over MCP. No orchestrator. ADR-0012. |
| B3 | 6-level context hierarchy + semantic index + relevance ranking (§13) | Claude already has CLAUDE.md hierarchy, rules, auto-memory and progressive skill disclosure. | Inject *pointers*, not content; Claude pulls via MCP (`search_memory`, FTS5). Semantic index only if FTS measurably fails. |
| B4 | NEXUS-managed constitution precedence (§16) | Inventing an override layer would conflict with native precedence. | Constitutions are native files (CLAUDE.md, `.claude/rules/`); workspace-level text is injected via the append prompt. NEXUS provides an *inspector* for the effective set. |
| B5 | "Live PTY reattachment" after app restart (§10) | PTY children die with the ConPTY host. Claude Code ships native background sessions (`--bg`, `attach`, `agents`). | MVP: honest "interrupted → resume conversation". Phase 2+: optional *durable* mode = native background session + `claude attach` in the PTY. |
| B6 | GitHub-backed workspace wizard with repo creation (§6.1) | `gh` is not even installed on the dev machine; the core value is a local folder. | Workspace = folder (optionally a Git repo). Remote creation is a guided `gh`/manual step later. |

## C. Risky assumptions

| # | Assumption | Reality / risk | Mitigation |
| --- | --- | --- | --- |
| C1 | Per-session injection always works | `disableSideloadFlags` makes `claude` **exit with an error** when `--plugin-dir`/`--mcp-config`/`--agents` are passed. | Read effective managed policy before launch; omit blocked flags; surface the reason. |
| C2 | Changing work profile mid-conversation changes behaviour | `--system-prompt-snapshot` (default on) freezes the appended prompt for the conversation. | Rigor is per conversation; UI says so on resume. |
| C3 | Resume restores everything | `--plugin-dir`, `--mcp-config`, `--settings`, `--add-dir` are **not** restored on resume. | Resume rebuilds the full LaunchSpec from the stored session + current profile. |
| C4 | Separate config dirs isolate accounts | True for OAuth (credentials file in the dir on Windows/Linux; keychain keyed per dir on macOS) but `%APPDATA%\Anthropic` and keyless Console sign-in are shared, and **machine-wide managed settings apply to every profile**. | Documented in profile UI; no false isolation claims. |
| C5 | `claude` is always a native executable | npm installs produce `claude.cmd` shims; spawning them requires `cmd.exe` and re-introduces shell parsing of arguments. | MVP launches native executables only and diagnoses shims with a fix (`claude install`). |
| C6 | macOS GUI apps see the user's shell PATH | They don't. | Resolve login-shell PATH once at startup on macOS/Linux (Phase 6 packaging). |
| C7 | `git status` is read-only and safe | A repo's `.git/config` can set `core.fsmonitor` to an arbitrary command executed on every status. | NEXUS runs `git -c core.fsmonitor=false … ` with `GIT_OPTIONAL_LOCKS=0` (also avoids racing the user's own git). |
| C8 | Signed releases "just work" | Windows Authenticode and macOS notarization need paid certificates; unsigned builds hit SmartScreen/Gatekeeper. Tauri updater signing is separate and free. | Phase 6 decision for the owner; updater key generated locally, never committed. |

## D. Reuse of native Claude Code features (instead of rebuilding)

| Need | Native feature reused |
| --- | --- |
| Session IDs | `--session-id`, `--resume`, `-n/--name` |
| Session liveness across terminals | `claude agents --json` (pid, cwd, sessionId, status) |
| Auth status per profile | `claude auth status --json` |
| Profile isolation | `CLAUDE_CONFIG_DIR` |
| Injected tools / hooks / skills | `--mcp-config`, `--settings`, `--plugin-dir` (all session-scoped) |
| Rigor prompt | `--append-system-prompt` |
| Reasoning effort / model | `--effort`, `--model` |
| Diagnostics | `claude doctor`, `claude plugin validate`, `claude plugin details` (token cost) |
| Durable background sessions | `--bg`, `claude attach/logs/stop` |
| Independent review | `codex review --uncommitted` (the installed "codex-harness" plugin is for asset generation, not review) |
| Code graph | `graphify update` (no LLM) + `graphify-mcp` |
| Multi-agent orchestration | native subagents / workflows |

## E. Clarifications resolved without asking

* "Same account, different AI Kits" (Scenario C) needs no special code: two config dirs, each logged in
  once, each with its own kit association.
* NEXUS work profiles never change permission modes; `--dangerously-skip-permissions` is never added by NEXUS.
* "Session" in the UI means a NEXUS-managed terminal running Claude (or a shell); "conversation" means the
  resumable Claude transcript. The UI uses both words precisely (spec §9, §10).
* Licensing of NEXUS itself is the owner's decision and is left open (no LICENSE file yet).

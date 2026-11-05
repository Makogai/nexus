# Architecture Decision Records

Format: context → decision → alternatives → consequences. Status is one of Proposed, Accepted, Superseded.
Add a new ADR rather than rewriting an accepted one; mark the old one Superseded.

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-desktop-stack-and-crate-layout.md) | Desktop stack (Tauri 2, Vue 3, Rust) and minimal crate layout | Accepted |
| [0002](0002-sqlite-storage.md) | SQLite via rusqlite, WAL, user_version migrations | Accepted |
| [0003](0003-claude-profile-isolation.md) | Claude profile isolation and child-environment hygiene | Accepted |
| [0004](0004-session-scoped-injection.md) | Session-scoped injection instead of mutating Claude configs | Accepted |
| [0005](0005-source-of-truth-split.md) | Portable files vs machine-local SQLite | Accepted, refined by 0013 |
| [0006](0006-git-via-cli.md) | Git through the git CLI; NEXUS holds no Git credentials | Accepted |
| [0007](0007-terminal.md) | Terminal: xterm.js + portable-pty with raw byte channels | Accepted |
| [0008](0008-session-identity-and-lifecycle.md) | NEXUS-assigned Claude session IDs and session state machine | Accepted |
| [0009](0009-mcp-server.md) | stdio MCP server in the `nexus` binary, scope bound at launch | Accepted, implemented |
| [0010](0010-usage-metrics-provenance.md) | Usage metrics with explicit provenance | Accepted; transcript source implemented by 0030 (OpenTelemetry opt-in still planned) |
| [0011](0011-work-profiles.md) | Work profiles as data, separate from native effort | Accepted |
| [0012](0012-workflows-reuse-native.md) | Workflows reuse native skills/subagents; no custom orchestrator | Accepted, implemented by 0029 |
| [0013](0013-portable-knowledge-format.md) | Portable knowledge files: identity, schema, atomic writes, indexing, sync boundaries | Accepted, implemented (sync keys: Phase 6) |
| [0014](0014-session-hosts-and-lifecycle.md) | Session hosts and lifecycle (prepares background-service mode) | Accepted |
| [0015](0015-production-webview-hardening.md) | Production webview hardening (threat T18) | Accepted, implemented |
| [0016](0016-lifecycle-hooks.md) | Claude lifecycle hooks: deterministic activity capture and checkpoint recovery | Accepted, implemented |
| [0017](0017-context-hierarchy-and-lazy-loading.md) | Context hierarchy and lazy context loading | Accepted, implemented |
| [0018](0018-task-sessions-and-branch-aware-knowledge.md) | Task-driven sessions and branch-aware knowledge | Accepted, implemented; task delivery amended by 0020 |
| [0019](0019-effective-instructions-inspector.md) | Effective-instructions inspector: Claude's own layers first, no duplicates | Accepted, implemented |
| [0020](0020-profile-instruction-isolation-and-task-data.md) | Profile instruction isolation and task text as untrusted data | Accepted, implemented |
| [0021](0021-resume-project.md) | Resume project: verified state, saved history, one evidence-based next step | Accepted, implemented |
| [0022](0022-proposed-memories.md) | Proposed memories: deterministic extraction, human approval before knowledge | Accepted, implemented |
| [0023](0023-profile-diagnostics.md) | Claude profile diagnostics: read-only, allowlisted, verified against Claude | Accepted, implemented |
| [0024](0024-ai-kits.md) | AI Kits: reviewed plugins enabled per profile through session-scoped `--plugin-dir` | Accepted, implemented |
| [0025](0025-notifications-and-session-attention.md) | Notifications and session attention: reported signals only, one event sink, desktop delivery | Accepted, implemented |
| [0026](0026-intelligence-subsystems.md) | Five intelligence subsystems (Constitutions, Context, Memory, Skills, Workflows) on shared infrastructure | Accepted |
| [0027](0027-constitutions.md) | Constitutions: structured rule documents with stable ids, enforcement levels and scoped inheritance | Accepted, implemented |
| [0028](0028-context-manager.md) | Context Manager: per-project strategies, cited retrieval from project docs, an inspector built on observation | Accepted, implemented |
| [0029](0029-workflow-engine.md) | Workflow Engine: portable definitions on tasks and sessions, trusted gates, AI-reported vs. verified | Accepted, implemented |
| [0030](0030-usage-analytics.md) | Usage analytics: per-response counts parsed from local transcripts, provenance on every figure | Accepted, implemented; implements 0010's transcript source |
| [0031](0031-git-workspace-sync.md) | Git-based workspace sync: the person's repository, explicit outgoing review, no sync service | Accepted, implemented (M29) |

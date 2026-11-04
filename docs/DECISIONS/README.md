# Architecture Decision Records

Format: context → decision → alternatives → consequences. Status is one of Proposed, Accepted, Superseded.
Add a new ADR rather than rewriting an accepted one; mark the old one Superseded.

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-desktop-stack-and-crate-layout.md) | Desktop stack (Tauri 2, Vue 3, Rust) and minimal crate layout | Accepted |
| [0002](0002-sqlite-storage.md) | SQLite via rusqlite, WAL, user_version migrations | Accepted |
| [0003](0003-claude-profile-isolation.md) | Claude profile isolation and child-environment hygiene | Accepted |
| [0004](0004-session-scoped-injection.md) | Session-scoped injection instead of mutating Claude configs | Accepted |
| [0005](0005-source-of-truth-split.md) | Portable files vs machine-local SQLite | Accepted |
| [0006](0006-git-via-cli.md) | Git through the git CLI; NEXUS holds no Git credentials | Accepted |
| [0007](0007-terminal.md) | Terminal: xterm.js + portable-pty with raw byte channels | Accepted |
| [0008](0008-session-identity-and-lifecycle.md) | NEXUS-assigned Claude session IDs and session state machine | Accepted |
| [0009](0009-mcp-server.md) | stdio MCP server in the `nexus` binary, scope bound at launch | Accepted (Phase 2) |
| [0010](0010-usage-metrics-provenance.md) | Usage metrics with explicit provenance | Accepted (Phase 5) |
| [0011](0011-work-profiles.md) | Work profiles as data, separate from native effort | Accepted |
| [0012](0012-workflows-reuse-native.md) | Workflows reuse native skills/subagents; no custom orchestrator | Accepted (Phase 4) |

# ADR-0002 — SQLite via rusqlite, WAL, user_version migrations

Status: Accepted (2026-11-04)

## Context
NEXUS needs durable structured local state shared by the desktop app and (Phase 2) CLI, MCP server and hook
processes that run concurrently.

## Decision
* `rusqlite` with the `bundled` feature (identical SQLite + FTS5 on every platform).
* `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5000`.
* Forward-only SQL migrations embedded in the binary, applied in one transaction each, tracked by
  `PRAGMA user_version`. Opening a DB whose version is *newer* than the binary knows is refused (prevents an
  old build from corrupting a newer schema). Before upgrading an existing DB a copy is written to `backups/`.
* One connection per process behind a mutex; multi-step updates use transactions.
* Location: the platform *local* data dir (`%LOCALAPPDATA%\NEXUS` etc.) — the DB contains machine-specific
  paths and must not roam. `NEXUS_DATA_DIR` overrides it (tests, isolated development).
* No credentials are ever stored in SQLite.

## Alternatives
* `tauri-plugin-sql` — exposes SQL to the webview; violates least privilege and cannot be shared with the CLI.
* `sqlx` — async + compile-time checks are nice, but adds a runtime and offline-metadata friction for little gain.
* `rusqlite_migration` — fine, but `user_version` + ~40 lines covers our needs with zero coupling.

## Consequences
* Cross-process change detection uses `PRAGMA data_version` polling (ADR-0009).
* Indexes derived from workspace files (FTS) can be dropped and rebuilt by a migration.

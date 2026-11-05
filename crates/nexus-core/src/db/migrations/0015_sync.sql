-- NEXUS schema v15 (ADR-0031, M29): workspace synchronisation history (machine-local; never synced).
-- One row per fetch / pull / commit / push / clone. `running` rows of a host that died are reconciled to
-- `interrupted` (with the repository state checked) when a host starts.
CREATE TABLE sync_history (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT NOT NULL,              -- empty while a clone has not become a workspace yet
    op            TEXT NOT NULL CHECK (op IN ('fetch', 'pull', 'commit', 'push', 'clone')),
    status        TEXT NOT NULL CHECK (status IN ('running', 'ok', 'refused', 'failed', 'interrupted')),
    host_id       TEXT,
    branch        TEXT,
    head_before   TEXT,
    head_after    TEXT,
    commits       INTEGER NOT NULL DEFAULT 0,
    files         INTEGER NOT NULL DEFAULT 0,
    error_kind    TEXT,                       -- offline, auth, remote-missing, rejected, timeout, blocked, diverged, ...
    message       TEXT,                       -- shown to the person; credentials removed, never file contents
    started_at    INTEGER NOT NULL,
    ended_at      INTEGER
);
CREATE INDEX idx_sync_history_ws ON sync_history(workspace_id, started_at DESC);
CREATE INDEX idx_sync_history_running ON sync_history(status) WHERE status = 'running';

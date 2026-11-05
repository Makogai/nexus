-- Portable Workspace P6 (ADR-0038): where a session started in Git, and its development record.
ALTER TABLE sessions ADD COLUMN head_start TEXT;     -- project HEAD when the session started (commits made during it)
ALTER TABLE sessions ADD COLUMN record_path TEXT;    -- workspace file of the development record (NULL: none / trivial)
ALTER TABLE sessions ADD COLUMN record_at INTEGER;   -- when the record was decided (written or judged trivial)
CREATE INDEX idx_sessions_record_pending ON sessions(record_at, state) WHERE record_at IS NULL;

-- Portable Workspace P7 (ADR-0039): the sync history also audits the policy's automatic runs (`auto-sync`).
CREATE TABLE sync_history_v17 (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT NOT NULL,
    op            TEXT NOT NULL CHECK (op IN ('fetch', 'pull', 'commit', 'push', 'clone', 'auto-sync')),
    status        TEXT NOT NULL CHECK (status IN ('running', 'ok', 'refused', 'failed', 'interrupted')),
    host_id       TEXT,
    branch        TEXT,
    head_before   TEXT,
    head_after    TEXT,
    commits       INTEGER NOT NULL DEFAULT 0,
    files         INTEGER NOT NULL DEFAULT 0,
    error_kind    TEXT,
    message       TEXT,
    started_at    INTEGER NOT NULL,
    ended_at      INTEGER
);
INSERT INTO sync_history_v17 (id, workspace_id, op, status, host_id, branch, head_before, head_after, commits, files, error_kind, message, started_at, ended_at)
    SELECT id, workspace_id, op, status, host_id, branch, head_before, head_after, commits, files, error_kind, message, started_at, ended_at FROM sync_history;
DROP TABLE sync_history;
ALTER TABLE sync_history_v17 RENAME TO sync_history;
CREATE INDEX idx_sync_history_ws ON sync_history(workspace_id, started_at DESC);
CREATE INDEX idx_sync_history_running ON sync_history(status) WHERE status = 'running';

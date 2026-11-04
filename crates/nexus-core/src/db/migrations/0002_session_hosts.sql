-- NEXUS schema v2: session hosts (ADR-0014).
-- Rebuilds `sessions` (SQLite cannot alter a CHECK constraint) to allow the future 'service' host kind and to
-- record which host instance owns each session. Runs with foreign keys disabled; the runner verifies
-- `PRAGMA foreign_key_check` before committing.

CREATE TABLE session_hosts (
    id            TEXT PRIMARY KEY,
    kind          TEXT NOT NULL CHECK (kind IN ('desktop', 'cli', 'service')),
    pid           INTEGER,
    version       TEXT NOT NULL,
    started_at    INTEGER NOT NULL,
    heartbeat_at  INTEGER NOT NULL,
    stopped_at    INTEGER
);
CREATE INDEX idx_session_hosts_live ON session_hosts(kind, stopped_at);

CREATE TABLE sessions_v2 (
    id                 TEXT PRIMARY KEY,
    kind               TEXT NOT NULL CHECK (kind IN ('claude', 'shell')),
    host               TEXT NOT NULL DEFAULT 'desktop' CHECK (host IN ('desktop', 'cli', 'service')),
    host_id            TEXT REFERENCES session_hosts(id) ON DELETE SET NULL,
    workspace_id       TEXT REFERENCES workspaces(id) ON DELETE SET NULL,
    project_id         TEXT REFERENCES projects(id) ON DELETE SET NULL,
    profile_id         TEXT REFERENCES claude_profiles(id) ON DELETE SET NULL,
    claude_session_id  TEXT,
    title              TEXT NOT NULL,
    cwd                TEXT NOT NULL,
    branch             TEXT,
    work_profile       TEXT,
    effort             TEXT,
    model              TEXT,
    program            TEXT NOT NULL,
    pid                INTEGER,
    state              TEXT NOT NULL CHECK (state IN ('starting', 'running', 'exited', 'interrupted', 'failed')),
    exit_code          INTEGER,
    error              TEXT,
    resumed_from       TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    started_at         INTEGER NOT NULL,
    ended_at           INTEGER,
    archived_at        INTEGER
);

INSERT INTO sessions_v2 (id, kind, host, host_id, workspace_id, project_id, profile_id, claude_session_id, title, cwd,
    branch, work_profile, effort, model, program, pid, state, exit_code, error, resumed_from, started_at, ended_at,
    archived_at)
SELECT id, kind, host, NULL, workspace_id, project_id, profile_id, claude_session_id, title, cwd,
    branch, work_profile, effort, model, program, pid, state, exit_code, error, resumed_from, started_at, ended_at,
    archived_at
FROM sessions;

DROP TABLE sessions;
ALTER TABLE sessions_v2 RENAME TO sessions;

CREATE INDEX idx_sessions_state ON sessions(state);
CREATE INDEX idx_sessions_project ON sessions(project_id, started_at DESC);
CREATE INDEX idx_sessions_started ON sessions(started_at DESC);
CREATE INDEX idx_sessions_claude ON sessions(claude_session_id);
CREATE INDEX idx_sessions_host ON sessions(host_id);

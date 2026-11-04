-- NEXUS schema v1. Timestamps are Unix milliseconds.

CREATE TABLE workspaces (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    root_path       TEXT NOT NULL UNIQUE,
    created_at      INTEGER NOT NULL,
    last_opened_at  INTEGER,
    archived_at     INTEGER
);

CREATE TABLE claude_profiles (
    id                    TEXT PRIMARY KEY,
    name                  TEXT NOT NULL UNIQUE COLLATE NOCASE,
    description           TEXT NOT NULL DEFAULT '',
    color                 TEXT NOT NULL DEFAULT '#7c6cf2',
    config_dir            TEXT,            -- NULL = default location (~/.claude + ~/.claude.json)
    executable            TEXT,            -- NULL = auto-detect
    extra_args_json       TEXT NOT NULL DEFAULT '[]',
    managed_by_nexus      INTEGER NOT NULL DEFAULT 0 CHECK (managed_by_nexus IN (0, 1)),
    inherit_provider_env  INTEGER NOT NULL DEFAULT 0 CHECK (inherit_provider_env IN (0, 1)),
    default_work_profile  TEXT,
    default_effort        TEXT,
    default_model         TEXT,
    created_at            INTEGER NOT NULL,
    updated_at            INTEGER NOT NULL,
    last_used_at          INTEGER,
    archived_at           INTEGER
);

CREATE TABLE projects (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    local_path      TEXT,                  -- NULL = not present on this machine
    remote_url      TEXT,                  -- credentials redacted
    stack_json      TEXT NOT NULL DEFAULT '{}',
    created_at      INTEGER NOT NULL,
    last_opened_at  INTEGER
);
CREATE INDEX idx_projects_workspace ON projects(workspace_id);
CREATE UNIQUE INDEX idx_projects_ws_path ON projects(workspace_id, local_path) WHERE local_path IS NOT NULL;

CREATE TABLE sessions (
    id                 TEXT PRIMARY KEY,
    kind               TEXT NOT NULL CHECK (kind IN ('claude', 'shell')),
    host               TEXT NOT NULL DEFAULT 'desktop' CHECK (host IN ('desktop', 'cli')),
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
CREATE INDEX idx_sessions_state ON sessions(state);
CREATE INDEX idx_sessions_project ON sessions(project_id, started_at DESC);
CREATE INDEX idx_sessions_started ON sessions(started_at DESC);
CREATE INDEX idx_sessions_claude ON sessions(claude_session_id);

CREATE TABLE app_settings (
    key         TEXT PRIMARY KEY,
    value_json  TEXT NOT NULL
);

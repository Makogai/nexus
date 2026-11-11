-- AI agents beyond Claude Code (ADR-0052). Claude keeps `claude_profiles`; other agents' profiles live here. A home of
-- NULL is the agent's own default (Codex: ~/.codex); otherwise an existing folder the person registered — NEXUS never
-- writes into it. `sessions.kind = 'claude'` keeps meaning "AI agent session"; `agent` says which one.
CREATE TABLE agent_profiles (
    id                    TEXT PRIMARY KEY,
    agent                 TEXT NOT NULL,
    name                  TEXT NOT NULL COLLATE NOCASE,
    home_dir              TEXT,
    executable            TEXT,
    extra_args_json       TEXT NOT NULL DEFAULT '[]',
    inherit_provider_env  INTEGER NOT NULL DEFAULT 0 CHECK (inherit_provider_env IN (0, 1)),
    turn_notifications    INTEGER NOT NULL DEFAULT 1 CHECK (turn_notifications IN (0, 1)),
    default_work_profile  TEXT,
    default_model         TEXT,
    created_at            INTEGER NOT NULL,
    updated_at            INTEGER NOT NULL,
    last_used_at          INTEGER,
    archived_at           INTEGER,
    UNIQUE (agent, name)
);

ALTER TABLE sessions ADD COLUMN agent TEXT;
UPDATE sessions SET agent = 'claude' WHERE kind = 'claude';
ALTER TABLE sessions ADD COLUMN agent_profile_id TEXT REFERENCES agent_profiles(id) ON DELETE SET NULL;

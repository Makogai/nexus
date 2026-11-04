-- NEXUS schema v3: session checkpoints and per-session integration status.
-- Checkpoints are session runtime state (machine-local, ADR-0013); durable decisions are promoted to portable
-- knowledge files in Phase 3.

CREATE TABLE session_events (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('progress', 'decision', 'blocker', 'done', 'note')),
    summary     TEXT NOT NULL,
    source      TEXT NOT NULL CHECK (source IN ('claude', 'human', 'nexus')),
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_session_events_session ON session_events(session_id, created_at DESC);

-- JSON describing which NEXUS integrations were active for the session (e.g. MCP on, or blocked by policy).
ALTER TABLE sessions ADD COLUMN integrations TEXT;

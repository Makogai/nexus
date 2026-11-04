-- NEXUS schema v4: lifecycle hook activity (ADR-0016).
-- Rebuilds session_events without a CHECK on `kind` (kinds are validated in code, so new kinds need no rebuild),
-- adds a structured `data` column, and adds per-session activity counters. Runs with foreign keys disabled; the
-- runner verifies `PRAGMA foreign_key_check` before committing.

CREATE TABLE session_events_v4 (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,
    summary     TEXT NOT NULL DEFAULT '',
    data        TEXT,
    source      TEXT NOT NULL CHECK (source IN ('claude', 'human', 'nexus', 'hook')),
    created_at  INTEGER NOT NULL
);
INSERT INTO session_events_v4 (id, session_id, kind, summary, data, source, created_at)
SELECT id, session_id, kind, summary, NULL, source, created_at FROM session_events;
DROP TABLE session_events;
ALTER TABLE session_events_v4 RENAME TO session_events;
CREATE INDEX idx_session_events_session ON session_events(session_id, created_at DESC);
CREATE INDEX idx_session_events_kind ON session_events(session_id, kind);

ALTER TABLE sessions ADD COLUMN turns INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN prompts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN subagents INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN last_activity_at INTEGER;
ALTER TABLE sessions ADD COLUMN attention TEXT;
ALTER TABLE sessions ADD COLUMN end_reason TEXT;
ALTER TABLE sessions ADD COLUMN transcript_path TEXT;

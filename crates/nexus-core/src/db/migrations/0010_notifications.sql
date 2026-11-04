-- NEXUS schema v10 (ADR-0025): durable, user-facing notifications produced by the unified event system (hooks, MCP,
-- session lifecycle, tasks, reviews) and structured session attention. Machine-local; no prompt text or terminal output.
CREATE TABLE notifications (
    id             TEXT PRIMARY KEY,
    category       TEXT NOT NULL,             -- input-required | permission-requested | agent-finished | task-completed | …
    priority       TEXT NOT NULL CHECK (priority IN ('critical', 'action', 'success', 'warning', 'info')),
    source         TEXT NOT NULL,             -- hook | mcp | session | task | knowledge | kit | nexus
    title          TEXT NOT NULL,
    body           TEXT NOT NULL DEFAULT '',
    workspace_id   TEXT,
    project_id     TEXT,
    profile_id     TEXT,
    session_id     TEXT,
    task_id        TEXT,
    workflow_id    TEXT,
    target_kind    TEXT,                      -- session | task | proposals | rules | kit | project
    target_id      TEXT,
    dedupe_key     TEXT,
    occurrences    INTEGER NOT NULL DEFAULT 1,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    read_at        INTEGER,
    dismissed_at   INTEGER,
    snoozed_until  INTEGER,
    resolved_at    INTEGER,                   -- the situation ended (e.g. the user answered the prompt)
    desktop_state  TEXT NOT NULL DEFAULT 'pending' CHECK (desktop_state IN ('pending', 'sent', 'suppressed')),
    desktop_reason TEXT
);
CREATE INDEX idx_notifications_created ON notifications(created_at);
CREATE INDEX idx_notifications_dedupe ON notifications(dedupe_key, updated_at);
CREATE INDEX idx_notifications_desktop ON notifications(desktop_state, created_at);
CREATE INDEX idx_notifications_session ON notifications(session_id, resolved_at);

-- Structured attention reported by supported Claude hooks (free text stays in `attention`).
ALTER TABLE sessions ADD COLUMN attention_kind TEXT;   -- permission | input | idle | error
ALTER TABLE sessions ADD COLUMN attention_at INTEGER;

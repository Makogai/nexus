-- Push requests (ADR-0043): a Claude session that may not push asks NEXUS; the person reviews the outgoing commits and
-- NEXUS pushes (never forced) or declines. Machine-local: the request, its decision and the push result.
CREATE TABLE push_requests (
    id          TEXT PRIMARY KEY,
    session_id  TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    branch      TEXT NOT NULL,
    head        TEXT NOT NULL,
    reason      TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL CHECK (status IN ('pending', 'pushed', 'declined', 'failed')),
    message     TEXT,
    created_at  INTEGER NOT NULL,
    decided_at  INTEGER
);
CREATE INDEX idx_push_requests_project ON push_requests(project_id, created_at DESC);

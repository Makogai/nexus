-- NEXUS schema v8 (ADR-0022): memory proposals extracted deterministically from session activity. They are
-- machine-local suggestions, NOT knowledge: only a person's approval turns one into an authoritative knowledge file.
CREATE TABLE memory_proposals (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL,
    project_id      TEXT,
    session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('note', 'decision')),
    title           TEXT NOT NULL,
    body            TEXT NOT NULL,
    source          TEXT NOT NULL,            -- checkpoint kind it came from (decision | blocker | note)
    source_event_id TEXT,
    branch          TEXT,                     -- branch the session was launched on (machine-local record)
    fingerprint     TEXT NOT NULL,            -- normalised content hash (dedupe; a rejected proposal is not re-proposed)
    status          TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),
    document_id     TEXT,                     -- knowledge id once approved
    created_at      INTEGER NOT NULL,
    decided_at      INTEGER,
    UNIQUE (workspace_id, fingerprint)
);
CREATE INDEX idx_memory_proposals_ws ON memory_proposals(workspace_id, status, created_at);

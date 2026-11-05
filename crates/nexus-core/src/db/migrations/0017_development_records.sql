-- Portable Workspace P6 (ADR-0038): where a session started in Git, and its development record.
ALTER TABLE sessions ADD COLUMN head_start TEXT;     -- project HEAD when the session started (commits made during it)
ALTER TABLE sessions ADD COLUMN record_path TEXT;    -- workspace file of the development record (NULL: none / trivial)
ALTER TABLE sessions ADD COLUMN record_at INTEGER;   -- when the record was decided (written or judged trivial)
CREATE INDEX idx_sessions_record_pending ON sessions(record_at, state) WHERE record_at IS NULL;

-- NEXUS schema v6 (ADR-0018): sessions launched for a task, and branch-aware knowledge.

-- The knowledge task a Claude session works on (id from the task file; the file stays authoritative).
ALTER TABLE sessions ADD COLUMN task_id TEXT;
CREATE INDEX idx_sessions_task ON sessions(task_id);

-- Git identifiers recorded in a document's frontmatter (`branch`, `commit`). Git stays authoritative for what a
-- branch currently contains; these columns only cache what the file says.
ALTER TABLE knowledge_items ADD COLUMN branch TEXT;
ALTER TABLE knowledge_items ADD COLUMN commit_sha TEXT;

-- The index is a cache: make the next refresh re-read every file so the new columns are filled.
UPDATE knowledge_items SET size = -1;

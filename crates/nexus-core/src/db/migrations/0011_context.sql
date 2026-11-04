-- NEXUS schema v11 (ADR-0028): Context Manager — per-project context strategy (machine-local preference) and a
-- rebuildable index of the project's own documentation (README, docs/…) for targeted, cited retrieval.
CREATE TABLE context_configs (
    project_id  TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    strategy    TEXT NOT NULL CHECK (strategy IN ('minimal', 'balanced', 'deep', 'custom')),
    custom      TEXT,                       -- JSON settings when strategy = 'custom'
    updated_at  INTEGER NOT NULL
);

CREATE TABLE project_docs (
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    path        TEXT NOT NULL,              -- relative to the project folder, forward slashes
    title       TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    size        INTEGER NOT NULL,
    mtime_ms    INTEGER NOT NULL,
    indexed_at  INTEGER NOT NULL,
    PRIMARY KEY (project_id, path)
);
CREATE VIRTUAL TABLE project_docs_fts USING fts5(project_id UNINDEXED, path UNINDEXED, title, body, tokenize = 'unicode61');

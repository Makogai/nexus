-- NEXUS schema v5: index of portable knowledge files (ADR-0013). Everything here is a cache that can be dropped and
-- rebuilt from the files, except knowledge_approvals (machine-local trust decisions, threat T19).

CREATE TABLE knowledge_items (
    path          TEXT PRIMARY KEY,         -- absolute path of the file on this machine
    root          TEXT NOT NULL,            -- knowledge root the file was found under
    workspace_id  TEXT,                     -- NULL = global (NEXUS data dir)
    id            TEXT,                     -- frontmatter id (duplicates are reported as conflicts)
    kind          TEXT,
    title         TEXT NOT NULL DEFAULT '',
    status        TEXT,
    project_id    TEXT,
    priority      TEXT,
    stacks        TEXT NOT NULL DEFAULT '[]', -- JSON array (rules)
    applies       TEXT,                     -- rules: always | on-demand
    privacy       TEXT NOT NULL,
    updated       TEXT,
    content_hash  TEXT NOT NULL,
    size          INTEGER NOT NULL,
    mtime_ms      INTEGER NOT NULL,
    valid         INTEGER NOT NULL CHECK (valid IN (0, 1)),
    error         TEXT,
    conflict      TEXT,
    indexed_at    INTEGER NOT NULL
);
CREATE INDEX idx_knowledge_scope ON knowledge_items(workspace_id, kind);
CREATE INDEX idx_knowledge_id ON knowledge_items(id);
CREATE INDEX idx_knowledge_root ON knowledge_items(root);

CREATE VIRTUAL TABLE knowledge_fts USING fts5(path UNINDEXED, title, body, tokenize = 'unicode61');

-- Always-on rules from a (possibly shared) workspace are injected into Claude's system prompt only after they were
-- approved on this machine in their current version.
CREATE TABLE knowledge_approvals (
    path          TEXT PRIMARY KEY,
    content_hash  TEXT NOT NULL,
    approved_at   INTEGER NOT NULL
);

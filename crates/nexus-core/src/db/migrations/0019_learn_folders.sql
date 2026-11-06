-- Folders a project's learning sessions may read besides the project (ADR-0045) — e.g. the repository the project lives
-- in. Chosen by the person on this machine and passed to Claude as --add-dir; machine-local, never synced (a synced file
-- must not be able to widen what Claude may read).
CREATE TABLE learn_folders (
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    path        TEXT NOT NULL,
    added_at    INTEGER NOT NULL,
    PRIMARY KEY (project_id, path)
);

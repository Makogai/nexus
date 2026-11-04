-- NEXUS schema v9 (ADR-0024): AI Kits — Claude Code plugins or skill folders registered with NEXUS and enabled per
-- profile through session-scoped --plugin-dir. NEXUS never installs anything into a Claude config dir.
CREATE TABLE kits (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,                 -- plugin name (manifest) or folder name
    source_kind   TEXT NOT NULL CHECK (source_kind IN ('local', 'git')),
    location      TEXT NOT NULL,                 -- local folder, or Git URL with credentials removed
    git_ref       TEXT,                          -- branch or tag to follow (Git kits)
    subdir        TEXT NOT NULL DEFAULT '',      -- plugin folder inside the source (marketplace repositories)
    kind          TEXT NOT NULL CHECK (kind IN ('plugin', 'skills')),
    version       TEXT,
    content_hash  TEXT NOT NULL,                 -- hash of the kit's files at the last inspection
    approved_hash TEXT,                          -- the version a person reviewed; NULL or different = not used
    commit_sha    TEXT,                          -- Git HEAD of the NEXUS-owned checkout
    registered_at INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE kit_profiles (
    kit_id      TEXT NOT NULL REFERENCES kits(id) ON DELETE CASCADE,
    profile_id  TEXT NOT NULL REFERENCES claude_profiles(id) ON DELETE CASCADE,
    skills_json TEXT,                            -- selected skill names (JSON array); NULL = all skills
    agents      INTEGER NOT NULL DEFAULT 1 CHECK (agents IN (0, 1)),
    commands    INTEGER NOT NULL DEFAULT 1 CHECK (commands IN (0, 1)),
    hooks       INTEGER NOT NULL DEFAULT 0 CHECK (hooks IN (0, 1)),   -- runs commands: explicit opt-in
    mcp         INTEGER NOT NULL DEFAULT 0 CHECK (mcp IN (0, 1)),     -- starts servers: explicit opt-in
    enabled_at  INTEGER NOT NULL,
    PRIMARY KEY (kit_id, profile_id)
);

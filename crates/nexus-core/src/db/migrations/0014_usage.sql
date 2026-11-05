-- NEXUS schema v14 (ADR-0030): local token usage analytics. Numbers only — no prompt, response or tool text is stored.
-- Source: the session transcripts Claude Code writes into the profile's config dir (read-only, incremental).

-- Incremental read position per transcript file of a Claude conversation (main file and subagent files).
CREATE TABLE usage_sources (
    profile_id         TEXT NOT NULL,
    claude_session_id  TEXT NOT NULL,
    file               TEXT NOT NULL,            -- relative to the profile's config dir, forward slashes
    read_offset        INTEGER NOT NULL DEFAULT 0,
    lines              INTEGER NOT NULL DEFAULT 0,
    skipped            INTEGER NOT NULL DEFAULT 0, -- malformed or oversized lines
    local_errors       INTEGER NOT NULL DEFAULT 0, -- Claude-generated error messages without an API request
    claude_version     TEXT,
    last_cost          REAL,                      -- last cost-state snapshot (to turn snapshots into deltas)
    last_start         INTEGER,
    cost_seq           INTEGER NOT NULL DEFAULT 0,
    scanned_at         INTEGER NOT NULL,
    PRIMARY KEY (profile_id, claude_session_id, file)
);

-- One row per API response (deduplicated by message id), as reported by the Claude API and recorded by Claude Code.
CREATE TABLE usage_messages (
    profile_id         TEXT NOT NULL,
    claude_session_id  TEXT NOT NULL,
    message_id         TEXT NOT NULL,
    session_id         TEXT,                      -- the NEXUS session it belongs to (by time); NULL = outside NEXUS
    ts                 INTEGER NOT NULL,
    model              TEXT NOT NULL,
    input_tokens       INTEGER NOT NULL DEFAULT 0,
    output_tokens      INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens  INTEGER NOT NULL DEFAULT 0,
    web_searches       INTEGER NOT NULL DEFAULT 0,
    subagent           INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (profile_id, claude_session_id, message_id)
);
CREATE INDEX idx_usage_messages_session ON usage_messages(session_id, ts);
CREATE INDEX idx_usage_messages_ts ON usage_messages(ts);

-- Claude Code's own cost estimate (its cost-state snapshots, turned into deltas). An estimate, never a bill.
CREATE TABLE usage_costs (
    profile_id         TEXT NOT NULL,
    claude_session_id  TEXT NOT NULL,
    file               TEXT NOT NULL,
    seq                INTEGER NOT NULL,
    session_id         TEXT,
    ts                 INTEGER NOT NULL,
    cost_usd           REAL NOT NULL,
    unknown_model      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (profile_id, claude_session_id, file, seq)
);
CREATE INDEX idx_usage_costs_session ON usage_costs(session_id);

-- Per NEXUS session: is usage measured, absent, or unavailable (and why)?
CREATE TABLE usage_state (
    session_id         TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    status             TEXT NOT NULL CHECK (status IN ('measured', 'no-requests', 'pending', 'missing', 'unreadable')),
    detail             TEXT,
    files              INTEGER NOT NULL DEFAULT 0,
    behind             INTEGER NOT NULL DEFAULT 0, -- unread bytes remain (large transcript caught up over scans)
    scanned_at         INTEGER NOT NULL
);

-- Every Claude conversation a NEXUS session worked in (after /clear Claude continues in a new one), with the time
-- the session joined it — usage of a conversation is attributed to its sessions by time.
CREATE TABLE session_conversations (
    session_id         TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    claude_session_id  TEXT NOT NULL,
    since              INTEGER NOT NULL,
    PRIMARY KEY (session_id, claude_session_id)
);
CREATE INDEX idx_session_conversations_claude ON session_conversations(claude_session_id);
INSERT OR IGNORE INTO session_conversations (session_id, claude_session_id, since)
SELECT id, claude_session_id, started_at FROM sessions WHERE claude_session_id IS NOT NULL;

-- How a profile is billed, from its last `claude auth status` (classification only — no identity is stored). Cost
-- estimates are shown only for API-billed profiles: subscription usage is never converted into spend (ADR-0010).
CREATE TABLE profile_billing (
    profile_id         TEXT PRIMARY KEY REFERENCES claude_profiles(id) ON DELETE CASCADE,
    kind               TEXT NOT NULL CHECK (kind IN ('subscription', 'api', 'unknown')),
    checked_at         INTEGER NOT NULL
);

-- What NEXUS itself added to a session (for estimates): SessionStart context length, recorded by the hook.
ALTER TABLE sessions ADD COLUMN session_start_chars INTEGER;

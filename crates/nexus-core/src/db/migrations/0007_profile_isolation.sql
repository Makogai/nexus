-- NEXUS schema v7 (ADR-0020): opt-in isolation of a profile's sessions from the default profile's personal
-- instruction files (~/.claude/CLAUDE.md, ~/.claude/rules), applied through session-scoped claudeMdExcludes.
ALTER TABLE claude_profiles ADD COLUMN isolate_instructions INTEGER NOT NULL DEFAULT 0 CHECK (isolate_instructions IN (0, 1));

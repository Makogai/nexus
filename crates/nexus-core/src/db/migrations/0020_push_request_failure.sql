-- Push request outcomes (ADR-0047): a failed push keeps why it failed — the kind (rejected | auth | offline |
-- remote-missing | timeout | other) next to Git's redacted message — and stays listed on the project page until the
-- person dismisses it or a newer request for the branch supersedes it.
ALTER TABLE push_requests ADD COLUMN failure_kind TEXT;
ALTER TABLE push_requests ADD COLUMN dismissed_at INTEGER;

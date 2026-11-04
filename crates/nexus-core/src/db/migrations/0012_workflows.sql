-- NEXUS schema v12 (ADR-0029): workflow runs and quality gates. Machine-local state; the portable definition is the
-- workflow document, the portable progress record is the run's task file.

-- Trusted, deterministic checks per project, defined by the user on this machine (argument vector, no shell).
CREATE TABLE quality_gates (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,             -- test | build | lint | typecheck | … (what workflows refer to)
    title        TEXT NOT NULL,
    program      TEXT NOT NULL,
    args         TEXT NOT NULL DEFAULT '[]', -- JSON array
    timeout_secs INTEGER NOT NULL DEFAULT 900,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    UNIQUE (project_id, name)
);

CREATE TABLE workflow_runs (
    id             TEXT PRIMARY KEY,
    workspace_id   TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    project_id     TEXT REFERENCES projects(id) ON DELETE SET NULL,
    workflow_id    TEXT NOT NULL,            -- id of the workflow document
    workflow_title TEXT NOT NULL,
    workflow_hash  TEXT NOT NULL,            -- the approved version the run was started from
    definition     TEXT NOT NULL,            -- JSON snapshot of the steps (a run is not changed by later edits)
    task_id        TEXT NOT NULL,            -- the knowledge task carrying the run's portable progress
    goal           TEXT NOT NULL DEFAULT '',
    rigor          TEXT NOT NULL,
    profile_id     TEXT,
    skills         TEXT NOT NULL DEFAULT '[]',
    status         TEXT NOT NULL CHECK (status IN ('running', 'waiting', 'completed', 'failed', 'cancelled')),
    error          TEXT,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    ended_at       INTEGER
);
CREATE INDEX idx_workflow_runs_task ON workflow_runs(task_id, status);
CREATE INDEX idx_workflow_runs_ws ON workflow_runs(workspace_id, created_at DESC);

CREATE TABLE workflow_steps (
    run_id        TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
    step_id       TEXT NOT NULL,
    position      INTEGER NOT NULL,
    kind          TEXT NOT NULL,
    status        TEXT NOT NULL CHECK (status IN ('pending', 'ready', 'blocked', 'verifying', 'awaiting-review', 'passed', 'failed', 'skipped', 'cancelled')),
    -- How completion is known: ai-reported (Claude said so), verified (gates passed), human (review), skipped.
    verification  TEXT,
    attempts      INTEGER NOT NULL DEFAULT 0,
    max_attempts  INTEGER NOT NULL,
    report        TEXT,                      -- Claude's last short report (bounded, escaped on output)
    criteria_met  TEXT NOT NULL DEFAULT '[]',
    last_failure  TEXT,
    updated_at    INTEGER NOT NULL,
    PRIMARY KEY (run_id, step_id)
);

CREATE TABLE gate_results (
    id           TEXT PRIMARY KEY,
    project_id   TEXT REFERENCES projects(id) ON DELETE CASCADE,
    run_id       TEXT REFERENCES workflow_runs(id) ON DELETE CASCADE,
    step_id      TEXT,
    gate_name    TEXT NOT NULL,
    command      TEXT NOT NULL,              -- program + arguments as shown to the user
    status       TEXT NOT NULL CHECK (status IN ('running', 'passed', 'failed', 'timeout', 'error', 'interrupted', 'cancelled')),
    exit_code    INTEGER,
    duration_ms  INTEGER,
    output_tail  TEXT NOT NULL DEFAULT '',   -- last part of stdout+stderr, credential-like tokens redacted
    host_id      TEXT,
    started_at   INTEGER NOT NULL,
    ended_at     INTEGER
);
CREATE INDEX idx_gate_results_step ON gate_results(run_id, step_id, started_at DESC);
CREATE INDEX idx_gate_results_gate ON gate_results(project_id, gate_name, started_at DESC);

-- NEXUS schema v13 (ADR-0029 amendment): explicit approval and version tracking of quality-gate commands. A gate runs
-- only when its current command (program + arguments) was approved by the person; any change to the command makes a
-- new version that needs approval again. Independent of approving a workflow definition.
ALTER TABLE quality_gates ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE quality_gates ADD COLUMN command_hash TEXT NOT NULL DEFAULT '';
ALTER TABLE quality_gates ADD COLUMN approved_hash TEXT;
ALTER TABLE quality_gates ADD COLUMN approved_at INTEGER;
-- Which version of the gate produced a result.
ALTER TABLE gate_results ADD COLUMN gate_version INTEGER;
ALTER TABLE gate_results ADD COLUMN command_hash TEXT;
-- 'gates': the step's work was reported but its gates are not defined or not approved yet; approving them resumes
-- verification (no new report from Claude needed).
ALTER TABLE workflow_steps ADD COLUMN blocked_on TEXT;

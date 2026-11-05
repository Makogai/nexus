-- ADR-0037: a workspace's own skills folder is an AI Kit for that workspace's sessions only.
ALTER TABLE kits ADD COLUMN workspace_id TEXT;
CREATE UNIQUE INDEX kits_one_per_workspace ON kits(workspace_id) WHERE workspace_id IS NOT NULL;

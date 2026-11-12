-- S06c — MCP tasks: the two indexes the project-execution task needs. No tables.
--
-- (1) `uq_run_live` cannot dedupe a project execution: that run has `card_id IS NULL`, and a
--     UNIQUE index treats every NULL as distinct (S06c trap 3). The card-less live execution is
--     therefore its own partial unique index — the database, not a check-then-insert, is what
--     stops two concurrent `projects.execute` calls creating two live runs.
--
-- (2) A task's `inputRequests` lists the project's PENDING approvals by `project_id` (trap 5:
--     by project, not by run, or card-level gates are invisible). No index covered that read;
--     the existing ones are (tenant, run) and (tenant, card). Review gap recorded 2026-11-09.
CREATE UNIQUE INDEX uq_run_project_live
    ON run (tenant_id, project_id)
    WHERE card_id IS NULL AND status IN ('queued', 'running', 'waiting_approval');

CREATE INDEX approval_project_idx ON approval (tenant_id, project_id);

-- 0037: the run tree accepts a WORKFLOW parent's step.
--
-- WHY (W5, measured 2026-11-10). `process_runs.start` accepts `parent_run_id`/`step_id`, and
-- `run.step_id` is FK-bound to `process_step`. That is right for a process `call` step, but a
-- WORKFLOW's procedure node starts its child from a `run_step` row — a row in a different table
-- — so `insert_child_run_tx` answered `StepNotFound` and the child was never started. A workflow
-- could therefore not compose a process at all, and starting it as a root would 409 on
-- `uq_run_live` against the parent that is running right now.
--
-- WHAT THIS ADDS (additive; no column is dropped, no row is rewritten):
--   1. `run.workflow_step_id uuid` FK -> `run_step (id)`, the workflow parent's step.
--   2. `run_parent_step_check` replaced: a child names EXACTLY ONE step kind.
--   3. The child-uniqueness pair for the workflow kind. `uq_run_child_live` and
--      `uq_run_child_attempt` key on `step_id`; a NULL there is distinct in a unique index, so
--      without this pair two workflow children could start for one step/attempt.
--
-- DOWNGRADE (manual): drop the two indexes and the constraint, restore
-- `CHECK (parent_run_id IS NULL OR step_id IS NOT NULL)`, drop `run_workflow_step_fk`, then drop
-- the column. Rows of the workflow kind must be removed first; they cannot exist pre-0037.

BEGIN;

ALTER TABLE run ADD COLUMN workflow_step_id uuid;
ALTER TABLE run ADD CONSTRAINT run_workflow_step_fk
    FOREIGN KEY (workflow_step_id) REFERENCES run_step (id);

ALTER TABLE run DROP CONSTRAINT run_parent_step_check;
ALTER TABLE run ADD CONSTRAINT run_parent_step_check CHECK (
    parent_run_id IS NULL
    OR ((step_id IS NULL) <> (workflow_step_id IS NULL))
);

CREATE UNIQUE INDEX uq_run_child_live_workflow
    ON run (parent_run_id, workflow_step_id)
    WHERE workflow_step_id IS NOT NULL
      AND status IN ('queued', 'running', 'waiting_approval');
CREATE UNIQUE INDEX uq_run_child_attempt_workflow
    ON run (tenant_id, parent_run_id, workflow_step_id, attempt)
    WHERE workflow_step_id IS NOT NULL;

COMMIT;

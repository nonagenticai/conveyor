-- 0036_s19_output_grade.sql — S19's per-output grading path (owner ruling 2026-11-09).
--
-- S19's eval judges a stored CASE SET: the runner invokes a subject per case and grades the
-- captured output. A process step has no case — the output is produced by the step — so an
-- `inspection.kind='eval'` SOP needs a one-shot grade of one output against the eval's metrics.
-- This is that path, and only that path: the case runner, the report and the climb are
-- unchanged.
--
-- One row per grade attempt, written by the `eval.grade` job in its own transaction. The row is
-- the record; the SOP judge reads `status`/`accepted`/`score` from it. The output itself is not
-- duplicated here — it is whatever the caller passed, and the step's own `run_step.output` holds
-- it too; `output_hash` is the identity, exactly as `inference_call.prompt_sha256` is for a
-- prompt.

CREATE TABLE eval_grade (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    eval_id      uuid NOT NULL,
    version_id   uuid NOT NULL,
    -- The graded output's identity. Never the answer key.
    output_hash  text NOT NULL,
    status       text NOT NULL DEFAULT 'pending'
                 CHECK (status IN ('pending', 'accepted', 'rejected', 'failed')),
    -- Per-metric scores, the same map shape `eval_case_result.grade` carries.
    grade        jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- Per-metric explanation/detail (the grader's evidence).
    detail       jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- The mean over the quality metrics; NULL until graded.
    score        numeric,
    -- The threshold the caller judged at; kept so a disputed verdict is replayable.
    threshold    numeric NOT NULL CHECK (threshold >= 0 AND threshold <= 1),
    reason       text,
    error        text,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_grade_eval_fk FOREIGN KEY (eval_id) REFERENCES eval (id),
    CONSTRAINT eval_grade_version_fk FOREIGN KEY (version_id) REFERENCES eval_version (id)
);
CREATE INDEX eval_grade_tenant_created_idx ON eval_grade (tenant_id, created_at DESC, id);
CREATE INDEX eval_grade_pending_idx ON eval_grade (tenant_id, id) WHERE status = 'pending';

ALTER TABLE eval_grade ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval_grade FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON eval_grade FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

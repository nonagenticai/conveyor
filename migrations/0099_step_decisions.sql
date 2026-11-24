-- 0099_step_decisions.sql — §10.1: a step judge's verdict is a decision row, not a second store
-- (#526, v1-p101-1).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- `run_step.judge` used to be the only record of a step's accept/reject: a per-step decision
-- store the decision model does not declare, invisible to `ladder.list` and without the policy
-- versions, input hash or latency the ladder's own rows carry. Every readable verdict (checker,
-- laya, eval) now writes one `decision` row in the SAME transaction as the verdict, and these
-- two nullable columns are the link back to the run and the exact step attempt that was judged.
--
-- Nullable on purpose: every decision written before this migration — and every ladder decision
-- after it — is not about a run step. The partial index is the one read the link serves: the
-- decisions of one step, newest first.
--
-- No cascade: a decision is an audit record, and deleting a run must not silently delete it.

ALTER TABLE decision
    ADD COLUMN run_id      uuid,
    ADD COLUMN run_step_id uuid;

ALTER TABLE decision
    ADD CONSTRAINT decision_run_fk
        FOREIGN KEY (run_id) REFERENCES run (id),
    ADD CONSTRAINT decision_run_step_fk
        FOREIGN KEY (run_step_id) REFERENCES run_step (id);

CREATE INDEX decision_run_step_idx ON decision (tenant_id, run_step_id, created_at DESC)
    WHERE run_step_id IS NOT NULL;

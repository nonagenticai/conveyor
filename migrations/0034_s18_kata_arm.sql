-- S18 — the `conveyor` arm and the improvement loop (steps 1, 3 and 4 of the slice).
--
-- Three facts had no home before this migration:
--
-- * **What the run measured.** `benchmark_run` pins the subject's *id*, never a version; S18 §3
--   makes version pinning the measurement, so the version the run was started against is recorded
--   here rather than re-derived from a row that an edit would have moved.
-- * **Where a `conveyor` case's patch came from.** `produce_status` already says *whether* one was
--   produced (`pending | ok | fail | no_attempt`); `produced_at` says *when*, so a case stuck at
--   `pending` past its run's life is visible rather than indistinguishable from a fresh one.
-- * **The improvement verdict.** S18 ships no table and no event, so `keep | revert | inconclusive`
--   and the baseline it was measured against live on the candidate run itself.

ALTER TABLE benchmark_run ADD COLUMN subject_version bigint;

ALTER TABLE benchmark_run
    ADD COLUMN improvement_verdict text
    CHECK (improvement_verdict IS NULL
           OR improvement_verdict IN ('keep','revert','inconclusive'));

ALTER TABLE benchmark_run ADD COLUMN improvement_of_run_id uuid;
ALTER TABLE benchmark_run ADD COLUMN improvement_note text;

-- `keep|revert|inconclusive` is a judgement about a compared pair, so it always names one.
ALTER TABLE benchmark_run
    ADD CONSTRAINT benchmark_run_improvement_pair
    CHECK (improvement_verdict IS NULL OR improvement_of_run_id IS NOT NULL);

ALTER TABLE benchmark_case ADD COLUMN produced_at timestamptz;

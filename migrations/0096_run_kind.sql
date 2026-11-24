-- 0096_run_kind.sql — a run says what sort of work it is (#526, SPEC_V2 §4.4).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- Why the column exists: `run.job_binding_id` alone cannot say whether a run owed a binding.
-- A job run executes a card's work package and MUST record the binding and the resolved release
-- it actually used (§4.4); a project run (`projects.execute`) and a process/workflow run have
-- their own paths and may stay unbound. The vocabulary is closed because the executor keys on
-- it: `job` without a binding is refused by `run_job_binding_required`, and the store refuses
-- the insert first (`422 binding_required`).
--
-- Backfill: a bound ROOT run is the historical job run; every other existing row is a project
-- run. Children never carried a binding of their own — `insert_child_run_tx` copies the
-- parent's pin — and they copy the parent's kind from now on.
--
-- No seed rows.

ALTER TABLE run ADD COLUMN kind text;

-- `run` is FORCE RLS (the tenant_isolation policy applies to its owner too), so a bare UPDATE
-- with `app.current_tenant` unset matches ZERO rows and the NOT NULL below fails on a multi-tenant
-- dump. The backfill must see every tenant's rows; 0098 uses the same pattern.
ALTER TABLE run NO FORCE ROW LEVEL SECURITY;

UPDATE run
   SET kind = CASE
                  WHEN parent_run_id IS NULL AND job_binding_id IS NOT NULL THEN 'job'
                  ELSE 'project'
              END;

ALTER TABLE run FORCE ROW LEVEL SECURITY;

ALTER TABLE run
    -- A raw insert (a fixture, a repair) that says nothing is a project run: the safe default,
    -- because a job run MUST name its binding and the CHECK below would refuse a false job.
    ALTER COLUMN kind SET DEFAULT 'project',
    ALTER COLUMN kind SET NOT NULL,
    ADD CONSTRAINT run_kind_vocabulary
        CHECK (kind IN ('job', 'project', 'process', 'workflow')),
    ADD CONSTRAINT run_job_binding_required
        CHECK (kind <> 'job' OR job_binding_id IS NOT NULL);

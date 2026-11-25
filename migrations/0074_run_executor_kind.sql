-- 0074_run_executor_kind.sql — T18-3c: the run pins the executor kind it was created with.
--
-- SPEC_V2 §10.5.2: the run inspector shows the run's executor kind and its job binding /
-- release pin. The binding is the source at creation; copying the kind onto the run is the
-- pin, so a later binding edit cannot rewrite history. Nullable: a run with no organisation
-- binding has no kind, and every prior run backfills from the binding it named.

ALTER TABLE run ADD COLUMN executor_kind text;

UPDATE run
   SET executor_kind = binding.executor_kind
  FROM job_binding AS binding
 WHERE binding.id = run.job_binding_id
   AND binding.tenant_id = run.tenant_id;

ALTER TABLE run
    ADD CONSTRAINT run_executor_kind_format
    CHECK (executor_kind IS NULL OR executor_kind ~ '^[a-z][a-z0-9_]{1,31}$');

-- 0021_s11_run_capability.sql — the two `run` columns D22/§17 requires (S11's revisit).
--
-- They are FORWARD REFERENCES, exactly as 0015 did for `run.procedure_id`/`run.inputs`: the
-- capability tables (`capability`, `capability_agent`) are S15's `0020_capability.sql`, which adds
-- the FK constraints. S11 records the resolved context from the first release so `procedure_runs.start`
-- has one place to put it; until S15 fills `resolve_start_context`, both columns are NULL and the
-- readiness arm is unreachable (the enum is explicit about that, not a silently-passing check).
--
-- No FK here on purpose: a foreign key needs its target table, and adding the target now would pull
-- S15's schema into S11. Adding a nullable column with no constraint changes no existing query.

ALTER TABLE run
    ADD COLUMN capability_id uuid,
    ADD COLUMN capability_agent_id uuid;

CREATE INDEX run_capability_idx
    ON run (tenant_id, capability_id) WHERE capability_id IS NOT NULL;

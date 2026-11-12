-- 0027_run_capability_fks.sql — the foreign keys S11's 0021 could not carry.
--
-- 0021 added `run.capability_id` / `run.capability_agent_id` as forward references with no target
-- table. S15's 0020 creates `capability` / `capability_agent`, but 0020 runs BEFORE 0021 on a fresh
-- database, so its FKs cannot be declared there (measured: 42703 `column "capability_id" referenced
-- in foreign key constraint does not exist`, migration 20). This migration is the first point at
-- which both the columns and their targets exist on every database, fresh or already migrated.
--
-- The columns stay nullable: an external-executor run has no capability/agent (S15 §5), and the
-- readiness seam is what decides whether a run may start.

ALTER TABLE run ADD CONSTRAINT run_capability_fk
    FOREIGN KEY (capability_id) REFERENCES capability (id);
ALTER TABLE run ADD CONSTRAINT run_capability_agent_fk
    FOREIGN KEY (capability_agent_id) REFERENCES capability_agent (id);

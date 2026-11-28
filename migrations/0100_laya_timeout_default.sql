-- 0100_laya_timeout_default.sql — the conveyor→laya seam's 1500 ms default and 3 s hard budget
-- (SPEC_V2 §10.2, finding v1-p102-1).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- 0040 shipped `timeout_ms int NOT NULL DEFAULT 30000 CHECK (timeout_ms BETWEEN 1 AND 600000)`,
-- and no code ever read the value, so a tenant's "budget" could sit at 30 s while §10.2 fixes
-- the seam at a 1500 ms default with a 3 s HARD budget. New rows now default to 1500 ms, and no
-- row may exceed 3000 ms; a legacy row above the cap is normalised to the hard budget (the
-- closest legal value to what it asked for) rather than left unreachable by the new CHECK.
--
-- ⚠️ `laya_inference_config` is FORCE RLS, so the migration owner (`conveyor_migrate`) is
-- subject to `tenant_isolation` too; with `app.current_tenant` unset a bare UPDATE matches ZERO
-- rows silently. The force is lifted for the statement and restored immediately (the pattern of
-- 0028, 0042 and 0098).

ALTER TABLE laya_inference_config NO FORCE ROW LEVEL SECURITY;

UPDATE laya_inference_config SET timeout_ms = 3000 WHERE timeout_ms > 3000;

ALTER TABLE laya_inference_config FORCE ROW LEVEL SECURITY;

ALTER TABLE laya_inference_config ALTER COLUMN timeout_ms SET DEFAULT 1500;

ALTER TABLE laya_inference_config
    DROP CONSTRAINT laya_inference_config_timeout_ms_check;

ALTER TABLE laya_inference_config
    ADD CONSTRAINT laya_inference_config_timeout_ms_check CHECK (timeout_ms BETWEEN 1 AND 3000);

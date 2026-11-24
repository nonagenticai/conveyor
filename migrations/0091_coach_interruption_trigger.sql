-- SPEC_V2 §6.2 (T4, v1-p6-5): `coach.interrupt` had one caller — a failed run. A WIP breach, a
-- stale lease, a gate timeout and a board event are the other four inputs the charter names,
-- and none of them carries a run. The interruption therefore records which input opened it, and
-- `failed_run_id` is present exactly when that input is a failed run.
--
-- The XOR is the invariant, not a convention: an interruption with neither a run nor a trigger
-- cannot exist, and a flow trigger cannot borrow a run's identity to look like a failure.
ALTER TABLE coach_interruption ALTER COLUMN failed_run_id DROP NOT NULL;

ALTER TABLE coach_interruption
    ADD COLUMN trigger_kind text NOT NULL DEFAULT 'failed_run'
        CONSTRAINT coach_interruption_trigger_kind
        CHECK (trigger_kind IN ('failed_run', 'wip_breach', 'stale_lease', 'gate_timeout',
                                'board_event'));

ALTER TABLE coach_interruption
    ADD CONSTRAINT coach_interruption_cause
    CHECK ((trigger_kind = 'failed_run') = (failed_run_id IS NOT NULL));

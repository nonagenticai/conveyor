-- 0077_benefit_measurement_typed.sql — a baseline/post reading is a number and a unit
-- (SPEC_V2 §8.3; rework finding v1-p83-2, P1).
--
-- `benefit_measurement` (0074) replaced the transition's timestamps with readings, but a
-- `baseline`/`post` row could be filed with no `actual_value` and the default empty `unit` —
-- presence-only again, one table down. A realized claim rests on measured values, not on a row
-- existing. The domain refuses a missing value, a blank unit and a placeholder unit first
-- (`validate_benefit_reading`); this CHECK is the second lock. Interim readings may stay
-- qualitative.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

ALTER TABLE benefit_measurement
    ADD CONSTRAINT benefit_measurement_baseline_post_typed CHECK (
        kind NOT IN ('baseline', 'post') OR (
            actual_value IS NOT NULL
            AND btrim(unit) <> ''
        )
    );

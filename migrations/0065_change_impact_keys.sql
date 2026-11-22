-- 0065_change_impact_keys.sql — a change's impact block carries a real §5.5 analysis (SPEC_V2
-- §5.5; finding v1-p5-3, conveyor#526).
--
-- 0044's `project_register_item_change_impact` only tested that `meta` HAS an `impact` key and
-- that it is not JSON null, so `{"impact": {}}` (and a bare number or string) passed the DB lock
-- while telling an approver nothing about scope, schedule, cost or risk. The replacement requires
-- an object carrying at least one of those four top-level keys WITH A NON-NULL VALUE — the same
-- rule `validate_change` applies, so `{"impact": {"scope": null}}` cannot slip past the DB lock
-- when the domain refuses it. `meta ? 'impact'` and `jsonb_typeof(...) = 'object'` are part of
-- the condition because `jsonb_typeof(NULL)` is NULL: without them a row missing the key
-- entirely, or carrying a scalar, would evaluate the CHECK to NULL and pass.
--
-- The domain/store rule (`crates/domain/src/spine.rs::validate_change`, enforced by
-- `insert_register_tx` and `update_register_tx`) is the first lock; this CHECK is the second.
--
-- NOT VALID: existing rows are not re-checked at deploy time, while every new INSERT and every
-- UPDATE still has to satisfy it. Constraint only: the table's tenant policy and table-level
-- grants already cover it. Forward only: never edited after this slice closes (CONTRACT §2).

ALTER TABLE project_register_item
    DROP CONSTRAINT project_register_item_change_impact;

ALTER TABLE project_register_item
    ADD CONSTRAINT project_register_item_change_impact CHECK (
        kind <> 'change'
        OR (baseline_ref IS NOT NULL
            AND meta ? 'impact'
            AND jsonb_typeof(meta->'impact') = 'object'
            AND (
                ((meta->'impact') ? 'scope'
                    AND (meta->'impact')->'scope' <> 'null'::jsonb)
                OR ((meta->'impact') ? 'schedule'
                    AND (meta->'impact')->'schedule' <> 'null'::jsonb)
                OR ((meta->'impact') ? 'cost'
                    AND (meta->'impact')->'cost' <> 'null'::jsonb)
                OR ((meta->'impact') ? 'risk'
                    AND (meta->'impact')->'risk' <> 'null'::jsonb)
            ))
    ) NOT VALID;

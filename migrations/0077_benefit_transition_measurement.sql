-- ── benefit_transition — typed measurement values (§8.3) ─────────────────────────────────
-- A realized claim is made of measurements, not two timestamps. Each value is the typed pair
-- `{ "value": <number>, "unit": <text> }`: the numeric reading and the unit the profile's
-- measurement method names. Prose, `{}`, a numeric string, a blank unit and a placeholder unit
-- never satisfy the shape; the domain refuses them first and these CHECKs are the second lock.
ALTER TABLE benefit_transition
    ADD COLUMN baseline_value jsonb,
    ADD COLUMN post_value jsonb;

ALTER TABLE benefit_transition
    ADD CONSTRAINT benefit_transition_baseline_value_typed CHECK (
        baseline_value IS NULL OR (
            jsonb_typeof(baseline_value) = 'object'
            AND jsonb_typeof(baseline_value -> 'value') = 'number'
            AND jsonb_typeof(baseline_value -> 'unit') = 'string'
            AND btrim(baseline_value ->> 'unit') <> ''
        )
    ),
    ADD CONSTRAINT benefit_transition_post_value_typed CHECK (
        post_value IS NULL OR (
            jsonb_typeof(post_value) = 'object'
            AND jsonb_typeof(post_value -> 'value') = 'number'
            AND jsonb_typeof(post_value -> 'unit') = 'string'
            AND btrim(post_value ->> 'unit') <> ''
        )
    ),
    -- A post-transition measurement without a measured baseline cannot claim an improvement.
    ADD CONSTRAINT benefit_transition_post_value_needs_baseline CHECK (
        post_value IS NULL OR baseline_value IS NOT NULL
    );

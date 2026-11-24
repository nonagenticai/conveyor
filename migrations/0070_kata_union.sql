-- 0070_kata_union.sql — the lane's candidate and propagation records on main's kata loop.
--
-- The union decision (SPEC_V2 §6.3, §9.3–§9.4, §12 T5; #526, 2026-11-24):
-- * main's 0066/0067 stay byte-identical — the six Gets, the held-out measurement (the literal
--   `k.split = 'test'` and its CHECK), the one-open-per-signature index and FORCE RLS are the
--   runtime;
-- * the lane's candidate and propagation records are added here, on the same `kata` aggregate,
--   because the coach, ops and architecture rows carry `kata_candidate_id`s;
-- * the lane's pre-registration fields are ADDED as nullable columns. Main's `prediction` stays
--   text; the lane's JSON payload is `prediction_payload`. No type is rewritten.
--
-- Production has not applied this (its `_sqlx_migrations` head is 65); every DB that applied the
-- lane's old 0084 is rebuilt from the dump.

ALTER TABLE kata
    ADD COLUMN project_id          uuid,
    ADD COLUMN instrument_kind     text,
    ADD COLUMN instrument_id       uuid,
    ADD COLUMN instrument_version  text,
    ADD COLUMN split               text,
    ADD COLUMN split_sha256        text,
    ADD COLUMN prediction_payload  jsonb,
    ADD COLUMN base_ref            jsonb,
    ADD COLUMN observations        jsonb NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN problem             jsonb,
    ADD COLUMN outcome             text,
    ADD COLUMN outcome_reason      text,
    ADD COLUMN restored_ref        jsonb;

ALTER TABLE kata
    ADD CONSTRAINT kata_union_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    ADD CONSTRAINT kata_union_instrument_kind
        CHECK (instrument_kind IS NULL OR instrument_kind IN ('eval', 'benchmark')),
    ADD CONSTRAINT kata_union_outcome
        CHECK (outcome IS NULL OR outcome IN ('kept', 'reverted', 'inconclusive')),
    ADD CONSTRAINT kata_union_prediction_payload_object
        CHECK (prediction_payload IS NULL OR jsonb_typeof(prediction_payload) = 'object'),
    ADD CONSTRAINT kata_union_base_ref_object
        CHECK (base_ref IS NULL OR jsonb_typeof(base_ref) = 'object');

CREATE INDEX kata_union_project_idx ON kata (tenant_id, project_id, status);

-- ── kata_candidate — the candidate record the dependents' `kata_candidate_id` names ─────
CREATE TABLE kata_candidate (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    kata_id     uuid NOT NULL,
    ordinal     int NOT NULL,
    title       text NOT NULL,
    rationale   text NOT NULL DEFAULT '',
    -- The target the candidate would change: tenant, kind, id and `base_version` (D-T5-6).
    target_ref  jsonb NOT NULL,
    prediction  jsonb,
    status      text NOT NULL DEFAULT 'proposed'
                CHECK (status IN ('proposed', 'measured', 'adopted', 'reverted',
                                  'inconclusive')),
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT kata_candidate_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT kata_candidate_kata_fk FOREIGN KEY (kata_id) REFERENCES kata (id),
    CONSTRAINT kata_candidate_ordinal_positive CHECK (ordinal > 0),
    CONSTRAINT kata_candidate_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT kata_candidate_target_is_object CHECK (jsonb_typeof(target_ref) = 'object'),
    CONSTRAINT kata_candidate_prediction_is_object
        CHECK (prediction IS NULL OR jsonb_typeof(prediction) = 'object'),
    CONSTRAINT kata_candidate_kata_ordinal_key UNIQUE (kata_id, ordinal)
);
CREATE INDEX kata_candidate_kata_status_idx ON kata_candidate (tenant_id, kata_id, status);

ALTER TABLE kata_candidate ENABLE ROW LEVEL SECURITY;
ALTER TABLE kata_candidate FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON kata_candidate FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── kata_propagation — the SPEC's propagation record, one row per target ────────────────
CREATE TABLE kata_propagation (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    kata_id     uuid NOT NULL,
    target_ref  jsonb NOT NULL,
    approval_id uuid,
    status      text NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending', 'approved', 'rejected', 'published')),
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT kata_propagation_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT kata_propagation_kata_fk FOREIGN KEY (kata_id) REFERENCES kata (id),
    CONSTRAINT kata_propagation_approval_fk FOREIGN KEY (approval_id) REFERENCES approval (id),
    CONSTRAINT kata_propagation_target_is_object
        CHECK (jsonb_typeof(target_ref) = 'object')
);
CREATE INDEX kata_propagation_kata_idx
    ON kata_propagation (tenant_id, kata_id, status, updated_at DESC);

ALTER TABLE kata_propagation ENABLE ROW LEVEL SECURITY;
ALTER TABLE kata_propagation FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON kata_propagation FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── the lane's measurement fields (main's measurement row stays the measurement) ────────
ALTER TABLE kata_measurement
    ADD COLUMN candidate_id       uuid,
    ADD COLUMN instrument_kind    text,
    ADD COLUMN instrument_id      uuid,
    ADD COLUMN instrument_version text,
    ADD COLUMN split_sha256       text,
    ADD COLUMN metric             jsonb,
    ADD COLUMN noise_band         jsonb;

ALTER TABLE kata_measurement
    ADD CONSTRAINT kata_measurement_union_candidate_fk FOREIGN KEY (candidate_id)
        REFERENCES kata_candidate (id),
    ADD CONSTRAINT kata_measurement_union_instrument_kind
        CHECK (instrument_kind IS NULL OR instrument_kind IN ('eval', 'benchmark'));

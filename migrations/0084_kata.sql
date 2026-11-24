-- 0084_kata.sql — T5 (#77): the kata loop (SPEC_V2 §6.3, §9.3–§9.4, §12 T5;
-- docs/plans/inception/T5/02-architecture-design.md, D-T5-1…D-T5-8).
--
-- Four tables, one aggregate each (D-T5-1: a kata is a project type, never a second runtime):
--
--   kata              the pre-registered improvement hypothesis. The instrument kind/id, its
--                     frozen version, the locked split (with the sha that pins it) and the
--                     prediction are fixed at open; observations and the problem statement are
--                     appended as the six Gets are worked. One OPEN row per `(tenant, signature)`
--                     — the partial unique index makes a duplicate kata structurally impossible
--                     (D-T5-7), and the store maps its violation to `SignatureOpen`.
--   kata_candidate    the proposed changes, ordinal monotonic per kata. `prediction` is
--                     nullable ONLY so a hand-written row can be refused by the store's
--                     `PredictionMissing`; `kata_propose_tx` never writes one without it.
--   kata_measurement  one immutable reading from an instrument run: pinned instrument version,
--                     the held-out split and its sha, the run id and the metric/noise band.
--                     Append-only — the module exposes no update function.
--   kata_propagation  one row per propagation target; each row carries its own approval
--                     (D-T5-6: a target is never written across tenants).
--
-- The `kata` measurement contract is NOT NULL at the database's second lock: an unmeasurable
-- kata cannot be inserted even by hand. `crates/store/src/kata.rs` refuses the same shape first
-- so a caller gets a typed error.
--
-- Every table is tenant-scoped with `app_current_tenant()`, carries `version` (except the
-- append-only measurement) and is FORCE RLS — the same shape as 0044's plan spine, 0054's
-- architecture and 0066's authoring family.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

-- ── kata — one improvement hypothesis, pinned before it is measured ──────────────────────
CREATE TABLE kata (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    -- The kata project (§12 T5: the kata job runs as an ordinary project, D-T5-1).
    project_id         uuid NOT NULL,
    -- The repeat signature: one open kata per (tenant, signature), never two (D-T5-7).
    signature          text NOT NULL,
    instrument_kind    text NOT NULL CHECK (instrument_kind IN ('eval', 'benchmark')),
    instrument_id      uuid NOT NULL,
    instrument_version text NOT NULL,
    -- The held-out split and the sha that pins it (D-T5-3: a re-drawn split voids the read).
    split              text NOT NULL,
    split_sha256       text NOT NULL,
    -- The pre-registered hypothesis (R2, §6.3) and the target version it is measured against.
    prediction         jsonb NOT NULL,
    base_ref           jsonb NOT NULL,
    -- Step 2 of the loop: the six-Gets evidence set and the data-backed problem statement.
    observations       jsonb NOT NULL DEFAULT '[]'::jsonb,
    problem            jsonb,
    status             text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
    outcome            text CHECK (outcome IN ('kept', 'reverted', 'inconclusive')),
    outcome_reason     text NOT NULL DEFAULT '',
    -- What a revert restored (D-T5-5); NULL for every other outcome.
    restored_ref       jsonb,
    opened_by          uuid NOT NULL,
    version            bigint NOT NULL DEFAULT 1,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT kata_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT kata_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT kata_opened_by_fk FOREIGN KEY (opened_by) REFERENCES app_user (id),
    CONSTRAINT kata_signature_not_blank CHECK (btrim(signature) <> ''),
    CONSTRAINT kata_instrument_version_not_blank CHECK (btrim(instrument_version) <> ''),
    CONSTRAINT kata_split_not_blank CHECK (btrim(split) <> ''),
    CONSTRAINT kata_split_sha256_not_blank CHECK (btrim(split_sha256) <> ''),
    CONSTRAINT kata_prediction_is_object CHECK (jsonb_typeof(prediction) = 'object'),
    CONSTRAINT kata_base_ref_is_object CHECK (jsonb_typeof(base_ref) = 'object'),
    CONSTRAINT kata_observations_is_array CHECK (jsonb_typeof(observations) = 'array'),
    CONSTRAINT kata_problem_is_object
        CHECK (problem IS NULL OR jsonb_typeof(problem) = 'object'),
    -- A restore is only ever the record of a revert.
    CONSTRAINT kata_restored_ref_is_revert
        CHECK (restored_ref IS NULL OR outcome = 'reverted')
);
CREATE INDEX kata_project_status_idx ON kata (tenant_id, project_id, status, updated_at DESC);
CREATE INDEX kata_updated_idx ON kata (tenant_id, updated_at DESC, id);
-- At most ONE OPEN kata per signature. The store maps a violation to `SignatureOpen`;
-- concurrent openers lose here, never at a read.
CREATE UNIQUE INDEX uq_kata_open_signature
    ON kata (tenant_id, signature) WHERE status = 'open';

ALTER TABLE kata ENABLE ROW LEVEL SECURITY;
ALTER TABLE kata FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON kata FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── kata_candidate — the proposed changes, in proposal order ────────────────────────────
CREATE TABLE kata_candidate (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    kata_id     uuid NOT NULL,
    ordinal     int NOT NULL,
    title       text NOT NULL,
    rationale   text NOT NULL DEFAULT '',
    -- The target the candidate would change: tenant, kind, id and `base_version` (D-T5-6).
    target_ref  jsonb NOT NULL,
    -- The pre-registered prediction; NULL only in a hand-written row, which measure refuses.
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
    -- Ordinals are monotonic per kata; a concurrent proposer loses here.
    CONSTRAINT kata_candidate_kata_ordinal_key UNIQUE (kata_id, ordinal)
);
CREATE INDEX kata_candidate_kata_status_idx ON kata_candidate (tenant_id, kata_id, status);

ALTER TABLE kata_candidate ENABLE ROW LEVEL SECURITY;
ALTER TABLE kata_candidate FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON kata_candidate FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── kata_measurement — an immutable reading from the held-out instrument ────────────────
-- No `updated_at`, no `version`: the store exposes INSERT and SELECT only. A re-drawn split is
-- a different sha, which `kata_measure_tx` refuses against the kata's pin (D-T5-3).
CREATE TABLE kata_measurement (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    kata_id            uuid NOT NULL,
    candidate_id       uuid NOT NULL,
    instrument_kind    text NOT NULL CHECK (instrument_kind IN ('eval', 'benchmark')),
    instrument_id      uuid NOT NULL,
    instrument_version text NOT NULL,
    split              text NOT NULL,
    split_sha256       text NOT NULL,
    run_id             uuid NOT NULL,
    metric             jsonb NOT NULL,
    noise_band         jsonb NOT NULL,
    measured_by        uuid NOT NULL,
    created_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT kata_measurement_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT kata_measurement_kata_fk FOREIGN KEY (kata_id) REFERENCES kata (id),
    CONSTRAINT kata_measurement_candidate_fk FOREIGN KEY (candidate_id)
        REFERENCES kata_candidate (id),
    CONSTRAINT kata_measurement_measured_by_fk FOREIGN KEY (measured_by)
        REFERENCES app_user (id)
);
CREATE INDEX kata_measurement_candidate_idx
    ON kata_measurement (tenant_id, kata_id, candidate_id, created_at DESC);

ALTER TABLE kata_measurement ENABLE ROW LEVEL SECURITY;
ALTER TABLE kata_measurement FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON kata_measurement FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── kata_propagation — one row per target, each with its own approval ───────────────────
CREATE TABLE kata_propagation (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    kata_id     uuid NOT NULL,
    -- The target tenant's `(kind, id, base_version)`; never written from here (D-T5-6).
    target_ref  jsonb NOT NULL,
    -- The `kata.propagate` approval that authorised the target's own publish.
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

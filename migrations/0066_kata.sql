-- 0066_kata.sql — T5: the kata on the V2 spine (SPEC_V2 §6.3, §9.3–§9.4, §12 T5; issue #77).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- The kata is an ordinary job on the spine (§10.3, D31): every kata names one `job_binding` and
-- copies its pinned `release_hash` at open. It does not own a second executor, a second card or a
-- second approval store.
--
-- What P1 owns here:
--   * `kata`            — the record: the pinned binding, the target surface and the stage;
--   * `kata_get`        — the six Gets' evidence, one immutable row per Get;
--   * `kata_measurement`— the held-out measurement: `split` is CHECKed to `test`, so a row cannot
--                         describe a training-split number.
--
-- The pre-registration is structural: `instrument_version_id` and `prediction` are set together
-- by the hypothesis Get and travel together, so a measurement cannot name an instrument that was
-- chosen after it ran (the CHECK below makes the pair atomic).

CREATE TABLE kata (
    id                    uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id             uuid NOT NULL DEFAULT app_current_tenant(),
    job_binding_id        uuid NOT NULL,
    release_hash          text NOT NULL,
    signature             text NOT NULL,
    title                 text NOT NULL,
    target_surface        text NOT NULL
                          CHECK (target_surface IN
                                 ('procedure', 'prompt', 'knowledge', 'tool', 'job_config', 'code')),
    target_ref            text NOT NULL,
    -- open → observed → analysed → hypothesised → proposed → measured; the terminal pair P2/P3
    -- add is adopt/revert (kept in the vocabulary now so a later slice adds behaviour, not a
    -- migration).
    status                text NOT NULL DEFAULT 'open'
                          CHECK (status IN ('open', 'observed', 'analysed', 'hypothesised',
                                            'proposed', 'measured', 'adopted', 'reverted', 'closed')),
    -- the pre-registered instrument (an eval version) and the prediction, set by the hypothesis
    instrument_version_id uuid,
    prediction            text,
    threshold             double precision,
    created_by            uuid,
    version               bigint NOT NULL DEFAULT 1,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT kata_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT kata_binding_fk FOREIGN KEY (job_binding_id) REFERENCES job_binding (id),
    CONSTRAINT kata_instrument_fk FOREIGN KEY (instrument_version_id) REFERENCES eval_version (id),
    CONSTRAINT kata_release_hash_shape CHECK (release_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT kata_signature_not_blank CHECK (btrim(signature) <> ''),
    CONSTRAINT kata_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT kata_target_ref_not_blank CHECK (btrim(target_ref) <> ''),
    CONSTRAINT kata_threshold_check CHECK (threshold IS NULL OR threshold >= 0),
    -- the hypothesis is one atomic act: an instrument with no prediction, or a prediction with
    -- no instrument, is not a pre-registration
    CONSTRAINT kata_prediction_pair CHECK ((instrument_version_id IS NULL) = (prediction IS NULL))
);

-- P4.2's one-open-kata-per-signature rule, enforced where a race cannot defeat it. The action
-- still names the existing kata in its refusal; the index is the guarantee behind it.
CREATE UNIQUE INDEX uq_kata_open_signature
    ON kata (tenant_id, signature)
    WHERE status IN ('open', 'observed', 'analysed', 'hypothesised', 'proposed', 'measured');
CREATE INDEX kata_tenant_idx ON kata (tenant_id, created_at DESC, id);
CREATE INDEX kata_binding_idx ON kata (tenant_id, job_binding_id, status);

ALTER TABLE kata ENABLE ROW LEVEL SECURITY;
ALTER TABLE kata FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON kata FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── kata_get — the six Gets' evidence ─────────────────────────────────────────────────────
-- One immutable row per Get per kata. The hypothesis row is where the pre-registered prediction
-- and instrument version live as evidence; the columns on `kata` are the readable fast path.
CREATE TABLE kata_get (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    kata_id     uuid NOT NULL,
    get_key     text NOT NULL
                CHECK (get_key IN ('observe', 'analyse', 'hypothesis', 'solution',
                                   'standardisation', 'sustainability', 'stretch')),
    content     jsonb NOT NULL,
    recorded_by uuid,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT kata_get_kata_fk FOREIGN KEY (kata_id) REFERENCES kata (id) ON DELETE CASCADE,
    CONSTRAINT kata_get_unique UNIQUE (tenant_id, kata_id, get_key),
    CONSTRAINT kata_get_content_is_object CHECK (jsonb_typeof(content) = 'object')
);

ALTER TABLE kata_get ENABLE ROW LEVEL SECURITY;
ALTER TABLE kata_get FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON kata_get FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── kata_measurement — the held-out number ───────────────────────────────────────────────
-- P1.2: the measurement reads ONLY the held-out split, and the record names it. `split` is
-- CHECKed to `test`: there is no shape in which a kata measurement stores a training-split
-- figure. One measurement per kata: a new candidate is a new kata (P4.2's signature rule).
CREATE TABLE kata_measurement (
    id                    uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id             uuid NOT NULL DEFAULT app_current_tenant(),
    kata_id               uuid NOT NULL,
    instrument_version_id uuid NOT NULL,
    split                 text NOT NULL DEFAULT 'test' CHECK (split = 'test'),
    metric_key            text NOT NULL,
    candidate_run_id      uuid NOT NULL,
    baseline_run_id       uuid NOT NULL,
    candidate_cases       int  NOT NULL CHECK (candidate_cases >= 0),
    baseline_cases        int  NOT NULL CHECK (baseline_cases >= 0),
    candidate_score       double precision NOT NULL,
    baseline_score        double precision NOT NULL,
    delta                 double precision NOT NULL,
    result                jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at            timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT kata_measurement_kata_fk FOREIGN KEY (kata_id) REFERENCES kata (id) ON DELETE CASCADE,
    CONSTRAINT kata_measurement_version_fk FOREIGN KEY (instrument_version_id)
        REFERENCES eval_version (id),
    CONSTRAINT kata_measurement_candidate_fk FOREIGN KEY (candidate_run_id) REFERENCES eval_run (id),
    CONSTRAINT kata_measurement_baseline_fk FOREIGN KEY (baseline_run_id) REFERENCES eval_run (id),
    CONSTRAINT kata_measurement_result_is_object CHECK (jsonb_typeof(result) = 'object')
);
CREATE UNIQUE INDEX uq_kata_measurement_kata ON kata_measurement (tenant_id, kata_id);

ALTER TABLE kata_measurement ENABLE ROW LEVEL SECURITY;
ALTER TABLE kata_measurement FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON kata_measurement FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

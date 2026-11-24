-- 0089_benefit_measurement.sql — the measurements a benefit's evaluation reads
-- (SPEC_V2 §8.3; ADR 0010 docs/decisions/0010-no-deferrals.md; plan v5 §4.3).
--
-- Before this migration a benefit's "realised" claim rested on two nullable timestamps on
-- `benefit_transition`. A timestamp is an assertion, not a measurement: §8.3 requires the BCM
-- to measure the baseline while the old practice is observable, monitor against the profile,
-- and measure the post-transition performance. This table is that record — one row per
-- reading, kind `baseline | interim | post`, with the value, the target it is compared with,
-- the unit, the method and the evidence.
--
-- `programme_id` is denormalised from the profile so a programme's evaluation reads one index
-- and cannot join a benefit of another programme. `tranche_id` is optional: a reading may be
-- attributed to the tranche that produced it, but a baseline reading typically precedes any
-- tranche.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

CREATE TABLE benefit_measurement (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    benefit_id    uuid NOT NULL,
    programme_id  uuid NOT NULL,
    tranche_id    uuid,
    kind          text NOT NULL CHECK (kind IN ('baseline', 'interim', 'post')),
    target_value  numeric,
    actual_value  numeric,
    unit          text NOT NULL DEFAULT '',
    method        text NOT NULL DEFAULT '',
    measured_at   timestamptz NOT NULL,
    evidence      jsonb NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT benefit_measurement_tenant_fk
        FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT benefit_measurement_benefit_fk
        FOREIGN KEY (benefit_id) REFERENCES benefit_profile (id) ON DELETE CASCADE,
    CONSTRAINT benefit_measurement_programme_fk
        FOREIGN KEY (programme_id) REFERENCES programme (id),
    CONSTRAINT benefit_measurement_tranche_fk
        FOREIGN KEY (tranche_id) REFERENCES programme_tranche (id)
);

-- The evaluation reads a benefit's latest reading and whether a `post` follows the enactment;
-- the programme metric reads every benefit of a programme.
CREATE INDEX benefit_measurement_benefit_idx
    ON benefit_measurement (tenant_id, benefit_id, kind, measured_at DESC);
CREATE INDEX benefit_measurement_programme_idx
    ON benefit_measurement (tenant_id, programme_id, measured_at DESC);

ALTER TABLE benefit_measurement ENABLE ROW LEVEL SECURITY;
ALTER TABLE benefit_measurement FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON benefit_measurement FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── governed_asset — the programme's strategy/plan kinds (§8.3) ─────────────────────────
-- "Information, risk, issue, monitoring, communications, quality and resource strategies and
-- plans" are governed assets, not a new table. T1 (0044) widened the vocabulary to the MSP
-- definition set; this adds the seven governance artefacts the programme board baselines.
ALTER TABLE governed_asset DROP CONSTRAINT governed_asset_kind_check;
ALTER TABLE governed_asset ADD CONSTRAINT governed_asset_kind_check CHECK (
    kind IN ('requirement', 'principle', 'adr', 'charter', 'closure', 'postmortem',
             'sop_source', 'mandate', 'programme_brief', 'blueprint', 'business_case',
             'refined_vision', 'benefits_strategy', 'benefits_realization_plan',
             'projects_dossier', 'information_strategy', 'risk_strategy', 'issue_strategy',
             'monitoring_strategy', 'communications_strategy', 'quality_strategy',
             'resource_strategy')
);

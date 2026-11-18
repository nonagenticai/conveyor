-- 0056_cadence_tranche.sql — T10: the SAFe cadence's products and the tranche authorisation
-- record (SPEC_V2 §8.2, §8.3, §10.3; issue #82).
--
-- SAFe gives the programme its cadence, MSP its definition and benefit track. This migration
-- adds the two rows the cadence produces (a lesson from a retro, a demo credit for a completed
-- deliverable) and the columns a tranche carries once its predecessor's review authorises it.
-- The benefit and tranche tables themselves are T1's (0044) and #305's (0051).

-- ── 1. lesson — the organisational-memory proposal a retro produces ──────────────────────
CREATE TABLE lesson (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    programme_id      uuid NOT NULL,
    cadence_id        uuid,
    finding_key       text NOT NULL DEFAULT '',
    statement         text NOT NULL,
    evidence          jsonb NOT NULL DEFAULT '[]'::jsonb,
    independent_count integer NOT NULL DEFAULT 0 CHECK (independent_count >= 0),
    state             text NOT NULL DEFAULT 'proposed'
                      CHECK (state IN ('proposed', 'reviewed', 'promoted', 'rejected')),
    source            text NOT NULL DEFAULT 'retro',
    meta              jsonb NOT NULL DEFAULT '{}'::jsonb,
    version           bigint NOT NULL DEFAULT 1,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT lesson_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT lesson_programme_fk FOREIGN KEY (programme_id) REFERENCES programme (id),
    CONSTRAINT lesson_cadence_fk FOREIGN KEY (cadence_id) REFERENCES programme_cadence (id),
    CONSTRAINT lesson_statement_not_blank CHECK (btrim(statement) <> ''),
    CONSTRAINT lesson_evidence_is_array CHECK (jsonb_typeof(evidence) = 'array'),
    CONSTRAINT lesson_cadence_finding_key UNIQUE (tenant_id, cadence_id, finding_key)
);
CREATE INDEX lesson_programme_idx ON lesson (tenant_id, programme_id, state);
ALTER TABLE lesson ENABLE ROW LEVEL SECURITY;
ALTER TABLE lesson FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON lesson FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 2. programme_demo_credit — no demo, no credit ────────────────────────────────────────
-- A credit is recorded once per (cadence, deliverable); the evidence column is the refs the
-- deliverable exhibited at the demo, copied onto the credit so the credit stands alone.
CREATE TABLE programme_demo_credit (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    cadence_id     uuid NOT NULL,
    deliverable_id uuid NOT NULL,
    evidence       jsonb NOT NULL DEFAULT '[]'::jsonb,
    credited_at    timestamptz NOT NULL DEFAULT now(),
    version        bigint NOT NULL DEFAULT 1,
    CONSTRAINT programme_demo_credit_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT programme_demo_credit_cadence_fk FOREIGN KEY (cadence_id) REFERENCES programme_cadence (id) ON DELETE CASCADE,
    CONSTRAINT programme_demo_credit_deliverable_fk FOREIGN KEY (deliverable_id) REFERENCES wbs_node (id),
    CONSTRAINT programme_demo_credit_evidence_is_array CHECK (jsonb_typeof(evidence) = 'array'),
    CONSTRAINT programme_demo_credit_once UNIQUE (tenant_id, cadence_id, deliverable_id)
);
CREATE INDEX programme_demo_credit_cadence_idx
    ON programme_demo_credit (tenant_id, cadence_id);
ALTER TABLE programme_demo_credit ENABLE ROW LEVEL SECURITY;
ALTER TABLE programme_demo_credit FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON programme_demo_credit FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 3. the tranche authorisation record ───────────────────────────────────────────────────
-- A review's `continue` on tranche N authorises tranche N+1; the columns name the review and
-- the moment, so the start gate reads an explicit authorisation rather than a state alone.
ALTER TABLE programme_tranche
    ADD COLUMN authorised_at timestamptz,
    ADD COLUMN authorisation_review_id uuid
        REFERENCES programme_tranche_review (id) ON DELETE SET NULL;

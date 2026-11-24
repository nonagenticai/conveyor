-- 0067_kata_gate.sql — T5 P2–P4: the gate, the revert and the person (SPEC_V2 §6.3, §9.3–§9.4;
-- issue #77).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- P4 changes what `kata.open` requires: a NAMED instrument (an eval version) and a success
-- threshold. The instrument is therefore set at open, before the hypothesis; the P1 pair rule
-- still holds in the direction that matters — a prediction can never exist without the
-- instrument it pre-registered, so a measurement cannot name an instrument chosen afterwards.
--
-- The kata also pins the procedure version that was published when it opened (the pre-kata
-- version). P2's revert restores exactly that row.
ALTER TABLE kata DROP CONSTRAINT kata_prediction_pair;
ALTER TABLE kata ADD CONSTRAINT kata_prediction_requires_instrument
    CHECK (prediction IS NULL OR instrument_version_id IS NOT NULL);

ALTER TABLE kata ADD COLUMN baseline_version_id uuid;
ALTER TABLE kata ADD CONSTRAINT kata_baseline_fk
    FOREIGN KEY (baseline_version_id) REFERENCES procedure (id);

-- ── kata_revert — P2's append-only revert ────────────────────────────────────────────────
-- One row per reverted kata: the reason, the two procedure versions, and whether the target
-- surface was actually restored. There is no UPDATE or DELETE policy, and the runtime role's
-- grants are revoked in scripts/grants-runtime.sql: the record is evidence.
CREATE TABLE kata_revert (
    id                    uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id             uuid NOT NULL DEFAULT app_current_tenant(),
    kata_id               uuid NOT NULL,
    reason                text NOT NULL,
    baseline_version_id   uuid,
    candidate_version_id  uuid,
    restored              boolean NOT NULL,
    evidence              jsonb NOT NULL DEFAULT '{}'::jsonb,
    reverted_by           uuid,
    created_at            timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT kata_revert_kata_fk FOREIGN KEY (kata_id) REFERENCES kata (id) ON DELETE CASCADE,
    CONSTRAINT kata_revert_baseline_fk FOREIGN KEY (baseline_version_id) REFERENCES procedure (id),
    CONSTRAINT kata_revert_candidate_fk FOREIGN KEY (candidate_version_id) REFERENCES procedure (id),
    CONSTRAINT kata_revert_reason_not_blank CHECK (btrim(reason) <> ''),
    CONSTRAINT kata_revert_evidence_is_object CHECK (jsonb_typeof(evidence) = 'object')
);
CREATE UNIQUE INDEX uq_kata_revert_kata ON kata_revert (tenant_id, kata_id);

ALTER TABLE kata_revert ENABLE ROW LEVEL SECURITY;
ALTER TABLE kata_revert FORCE ROW LEVEL SECURITY;
-- SELECT and INSERT only: a revert is written once and never rewritten.
CREATE POLICY kata_revert_select ON kata_revert FOR SELECT
    USING (tenant_id = app_current_tenant());
CREATE POLICY kata_revert_insert ON kata_revert FOR INSERT
    WITH CHECK (tenant_id = app_current_tenant());

-- ── kata_adoption — P3's person's gate ───────────────────────────────────────────────────
-- The adoption decision is the human's, recorded once. It is not a Get: the six Gets are the
-- work's evidence; this row is the authority that releases the change.
CREATE TABLE kata_adoption (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    kata_id     uuid NOT NULL,
    note        text NOT NULL DEFAULT '',
    adopted_by  uuid NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT kata_adoption_kata_fk FOREIGN KEY (kata_id) REFERENCES kata (id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX uq_kata_adoption_kata ON kata_adoption (tenant_id, kata_id);

ALTER TABLE kata_adoption ENABLE ROW LEVEL SECURITY;
ALTER TABLE kata_adoption FORCE ROW LEVEL SECURITY;
CREATE POLICY kata_adoption_select ON kata_adoption FOR SELECT
    USING (tenant_id = app_current_tenant());
CREATE POLICY kata_adoption_insert ON kata_adoption FOR INSERT
    WITH CHECK (tenant_id = app_current_tenant());

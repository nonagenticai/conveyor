-- 0048_tranche_review.sql — the durable tranche-review record (SPEC_V2 §8.3; issue #305).
--
-- `tranches.review` validated its evidence and then kept only the derived state: the decision
-- and the evidence were discarded, so a review could not be read back. The review is an MSP
-- record — the sponsoring group's continue/pause/stop decision and the evidence it was taken
-- on — and it must survive the transition.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- The tranche row keeps its `state` (the outcome). This table keeps the decision history: each
-- review is one append-only row, so a later review does not overwrite the earlier one, and the
-- tranche's `ON DELETE CASCADE` removes the history only when the tranche itself is removed.

CREATE TABLE programme_tranche_review (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    tranche_id  uuid NOT NULL,
    decision    text NOT NULL CHECK (decision IN ('continue', 'pause', 'stop')),
    evidence    jsonb NOT NULL,
    reviewed_by uuid NOT NULL,
    reviewed_at timestamptz NOT NULL DEFAULT now(),
    version     bigint NOT NULL DEFAULT 1,
    CONSTRAINT programme_tranche_review_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT programme_tranche_review_tranche_fk FOREIGN KEY (tranche_id) REFERENCES programme_tranche (id) ON DELETE CASCADE,
    CONSTRAINT programme_tranche_review_reviewer_fk FOREIGN KEY (reviewed_by) REFERENCES app_user (id),
    CONSTRAINT programme_tranche_review_evidence_object CHECK (jsonb_typeof(evidence) = 'object'),
    CONSTRAINT programme_tranche_review_evidence_not_empty CHECK (evidence <> '{}'::jsonb)
);

CREATE INDEX programme_tranche_review_tranche_idx
    ON programme_tranche_review (tenant_id, tranche_id, reviewed_at DESC, id);

ALTER TABLE programme_tranche_review ENABLE ROW LEVEL SECURITY;
ALTER TABLE programme_tranche_review FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON programme_tranche_review FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

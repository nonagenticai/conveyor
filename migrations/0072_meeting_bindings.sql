-- 0072_meeting_bindings.sql — T19F: forum-policy authoring and the T9/T10 binding.
--
-- SPEC_V2 §6.8, §12 T19F; the plan is docs/plans/T19F-meeting-bindings.md.
--
-- A meeting_type version is written by the tenant's authoring route: publishing a
-- kind='policy' asset whose payload carries `meeting_type` writes the next immutable version
-- and records the output on the asset. A governance body and a cadence row name their forum
-- type explicitly. No table is new; every altered column is nullable, so the direct paths keep
-- working for rows without a binding.

-- ── the authoring → meeting_type output link ─────────────────────────────────────────────
ALTER TABLE authoring_asset
    ADD COLUMN published_meeting_type_id uuid REFERENCES meeting_type (id);

ALTER TABLE authoring_asset
    ADD CONSTRAINT authoring_asset_policy_output
    CHECK (published_meeting_type_id IS NULL OR kind = 'policy');

-- One asset produces one version; the next version needs a new asset key.
CREATE UNIQUE INDEX uq_authoring_asset_meeting_type
    ON authoring_asset (tenant_id, published_meeting_type_id)
    WHERE published_meeting_type_id IS NOT NULL;

-- ── the explicit binding on the two T9/T10 consumers ────────────────────────────────────
ALTER TABLE governance_body
    ADD COLUMN meeting_type_key text;

ALTER TABLE governance_body
    ADD CONSTRAINT governance_body_meeting_type_key_not_blank
    CHECK (meeting_type_key IS NULL OR btrim(meeting_type_key) <> '');

ALTER TABLE programme_cadence
    ADD COLUMN meeting_type_key text;

ALTER TABLE programme_cadence
    ADD CONSTRAINT programme_cadence_meeting_type_key_not_blank
    CHECK (meeting_type_key IS NULL OR btrim(meeting_type_key) <> '');

-- One cadence row materialises one non-cancelled meeting. T19's `uq_meeting_live` covers only
-- status='live'; this is the stronger instance rule the materialise action is idempotent
-- against. A cancelled instance may be replaced.
CREATE UNIQUE INDEX uq_meeting_cadence_instance
    ON meeting (tenant_id, cadence_slot)
    WHERE cadence_slot IS NOT NULL AND status <> 'cancelled';

-- 0052_card_ledger.sql — the per-card append-only claim/fact ledger (issue #295, ADR 0008).
--
-- ADR 0008 decides the home: a new append-only table, not `work_package_memory` (that table is
-- project-scoped and carries consolidation state and derived digest rows) and not `event` (the
-- board stream: relayed, pruned, and one doorbell per mutation rather than a record a caller
-- reads back).
--
-- The key is the conveyor card id. The old `planka_card_id` is deliberately NOT carried: nothing
-- is copied from the RM database at cutover (D7), so there is no compatibility column and no
-- backfill.
--
-- Append-only is enforced by construction: this table has INSERT and SELECT paths only. No action
-- and no store function updates or deletes a row, so `scripts/grants-runtime.sql` needs no revoke
-- for an operation that does not exist. Adding a mutating path is the change that would break the
-- guarantee — it is not an oversight.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

CREATE TABLE card_ledger (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid NOT NULL,
    card_id    uuid NOT NULL,
    kind       text NOT NULL CHECK (kind IN ('claim', 'fact')),
    body       text NOT NULL,
    source     jsonb NOT NULL DEFAULT '{}'::jsonb,
    verdict    jsonb,
    created_by uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT card_ledger_card_fk FOREIGN KEY (card_id) REFERENCES card (id) ON DELETE RESTRICT
);
-- The read path: one card's ledger, oldest first, optionally filtered by kind. The trailing id
-- is the tiebreaker for rows written inside one transaction.
CREATE INDEX card_ledger_card_created_idx ON card_ledger (tenant_id, card_id, created_at, id);

-- The house policy shape: one FOR ALL policy with USING (reads) AND WITH CHECK (writes), so a
-- forged tenant_id cannot be written even by a caller that bypasses the explicit store predicate.
ALTER TABLE card_ledger ENABLE ROW LEVEL SECURITY;
ALTER TABLE card_ledger FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON card_ledger FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- 0012 — the three retention rules that had no producer (data-arch §6).
--
-- Until now `action_invocation` (30 d), `run` (180 d after terminal) and `delivery` (30 d after terminal)
-- were documented rules with no job behind them: the tables grew forever. This migration supplies what the
-- three prunes need, and nothing else.

-- ── approval.run_id: detach, never delete ────────────────────────────────────────────────────────────────
-- A pruned run must not take its approvals with it. `approval_decision.note` and `item_decisions` exist
-- NOWHERE else — `audit_event` records the actor, the action and the resource id, not the human's reason —
-- so CASCADE would silently delete the governance record to reclaim a row. SET NULL is the same choice
-- `0010_s08_retention.sql` made for `model_validation_run.revision_id`.
--
-- The column was already nullable (`run_id uuid`), so no column change is needed. `card_id`/`project_id` keep
-- the context after the detach, and data-arch §6 now states the rule: **approvals are never pruned.**
--
-- `uq_appr_pending (tenant_id, run_id, gate) WHERE status = 'pending'` is why `run.prune` also refuses a run
-- that still has a pending gate: with `run_id` NULL, NULLs are distinct and the partial unique index would no
-- longer stop a second pending gate on the same key.
ALTER TABLE approval DROP CONSTRAINT approval_run_fk;
ALTER TABLE approval ADD CONSTRAINT approval_run_fk
    FOREIGN KEY (run_id) REFERENCES run (id) ON DELETE SET NULL;

-- The cascade's lookup: the existing indexes on `approval` are pending-only or card-ordered.
CREATE INDEX approval_run_idx ON approval (tenant_id, run_id);

-- ── the three scan paths ────────────────────────────────────────────────────────────────────────────────
-- Each prune deletes a batch ordered by (time, id) inside the tenant scope. The existing indexes cannot
-- serve those ranges — `action_invocation_tenant_action_idx` leads with `action`, and both the `run` and
-- `delivery` partial indexes cover only LIVE states — so a prune without these would seq-scan the table it
-- is trying to keep small.
CREATE INDEX action_invocation_created_idx ON action_invocation (tenant_id, created_at, id);

CREATE INDEX run_terminal_finished_idx ON run (tenant_id, finished_at, id)
    WHERE status IN ('succeeded', 'failed', 'cancelled');

CREATE INDEX delivery_terminal_updated_idx ON delivery (tenant_id, updated_at, id)
    WHERE state IN ('delivered', 'failed', 'expired');

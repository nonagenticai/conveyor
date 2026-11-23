-- 0078_approval_policy_approver_kind.sql — who may decide a gate, as tenant data.
--
-- SPEC_V2 §5.4: an approver is a person by default. A tenant may authorise a non-human
-- approver for a gate only by explicitly declaring that approver kind non-governance in the
-- gate's policy row. A "job" decision reaches the shared writer as `agent` or `service` (see
-- `crates/actions/src/control/approvals.rs::decider_kind`), so the vocabulary here is the
-- domain's `DeciderKind` spelling.
--
-- ⛔ Absent means `human`: the strict default. A row can only ever lower the requirement for
-- the one action/gate it names, and a tenant that has decided nothing gets the §5.4 rule.
ALTER TABLE approval_policy
    ADD COLUMN approver_kind text NOT NULL DEFAULT 'human';

ALTER TABLE approval_policy
    ADD CONSTRAINT approval_policy_approver_kind_vocabulary
    CHECK (approver_kind IN ('human', 'triage_agent', 'rule', 'agent', 'service'));

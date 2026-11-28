-- 0093_approval_policy_approver_kind.sql — one approver vocabulary for the tenant's gate policy.
--
-- SPEC_V2 §5.4: an approver is a person by default. A tenant may authorise a non-human
-- approver for a gate only by explicitly declaring that approver kind non-governance in the
-- gate's policy row. The shared writer distinguishes four non-human deciders (`triage_agent`,
-- `rule`, `agent`, `service` — the lane's `DeciderKind` spellings) and T6's auto-merge adds
-- `auto`, so the single vocabulary is the union.
--
-- `0068_development_project.sql` creates the column and its two-kind constraint; this
-- migration widens the constraint to the union. It never re-adds the column.
--
-- ⛔ Absent means `human`: the strict default. A row can only ever lower the requirement for
-- the one action/gate it names, and a tenant that has decided nothing gets the §5.4 rule.
ALTER TABLE approval_policy
    DROP CONSTRAINT approval_policy_approver_kind_vocabulary;

ALTER TABLE approval_policy
    ADD CONSTRAINT approval_policy_approver_kind_vocabulary
    CHECK (approver_kind IN ('human', 'auto', 'triage_agent', 'rule', 'agent', 'service'));

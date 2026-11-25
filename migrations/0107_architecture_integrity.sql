-- 0107_architecture_integrity.sql — #526 (architecture findings): adoption is a board act at
-- the table, and phase H has exactly one outcome.
--
-- Two indexes the actions already assume:
--
-- 1. `adm_profile.board_decision_id` was an unconstrained uuid. The action checks that a cited
--    decision is an approved architecture-board decision about THIS profile, but a raw UPDATE
--    could adopt a profile citing nothing (or a random uuid). The FK names the approval row and
--    the CHECK makes an adoption without one impossible. Every existing adopted profile cites a
--    real approval (verified before this migration), and `approval.id` is globally unique.
--
-- 2. `advance_phase_tx` reads the phase-H outcome review, and exactly one may govern a project.
--    The partial unique index is the table-level half of that rule: one non-null outcome per
--    (tenant, project), so a second verdict is a conflict rather than a stronger case. Reviews
--    without an outcome (the Phase-G level) are not constrained.
--
-- Forward only: never edited after this slice closes (CONTRACT §2). Additive: old rows satisfy
-- both constraints (expand), so an application-only rollback may run against this schema.

ALTER TABLE adm_profile
    ADD CONSTRAINT adm_profile_board_decision_fk
    FOREIGN KEY (board_decision_id) REFERENCES approval (id);

ALTER TABLE adm_profile
    ADD CONSTRAINT adm_profile_adopted_needs_decision
    CHECK (status <> 'adopted' OR board_decision_id IS NOT NULL);

CREATE UNIQUE INDEX architecture_compliance_review_one_outcome
    ON architecture_compliance_review (tenant_id, project_id)
    WHERE outcome IS NOT NULL;

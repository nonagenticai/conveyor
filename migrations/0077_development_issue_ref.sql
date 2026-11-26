-- 0077_development_issue_ref.sql — a delivery unit names the GitHub issue that carries it (#779).
--
-- SPEC_V2 §9.1/§10.1: a development project is the unit of a delivery, and the issue is the
-- intent that names it. The column is nullable and every old row satisfies the constraint, so
-- this is an expand-only migration: the prior release reads the same table without it.

ALTER TABLE development_project
    ADD COLUMN issue_ref text;

-- `owner/name#N`. The action validates the same shape and refuses a bare number; the constraint
-- is the last line of defence, not the first.
ALTER TABLE development_project
    ADD CONSTRAINT development_project_issue_ref_shape
    CHECK (issue_ref IS NULL OR issue_ref ~ '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+#[0-9]+$');

-- One unit per issue. A second charter naming the same issue is a different unit pretending to
-- be this one; the store maps this index to `issue_already_chartered`.
CREATE UNIQUE INDEX uq_development_project_issue
    ON development_project (tenant_id, issue_ref)
    WHERE issue_ref IS NOT NULL;

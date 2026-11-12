-- 0016_procedure_draft.sql — at most one DRAFT per (tenant, key).
--
-- The slice plan requires a second draft of a key to be a `409 procedure.draft_exists`, and an application-level
-- SELECT would be a check two concurrent creates can both pass. 0015 created only the published index; this is
-- its draft twin, so the rule is the database's.

CREATE UNIQUE INDEX uq_procedure_draft
    ON procedure (tenant_id, key) WHERE status = 'draft';

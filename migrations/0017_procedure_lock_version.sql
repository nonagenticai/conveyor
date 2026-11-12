-- 0017_procedure_lock_version.sql — an optimistic-concurrency counter for `procedure`.
--
-- `procedure.version` is the FAMILY version: `uq (tenant_id, key, version)` and "an edit forks v+1" make it the
-- identity of a version, not an edit counter. That leaves `procedures.update`'s `If-Match` with nothing to
-- compare — a draft edited twice still reads `version = 1`, so a stale precondition would pass.
--
-- CONTRACT-CHANGE REQUEST (S11 plan §9, request 3): `02-DATA-ARCHITECTURE §2.12`'s `procedure` gains
-- `lock_version bigint nn d=1`, bumped by every mutation. `version` keeps its meaning.
--
-- The same split the other tables get for free because their `version` never had a second job.

ALTER TABLE procedure
    ADD COLUMN lock_version bigint NOT NULL DEFAULT 1;

COMMENT ON COLUMN procedure.lock_version IS
    'Optimistic-concurrency counter for If-Match; bumped by every mutation. `version` is the family version.';

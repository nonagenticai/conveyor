-- 0010_s08_retention.sql — `model_validation_run.revision_id` becomes `ON DELETE SET NULL`.
--
-- Forward only: `0009_s08_models.sql` is never edited after it shipped, because sqlx records
-- a checksum per migration and changing a file that a database has already applied makes its
-- next `migrate` fail (CONTRACT §2).
--
-- WHY: data-architecture §2.4 keeps the newest 50 revisions per model, and `model.prune` is
-- the job that enforces it. `0009` created `model_validation_run_revision_fk` with the
-- default NO ACTION, so the moment a run names a revision older than the 50th the prune's
-- DELETE fails on the foreign key — and because the candidate set is a batch, one referenced
-- row blocks the whole delete. A run from yesterday can name exactly such a revision.
--
-- A validation run is the record of what was checked, not ownership of the revision it read.
-- When that revision is pruned the run stays and `revision_id` becomes NULL: "the revision
-- this run named is no longer retained". The alternative — deleting the run with the
-- revision — would discard a report that is still readable, and it would also conflict with
-- the run's own 90-day retention (§5.3), which is the only rule allowed to expire a run.
--
-- No new tables and no grants: `migrate` re-applies `scripts/grants-runtime.sql` after the
-- last migration, and `model_validation_run` already carries its tenant policy and FORCE RLS
-- from 0009. The re-created constraint keeps the same name.

ALTER TABLE model_validation_run
    DROP CONSTRAINT model_validation_run_revision_fk;

ALTER TABLE model_validation_run
    ADD CONSTRAINT model_validation_run_revision_fk
        FOREIGN KEY (revision_id) REFERENCES revision (id) ON DELETE SET NULL;

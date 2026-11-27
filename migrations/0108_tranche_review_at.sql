-- 0108: backfill programme_tranche.review_at from the review that closed it.
--
-- WHY (T4 spot-audit, 2026-11-27). `review_tranche_tx` recorded the review row and moved the
-- tranche's state, but never copied the review's `reviewed_at` onto
-- `programme_tranche.review_at`. The audit's `tranche_plan` rule requires a closed tranche to
-- carry its review date, so closed tranches 3 and 4 were red on a fresh snapshot and S05's
-- `S05-artefact-standard` check failed with `tranche_plan:tranche-4` open. The store now writes
-- the date on every review; this file repairs the rows written before that fix.
--
-- WHAT THIS DOES: for every closed/paused/stopped tranche whose `review_at` is NULL and which has
-- at least one review row, set it to the LATEST review's `reviewed_at`. A tranche that was paused
-- and then resumed and continued (v1-tr2-1) therefore takes the continue's date. One transaction;
-- a re-run is a no-op; an operator's non-NULL date is never overwritten.

BEGIN;

UPDATE programme_tranche t
   SET review_at = (
           SELECT max(r.reviewed_at)
             FROM programme_tranche_review r
            WHERE r.tenant_id = t.tenant_id
              AND r.tranche_id = t.id
       ),
       updated_at = now()
 WHERE t.review_at IS NULL
   AND t.state IN ('closed', 'paused', 'stopped')
   AND EXISTS (
           SELECT 1
             FROM programme_tranche_review r
            WHERE r.tenant_id = t.tenant_id
              AND r.tranche_id = t.id
       );

COMMIT;

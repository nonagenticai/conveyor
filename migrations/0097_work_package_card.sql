-- 0097_work_package_card.sql — a work package names its card, and a job run names it too
-- (#526, SPEC_V2 §5.1, v1-p5-5).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- `wbs_node.card_id` and its `wbs_node_card_fk` foreign key to `card(id)` already ship in
-- 0044_plan_spine.sql. This migration deliberately does not re-add them: a second ADD COLUMN
-- would fail on every database that applied 0044, and the column is already the work package's
-- link. What was missing is the RUN half of the chain. A job run executes a card's work package,
-- so `run.card_id` must be named, not merely nullable. The store refuses the insert first
-- (`422 card_required`); this CHECK is the second door, so a raw writer (a fixture, a repair)
-- cannot create a cardless job row either.
--
-- NOT VALID: historical rows written before this invariant stay readable, while every new
-- INSERT and UPDATE is checked without a table rewrite. 0096's `run_job_binding_required` is the
-- sibling door for the binding and release pin.
--
-- The wbs_node work-package CHECK (a `work_package` node names its card) is a follow-up slice;
-- it is deliberately absent here.
--
-- No seed rows.

ALTER TABLE run
    ADD CONSTRAINT run_job_card_required
        CHECK (kind <> 'job' OR card_id IS NOT NULL) NOT VALID;

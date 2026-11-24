-- 0097_work_package_card.sql — a work package names its card, and a job run names it too
-- (#526, SPEC_V2 §5.1, v1-p5-5).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- `wbs_node.card_id` and its `wbs_node_card_fk` foreign key to `card(id)` already ship in
-- 0044_plan_spine.sql. This migration deliberately does not re-add them: a second ADD COLUMN
-- would fail on every database that applied 0044, and the column is already the work package's
-- link. What was missing is the invariant. A work package names the card that executes it, and
-- a job run names the work package's card. The actions refuse both first (`422 card_required`);
-- these CHECKs are the second door, so a raw writer (a fixture, a repair) cannot create either
-- cardless row.
--
-- NOT VALID: historical rows written before this invariant stay readable, while every new
-- INSERT and UPDATE is checked without a table rewrite. 0096's `run_job_binding_required` is the
-- sibling door for the binding and release pin.
--
-- No seed rows.

ALTER TABLE run
    ADD CONSTRAINT run_job_card_required
        CHECK (kind <> 'job' OR card_id IS NOT NULL) NOT VALID;

-- The plan half of the same chain: a `work_package` node owes its card; the other kinds
-- (phase, deliverable, milestone) have no executor and may stay unlinked.
ALTER TABLE wbs_node
    ADD CONSTRAINT wbs_node_work_package_card_required
        CHECK (kind <> 'work_package' OR card_id IS NOT NULL) NOT VALID;

-- 0058_data_model_gaps.sql — SPEC_V2 §10.3's remaining schema (issue #413).
--
-- The spec's "procedure/procedure_step extend with PDDL action text" is written against S11's
-- names; migration 0035 renamed `procedure_step` to `process_step` when the process tree replaced
-- the procedure/run-step model. Re-creating `procedure_step` would resurrect a retired table and
-- duplicate the live one, so the action text lands where the executable step actually lives:
--
--   procedure.action_text    the SOP's PDDL action text (T16/#85 writes it)
--   process_step.action_text the step's PDDL action text
--   card.wbs_node_id         the WBS node a card executes (§10.3's "new columns only where
--                            unavoidable"), nullable so a card outside a plan is unchanged
--
-- ⛔ The PKO tables (§10.3: pko_release / pko_entity / pko_relation) are deliberately NOT added:
-- owner decision 2026-11-19 — kag is archived and knowledge lives in strata_rag/strata_ingest.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

ALTER TABLE procedure ADD COLUMN action_text text;
ALTER TABLE process_step ADD COLUMN action_text text;

ALTER TABLE card ADD COLUMN wbs_node_id uuid;
ALTER TABLE card
    ADD CONSTRAINT card_wbs_node_fk
    FOREIGN KEY (wbs_node_id) REFERENCES wbs_node (id) ON DELETE SET NULL;

CREATE INDEX card_wbs_node_idx ON card (wbs_node_id) WHERE wbs_node_id IS NOT NULL;

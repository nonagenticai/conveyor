-- 0060_procedure_ingestion.sql — T16 (#85): SOP / process-document ingestion, the publication
-- gate and the PDDL execution state.
--
-- Owner decision 2026-11-19 (recorded in #413 / PR #419): the PKO tables are NOT added — kag is
-- archived and knowledge lives in strata_rag/strata_ingest. An RDF/OWL/Turtle document therefore
-- lands as a governed source whose knowledge pack is a strata reference, never as local rows.
--
-- 0058_data_model_gaps (#413) already added the executable step's PDDL action text on
-- `process_step.action_text` and the SOP's on `procedure.action_text`. This migration adds only
-- what T16 still needs:
--   * provenance on the derived SOP and process (the source asset, the converter version, the
--     knowledge-pack reference);
--   * the executable PDDL state: the domain text, the initial facts, each step's precondition and
--     effect literals, and the fact state either side of a run step;
--   * the publication approval recorded against the released process.
--
-- Forward only: no backfill, no rewrite of existing rows beyond defaults.

-- ── 1. procedure — the derived SOP names its source and its knowledge pack ────────────────
ALTER TABLE procedure
    ADD COLUMN source_asset_id   uuid REFERENCES governed_asset (id),
    ADD COLUMN knowledge_pack_ref text NOT NULL DEFAULT '',
    ADD COLUMN approval_id       uuid REFERENCES approval (id);

COMMENT ON COLUMN procedure.source_asset_id IS
    'The governed_asset(kind=''sop_source'') this SOP was converted from; NULL for a hand-authored SOP (T16, #85).';
COMMENT ON COLUMN procedure.knowledge_pack_ref IS
    'The paired strata knowledge pack (`<store_id>/<job_id>`), when the conversion produced one (T16, #85).';
COMMENT ON COLUMN procedure.approval_id IS
    'The approved publication gate that released this derived SOP; set by procedures.publish (T16, #85).';

-- ── 2. process — the executable derived from the source ───────────────────────────────────
-- `pddl_domain` is the domain text the converter produced (the human-readable contract);
-- `pddl_init` is the typed initial state a run starts from. Both are data, never a planner input:
-- the executor checks ground literals only (SPEC_V2 §5.6, D53).
ALTER TABLE process
    ADD COLUMN source_asset_id    uuid REFERENCES governed_asset (id),
    ADD COLUMN knowledge_pack_ref text NOT NULL DEFAULT '',
    ADD COLUMN converter_version  text NOT NULL DEFAULT '',
    ADD COLUMN pddl_domain        text NOT NULL DEFAULT '',
    ADD COLUMN pddl_init          jsonb NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN approval_id        uuid REFERENCES approval (id);

ALTER TABLE process
    ADD CONSTRAINT process_pddl_init_is_array CHECK (jsonb_typeof(pddl_init) = 'array');

CREATE INDEX process_source_idx
    ON process (tenant_id, source_asset_id) WHERE source_asset_id IS NOT NULL;

COMMENT ON COLUMN process.source_asset_id IS
    'The untrusted source this executable was converted from; publish refuses while it is unreviewed (T16, #85).';
COMMENT ON COLUMN process.converter_version IS
    'The converter that produced the draft (`prose@1`, `bpmn@1`, …), so a re-ingest is reproducible (T16, #85).';
COMMENT ON COLUMN process.approval_id IS
    'The approved publication gate that released this process; set by processes.publish (T16, #85).';

-- ── 3. process_step — the PDDL action space ──────────────────────────────────────────────
-- The action text itself is 0058's `process_step.action_text`. Preconditions and effects are
-- ground-literal arrays (`["stock_verified(sku-1)", "not(on_hold(sku-1))"]`); an effect list is
-- `{"add": [...], "delete": [...]}`. A PKO entity never lives here (D53 — no rows for it exist).
ALTER TABLE process_step
    ADD COLUMN pddl_preconditions jsonb NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN pddl_effects      jsonb NOT NULL DEFAULT '{"add":[],"delete":[]}'::jsonb;

ALTER TABLE process_step
    ADD CONSTRAINT process_step_pddl_preconditions_is_array
        CHECK (jsonb_typeof(pddl_preconditions) = 'array'),
    ADD CONSTRAINT process_step_pddl_effects_is_object
        CHECK (jsonb_typeof(pddl_effects) = 'object');

-- ── 4. run_step — the fact state either side of a step ────────────────────────────────────
-- The executor threads the state through the run's own trace: `before` is read from the previous
-- step's `after` (or the process's `pddl_init`), never from a caller.
ALTER TABLE run_step
    ADD COLUMN pddl_state_before jsonb,
    ADD COLUMN pddl_state_after  jsonb;

ALTER TABLE run_step
    ADD CONSTRAINT run_step_pddl_state_before_is_array
        CHECK (pddl_state_before IS NULL OR jsonb_typeof(pddl_state_before) = 'array'),
    ADD CONSTRAINT run_step_pddl_state_after_is_array
        CHECK (pddl_state_after IS NULL OR jsonb_typeof(pddl_state_after) = 'array');

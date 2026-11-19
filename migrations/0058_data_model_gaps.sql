-- 0058_data_model_gaps.sql — SPEC_V2 §10.3's remaining schema (issue #413).
--
-- Three additions, each named in the spec and measured absent on 2026-11-19:
--
--   procedure.action_text   the procedure's PDDL action text (§10.3: "procedure/procedure_step
--                           extend with PDDL action text"). T16 (#85) writes it; this ships the
--                           column so the ingestion has somewhere to land.
--   procedure_step          one ordered step of a procedure, with its own action text. The SOP's
--                           five documents stay on `procedure`; the steps are rows because a step
--                           is judged and cited individually.
--   card.wbs_node_id        the WBS node a card executes (§10.3's "new columns only where
--                           unavoidable"), a nullable FK so a card outside a plan is unchanged.
--
-- ⛔ The PKO tables (§10.3: pko_release / pko_entity / pko_relation) are deliberately NOT added:
-- owner decision 2026-11-19 — kag is archived and knowledge lives in strata_rag/strata_ingest.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

-- ── procedure.action_text — the PDDL text the procedure runs ─────────────────────────────
ALTER TABLE procedure ADD COLUMN action_text text;

-- ── procedure_step — one ordered step of a procedure ─────────────────────────────────────
CREATE TABLE procedure_step (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    procedure_id uuid NOT NULL,
    position     integer NOT NULL CHECK (position >= 1),
    action_text  text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT procedure_step_tenant_fk
        FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT procedure_step_procedure_fk
        FOREIGN KEY (procedure_id) REFERENCES procedure (id) ON DELETE CASCADE,
    CONSTRAINT procedure_step_position_unique UNIQUE (procedure_id, position),
    CONSTRAINT procedure_step_action_not_blank CHECK (btrim(action_text) <> '')
);

CREATE INDEX procedure_step_procedure_idx ON procedure_step (procedure_id, position);

ALTER TABLE procedure_step ENABLE ROW LEVEL SECURITY;
ALTER TABLE procedure_step FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON procedure_step FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── card.wbs_node_id — the plan node this card executes ──────────────────────────────────
ALTER TABLE card ADD COLUMN wbs_node_id uuid;
ALTER TABLE card
    ADD CONSTRAINT card_wbs_node_fk
    FOREIGN KEY (wbs_node_id) REFERENCES wbs_node (id) ON DELETE SET NULL;

CREATE INDEX card_wbs_node_idx ON card (wbs_node_id) WHERE wbs_node_id IS NOT NULL;

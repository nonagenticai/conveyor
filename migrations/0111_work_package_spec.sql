-- 0111_work_package_spec.sql — the delivery-item standard's one home on the card
-- (SPEC_V2 §5.8, decision D69; conveyor#935).
--
-- A work package is a card of the `delivery` card type, or a card that a
-- `wbs_node(kind='work_package')` links to. It carries exactly one spec: the BDD user story
-- (`As a …, I want …, so that …`), the Gherkin `feature` document, and the INVEST
-- Definition-of-Ready evidence. The parsed scenarios and the acceptance DoD are derived (the
-- former from `feature`, the latter from the approved `card_done` gate's evidence), so neither
-- is stored here — a second copy would be a second home.
--
-- The story clauses are non-blank at the database as the second door: the domain validator and
-- the typed 422 refuse first (§5.8), and a caller that skips them cannot store half a story.
-- `(tenant_id, card_id)` is the write key (`cards.spec_put` upserts it and bumps `version`);
-- `(tenant_id, id)` makes the row referenceable inside its tenant. Forward only: never edited
-- after this slice closes (CONTRACT §2).

CREATE TABLE work_package_spec (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    card_id       uuid NOT NULL,
    story_persona text NOT NULL,
    story_action  text NOT NULL,
    story_benefit text NOT NULL,
    feature       text NOT NULL,
    dor           jsonb NOT NULL DEFAULT '{}'::jsonb,
    version       bigint NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT work_package_spec_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT work_package_spec_card_fk FOREIGN KEY (card_id) REFERENCES card (id)
        ON DELETE CASCADE,
    CONSTRAINT work_package_spec_tenant_card_key UNIQUE (tenant_id, card_id),
    CONSTRAINT work_package_spec_tenant_id_key UNIQUE (tenant_id, id),
    CONSTRAINT work_package_spec_persona_not_blank CHECK (btrim(story_persona) <> ''),
    CONSTRAINT work_package_spec_action_not_blank CHECK (btrim(story_action) <> ''),
    CONSTRAINT work_package_spec_benefit_not_blank CHECK (btrim(story_benefit) <> ''),
    CONSTRAINT work_package_spec_dor_is_object CHECK (jsonb_typeof(dor) = 'object')
);

ALTER TABLE work_package_spec ENABLE ROW LEVEL SECURITY;
ALTER TABLE work_package_spec FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON work_package_spec FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

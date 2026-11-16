-- 0044_plan_spine.sql — T1: the plan spine and the MSP rows (SPEC_V2 §5.1, §5.3–§5.5, §8.2,
-- §8.3, §10.3, §12 T1; GitHub issue #72).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- The boundary is the T1 row and issue #72's Out of scope:
--   * `wbs_node` is a NEW, project-scoped tree. V1's `outline` / `outline_node` (0007) stay
--     untouched and are not backfilled: the V1 tree has no typed WSJF inputs, no baseline and
--     no acceptance-by-artefact. `outline.kind = 'wbs'` is a name, not this model (D-T1-1).
--   * A baseline is an immutable JSON snapshot, never a copy of current rows (D-T1-2). Later
--     row changes never update it.
--   * A change is `project_register_item(kind = 'change')` with an impact block and a
--     `baseline_ref`; a direct row edit is not the change path (D-T1-3).
--   * WSJF has FOUR typed inputs and NO score column (D-T1-4); the score is computed on read.
--   * `project_dependency` is the edge source (D-T1-5); the programme gate falls back to
--     `project.meta.depends_on` only for unmigrated seed rows.
--   * The portfolio is a read model; the in-flight set is tenant configuration (D-T1-7).
--   * The MSP rows are here because T1 owns the record and the refusals; the cadence
--     materialisation and the realization procedure are T10 (D-T1-11).
--
-- Every table is tenant-scoped with `app_current_tenant()`, carries `version` for If-Match
-- (mutable rows only), and is FORCE RLS — the same shape as 0040's organisation spine.

-- ── wbs_node — the project's phase/deliverable/work-package/milestone tree ────────────────
-- The four WSJF inputs are stored, non-negative integers. `job_size > 0` because the score
-- divides by it; zero is a refusal, never a division. `acceptance` is by artefact/run, not a
-- checkbox; `evidence` names what met it.
CREATE TABLE wbs_node (
    id               uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id        uuid NOT NULL DEFAULT app_current_tenant(),
    project_id       uuid NOT NULL,
    parent_id        uuid,
    position         numeric NOT NULL DEFAULT 0,
    kind             text NOT NULL
                     CHECK (kind IN ('phase', 'deliverable', 'work_package', 'milestone')),
    title            text NOT NULL,
    body             text NOT NULL DEFAULT '',
    status           text NOT NULL DEFAULT 'open'
                     CHECK (status IN ('open', 'accepted', 'cancelled')),
    acceptance       jsonb NOT NULL DEFAULT '[]'::jsonb,
    evidence         jsonb NOT NULL DEFAULT '[]'::jsonb,
    card_id          uuid,
    business_value   int NOT NULL DEFAULT 0 CHECK (business_value >= 0),
    time_criticality int NOT NULL DEFAULT 0 CHECK (time_criticality >= 0),
    risk_reduction   int NOT NULL DEFAULT 0 CHECK (risk_reduction >= 0),
    job_size         int NOT NULL DEFAULT 1 CHECK (job_size > 0),
    meta             jsonb NOT NULL DEFAULT '{}'::jsonb,
    version          bigint NOT NULL DEFAULT 1,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT wbs_node_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT wbs_node_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT wbs_node_parent_fk FOREIGN KEY (parent_id) REFERENCES wbs_node (id),
    CONSTRAINT wbs_node_card_fk FOREIGN KEY (card_id) REFERENCES card (id),
    CONSTRAINT wbs_node_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT wbs_node_not_own_parent CHECK (parent_id IS DISTINCT FROM id)
);
CREATE INDEX wbs_node_project_idx ON wbs_node (tenant_id, project_id, parent_id, position, id);
CREATE INDEX wbs_node_card_idx ON wbs_node (tenant_id, card_id) WHERE card_id IS NOT NULL;

ALTER TABLE wbs_node ENABLE ROW LEVEL SECURITY;
ALTER TABLE wbs_node FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wbs_node FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── project_baseline — a frozen, approved snapshot ────────────────────────────────────────
-- Append-only: `take` inserts, nothing updates. The snapshot holds the WBS/register/budget
-- facts as they were at the take, so a later row change cannot move it (D-T1-2).
CREATE TABLE project_baseline (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    project_id  uuid NOT NULL,
    kind        text NOT NULL DEFAULT 'scope'
                CHECK (kind IN ('scope', 'schedule', 'cost')),
    snapshot    jsonb NOT NULL,
    taken_at    timestamptz NOT NULL DEFAULT now(),
    taken_by    uuid NOT NULL,
    approved_by uuid,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT project_baseline_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT project_baseline_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT project_baseline_taken_by_fk FOREIGN KEY (taken_by) REFERENCES app_user (id),
    CONSTRAINT project_baseline_approved_by_fk FOREIGN KEY (approved_by) REFERENCES app_user (id)
);
CREATE INDEX project_baseline_project_idx
    ON project_baseline (tenant_id, project_id, taken_at DESC, id);

ALTER TABLE project_baseline ENABLE ROW LEVEL SECURITY;
ALTER TABLE project_baseline FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON project_baseline FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── project_register_item — the one six-kind register ────────────────────────────────────
-- risk | issue | assumption | dependency | benefit | change. A `change` carries the impact
-- block and the baseline it changes (D-T1-3). A `benefit` indexes its benefit_profile.
CREATE TABLE project_register_item (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    project_id      uuid NOT NULL,
    kind            text NOT NULL
                    CHECK (kind IN ('risk', 'issue', 'assumption', 'dependency', 'benefit',
                                    'change')),
    code            text NOT NULL DEFAULT '',
    title           text NOT NULL,
    body            text NOT NULL DEFAULT '',
    owner_job_id    uuid,
    status          text NOT NULL DEFAULT 'open',
    severity        text,
    probability     text,
    impact          int CHECK (impact IS NULL OR impact >= 0),
    links           jsonb NOT NULL DEFAULT '[]'::jsonb,
    baseline_ref    uuid,
    meta            jsonb NOT NULL DEFAULT '{}'::jsonb,
    version         bigint NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT project_register_item_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT project_register_item_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT project_register_item_owner_fk FOREIGN KEY (owner_job_id) REFERENCES job_catalogue (id),
    CONSTRAINT project_register_item_baseline_fk FOREIGN KEY (baseline_ref) REFERENCES project_baseline (id),
    CONSTRAINT project_register_item_title_not_blank CHECK (btrim(title) <> ''),
    -- A change is not a note: it names the baseline it changes and carries the impact block.
    CONSTRAINT project_register_item_change_impact CHECK (
        kind <> 'change'
        OR (baseline_ref IS NOT NULL AND meta ? 'impact' AND meta->'impact' <> 'null'::jsonb)
    )
);
CREATE INDEX project_register_item_project_idx
    ON project_register_item (tenant_id, project_id, kind, status, created_at DESC, id);
CREATE INDEX project_register_item_code_idx
    ON project_register_item (tenant_id, project_id, code) WHERE code <> '';

ALTER TABLE project_register_item ENABLE ROW LEVEL SECURITY;
ALTER TABLE project_register_item FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON project_register_item FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── project_dependency — edges, not prose ────────────────────────────────────────────────
-- The cycle check runs in the creating transaction (D-T1-6). The unique edge stops a
-- duplicate; the self-edge is refused here, and the deep cycle in the store's recursive CTE.
CREATE TABLE project_dependency (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    programme_id uuid,
    from_project uuid NOT NULL,
    to_project   uuid NOT NULL,
    kind        text NOT NULL CHECK (kind IN ('blocks', 'needs', 'supersedes', 'informs')),
    status      text NOT NULL DEFAULT 'open',
    meta        jsonb NOT NULL DEFAULT '{}'::jsonb,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT project_dependency_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT project_dependency_programme_fk FOREIGN KEY (programme_id) REFERENCES programme (id),
    CONSTRAINT project_dependency_from_fk FOREIGN KEY (from_project) REFERENCES project (id),
    CONSTRAINT project_dependency_to_fk FOREIGN KEY (to_project) REFERENCES project (id),
    CONSTRAINT project_dependency_not_self CHECK (from_project <> to_project),
    CONSTRAINT project_dependency_edge_key UNIQUE (tenant_id, from_project, to_project, kind)
);
CREATE INDEX project_dependency_from_idx ON project_dependency (tenant_id, from_project);
CREATE INDEX project_dependency_to_idx ON project_dependency (tenant_id, to_project);

ALTER TABLE project_dependency ENABLE ROW LEVEL SECURITY;
ALTER TABLE project_dependency FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON project_dependency FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── programme_cadence — T1 stores the row; T10 materialises its cards ────────────────────
-- The five kinds are §10.3's; only the row is T1's. No scheduler, no card materialisation.
CREATE TABLE programme_cadence (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    programme_id  uuid NOT NULL,
    kind          text NOT NULL
                  CHECK (kind IN ('pi', 'sprint', 'demo', 'retro', 'refinement')),
    period        text NOT NULL DEFAULT '',
    state         text NOT NULL DEFAULT 'planned',
    meta          jsonb NOT NULL DEFAULT '{}'::jsonb,
    version       bigint NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT programme_cadence_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT programme_cadence_programme_fk FOREIGN KEY (programme_id) REFERENCES programme (id),
    CONSTRAINT programme_cadence_period_not_blank CHECK (btrim(period) <> '')
);
CREATE INDEX programme_cadence_programme_idx
    ON programme_cadence (tenant_id, programme_id, kind, period);

ALTER TABLE programme_cadence ENABLE ROW LEVEL SECURITY;
ALTER TABLE programme_cadence FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON programme_cadence FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── programme_baseline — the management/governance/boundary trio ─────────────────────────
-- A tranche is managed against a version of these; the review never rewrites one.
CREATE TABLE programme_baseline (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    programme_id uuid NOT NULL,
    kind        text NOT NULL CHECK (kind IN ('management', 'governance', 'boundary')),
    snapshot    jsonb NOT NULL,
    taken_at    timestamptz NOT NULL DEFAULT now(),
    taken_by    uuid NOT NULL,
    approved_by uuid,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT programme_baseline_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT programme_baseline_programme_fk FOREIGN KEY (programme_id) REFERENCES programme (id),
    CONSTRAINT programme_baseline_taken_by_fk FOREIGN KEY (taken_by) REFERENCES app_user (id),
    CONSTRAINT programme_baseline_approved_by_fk FOREIGN KEY (approved_by) REFERENCES app_user (id)
);
CREATE INDEX programme_baseline_programme_idx
    ON programme_baseline (tenant_id, programme_id, kind, taken_at DESC);

ALTER TABLE programme_baseline ENABLE ROW LEVEL SECURITY;
ALTER TABLE programme_baseline FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON programme_baseline FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── programme_tranche — the unit the programme manages and reviews together ──────────────
-- The review is an approval (`tranches.review`) with baselines, benefit status and the risk
-- register as evidence; a re-baseline goes through change control, never the review.
CREATE TABLE programme_tranche (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    programme_id   uuid NOT NULL,
    number         int NOT NULL CHECK (number >= 0),
    state          text NOT NULL DEFAULT 'planned'
                   CHECK (state IN ('planned', 'authorised', 'in_review', 'closed', 'paused',
                                    'stopped')),
    entry_criteria jsonb NOT NULL DEFAULT '[]'::jsonb,
    exit_criteria  jsonb NOT NULL DEFAULT '[]'::jsonb,
    baseline_ref   uuid,
    review_at      timestamptz,
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT programme_tranche_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT programme_tranche_programme_fk FOREIGN KEY (programme_id) REFERENCES programme (id),
    CONSTRAINT programme_tranche_baseline_fk FOREIGN KEY (baseline_ref) REFERENCES programme_baseline (id),
    CONSTRAINT programme_tranche_number_key UNIQUE (tenant_id, programme_id, number)
);
CREATE INDEX programme_tranche_programme_idx
    ON programme_tranche (tenant_id, programme_id, number);

ALTER TABLE programme_tranche ENABLE ROW LEVEL SECURITY;
ALTER TABLE programme_tranche FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON programme_tranche FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── benefit_profile — the measurement record behind a benefit register row ───────────────
-- The six field groups of §8.3 are mandatory. A profile missing its baseline, target,
-- measurement method or owner is a wish, and is refused by the action (D-T1-9).
CREATE TABLE benefit_profile (
    id                  uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id           uuid NOT NULL DEFAULT app_current_tenant(),
    programme_id        uuid NOT NULL,
    project_id          uuid,
    register_item_id    uuid,
    reference           text NOT NULL,
    description         text NOT NULL DEFAULT '',
    category            text NOT NULL DEFAULT '',
    objectives          jsonb NOT NULL DEFAULT '[]'::jsonb,
    observable_outcome  text NOT NULL DEFAULT '',
    kpis                jsonb NOT NULL DEFAULT '[]'::jsonb,
    baseline            jsonb,
    target              jsonb,
    measurement_method  text NOT NULL DEFAULT '',
    financial_value     text NOT NULL DEFAULT '',
    cost                jsonb NOT NULL DEFAULT '{}'::jsonb,
    capabilities        jsonb NOT NULL DEFAULT '[]'::jsonb,
    preconditions       jsonb NOT NULL DEFAULT '[]'::jsonb,
    risks               jsonb NOT NULL DEFAULT '[]'::jsonb,
    owner_job_id        uuid,
    benefit_owner_job_id uuid,
    operational_area    text NOT NULL DEFAULT '',
    status              text NOT NULL DEFAULT 'draft'
                        CHECK (status IN ('draft', 'active', 'realized', 'disbenefit',
                                          'closed')),
    version             bigint NOT NULL DEFAULT 1,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT benefit_profile_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT benefit_profile_programme_fk FOREIGN KEY (programme_id) REFERENCES programme (id),
    CONSTRAINT benefit_profile_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT benefit_profile_register_fk FOREIGN KEY (register_item_id) REFERENCES project_register_item (id),
    CONSTRAINT benefit_profile_owner_fk FOREIGN KEY (owner_job_id) REFERENCES job_catalogue (id),
    CONSTRAINT benefit_profile_benefit_owner_fk FOREIGN KEY (benefit_owner_job_id) REFERENCES job_catalogue (id),
    CONSTRAINT benefit_profile_reference_key UNIQUE (tenant_id, programme_id, reference),
    CONSTRAINT benefit_profile_reference_not_blank CHECK (btrim(reference) <> '')
);
CREATE INDEX benefit_profile_programme_idx
    ON benefit_profile (tenant_id, programme_id, status, created_at DESC, id);
CREATE INDEX benefit_profile_project_idx
    ON benefit_profile (tenant_id, project_id) WHERE project_id IS NOT NULL;

ALTER TABLE benefit_profile ENABLE ROW LEVEL SECURITY;
ALTER TABLE benefit_profile FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON benefit_profile FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── benefit_link — the benefits-map edge, all five levels named ──────────────────────────
CREATE TABLE benefit_link (
    id                  uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id           uuid NOT NULL DEFAULT app_current_tenant(),
    programme_id        uuid NOT NULL,
    output_ref          text NOT NULL,
    capability_ref      text NOT NULL,
    operational_outcome text NOT NULL,
    benefit_id          uuid NOT NULL,
    strategic_objective text NOT NULL,
    kind                text NOT NULL DEFAULT 'benefit'
                        CHECK (kind IN ('benefit', 'disbenefit')),
    meta                jsonb NOT NULL DEFAULT '{}'::jsonb,
    version             bigint NOT NULL DEFAULT 1,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT benefit_link_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT benefit_link_programme_fk FOREIGN KEY (programme_id) REFERENCES programme (id),
    CONSTRAINT benefit_link_benefit_fk FOREIGN KEY (benefit_id) REFERENCES benefit_profile (id) ON DELETE CASCADE,
    CONSTRAINT benefit_link_five_levels CHECK (
        btrim(output_ref) <> '' AND btrim(capability_ref) <> ''
        AND btrim(operational_outcome) <> '' AND btrim(strategic_objective) <> ''
    )
);
CREATE INDEX benefit_link_programme_idx ON benefit_link (tenant_id, programme_id);
CREATE INDEX benefit_link_benefit_idx ON benefit_link (tenant_id, benefit_id);

ALTER TABLE benefit_link ENABLE ROW LEVEL SECURITY;
ALTER TABLE benefit_link FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON benefit_link FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── benefit_transition — the realization record per benefit ──────────────────────────────
-- The baseline is measured while the old practice is observable; the post-transition
-- measurement is what the sponsor compares. A baseline that was never measured cannot claim an
-- improvement (the action refuses it; the column records the fact).
CREATE TABLE benefit_transition (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    benefit_id         uuid NOT NULL,
    baseline_measured_at timestamptz,
    readiness          jsonb NOT NULL DEFAULT '{}'::jsonb,
    enacted_at         timestamptz,
    post_measured_at   timestamptz,
    retired_at         timestamptz,
    evidence           jsonb NOT NULL DEFAULT '[]'::jsonb,
    version            bigint NOT NULL DEFAULT 1,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT benefit_transition_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT benefit_transition_benefit_fk FOREIGN KEY (benefit_id) REFERENCES benefit_profile (id) ON DELETE CASCADE
);
CREATE INDEX benefit_transition_benefit_idx ON benefit_transition (tenant_id, benefit_id);

ALTER TABLE benefit_transition ENABLE ROW LEVEL SECURITY;
ALTER TABLE benefit_transition FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON benefit_transition FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── stakeholder_profile — the MSP profile, distinct from `project_stakeholder` ───────────
-- `project_stakeholder` (0008) answers "who is involved"; this answers "what do they care
-- about, how much influence do they have, and how is support moving" (§8.3).
CREATE TABLE stakeholder_profile (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    programme_id      uuid NOT NULL,
    project_id        uuid,
    name              text NOT NULL,
    concerns          text NOT NULL DEFAULT '',
    sensitivities     text NOT NULL DEFAULT '',
    importance        text NOT NULL DEFAULT '',
    influence         text NOT NULL DEFAULT '',
    interest          text NOT NULL DEFAULT '',
    exposure          text NOT NULL DEFAULT '',
    support_level     text NOT NULL DEFAULT '',
    support_trend     text NOT NULL DEFAULT '',
    engagement_target text NOT NULL DEFAULT '',
    benefits          jsonb NOT NULL DEFAULT '[]'::jsonb,
    disbenefits       jsonb NOT NULL DEFAULT '[]'::jsonb,
    version           bigint NOT NULL DEFAULT 1,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT stakeholder_profile_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT stakeholder_profile_programme_fk FOREIGN KEY (programme_id) REFERENCES programme (id),
    CONSTRAINT stakeholder_profile_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT stakeholder_profile_name_not_blank CHECK (btrim(name) <> '')
);
CREATE INDEX stakeholder_profile_programme_idx
    ON stakeholder_profile (tenant_id, programme_id, created_at DESC, id);

ALTER TABLE stakeholder_profile ENABLE ROW LEVEL SECURITY;
ALTER TABLE stakeholder_profile FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON stakeholder_profile FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── governed_asset — the MSP narrative artefacts are kinds, not new tables ───────────────
-- The lifecycle and the approval path are 0002's; T1 only widens the vocabulary (§10.3).
ALTER TABLE governed_asset DROP CONSTRAINT governed_asset_kind_check;
ALTER TABLE governed_asset ADD CONSTRAINT governed_asset_kind_check CHECK (
    kind IN ('requirement', 'principle', 'adr', 'charter', 'closure', 'postmortem',
             'sop_source', 'mandate', 'programme_brief', 'blueprint', 'business_case',
             'refined_vision', 'benefits_strategy', 'benefits_realization_plan',
             'projects_dossier')
);

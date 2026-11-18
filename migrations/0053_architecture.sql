-- 0053_architecture.sql — T9: the TOGAF ADM capability (SPEC_V2 §8.1, §10.3; issue #81).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- The capability is a set of typed rows, never a document library: an artefact is metadata plus
-- entries plus a document; a phase closes on a live contract; a compliance review records exactly
-- one level; a waiver names its accepting owner and reassessment date. Every table is tenant
-- scoped with ENABLE + FORCE RLS.
--
-- Vocabulary (lowercase machine codes; prose lives in the actions and the docs):
--   phase_code   preliminary | a | b | c | d | e | f | g | h
--   plateau      psa | hld | sd
--   artefact     catalog | matrix | diagram | principles_catalog
--   contract     statement_of_architecture_work | delivery_contract |
--                statement_of_intent | request_for_architecture
--   compliance   irrelevant | consistent | compliant | conformant | fully_conformant |
--                non_conformant

-- ── 1. artifact_type — the seeded artefact standard ─────────────────────────────────────
CREATE TABLE artifact_type (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    key           text NOT NULL,
    name          text NOT NULL,
    artifact_kind text NOT NULL
                  CHECK (artifact_kind IN ('catalog', 'matrix', 'diagram', 'principles_catalog')),
    phase_code    text NOT NULL
                  CHECK (phase_code IN ('preliminary','a','b','c','d','e','f','g','h')),
    plateau       text NOT NULL CHECK (plateau IN ('psa', 'hld', 'sd')),
    mandatory     boolean NOT NULL DEFAULT false,
    position      integer NOT NULL DEFAULT 0,
    version       bigint NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT artifact_type_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT artifact_type_key_not_blank CHECK (btrim(key) <> ''),
    CONSTRAINT artifact_type_name_not_blank CHECK (btrim(name) <> ''),
    CONSTRAINT artifact_type_key_unique UNIQUE (tenant_id, key, phase_code, plateau)
);
CREATE INDEX artifact_type_phase_idx ON artifact_type (tenant_id, phase_code, mandatory);
ALTER TABLE artifact_type ENABLE ROW LEVEL SECURITY;
ALTER TABLE artifact_type FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON artifact_type FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 2. adm_profile — the tailored ADM ──────────────────────────────────────────────────
CREATE TABLE adm_profile (
    id                    uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id             uuid NOT NULL DEFAULT app_current_tenant(),
    key                   text NOT NULL,
    name                  text NOT NULL,
    phase_set             jsonb NOT NULL DEFAULT '[]'::jsonb,
    artifact_requirements jsonb NOT NULL DEFAULT '{}'::jsonb,
    contract_kinds        jsonb NOT NULL DEFAULT '[]'::jsonb,
    status                text NOT NULL DEFAULT 'draft'
                          CHECK (status IN ('draft', 'adopted', 'retired')),
    adopted_at            timestamptz,
    board_decision_id     uuid,
    version               bigint NOT NULL DEFAULT 1,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT adm_profile_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT adm_profile_key_not_blank CHECK (btrim(key) <> ''),
    CONSTRAINT adm_profile_key_unique UNIQUE (tenant_id, key)
);
CREATE INDEX adm_profile_status_idx ON adm_profile (tenant_id, status);
ALTER TABLE adm_profile ENABLE ROW LEVEL SECURITY;
ALTER TABLE adm_profile FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON adm_profile FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 3. governance_body — the Architecture Board and other forums ───────────────────────
CREATE TABLE governance_body (
    id                      uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id               uuid NOT NULL DEFAULT app_current_tenant(),
    kind                    text NOT NULL
                            CHECK (kind IN ('architecture_board', 'design_board', 'other')),
    key                     text NOT NULL,
    name                    text NOT NULL,
    charter                 text NOT NULL DEFAULT '',
    commissioning_authority text NOT NULL DEFAULT '',
    decision_limits         jsonb NOT NULL DEFAULT '{}'::jsonb,
    members                 jsonb NOT NULL DEFAULT '[]'::jsonb,
    status                  text NOT NULL DEFAULT 'draft'
                            CHECK (status IN ('draft', 'active', 'retired')),
    version                 bigint NOT NULL DEFAULT 1,
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT governance_body_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT governance_body_key_not_blank CHECK (btrim(key) <> ''),
    CONSTRAINT governance_body_key_unique UNIQUE (tenant_id, key)
);
CREATE INDEX governance_body_kind_idx ON governance_body (tenant_id, kind, status);
ALTER TABLE governance_body ENABLE ROW LEVEL SECURITY;
ALTER TABLE governance_body FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON governance_body FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 4. architecture_contract — the four gate objects ───────────────────────────────────
CREATE TABLE architecture_contract (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    project_id    uuid NOT NULL,
    kind          text NOT NULL CHECK (kind IN (
                      'statement_of_architecture_work', 'delivery_contract',
                      'statement_of_intent', 'request_for_architecture')),
    phase_code    text NOT NULL
                  CHECK (phase_code IN ('preliminary','a','b','c','d','e','f','g','h')),
    title         text NOT NULL,
    body          jsonb NOT NULL DEFAULT '{}'::jsonb,
    status        text NOT NULL DEFAULT 'draft'
                  CHECK (status IN ('draft', 'live', 'superseded')),
    approval_id   uuid,
    approved_by   uuid,
    approved_at   timestamptz,
    supersedes_id uuid,
    superseded_by_id uuid,
    version       bigint NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT architecture_contract_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT architecture_contract_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT architecture_contract_approval_fk FOREIGN KEY (approval_id) REFERENCES approval (id),
    CONSTRAINT architecture_contract_approved_by_fk FOREIGN KEY (approved_by) REFERENCES app_user (id),
    CONSTRAINT architecture_contract_supersedes_fk FOREIGN KEY (supersedes_id) REFERENCES architecture_contract (id),
    CONSTRAINT architecture_contract_superseded_by_fk FOREIGN KEY (superseded_by_id) REFERENCES architecture_contract (id),
    CONSTRAINT architecture_contract_title_not_blank CHECK (btrim(title) <> '')
);
CREATE INDEX architecture_contract_project_idx
    ON architecture_contract (tenant_id, project_id, kind, phase_code);
CREATE UNIQUE INDEX architecture_contract_live_idx
    ON architecture_contract (tenant_id, project_id, kind, phase_code)
    WHERE status = 'live';
ALTER TABLE architecture_contract ENABLE ROW LEVEL SECURITY;
ALTER TABLE architecture_contract FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON architecture_contract FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 5. architecture_artifact — the typed artefact ──────────────────────────────────────
CREATE TABLE architecture_artifact (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    project_id        uuid,
    artifact_type_id  uuid,
    phase_code        text NOT NULL
                      CHECK (phase_code IN ('preliminary','a','b','c','d','e','f','g','h')),
    plateau           text NOT NULL CHECK (plateau IN ('psa', 'hld', 'sd')),
    artifact_kind     text NOT NULL
                      CHECK (artifact_kind IN ('catalog', 'matrix', 'diagram', 'principles_catalog')),
    title             text NOT NULL,
    status            text NOT NULL DEFAULT 'draft'
                      CHECK (status IN ('draft', 'in_review', 'approved', 'superseded', 'archived')),
    artifact_version  bigint NOT NULL DEFAULT 1,
    author_id         uuid,
    authored_at       timestamptz,
    reviewer_id       uuid,
    reviewed_at       timestamptz,
    change_initiative text NOT NULL DEFAULT '',
    purpose           text NOT NULL DEFAULT '',
    stakeholders      jsonb NOT NULL DEFAULT '[]'::jsonb,
    scope             text NOT NULL DEFAULT '',
    document_id       uuid,
    contract_id       uuid,
    supersedes_id     uuid,
    superseded_by_id  uuid,
    meta              jsonb NOT NULL DEFAULT '{}'::jsonb,
    version           bigint NOT NULL DEFAULT 1,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT architecture_artifact_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT architecture_artifact_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT architecture_artifact_type_fk FOREIGN KEY (artifact_type_id) REFERENCES artifact_type (id),
    CONSTRAINT architecture_artifact_author_fk FOREIGN KEY (author_id) REFERENCES app_user (id),
    CONSTRAINT architecture_artifact_reviewer_fk FOREIGN KEY (reviewer_id) REFERENCES app_user (id),
    CONSTRAINT architecture_artifact_document_fk FOREIGN KEY (document_id) REFERENCES document (id),
    CONSTRAINT architecture_artifact_contract_fk FOREIGN KEY (contract_id) REFERENCES architecture_contract (id),
    CONSTRAINT architecture_artifact_supersedes_fk FOREIGN KEY (supersedes_id) REFERENCES architecture_artifact (id),
    CONSTRAINT architecture_artifact_superseded_by_fk FOREIGN KEY (superseded_by_id) REFERENCES architecture_artifact (id),
    CONSTRAINT architecture_artifact_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT architecture_artifact_not_own_superseder CHECK (supersedes_id IS DISTINCT FROM id)
);
CREATE INDEX architecture_artifact_project_idx
    ON architecture_artifact (tenant_id, project_id, phase_code, plateau);
CREATE INDEX architecture_artifact_status_idx
    ON architecture_artifact (tenant_id, status);
ALTER TABLE architecture_artifact ENABLE ROW LEVEL SECURITY;
ALTER TABLE architecture_artifact FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON architecture_artifact FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 6. architecture_artifact_entry — catalog items and matrix cells ────────────────────
CREATE TABLE architecture_artifact_entry (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    artifact_id uuid NOT NULL,
    axis_a      text NOT NULL,
    axis_b      text NOT NULL DEFAULT '',
    value       jsonb NOT NULL DEFAULT '{}'::jsonb,
    position    integer NOT NULL DEFAULT 0,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT architecture_artifact_entry_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT architecture_artifact_entry_artifact_fk FOREIGN KEY (artifact_id) REFERENCES architecture_artifact (id) ON DELETE CASCADE,
    CONSTRAINT architecture_artifact_entry_axis_a_not_blank CHECK (btrim(axis_a) <> ''),
    CONSTRAINT architecture_artifact_entry_unique UNIQUE (tenant_id, artifact_id, axis_a, axis_b)
);
CREATE INDEX architecture_artifact_entry_artifact_idx
    ON architecture_artifact_entry (tenant_id, artifact_id, position);
ALTER TABLE architecture_artifact_entry ENABLE ROW LEVEL SECURITY;
ALTER TABLE architecture_artifact_entry FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON architecture_artifact_entry FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 7. architecture_artifact_link — derivation / realization / reuse / dependency ──────
CREATE TABLE architecture_artifact_link (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    from_artifact_id  uuid NOT NULL,
    to_artifact_id    uuid NOT NULL,
    kind              text NOT NULL
                      CHECK (kind IN ('derivation', 'realization', 'reuse', 'dependency')),
    version           bigint NOT NULL DEFAULT 1,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT architecture_artifact_link_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT architecture_artifact_link_from_fk FOREIGN KEY (from_artifact_id) REFERENCES architecture_artifact (id) ON DELETE CASCADE,
    CONSTRAINT architecture_artifact_link_to_fk FOREIGN KEY (to_artifact_id) REFERENCES architecture_artifact (id) ON DELETE CASCADE,
    CONSTRAINT architecture_artifact_link_not_self CHECK (from_artifact_id IS DISTINCT FROM to_artifact_id),
    CONSTRAINT architecture_artifact_link_unique UNIQUE (tenant_id, from_artifact_id, to_artifact_id, kind)
);
CREATE INDEX architecture_artifact_link_to_idx
    ON architecture_artifact_link (tenant_id, to_artifact_id, kind);
ALTER TABLE architecture_artifact_link ENABLE ROW LEVEL SECURITY;
ALTER TABLE architecture_artifact_link FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON architecture_artifact_link FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 8. architecture_compliance_review — one typed level per delivery ───────────────────
CREATE TABLE architecture_compliance_review (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    project_id  uuid NOT NULL,
    contract_id uuid NOT NULL,
    level       text NOT NULL CHECK (level IN (
                    'irrelevant', 'consistent', 'compliant', 'conformant',
                    'fully_conformant', 'non_conformant')),
    findings    jsonb NOT NULL DEFAULT '[]'::jsonb,
    evidence    jsonb NOT NULL DEFAULT '[]'::jsonb,
    outcome     jsonb,
    reviewed_by uuid NOT NULL,
    reviewed_at timestamptz NOT NULL DEFAULT now(),
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT architecture_compliance_review_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT architecture_compliance_review_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT architecture_compliance_review_contract_fk FOREIGN KEY (contract_id) REFERENCES architecture_contract (id),
    CONSTRAINT architecture_compliance_review_reviewer_fk FOREIGN KEY (reviewed_by) REFERENCES app_user (id),
    CONSTRAINT architecture_compliance_review_outcome CHECK (
        outcome IS NULL
        OR (jsonb_typeof(outcome) = 'object' AND outcome->>'kind' IN ('change', 'kata'))),
    CONSTRAINT architecture_compliance_review_one UNIQUE (tenant_id, project_id, contract_id)
);
CREATE INDEX architecture_compliance_review_project_idx
    ON architecture_compliance_review (tenant_id, project_id, level);
ALTER TABLE architecture_compliance_review ENABLE ROW LEVEL SECURITY;
ALTER TABLE architecture_compliance_review FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON architecture_compliance_review FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 9. architecture_waiver — a time-boxed accepted deviation ───────────────────────────
CREATE TABLE architecture_waiver (
    id               uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id        uuid NOT NULL DEFAULT app_current_tenant(),
    review_id        uuid NOT NULL,
    owner_user_id    uuid NOT NULL,
    reason           text NOT NULL,
    cost_impact      text NOT NULL DEFAULT '',
    risk_impact      text NOT NULL DEFAULT '',
    reassessment_at  timestamptz NOT NULL,
    status           text NOT NULL DEFAULT 'active'
                     CHECK (status IN ('active', 'expired', 'closed')),
    version          bigint NOT NULL DEFAULT 1,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT architecture_waiver_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT architecture_waiver_review_fk FOREIGN KEY (review_id) REFERENCES architecture_compliance_review (id) ON DELETE CASCADE,
    CONSTRAINT architecture_waiver_owner_fk FOREIGN KEY (owner_user_id) REFERENCES app_user (id),
    CONSTRAINT architecture_waiver_reason_not_blank CHECK (btrim(reason) <> '')
);
CREATE INDEX architecture_waiver_review_idx
    ON architecture_waiver (tenant_id, review_id, status);
ALTER TABLE architecture_waiver ENABLE ROW LEVEL SECURITY;
ALTER TABLE architecture_waiver FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON architecture_waiver FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 10. architecture_maturity — nine dimensions, 0–5, dated ────────────────────────────
CREATE TABLE architecture_maturity (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    assessment_id uuid NOT NULL,
    stage         text NOT NULL,
    dimension     text NOT NULL CHECK (dimension IN (
                      'architecture_process', 'architecture_development', 'business_linkage',
                      'senior_management', 'operating_unit_participation',
                      'architecture_communication', 'it_security', 'architecture_governance',
                      'investment_strategy')),
    score         integer NOT NULL CHECK (score BETWEEN 0 AND 5),
    assessed_at   timestamptz NOT NULL DEFAULT now(),
    evidence      jsonb NOT NULL DEFAULT '[]'::jsonb,
    version       bigint NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT architecture_maturity_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT architecture_maturity_stage_not_blank CHECK (btrim(stage) <> ''),
    CONSTRAINT architecture_maturity_one_per_dimension UNIQUE (tenant_id, assessment_id, dimension)
);
CREATE INDEX architecture_maturity_assessment_idx
    ON architecture_maturity (tenant_id, assessment_id, dimension);
ALTER TABLE architecture_maturity ENABLE ROW LEVEL SECURITY;
ALTER TABLE architecture_maturity FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON architecture_maturity FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 11. architecture_skill_assessment — seven categories per role ──────────────────────
CREATE TABLE architecture_skill_assessment (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    assessment_id uuid NOT NULL,
    role          text NOT NULL,
    job_key       text NOT NULL DEFAULT '',
    category      text NOT NULL CHECK (category IN (
                      'general', 'business', 'architecture', 'programme_management',
                      'it_knowledge', 'technical', 'legal')),
    proficiency   integer NOT NULL CHECK (proficiency BETWEEN 0 AND 5),
    assessed_at   timestamptz NOT NULL DEFAULT now(),
    evidence      jsonb NOT NULL DEFAULT '[]'::jsonb,
    version       bigint NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT architecture_skill_assessment_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT architecture_skill_assessment_role_not_blank CHECK (btrim(role) <> ''),
    CONSTRAINT architecture_skill_assessment_one_per_category UNIQUE (tenant_id, assessment_id, role, category)
);
CREATE INDEX architecture_skill_assessment_role_idx
    ON architecture_skill_assessment (tenant_id, role, category);
ALTER TABLE architecture_skill_assessment ENABLE ROW LEVEL SECURITY;
ALTER TABLE architecture_skill_assessment FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON architecture_skill_assessment FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

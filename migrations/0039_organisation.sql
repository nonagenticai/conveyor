-- 0039_organisation.sql — T0: the organisation spine (SPEC_V2 §4.1–§4.6, §10.3, §12 T0;
-- GitHub issue #34).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- Vocabulary, deliberately:
--   * `job` (0001_platform.sql:120) is V1's durable WORK QUEUE. `job_catalogue` is the POSITION —
--     the conveyor's own job table (SPEC_V2 §10.3, "not V1's durable `job` queue"). They are two
--     concepts with two stores; neither is renamed into the other.
--   * `procedure` (0035_process_tree.sql:65) is the SOP a step is judged against. The executable
--     sequence was renamed `process` by 0035. A binding therefore pins `process_id`; the SPEC's
--     `procedure_id` means this executable `process` in code.
--   * `executor_kind` is an OPEN slug with no value CHECK. The code registry is the authority
--     (crates/domain/src/executor.rs), so registering a kind never needs a migration and the
--     test-only third kind T0 requires is expressible. A two-value CHECK would forbid it.
--   * `job_release` is append-only: publishing again writes a NEW row. Only `status`, `version`
--     and `updated_at` are updatable by the runtime role (scripts/grants-runtime.sql), so the
--     frozen `release_hash`/`release_snapshot` cannot be rewritten by an application bug.

-- ── industry ───────────────────────────────────────────────────────────────────────────────
-- The top of the organisation spine (§4.1): one tenant may hold several industries. The first
-- three rows are seeded from the design fixtures (seed/organisation.json, P4).
CREATE TABLE industry (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    key        text NOT NULL,
    name       text NOT NULL,
    status     text NOT NULL DEFAULT 'active'
               CHECK (status IN ('active', 'retired')),
    meta       jsonb NOT NULL DEFAULT '{}'::jsonb,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT industry_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT industry_tenant_key_key UNIQUE (tenant_id, key),
    CONSTRAINT industry_key_format CHECK (key ~ '^[a-z][a-z0-9_-]{1,63}$'),
    CONSTRAINT industry_name_not_blank CHECK (btrim(name) <> '')
);

ALTER TABLE industry ENABLE ROW LEVEL SECURITY;
ALTER TABLE industry FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON industry FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── department gains its place in the spine ───────────────────────────────────────────────
-- One department concept, not a fork: the existing table (tenant, name, parent_id) grows the
-- industry edge and the order inside it. Both columns are nullable so the rows that already
-- exist (`membership.department_id` points at them) stay valid; the seeder fills the real set.
ALTER TABLE department
    ADD COLUMN industry_id uuid,
    ADD COLUMN ordinal     int,
    ADD COLUMN is_primary  boolean NOT NULL DEFAULT false;

ALTER TABLE department
    ADD CONSTRAINT department_industry_fk FOREIGN KEY (industry_id) REFERENCES industry (id),
    ADD CONSTRAINT department_ordinal_check CHECK (ordinal IS NULL OR ordinal >= 1);

-- The ordered set inside an industry: no two departments share a slot.
CREATE UNIQUE INDEX uq_department_industry_ordinal
    ON department (tenant_id, industry_id, ordinal)
    WHERE industry_id IS NOT NULL AND ordinal IS NOT NULL;
-- Exactly one primary department per industry.
CREATE UNIQUE INDEX uq_department_industry_primary
    ON department (tenant_id, industry_id)
    WHERE is_primary AND industry_id IS NOT NULL;

-- ── job_catalogue — the position ──────────────────────────────────────────────────────────
-- The editable draft anatomy lives in `definition` (§4.2); `job_releases.publish` compiles it
-- into the immutable release snapshot. A catalogue row is never executed directly: a run names
-- a binding, and a binding pins a release.
CREATE TABLE job_catalogue (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    job_key     text NOT NULL,
    industry_id uuid NOT NULL,
    family      text NOT NULL DEFAULT '',
    title       text NOT NULL,
    status      text NOT NULL DEFAULT 'draft'
                CHECK (status IN ('draft', 'active', 'retired')),
    definition  jsonb NOT NULL DEFAULT '{}'::jsonb,
    meta        jsonb NOT NULL DEFAULT '{}'::jsonb,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT job_catalogue_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT job_catalogue_industry_fk FOREIGN KEY (industry_id) REFERENCES industry (id),
    CONSTRAINT job_catalogue_tenant_key_key UNIQUE (tenant_id, job_key),
    CONSTRAINT job_catalogue_key_format CHECK (job_key ~ '^[a-z][a-z0-9_]{1,63}$'),
    CONSTRAINT job_catalogue_title_not_blank CHECK (btrim(title) <> '')
);
CREATE INDEX job_catalogue_industry_idx ON job_catalogue (tenant_id, industry_id, status, job_key);

ALTER TABLE job_catalogue ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_catalogue FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON job_catalogue FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── job_department — the ordered placement ────────────────────────────────────────────────
-- A job lives in one or more departments of its industry, ordered from 1, exactly one primary
-- (§4.1; P1.3). The ordinals' contiguity is the writer's contract (job_catalogue.create/update);
-- the database enforces uniqueness and the single primary.
CREATE TABLE job_department (
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    job_id        uuid NOT NULL,
    department_id uuid NOT NULL,
    ordinal       int NOT NULL CHECK (ordinal >= 1),
    is_primary    boolean NOT NULL DEFAULT false,
    created_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT job_department_pkey PRIMARY KEY (job_id, department_id),
    CONSTRAINT job_department_ordinal_key UNIQUE (job_id, ordinal),
    CONSTRAINT job_department_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT job_department_job_fk FOREIGN KEY (job_id) REFERENCES job_catalogue (id) ON DELETE CASCADE,
    CONSTRAINT job_department_department_fk FOREIGN KEY (department_id) REFERENCES department (id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX uq_job_department_primary
    ON job_department (job_id) WHERE is_primary;

ALTER TABLE job_department ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_department FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON job_department FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── job_release — the immutable compiled release ──────────────────────────────────────────
-- `release_hash` is SHA-256 over the canonical snapshot (explicit recursive key sort,
-- crates/domain/src/organisation.rs). The same job + pins compiled twice produce byte-identical
-- snapshots and the same hash (P2.1). A change is a new row, never an edit.
CREATE TABLE job_release (
    id               uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id        uuid NOT NULL DEFAULT app_current_tenant(),
    job_id           uuid NOT NULL,
    release_hash     text NOT NULL,
    release_snapshot jsonb NOT NULL,
    status           text NOT NULL DEFAULT 'published'
                     CHECK (status IN ('published', 'retired')),
    version          bigint NOT NULL DEFAULT 1,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT job_release_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT job_release_job_fk FOREIGN KEY (job_id) REFERENCES job_catalogue (id),
    CONSTRAINT job_release_tenant_job_hash_key UNIQUE (tenant_id, job_id, release_hash),
    CONSTRAINT job_release_hash_shape CHECK (release_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT job_release_snapshot_is_object CHECK (jsonb_typeof(release_snapshot) = 'object')
);
CREATE INDEX job_release_job_idx ON job_release (tenant_id, job_id, created_at DESC, id);

ALTER TABLE job_release ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_release FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON job_release FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── job_binding — position × release × executor ──────────────────────────────────────────
-- Pins ONE immutable release (§4.4): the hash and the frozen snapshot are copied here, like a
-- lockfile, and never refreshed in place. A new release is a new binding. The binding names its
-- executor and the configuration that executor consumes:
--   * executor_kind = 'procedure'      → `process_id` (the executable sequence, 0035's `process`);
--   * executor_kind = 'internal_agent' → `capability_agent_id` (0020's agent configuration).
-- A binding with neither is refused at creation (422) and by the CHECK below.
CREATE TABLE job_binding (
    id                  uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id           uuid NOT NULL DEFAULT app_current_tenant(),
    job_id              uuid NOT NULL,
    release_id          uuid NOT NULL,
    release_hash        text NOT NULL,
    release_snapshot    jsonb NOT NULL,
    capability_id       uuid,
    process_id          uuid,
    capability_agent_id uuid,
    executor_kind       text NOT NULL,
    status              text NOT NULL DEFAULT 'draft'
                        CHECK (status IN ('draft', 'active', 'retired')),
    meta                jsonb NOT NULL DEFAULT '{}'::jsonb,
    version             bigint NOT NULL DEFAULT 1,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT job_binding_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT job_binding_job_fk FOREIGN KEY (job_id) REFERENCES job_catalogue (id),
    CONSTRAINT job_binding_release_fk FOREIGN KEY (release_id) REFERENCES job_release (id),
    CONSTRAINT job_binding_capability_fk FOREIGN KEY (capability_id) REFERENCES capability (id),
    CONSTRAINT job_binding_process_fk FOREIGN KEY (process_id) REFERENCES process (id),
    CONSTRAINT job_binding_agent_fk FOREIGN KEY (capability_agent_id) REFERENCES capability_agent (id),
    CONSTRAINT job_binding_executor_kind_format CHECK (executor_kind ~ '^[a-z][a-z0-9_]{1,31}$'),
    CONSTRAINT job_binding_release_hash_shape CHECK (release_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT job_binding_snapshot_is_object CHECK (jsonb_typeof(release_snapshot) = 'object'),
    CONSTRAINT job_binding_has_executor CHECK (process_id IS NOT NULL OR capability_agent_id IS NOT NULL)
);
CREATE INDEX job_binding_job_idx ON job_binding (tenant_id, job_id, status, created_at DESC);

ALTER TABLE job_binding ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_binding FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON job_binding FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── the pin travels to the card and the run ──────────────────────────────────────────────
-- §4.4: a card names a binding, not a free-text role. §10.3 lists `card.job_binding_id`.
-- The run records the binding AND the resolved release it actually used; the stale check reads
-- these rows, never a caller-declared hash.
ALTER TABLE card
    ADD COLUMN job_binding_id uuid,
    ADD CONSTRAINT card_job_binding_fk FOREIGN KEY (job_binding_id) REFERENCES job_binding (id);

ALTER TABLE run
    ADD COLUMN job_binding_id uuid,
    ADD COLUMN release_hash   text;

ALTER TABLE run
    ADD CONSTRAINT run_job_binding_fk FOREIGN KEY (job_binding_id) REFERENCES job_binding (id),
    ADD CONSTRAINT run_release_hash_shape
        CHECK (release_hash IS NULL OR release_hash ~ '^[0-9a-f]{64}$');

CREATE INDEX run_job_binding_idx ON run (tenant_id, job_binding_id)
    WHERE job_binding_id IS NOT NULL;

-- ── the decision ladder (schema and contracts only; T2 owns behaviour) ───────────────────
-- SPEC_V2 §10.3 and §12 T0: the five tables exist, and no evaluation path does. T2 adds the
-- laya client, policy evaluation and the generation split.

CREATE TABLE laya_inference_config (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    model_ref  text NOT NULL,
    params     jsonb NOT NULL DEFAULT '{}'::jsonb,
    timeout_ms int NOT NULL DEFAULT 30000 CHECK (timeout_ms BETWEEN 1 AND 600000),
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT laya_inference_config_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT laya_inference_config_model_not_blank CHECK (btrim(model_ref) <> '')
);

ALTER TABLE laya_inference_config ENABLE ROW LEVEL SECURITY;
ALTER TABLE laya_inference_config FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON laya_inference_config FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- Thresholds, fail behaviour and the action a decision kind takes. `fail_policy`/`action` are
-- open vocabulary at T0 — T2 fixes the evaluator's spellings.
CREATE TABLE decision_policy (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    kind        text NOT NULL,
    gate        text,
    threshold   numeric,
    fail_policy text NOT NULL DEFAULT 'ask',
    action      text NOT NULL DEFAULT 'ask',
    meta        jsonb NOT NULL DEFAULT '{}'::jsonb,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT decision_policy_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT decision_policy_kind_not_blank CHECK (btrim(kind) <> '')
);

ALTER TABLE decision_policy ENABLE ROW LEVEL SECURITY;
ALTER TABLE decision_policy FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON decision_policy FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- The tenant's per-case override of a policy: a threshold/rule change without a code change.
CREATE TABLE decision_policy_refinement (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    policy_id          uuid NOT NULL,
    threshold_override numeric,
    rule_override      jsonb NOT NULL DEFAULT '{}'::jsonb,
    fail_action        text,
    laya_config_id     uuid,
    version            bigint NOT NULL DEFAULT 1,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT decision_policy_refinement_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT decision_policy_refinement_policy_fk FOREIGN KEY (policy_id) REFERENCES decision_policy (id) ON DELETE CASCADE,
    CONSTRAINT decision_policy_refinement_laya_fk FOREIGN KEY (laya_config_id) REFERENCES laya_inference_config (id)
);

ALTER TABLE decision_policy_refinement ENABLE ROW LEVEL SECURITY;
ALTER TABLE decision_policy_refinement FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON decision_policy_refinement FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- One L1 decision: the question, its inputs' hash, the probabilities/confidence it produced and
-- the policy + refinement + laya config versions it pinned (§10.3, P4 in T2).
CREATE TABLE decision (
    id                   uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id            uuid NOT NULL DEFAULT app_current_tenant(),
    kind                 text NOT NULL,
    project_id           uuid,
    card_id              uuid,
    question             text NOT NULL DEFAULT '',
    input_hash           text,
    probabilities        jsonb NOT NULL DEFAULT '{}'::jsonb,
    confidence           numeric,
    policy_id            uuid,
    policy_refinement_id uuid,
    laya_config_id       uuid,
    latency_ms           int,
    outcome              text,
    decided_at           timestamptz,
    meta                 jsonb NOT NULL DEFAULT '{}'::jsonb,
    version              bigint NOT NULL DEFAULT 1,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT decision_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT decision_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT decision_card_fk FOREIGN KEY (card_id) REFERENCES card (id),
    CONSTRAINT decision_policy_fk FOREIGN KEY (policy_id) REFERENCES decision_policy (id),
    CONSTRAINT decision_policy_refinement_fk FOREIGN KEY (policy_refinement_id) REFERENCES decision_policy_refinement (id),
    CONSTRAINT decision_laya_fk FOREIGN KEY (laya_config_id) REFERENCES laya_inference_config (id),
    CONSTRAINT decision_kind_not_blank CHECK (btrim(kind) <> ''),
    CONSTRAINT decision_input_hash_shape CHECK (input_hash IS NULL OR input_hash ~ '^[0-9a-f]{64}$')
);
CREATE INDEX decision_card_idx ON decision (tenant_id, card_id, created_at DESC)
    WHERE card_id IS NOT NULL;
CREATE INDEX decision_project_idx ON decision (tenant_id, project_id, created_at DESC)
    WHERE project_id IS NOT NULL;

ALTER TABLE decision ENABLE ROW LEVEL SECURITY;
ALTER TABLE decision FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON decision FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- The authored options a choice/score question may pick between, versioned (§10.3).
CREATE TABLE decision_option (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    decision_id uuid NOT NULL,
    key         text NOT NULL,
    label       text NOT NULL DEFAULT '',
    payload     jsonb NOT NULL DEFAULT '{}'::jsonb,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT decision_option_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT decision_option_decision_fk FOREIGN KEY (decision_id) REFERENCES decision (id) ON DELETE CASCADE,
    CONSTRAINT decision_option_decision_key_key UNIQUE (decision_id, key),
    CONSTRAINT decision_option_key_not_blank CHECK (btrim(key) <> '')
);

ALTER TABLE decision_option ENABLE ROW LEVEL SECURITY;
ALTER TABLE decision_option FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON decision_option FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

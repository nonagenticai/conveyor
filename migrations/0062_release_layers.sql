-- 0062_release_layers.sql — the layered kind store a release compiles from (SPEC_V2 §4.1;
-- finding v1-w1a-2, conveyor#526).
--
-- A release compiles **base → industry → department archetype → each department in declared
-- order → job**. A layer declares a kind with an optional value and a `required` flag; `required`
-- can only be tightened downward. The refusals the compile owes (`409 release_not_ready` for a
-- required kind with no value, `422 department_conflict` for sibling departments at different
-- values) are decided by crates/store/src/organisation.rs from these rows — this migration stores
-- the declarations only, never a compiled result.
--
-- Deliberately not a knowledge store: domain knowledge bytes and embeddings stay in strata_rag
-- (SPEC_V2 §4.1, §10.1), `capability_knowledge` keeps the capability pack pins, and this table
-- holds the layer/kind configuration the release records in its snapshot. The job layer's values
-- are the keys of `job_catalogue.definition`; an explicit `job` row here overrides one, so the
-- anatomy is not duplicated as a second source of truth.
--
-- `department.archetype` names the archetype layer a placed department inherits. Archetype rows
-- are keyed by `(industry, archetype)`, so two departments of the same archetype share them and
-- the archetype is declared once per industry.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

ALTER TABLE department
    ADD COLUMN archetype text;

ALTER TABLE department
    ADD CONSTRAINT department_archetype_format
    CHECK (archetype IS NULL OR archetype ~ '^[a-z][a-z0-9_]{1,63}$');

-- One declaration of one kind at one layer. `value` is NULL when the layer requires the kind
-- without setting it — exactly the state that compiles to `release_not_ready` when no later
-- layer resolves it.
CREATE TABLE organisation_layer_setting (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    layer         text NOT NULL
                  CHECK (layer IN ('base', 'industry', 'department_archetype', 'department', 'job')),
    industry_id   uuid,
    department_id uuid,
    job_id        uuid,
    archetype     text,
    kind          text NOT NULL,
    value         jsonb,
    required      boolean NOT NULL DEFAULT false,
    meta          jsonb NOT NULL DEFAULT '{}'::jsonb,
    version       bigint NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT organisation_layer_setting_tenant_fk
        FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT organisation_layer_setting_industry_fk
        FOREIGN KEY (industry_id) REFERENCES industry (id),
    CONSTRAINT organisation_layer_setting_department_fk
        FOREIGN KEY (department_id) REFERENCES department (id),
    CONSTRAINT organisation_layer_setting_job_fk
        FOREIGN KEY (job_id) REFERENCES job_catalogue (id),
    CONSTRAINT organisation_layer_setting_kind_not_blank CHECK (btrim(kind) <> ''),
    CONSTRAINT organisation_layer_setting_archetype_format
        CHECK (archetype IS NULL OR archetype ~ '^[a-z][a-z0-9_]{1,63}$'),
    -- Exactly one scope is set, and it is the one the layer names. The FK alone would accept a
    -- department id on a base row; the compile would then silently ignore it.
    CONSTRAINT organisation_layer_setting_scope CHECK (
        (layer = 'base'
            AND industry_id IS NULL AND department_id IS NULL AND job_id IS NULL
            AND archetype IS NULL)
        OR (layer = 'industry'
            AND industry_id IS NOT NULL AND department_id IS NULL AND job_id IS NULL
            AND archetype IS NULL)
        OR (layer = 'department_archetype'
            AND industry_id IS NOT NULL AND department_id IS NULL AND job_id IS NULL
            AND archetype IS NOT NULL)
        OR (layer = 'department'
            AND industry_id IS NULL AND department_id IS NOT NULL AND job_id IS NULL
            AND archetype IS NULL)
        OR (layer = 'job'
            AND industry_id IS NULL AND department_id IS NULL AND job_id IS NOT NULL
            AND archetype IS NULL)
    )
);

-- One declaration per kind at one layer: partial unique indexes rather than a NULLS NOT
-- DISTINCT key, so the scope column each index names is the only one it depends on.
CREATE UNIQUE INDEX uq_organisation_layer_setting_base
    ON organisation_layer_setting (tenant_id, kind)
    WHERE layer = 'base';
CREATE UNIQUE INDEX uq_organisation_layer_setting_industry
    ON organisation_layer_setting (tenant_id, industry_id, kind)
    WHERE layer = 'industry';
CREATE UNIQUE INDEX uq_organisation_layer_setting_archetype
    ON organisation_layer_setting (tenant_id, industry_id, archetype, kind)
    WHERE layer = 'department_archetype';
CREATE UNIQUE INDEX uq_organisation_layer_setting_department
    ON organisation_layer_setting (tenant_id, department_id, kind)
    WHERE layer = 'department';
CREATE UNIQUE INDEX uq_organisation_layer_setting_job
    ON organisation_layer_setting (tenant_id, job_id, kind)
    WHERE layer = 'job';

ALTER TABLE organisation_layer_setting ENABLE ROW LEVEL SECURITY;
ALTER TABLE organisation_layer_setting FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON organisation_layer_setting FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- The service role reads the layer settings through the action path; without these grants the
-- table exists for the owner (tests) and is invisible to the runtime (live).
GRANT USAGE ON SCHEMA public TO conveyor_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO conveyor_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO conveyor_runtime;

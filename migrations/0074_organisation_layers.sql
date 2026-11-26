-- 0072_organisation_layers.sql — T0's release knowledge layers (SPEC_V2 §4.1; issue #697).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- Knowledge is layered base → industry → department archetype → each department in declared
-- order → job (SPEC_V2 §4.1). One row per (tenant, level, scope, kind). A row either supplies a
-- value, or declares a kind required without one — and usually both, a value that is required.
-- `required` tightens downward only: the resolved flag is the OR over the job's layer path.
--
--   level                  scope
--   base                   none
--   industry               industry_id
--   department_archetype   department_id of the archetype (a department's parent)
--   department             department_id
--   job                    job_id
--
-- Exactly one scope column is non-null and it matches the level. Two sibling departments in one
-- job's placement that set one kind to different values are refused by the publish
-- (`422 department_conflict`); a required kind with no value is `409 release_not_ready`.

CREATE TABLE organisation_layer (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    level         text NOT NULL
                  CHECK (level IN ('base', 'industry', 'department_archetype', 'department', 'job')),
    industry_id   uuid,
    department_id uuid,
    job_id        uuid,
    kind          text NOT NULL
                  CONSTRAINT organisation_layer_kind_format
                  CHECK (kind ~ '^[a-z][a-z0-9_.-]{0,63}$'),
    value         jsonb,
    required      boolean NOT NULL DEFAULT false,
    version       bigint NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT organisation_layer_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT organisation_layer_industry_fk FOREIGN KEY (industry_id)
        REFERENCES industry (id) ON DELETE CASCADE,
    CONSTRAINT organisation_layer_department_fk FOREIGN KEY (department_id)
        REFERENCES department (id) ON DELETE CASCADE,
    CONSTRAINT organisation_layer_job_fk FOREIGN KEY (job_id)
        REFERENCES job_catalogue (id) ON DELETE CASCADE,
    CONSTRAINT organisation_layer_scope CHECK (
        (level = 'base' AND industry_id IS NULL AND department_id IS NULL AND job_id IS NULL)
        OR (level = 'industry' AND industry_id IS NOT NULL AND department_id IS NULL AND job_id IS NULL)
        OR (level = 'department_archetype'
            AND industry_id IS NULL AND department_id IS NOT NULL AND job_id IS NULL)
        OR (level = 'department' AND industry_id IS NULL AND department_id IS NOT NULL AND job_id IS NULL)
        OR (level = 'job' AND industry_id IS NULL AND department_id IS NULL AND job_id IS NOT NULL)
    ),
    -- A row that neither supplies a value nor declares a requirement says nothing.
    CONSTRAINT organisation_layer_declares CHECK (value IS NOT NULL OR required)
);

-- One live row per (tenant, level, scope, kind). `NULLS NOT DISTINCT` is what makes the all-null
-- `base` scope collide with itself instead of every base row being "different".
CREATE UNIQUE INDEX uq_organisation_layer ON organisation_layer
    (tenant_id, level, industry_id, department_id, job_id, kind) NULLS NOT DISTINCT;
CREATE INDEX ix_organisation_layer_industry ON organisation_layer (tenant_id, industry_id)
    WHERE level = 'industry';
CREATE INDEX ix_organisation_layer_department ON organisation_layer (tenant_id, department_id)
    WHERE level IN ('department_archetype', 'department');
CREATE INDEX ix_organisation_layer_job ON organisation_layer (tenant_id, job_id)
    WHERE level = 'job';

ALTER TABLE organisation_layer ENABLE ROW LEVEL SECURITY;
ALTER TABLE organisation_layer FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON organisation_layer FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

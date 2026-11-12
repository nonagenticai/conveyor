-- 0003_action_registry.sql — the one S04 table (02-DATA-ARCHITECTURE §2.14), verbatim,
-- plus the created_at/updated_at columns CONTRACT §2 requires of a tenant-owned table.
--
-- Rows exist ONLY for kind='external' (and, from S13, kind='workflow'): internal actions
-- are compile-time `inventory` entries and are not stored. `approved_at` is the enable /
-- disable switch (S4-D3); the highest approved spec_version per name serves, and an
-- ingest writes an unapproved row alongside it.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

CREATE TABLE action_registry (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    name         text NOT NULL,
    spec_version int NOT NULL DEFAULT 1,
    kind         text NOT NULL DEFAULT 'external'
                 CHECK (kind IN ('external', 'workflow')),
    service      text NOT NULL,
    spec         jsonb NOT NULL DEFAULT '{}'::jsonb,
    schema_hash  text NOT NULL,
    timeout_ms   int NOT NULL DEFAULT 30000,
    idempotent   boolean NOT NULL DEFAULT false,
    approved_at  timestamptz,
    approved_by  uuid,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT action_registry_tenant_name_version_key
        UNIQUE (tenant_id, name, spec_version),
    CONSTRAINT action_registry_approved_by_fk FOREIGN KEY (approved_by) REFERENCES app_user (id)
);
CREATE INDEX action_registry_tenant_service_name_idx
    ON action_registry (tenant_id, service, name);

ALTER TABLE action_registry ENABLE ROW LEVEL SECURITY;
ALTER TABLE action_registry FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON action_registry FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- The runtime role is granted schema-wide by scripts/grants-runtime.sql after every
-- migration; these are stated here as well so a database migrated by another path is
-- correct once (the same order rule as 0001: GRANT first, then any REVOKE).
GRANT SELECT, INSERT, UPDATE, DELETE ON action_registry TO conveyor_runtime;

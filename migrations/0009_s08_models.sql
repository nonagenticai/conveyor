-- 0009_s08_models.sql — the seven S08 Part B model tables.
--
-- Column-for-column from 02-DATA-ARCHITECTURE §2.8. Forward only: never edited after this
-- slice closes (CONTRACT §2).
--
-- ArchiMate and UML share ONE shape; `model.language` is the discriminator and the profile
-- decides the rules, never the schema (S08-D1). The seven tables absorb RM's
-- `architecture_*`, `modeling_*` and `uml_*` families:
--
--   model                  one header per model. `head_revision` is the last revision's seq,
--                          advanced at publish/import, not per edit (S08 Part B decision 3:
--                          `revision.content` is capped at 256 KiB, and a full snapshot per
--                          `view_objects.create` would be O(N²)).
--   model_element          absorbs architecture_elements + uml_elements.
--   model_relationship     absorbs architecture_relationships + uml_relationships +
--                          uml_interactions.
--   model_view             absorbs architecture_views + uml_diagrams.
--   view_object            absorbs architecture_view_objects + uml_diagram_objects.
--   view_connection        absorbs architecture_view_connections + uml_diagram_connections.
--   model_validation_run   absorbs modeling_validation_runs; findings folded into jsonb.
--
-- Two tables deliberately carry NO foreign key on an endpoint:
--   * `model_relationship.source_id` / `target_id`
--   * `view_connection.source_object_id` / `target_object_id`
-- §2.8 lists only the owning FK for each. That is not an omission: a dangling endpoint is
-- precisely what `models.validate` reports (`dangling_relationship`), and an FK would make
-- the check unreachable. The store still resolves every endpoint inside the caller's tenant
-- in the same transaction (CONTRACT §2 — a single-column FK never enforces tenancy), and
-- deleting an element or object removes the rows that name it.
--
-- `model`, `model_element`, `model_relationship`, `model_view`, `view_object` and
-- `view_connection` are mutable aggregates (create/update/delete, `If-Match`), so each
-- carries `version bigint NOT NULL DEFAULT 1`, `created_at` and `updated_at`.
-- `model_validation_run` is an append-only fact: `created_at` only, no `version`.

-- ── model ───────────────────────────────────────────────────────────────────────────────
-- `project_id` is nullable: a model can exist before it is attached to a project (an
-- architecture sketched in a workshop). `description` defaults to '' so the row is a valid
-- record from the first insert. `published_revision_id` points at the revision `models.publish`
-- froze; `head_revision` is that revision's `seq`, so `0` means "never published".
CREATE TABLE model (
    id                    uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id             uuid NOT NULL DEFAULT app_current_tenant(),
    project_id            uuid,
    language              text NOT NULL,
    language_version      text NOT NULL,
    title                 text NOT NULL,
    description           text NOT NULL DEFAULT '',
    head_revision         int NOT NULL DEFAULT 0,
    published_revision_id uuid,
    meta                  jsonb NOT NULL DEFAULT '{}'::jsonb,
    version               bigint NOT NULL DEFAULT 1,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT model_language_check CHECK (language IN ('archimate', 'uml')),
    CONSTRAINT model_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT model_published_revision_fk
        FOREIGN KEY (published_revision_id) REFERENCES revision (id)
);
CREATE INDEX model_project_idx ON model (tenant_id, project_id);
-- The only ordered read is "this tenant's models, newest first", which is also the keyset
-- the cursor uses.
CREATE INDEX model_updated_idx ON model (tenant_id, updated_at DESC, id);

-- ── model_element ───────────────────────────────────────────────────────────────────────
-- `kind` is free text here and validated against the model's profile by the store and by
-- `models.validate`: a CHECK would freeze a vocabulary the profile owns versioned
-- (`conveyor-archimate-3.2-v1`). `parent_element_id` is the containment tree, not a second
-- relationship table.
CREATE TABLE model_element (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    model_id          uuid NOT NULL,
    kind              text NOT NULL,
    name              text NOT NULL,
    documentation     text NOT NULL DEFAULT '',
    parent_element_id uuid,
    meta              jsonb NOT NULL DEFAULT '{}'::jsonb,
    version           bigint NOT NULL DEFAULT 1,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT model_element_model_fk FOREIGN KEY (model_id) REFERENCES model (id),
    CONSTRAINT model_element_parent_fk
        FOREIGN KEY (parent_element_id) REFERENCES model_element (id)
);
CREATE INDEX model_element_kind_idx ON model_element (tenant_id, model_id, kind);
-- The layout reads a model's elements in a stable order; `(position, id)` is what
-- `model_views.generate` and the renderer both band on.
CREATE INDEX model_element_model_idx ON model_element (tenant_id, model_id, id);

-- ── model_relationship ──────────────────────────────────────────────────────────────────
-- `check(source_id <> target_id)` forbids the one relationship ArchiMate cannot express: a
-- thing related to itself. Endpoints are resolved by the store (see the header).
CREATE TABLE model_relationship (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    model_id   uuid NOT NULL,
    kind       text NOT NULL,
    source_id  uuid NOT NULL,
    target_id  uuid NOT NULL,
    meta       jsonb NOT NULL DEFAULT '{}'::jsonb,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT model_relationship_distinct_check CHECK (source_id <> target_id),
    CONSTRAINT model_relationship_model_fk FOREIGN KEY (model_id) REFERENCES model (id)
);
CREATE INDEX model_relationship_kind_idx ON model_relationship (tenant_id, model_id, kind);
CREATE INDEX model_relationship_source_idx ON model_relationship (source_id);
CREATE INDEX model_relationship_target_idx ON model_relationship (target_id);

-- ── model_view ──────────────────────────────────────────────────────────────────────────
-- `kind` is the diagram family. It does NOT decide the renderer — every view renders through
-- the same SVG path — it exists so a UI can group views without reading `meta`.
CREATE TABLE model_view (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    model_id   uuid NOT NULL,
    kind       text NOT NULL DEFAULT 'diagram',
    name       text NOT NULL,
    meta       jsonb NOT NULL DEFAULT '{}'::jsonb,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT model_view_kind_check
        CHECK (kind IN ('diagram', 'archimate_view', 'uml_diagram')),
    CONSTRAINT model_view_model_fk FOREIGN KEY (model_id) REFERENCES model (id)
);
CREATE INDEX model_view_model_idx ON model_view (tenant_id, model_id);

-- ── view_object ─────────────────────────────────────────────────────────────────────────
-- `bounds` and `style` are JSONB and are read whole with the view (§5): nothing may filter
-- on `bounds->>…`. Hit-testing, culling and overlap checks parse the rect in Rust (trap 3);
-- a measured hot predicate turns `bounds` into four generated numeric columns.
CREATE TABLE view_object (
    id               uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id        uuid NOT NULL DEFAULT app_current_tenant(),
    view_id          uuid NOT NULL,
    element_id       uuid,
    parent_object_id uuid,
    kind             text NOT NULL DEFAULT 'element',
    bounds           jsonb NOT NULL DEFAULT '{}'::jsonb,
    style            jsonb NOT NULL DEFAULT '{}'::jsonb,
    label            text,
    position         numeric NOT NULL DEFAULT 0,
    version          bigint NOT NULL DEFAULT 1,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT view_object_view_fk FOREIGN KEY (view_id) REFERENCES model_view (id),
    CONSTRAINT view_object_element_fk FOREIGN KEY (element_id) REFERENCES model_element (id),
    CONSTRAINT view_object_parent_fk FOREIGN KEY (parent_object_id) REFERENCES view_object (id)
);
CREATE INDEX view_object_view_idx ON view_object (tenant_id, view_id, position, id);

-- ── view_connection ─────────────────────────────────────────────────────────────────────
-- `relationship_id` is nullable: a view may draw a connector between two objects before the
-- model's relationship exists. `bendpoints` is the route, read whole with the view.
CREATE TABLE view_connection (
    id               uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id        uuid NOT NULL DEFAULT app_current_tenant(),
    view_id          uuid NOT NULL,
    relationship_id  uuid,
    source_object_id uuid NOT NULL,
    target_object_id uuid NOT NULL,
    bendpoints       jsonb NOT NULL DEFAULT '[]'::jsonb,
    style            jsonb NOT NULL DEFAULT '{}'::jsonb,
    version          bigint NOT NULL DEFAULT 1,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT view_connection_view_fk FOREIGN KEY (view_id) REFERENCES model_view (id),
    CONSTRAINT view_connection_relationship_fk
        FOREIGN KEY (relationship_id) REFERENCES model_relationship (id)
);
CREATE INDEX view_connection_view_idx ON view_connection (tenant_id, view_id);

-- ── model_validation_run ────────────────────────────────────────────────────────────────
-- One row per `model.validate` job. `findings` is the whole report (`[{code, severity,
-- element_id?, relationship_id?, pointer, message}]`); a findings table would be a second
-- shape for a document that is always read whole with its run. `content_hash` is the hash of
-- the graph the run actually saw, which is also the job's dedupe key, so a second run for an
-- unchanged graph cannot be created.
CREATE TABLE model_validation_run (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    model_id        uuid NOT NULL,
    revision_id     uuid,
    ruleset         text NOT NULL,
    ruleset_version text NOT NULL,
    status          text NOT NULL DEFAULT 'ok',
    findings        jsonb NOT NULL DEFAULT '[]'::jsonb,
    content_hash    text NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT model_validation_run_status_check
        CHECK (status IN ('ok', 'findings', 'error')),
    CONSTRAINT model_validation_run_model_fk FOREIGN KEY (model_id) REFERENCES model (id),
    CONSTRAINT model_validation_run_revision_fk
        FOREIGN KEY (revision_id) REFERENCES revision (id)
);
CREATE INDEX model_validation_run_model_idx
    ON model_validation_run (tenant_id, model_id, created_at DESC);

-- ── Row level security ──────────────────────────────────────────────────────────────────
-- Seven tables, no exception. One FOR ALL policy each with USING (reads) AND WITH CHECK
-- (writes): a policy with only USING lets a forged tenant_id write escape. FORCE is not
-- optional — without it the schema owner reads every tenant and the isolation gate would
-- pass while production leaked (CONTRACT §2).
ALTER TABLE model                  ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_element          ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_relationship     ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_view             ENABLE ROW LEVEL SECURITY;
ALTER TABLE view_object            ENABLE ROW LEVEL SECURITY;
ALTER TABLE view_connection        ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_validation_run   ENABLE ROW LEVEL SECURITY;

ALTER TABLE model                  FORCE ROW LEVEL SECURITY;
ALTER TABLE model_element          FORCE ROW LEVEL SECURITY;
ALTER TABLE model_relationship     FORCE ROW LEVEL SECURITY;
ALTER TABLE model_view             FORCE ROW LEVEL SECURITY;
ALTER TABLE view_object            FORCE ROW LEVEL SECURITY;
ALTER TABLE view_connection        FORCE ROW LEVEL SECURITY;
ALTER TABLE model_validation_run   FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON model FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON model_element FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON model_relationship FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON model_view FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON view_object FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON view_connection FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON model_validation_run FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── Runtime grants ───────────────────────────────────────────────────────────────────────
-- Makes the seven new tables visible to `conveyor_runtime` (a schema-wide GRANT) and revokes
-- the migration ledger. No append-only revokes: `model_validation_run` has no UPDATE/DELETE
-- action in this slice, but `scripts/grants-runtime.sql` is the file that owns append-only
-- policy — if a later slice relies on its immutability, add the revoke there, not here.
GRANT USAGE ON SCHEMA public TO conveyor_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO conveyor_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO conveyor_runtime;
REVOKE ALL ON _sqlx_migrations FROM conveyor_runtime;

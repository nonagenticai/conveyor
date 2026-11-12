-- 0008_s08_portfolio.sql — the four S08 Part A portfolio tables.
--
-- Column-for-column from 02-DATA-ARCHITECTURE §2.9. Forward only: never edited after this
-- slice closes (CONTRACT §2).
--
-- Part A reuses S03's `programme`, `governed_asset`, `catalogue_entry`, `revision` and
-- `trace_link`, and S07's `outline` / `outline_node` (the TOC actions write those). The only
-- tables it needs of its own are these four:
--
--   project_stakeholder   absorbs fr project_stakeholders.
--   project_capability    absorbs RM pm_project_capabilities. `capability_code` is validated
--                         against `catalogue_entry` by the store, never by an FK: the
--                         catalogue row is tenant-scoped and a single-column FK would not
--                         enforce tenancy (CONTRACT §2).
--   deliverable           absorbs fr project_deliverables + project_milestones and RM's two
--                         kind catalogues; one table with `kind_code` (S08-D9).
--   register_item         absorbs RM pm_resources + pm_risks + pm_benefits — one shape with
--                         `kind` (S08-D9). `project_id` is nullable because S08-D9's register
--                         is tenant-wide as well as per project.
--
-- All four are mutable aggregates (every one has create/update/delete in S08 §5.2 inputs
-- `{id, …}` with `If-Match`), so each carries `version bigint NOT NULL DEFAULT 1`,
-- `created_at` and `updated_at` (CONTRACT §2 and §3). None is append-only, so 0008 performs
-- no extra revoke — unlike 0007's `quality_evaluation`, which loses UPDATE and DELETE in
-- `scripts/grants-runtime.sql`.
--
-- The `…_code` and `status` vocabularies are free text here: §2.9 fixes only
-- `register_item.kind` with a CHECK, and every other value is resolved against the tenant's
-- own catalogue by the store. A CHECK on a catalogue-backed code would freeze a vocabulary
-- the tenant is allowed to extend.

-- ── project_stakeholder ──────────────────────────────────────────────────────────────────
-- `user_id` is nullable: a stakeholder is often recorded before they have an `app_user` (an
-- external client contact). `name`/`role` default to '' so the row is a valid record from
-- the first insert, and `email` is nullable because not every stakeholder has one.
CREATE TABLE project_stakeholder (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    project_id uuid NOT NULL,
    user_id    uuid,
    name       text NOT NULL DEFAULT '',
    email      text,
    role       text NOT NULL DEFAULT '',
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT project_stakeholder_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT project_stakeholder_user_fk FOREIGN KEY (user_id) REFERENCES app_user (id)
);
CREATE INDEX project_stakeholder_project_idx
    ON project_stakeholder (tenant_id, project_id);

-- ── project_capability ──────────────────────────────────────────────────────────────────
-- One row per (project, capability): the unique key is what makes "covered twice" impossible
-- without a second table. `phase_code` names the roadmap phase the capability lands in;
-- `meta` is read whole with the row and never filtered.
CREATE TABLE project_capability (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    project_id      uuid NOT NULL,
    capability_code text NOT NULL,
    status          text NOT NULL DEFAULT 'planned',
    phase_code      text,
    meta            jsonb NOT NULL DEFAULT '{}'::jsonb,
    version         bigint NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT project_capability_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT project_capability_tenant_project_capability_key
        UNIQUE (tenant_id, project_id, capability_code)
);

-- ── deliverable ─────────────────────────────────────────────────────────────────────────
-- Deliverables AND milestones — one table, `kind_code` discriminates (S08-D9). The index
-- carries `due_at` because the only ordered read is "this project's deliverables by due
-- date" (and the overdue scan behind it).
CREATE TABLE deliverable (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    project_id uuid NOT NULL,
    kind_code  text NOT NULL,
    title      text NOT NULL,
    status     text NOT NULL DEFAULT 'planned',
    due_at     timestamptz,
    meta       jsonb NOT NULL DEFAULT '{}'::jsonb,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT deliverable_project_fk FOREIGN KEY (project_id) REFERENCES project (id)
);
CREATE INDEX deliverable_project_due_idx
    ON deliverable (tenant_id, project_id, due_at);

-- ── register_item ───────────────────────────────────────────────────────────────────────
-- The one register shape (S08-D9): `kind` is the vocabulary and is checked; everything
-- kind-specific lives in `meta`, because a resource, a risk and a benefit do not share
-- columns beyond these. `severity` is a column rather than a `meta` key because the risk
-- list filters and orders on it. `project_id` is nullable — a tenant-wide register.
CREATE TABLE register_item (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    project_id    uuid,
    kind          text NOT NULL,
    title         text NOT NULL,
    status        text NOT NULL DEFAULT '',
    owner_user_id uuid,
    severity      text,
    meta          jsonb NOT NULL DEFAULT '{}'::jsonb,
    version       bigint NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT register_item_kind_check CHECK (kind IN ('resource', 'risk', 'benefit')),
    CONSTRAINT register_item_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT register_item_owner_fk FOREIGN KEY (owner_user_id) REFERENCES app_user (id)
);
CREATE INDEX register_item_kind_project_idx
    ON register_item (tenant_id, kind, project_id);

-- ── Row level security ──────────────────────────────────────────────────────────────────
-- Four tables, no exception. One FOR ALL policy each with USING (reads) AND WITH CHECK
-- (writes): a policy with only USING lets a forged tenant_id write escape. FORCE is not
-- optional — without it the schema owner reads every tenant and the isolation gate would
-- pass while production leaked (CONTRACT §2).
ALTER TABLE project_stakeholder ENABLE ROW LEVEL SECURITY;
ALTER TABLE project_capability  ENABLE ROW LEVEL SECURITY;
ALTER TABLE deliverable         ENABLE ROW LEVEL SECURITY;
ALTER TABLE register_item       ENABLE ROW LEVEL SECURITY;

ALTER TABLE project_stakeholder FORCE ROW LEVEL SECURITY;
ALTER TABLE project_capability  FORCE ROW LEVEL SECURITY;
ALTER TABLE deliverable         FORCE ROW LEVEL SECURITY;
ALTER TABLE register_item       FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON project_stakeholder FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON project_capability FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON deliverable FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON register_item FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── Runtime grants ───────────────────────────────────────────────────────────────────────
-- What THIS block does: makes the four new tables visible to `conveyor_runtime` (a
-- schema-wide GRANT) and revokes the migration ledger. It performs NO append-only revokes —
-- all four tables are mutable, so `scripts/grants-runtime.sql` (re-applied by `migrate`
-- after the LAST migration) leaves their UPDATE and DELETE grants in place.
GRANT USAGE ON SCHEMA public TO conveyor_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO conveyor_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO conveyor_runtime;
REVOKE ALL ON _sqlx_migrations FROM conveyor_runtime;

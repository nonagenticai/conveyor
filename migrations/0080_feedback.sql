-- 0079_feedback.sql — feedback about the platform, submitted from inside a tenant's UI.
--
-- One tenant-scoped row per submission: a bug, a feature request, or other. `author_id` is the
-- app_user who submitted it; `route` records where they were, so the operator has the context a
-- screenshot would have carried. `status` starts `open`; the operator's triage moves it and
-- records who decided and when.
--
-- Tenant isolation is the same FORCE RLS every tenant aggregate carries. The operator's
-- cross-tenant read is the admin CLI (`conveyor admin feedback list`), which opens one
-- tenant-scoped transaction per tenant — the `seed_programme(pool, None)` pattern. It is
-- deliberately NOT a `Tenancy::Platform` action: the agent surface refuses that tenancy for a
-- tenant-bound caller (`crates/actions/src/agent_surface.rs`), and the operator's authority is
-- the global `platform_admin` grant, which gates the CLI and the super-admin routes.
CREATE TABLE feedback (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    author_id   uuid NOT NULL,
    kind        text NOT NULL,
    title       text NOT NULL,
    body        text NOT NULL,
    route       text,
    status      text NOT NULL DEFAULT 'open',
    triaged_by  uuid,
    triaged_at  timestamptz,
    note        text,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT feedback_author_fk FOREIGN KEY (author_id) REFERENCES app_user (id),
    CONSTRAINT feedback_triaged_by_fk FOREIGN KEY (triaged_by) REFERENCES app_user (id),
    CONSTRAINT feedback_kind_check CHECK (kind IN ('bug', 'feature', 'other')),
    CONSTRAINT feedback_status_check
        CHECK (status IN ('open', 'triaged', 'accepted', 'declined', 'duplicate')),
    CONSTRAINT feedback_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT feedback_body_not_blank CHECK (btrim(body) <> ''),
    CONSTRAINT feedback_route_len CHECK (route IS NULL OR char_length(route) <= 512),
    CONSTRAINT feedback_triage_shape CHECK (
        (status = 'open' AND triaged_by IS NULL AND triaged_at IS NULL)
        OR (status <> 'open' AND triaged_by IS NOT NULL AND triaged_at IS NOT NULL)
    )
);

-- The tenant's page is newest-first; the operator's triage list filters by status.
CREATE INDEX feedback_tenant_created_idx ON feedback (tenant_id, created_at DESC);
CREATE INDEX feedback_tenant_status_idx ON feedback (tenant_id, status, created_at DESC);

ALTER TABLE feedback ENABLE ROW LEVEL SECURITY;
ALTER TABLE feedback FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON feedback FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

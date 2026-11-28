-- 0095_job_authority.sql — the tenant's meta-role × action authority matrix (#526, SPEC_V2 §6.7).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- The design decisions this schema carries:
--
--   * **Absent row = compiled default.** §6.7 states the matrix as defaults and then says "The
--     matrix is data (`tenant_role_scope` and the binding's `authority`), not branches in code."
--     This table stores only the cells a tenant changed; the compiled §6.7 cell lives in
--     `store::job_authority::compiled_default`. The migration seeds NO rows, because a seeded
--     default would turn "the tenant has not decided" into "the tenant granted", and the finding
--     (ledger v1-w1b-5) is exactly that an unconfigured binding must not inherit authority.
--   * **The binding narrows; it never grants.** `job_binding.meta.authority`, when it is a string
--     or an array, is a second allowlist checked before this row (SPEC_V2 §4.4). A binding can
--     therefore refuse an action this table allows; it can never allow one this table refuses.
--   * **The tenant is the isolation boundary.** Same RLS shape as `tenant_role_scope` (0028): the
--     tenant policy is the only read path, and FORCE makes it apply to the migration owner too.
--
-- Grants are NOT repeated here: `apps/server`'s `migrate` re-applies `scripts/grants-runtime.sql`
-- after the last migration, and that file grants the whole schema before it revokes the
-- append-only exceptions (0001's comment states the rule).

CREATE TABLE job_authority (
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    -- The meta-job role: `pm`, `coach`, `kata`, `delivery`, `ops` (SPEC_V2 §6.7's columns).
    meta_role  text NOT NULL,
    -- The action's own name (`coach.remedy`), the same spelling the registry and the gate carry.
    action     text NOT NULL,
    -- `true` = this tenant granted it; `false` = this tenant revoked the compiled default.
    allowed    boolean NOT NULL,
    -- The caller's principal string — an audit fact, not an FK; it survives a principal rebind.
    updated_by text,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT job_authority_pkey PRIMARY KEY (tenant_id, meta_role, action),
    CONSTRAINT job_authority_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT job_authority_meta_role_not_blank CHECK (btrim(meta_role) <> ''),
    CONSTRAINT job_authority_action_not_blank CHECK (btrim(action) <> ''),
    -- A role is a slug like `job_catalogue.job_key`; an action is the registry's dotted name
    -- like `approval_policy.action` (0050). Both formats match the existing conventions.
    CONSTRAINT job_authority_meta_role_format CHECK (meta_role ~ '^[a-z][a-z0-9_]{1,63}$'),
    CONSTRAINT job_authority_action_format CHECK (action ~ '^[a-z][a-z0-9_.-]{0,63}$')
);

ALTER TABLE job_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON job_authority FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

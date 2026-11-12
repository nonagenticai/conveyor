-- 0028_rbac_provisioning.sql — the tenant role policy, invites, and platform admins; `owner` retires.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- The design decisions this schema carries:
--
--   * **Absent row = compiled default.** `tenant_role_scope` stores only the differences a
--     tenant-admin chose. The compiled table stays in `actions::spec::Scope::granted_to`, so an
--     untouched tenant has zero rows and the migration is empty for every existing tenant. A
--     stored row is read once per request and carried on the session; there is no cache to
--     invalidate and no per-action query.
--   * **The kernel cannot be revoked.** `me`, `tenants`, `actions`, `events` and `members` are
--     refused by `roles.policy_set` in Rust — otherwise a tenant could revoke its way out of the
--     screen that restores it. The database does not know which scopes are kernel; that rule is
--     the action's, stated once.
--   * **`owner` is gone.** There is no owner user role: the tenant owns itself and `admin` is the
--     tenant-admin (George 2026-11-09). Existing rows are rewritten rather than left to fail the
--     new CHECK.
--   * **An invite is the only sign-up path.** A verified email with no `app_user` is admitted only
--     when a PENDING invite names it; the invite is then accepted and its role becomes the
--     membership row. The `invite_self_read` policy is what lets login find its own invites
--     before any tenant is known — the same shape as `membership_self_read`.
--   * **`platform_admin` is global and has no RLS.** It is not tenant data; it names the people
--     who may provision tenants and rescue a tenant that lost every admin. Keyed to `app_user.id`
--     so the email-keyed identity rebind keeps the grant across a recreated Keycloak account.
--
-- Grants are NOT repeated here: `apps/server`'s `migrate` re-applies `scripts/grants-runtime.sql`
-- after the last migration (0001's comment states the rule).

-- ── the tenant's role policy ────────────────────────────────────────────────────────────────
CREATE TABLE tenant_role_scope (
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    role       text NOT NULL
               CHECK (role IN ('admin', 'member', 'viewer', 'kata_executor')),
    scope      text NOT NULL,
    allowed    boolean NOT NULL,
    version    bigint NOT NULL DEFAULT 1,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tenant_role_scope_pkey PRIMARY KEY (tenant_id, role, scope),
    CONSTRAINT tenant_role_scope_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE
);

ALTER TABLE tenant_role_scope ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_role_scope FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tenant_role_scope FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── invites ─────────────────────────────────────────────────────────────────────────────────
-- `email` is stored as the admin typed it; every comparison is `lower(email)` so an invite is
-- found whatever the casing of the sign-in claim.
CREATE TABLE invite (
    id               uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id        uuid NOT NULL DEFAULT app_current_tenant(),
    email            text NOT NULL,
    role             text NOT NULL CHECK (role IN ('admin', 'member', 'viewer')),
    status           text NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending', 'accepted', 'revoked')),
    invited_by       uuid,
    created_at       timestamptz NOT NULL DEFAULT now(),
    accepted_at      timestamptz,
    accepted_user_id uuid,
    version          bigint NOT NULL DEFAULT 1,
    CONSTRAINT invite_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id),
    CONSTRAINT invite_invited_by_fk FOREIGN KEY (invited_by) REFERENCES app_user (id),
    CONSTRAINT invite_accepted_user_fk FOREIGN KEY (accepted_user_id) REFERENCES app_user (id),
    -- A decision names a moment and a person; a pending invite names neither.
    CONSTRAINT invite_accepted_pair CHECK ((status = 'accepted') = (accepted_at IS NOT NULL)),
    CONSTRAINT invite_accepted_user_pair CHECK ((status = 'accepted') = (accepted_user_id IS NOT NULL))
);

-- One live invite per email per tenant: re-inviting is the same invitation, not a second one.
CREATE UNIQUE INDEX invite_pending_key ON invite (tenant_id, lower(email))
    WHERE status = 'pending';
CREATE INDEX invite_pending_email_idx ON invite (lower(email)) WHERE status = 'pending';
CREATE INDEX invite_tenant_status_idx ON invite (tenant_id, status);

ALTER TABLE invite ENABLE ROW LEVEL SECURITY;
ALTER TABLE invite FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON invite FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- Login resolves an invited person before any tenant is known: the verified email is set as a
-- transaction-local GUC (store::identity) and only that email's PENDING invites are readable.
-- SELECT-only, scoped to the runtime role, and it never widens a read of any other table.
CREATE FUNCTION app_current_email() RETURNS text
    LANGUAGE sql STABLE PARALLEL SAFE
    AS $$ SELECT nullif(current_setting('app.current_email', true), '') $$;

CREATE POLICY invite_self_read ON invite FOR SELECT TO conveyor_runtime
    USING (status = 'pending' AND lower(email) = app_current_email());

-- ── platform admins ─────────────────────────────────────────────────────────────────────────
-- The one global grant. No tenant_id and no RLS: it is a platform fact, not tenant data.
CREATE TABLE platform_admin (
    user_id    uuid PRIMARY KEY,
    granted_by uuid,
    granted_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT platform_admin_user_fk FOREIGN KEY (user_id) REFERENCES app_user (id)
        ON DELETE CASCADE,
    CONSTRAINT platform_admin_granted_by_fk FOREIGN KEY (granted_by) REFERENCES app_user (id)
);

-- ── owner retires ───────────────────────────────────────────────────────────────────────────
-- The tenant owns itself; `admin` is the tenant-admin. Rewrite before the CHECK is replaced so
-- an existing database upgrades rather than refusing.
--
-- ⚠️ RLS is lifted for the rewrite, and that is not a shortcut: `membership` is FORCE RLS, so
-- the migration owner (`conveyor_migrate`) is subject to `tenant_isolation` too, and with
-- `app.current_tenant` unset the UPDATE below would match ZERO rows SILENTLY — leaving the
-- `owner` rows for the CHECK to refuse. Measured on the dev database 2026-11-09. The window is
-- inside the migration's transaction; FORCE is restored immediately after.
ALTER TABLE membership DISABLE ROW LEVEL SECURITY;
ALTER TABLE membership NO FORCE ROW LEVEL SECURITY;
UPDATE membership SET role = 'admin' WHERE role = 'owner';
ALTER TABLE membership ENABLE ROW LEVEL SECURITY;
ALTER TABLE membership FORCE ROW LEVEL SECURITY;

ALTER TABLE membership DROP CONSTRAINT membership_role_check;
ALTER TABLE membership ADD CONSTRAINT membership_role_check
    CHECK (role IN ('admin', 'member', 'viewer', 'kata_executor'));

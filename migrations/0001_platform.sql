-- 0001_platform.sql — the eleven S02 platform/tenancy tables, forced RLS, and the runtime
-- grants. Forward only: never edited after this slice closes (CONTRACT §2).
--
-- Runs as `conveyor_migrate`, the schema owner (created by scripts/db-roles.sql with a
-- superuser). Every tenant-owned table is ENABLE + FORCE row level security: without FORCE
-- the owner reads every tenant's rows, and an isolation test running as the owner would
-- pass while production leaked.
--
-- The application's tenant scope is a TRANSACTION-LOCAL GUC set by
-- store::begin_scoped (`SELECT set_config('app.current_tenant', $1, true)`). A session-level
-- SET survives COMMIT and would be inherited by the next client of that pgbouncer server
-- connection — measured on the estate, so it is never used here.

CREATE FUNCTION app_current_tenant() RETURNS uuid
    LANGUAGE sql STABLE PARALLEL SAFE
    AS $$ SELECT nullif(current_setting('app.current_tenant', true), '')::uuid $$;

-- Set only while a login is being resolved (store::identity), in the same transaction-local
-- way, and read by membership_self_read below.
CREATE FUNCTION app_current_user() RETURNS uuid
    LANGUAGE sql STABLE PARALLEL SAFE
    AS $$ SELECT nullif(current_setting('app.current_user', true), '')::uuid $$;

-- ── tenant ────────────────────────────────────────────────────────────────────────────────
-- The root aggregate: the only table with no tenant_id, so no RLS.
CREATE TABLE tenant (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    slug       text NOT NULL,
    name       text NOT NULL,
    status     text NOT NULL DEFAULT 'active'
               CHECK (status IN ('active', 'suspended', 'closed')),
    settings   jsonb NOT NULL DEFAULT '{}'::jsonb,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tenant_slug_key UNIQUE (slug)
);
CREATE INDEX tenant_status_idx ON tenant (status);

-- ── app_user ──────────────────────────────────────────────────────────────────────────────
-- One row per human or agent principal. Identity is the OIDC subject, never a realm claim.
-- No tenant_id: a user is reached per tenant through membership, so no RLS here.
CREATE TABLE app_user (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    oidc_subject text,
    email        text,
    display_name text NOT NULL DEFAULT '',
    kind         text NOT NULL DEFAULT 'human'
                 CHECK (kind IN ('human', 'agent', 'service')),
    status       text NOT NULL DEFAULT 'active',
    version      bigint NOT NULL DEFAULT 1,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX app_user_oidc_subject_key
    ON app_user (oidc_subject) WHERE oidc_subject IS NOT NULL;
CREATE INDEX app_user_email_idx ON app_user (email);

-- ── department ────────────────────────────────────────────────────────────────────────────
CREATE TABLE department (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    name       text NOT NULL,
    parent_id  uuid,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT department_tenant_name_key UNIQUE (tenant_id, name),
    CONSTRAINT department_parent_fk FOREIGN KEY (parent_id) REFERENCES department (id),
    CONSTRAINT department_parent_not_self CHECK (parent_id <> id)
);

-- ── membership ────────────────────────────────────────────────────────────────────────────
-- The ONE role store per tenant (absorbs fr user_roles + agent_members +
-- department_members + member_knowledge_areas).
CREATE TABLE membership (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    user_id         uuid NOT NULL,
    role            text NOT NULL DEFAULT 'member'
                    CHECK (role IN ('owner', 'admin', 'member', 'viewer')),
    department_id   uuid,
    knowledge_areas text[] NOT NULL DEFAULT '{}',
    meta            jsonb NOT NULL DEFAULT '{}'::jsonb,
    version         bigint NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT membership_tenant_user_key UNIQUE (tenant_id, user_id),
    CONSTRAINT membership_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id),
    CONSTRAINT membership_user_fk FOREIGN KEY (user_id) REFERENCES app_user (id),
    CONSTRAINT membership_department_fk FOREIGN KEY (department_id) REFERENCES department (id)
);
CREATE INDEX membership_tenant_role_idx ON membership (tenant_id, role);
CREATE INDEX membership_user_idx ON membership (user_id);

-- ── event ─────────────────────────────────────────────────────────────────────────────────
-- The transactional outbox and the only fan-out source (CONTRACT §7 shape).
CREATE TABLE event (
    seq            bigserial PRIMARY KEY,
    id             uuid NOT NULL DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    topic          text NOT NULL,
    type           text NOT NULL,
    aggregate_type text NOT NULL,
    aggregate_id   uuid NOT NULL,
    payload        jsonb NOT NULL DEFAULT '{}'::jsonb,
    correlation_id uuid,
    causation_id   uuid,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT event_id_key UNIQUE (id)
);
CREATE INDEX event_tenant_seq_idx ON event (tenant_id, seq);
CREATE INDEX event_topic_seq_idx ON event (topic, seq);
CREATE INDEX event_aggregate_seq_idx ON event (aggregate_type, aggregate_id, seq);

-- ── job ───────────────────────────────────────────────────────────────────────────────────
-- Durable work queue; replaces every fr setInterval worker. No version column: a job is
-- claimed by a state transition guarded by lease_token, not by If-Match.
CREATE TABLE job (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    kind         text NOT NULL,
    dedupe_key   text NOT NULL,
    status       text NOT NULL DEFAULT 'pending'
                 CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'dead')),
    payload      jsonb NOT NULL DEFAULT '{}'::jsonb,
    run_after    timestamptz NOT NULL DEFAULT now(),
    attempts     int NOT NULL DEFAULT 0,
    max_attempts int NOT NULL DEFAULT 5,
    lease_until  timestamptz,
    lease_token  uuid,
    last_error   text,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    -- One live job per (tenant, kind, dedupe_key): a re-enqueue while one is pending or
    -- running is a no-op, not a second worker racing the first.
    CONSTRAINT job_lease_pair CHECK ((lease_until IS NULL) = (lease_token IS NULL))
);
CREATE UNIQUE INDEX job_live_dedupe_key
    ON job (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running');
CREATE INDEX job_claim_idx ON job (tenant_id, run_after, id) WHERE status = 'pending';
CREATE INDEX job_lease_idx ON job (lease_until) WHERE status = 'running';

-- ── schedule ──────────────────────────────────────────────────────────────────────────────
CREATE TABLE schedule (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    kind        text NOT NULL,
    cron        text NOT NULL,
    timezone    text NOT NULL DEFAULT 'UTC',
    payload     jsonb NOT NULL DEFAULT '{}'::jsonb,
    enabled     boolean NOT NULL DEFAULT true,
    next_run_at timestamptz NOT NULL,
    last_run_at timestamptz,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT schedule_tenant_kind_key UNIQUE (tenant_id, kind)
);
CREATE INDEX schedule_next_run_idx ON schedule (next_run_at) WHERE enabled;

-- ── audit_event ───────────────────────────────────────────────────────────────────────────
-- Append-only BY GRANT (see the grant block at the end): SELECT + INSERT only, and the
-- UPDATE/DELETE revoke must come AFTER the schema-wide GRANT or it is a no-op. Unbounded
-- retention: this is the record.
CREATE TABLE audit_event (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    actor_id      uuid,
    actor_kind    text NOT NULL DEFAULT 'user',
    action        text NOT NULL CHECK (action ~ '^[a-z0-9_]+\.[a-z0-9_]+$'),
    status        text NOT NULL CHECK (status IN ('ok', 'refused', 'error')),
    resource_type text,
    resource_id   uuid,
    request_id    text,
    details       jsonb NOT NULL DEFAULT '{}'::jsonb
                  CHECK (length(details::text) <= 8192),
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_event_tenant_created_idx
    ON audit_event (tenant_id, created_at DESC, id);
CREATE INDEX audit_event_resource_idx
    ON audit_event (tenant_id, resource_type, resource_id, created_at DESC);

-- ── action_invocation ─────────────────────────────────────────────────────────────────────
CREATE TABLE action_invocation (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    actor_id       uuid,
    action         text NOT NULL,
    action_version int NOT NULL DEFAULT 1,
    input_hash     text NOT NULL,
    idempotency_key text,
    outcome        text NOT NULL CHECK (outcome IN ('ok', 'refused', 'error', 'timeout')),
    duration_ms    int NOT NULL DEFAULT 0,
    trace_id       text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX action_invocation_tenant_action_idx
    ON action_invocation (tenant_id, action, created_at DESC);
CREATE INDEX action_invocation_trace_idx ON action_invocation (tenant_id, trace_id);

-- ── idempotency_key ───────────────────────────────────────────────────────────────────────
CREATE TABLE idempotency_key (
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    scope        text NOT NULL,
    key          text NOT NULL,
    request_hash text NOT NULL,
    status_code  int,
    response     jsonb,
    expires_at   timestamptz NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, scope, key)
);
CREATE INDEX idempotency_key_expires_idx ON idempotency_key (expires_at);

-- ── app_licence ───────────────────────────────────────────────────────────────────────────
-- The synced app-level entitlement, one row per (tenant, app_key), refreshed by licence.sync.
CREATE TABLE app_licence (
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    app_key    text NOT NULL DEFAULT 'conveyor',
    status     text NOT NULL
               CHECK (status IN ('active', 'expired', 'revoked', 'unknown')),
    expires_at timestamptz,
    checked_at timestamptz NOT NULL,
    raw        jsonb NOT NULL DEFAULT '{}'::jsonb,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, app_key)
);

-- ── Row level security ────────────────────────────────────────────────────────────────────
-- One FOR ALL tenant_isolation policy per tenant-owned table, with USING (reads) AND
-- WITH CHECK (writes): a policy with only USING lets a forged tenant_id write escape.
ALTER TABLE membership       ENABLE ROW LEVEL SECURITY;
ALTER TABLE department       ENABLE ROW LEVEL SECURITY;
ALTER TABLE event            ENABLE ROW LEVEL SECURITY;
ALTER TABLE job              ENABLE ROW LEVEL SECURITY;
ALTER TABLE schedule         ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_event      ENABLE ROW LEVEL SECURITY;
ALTER TABLE action_invocation ENABLE ROW LEVEL SECURITY;
ALTER TABLE idempotency_key  ENABLE ROW LEVEL SECURITY;
ALTER TABLE app_licence      ENABLE ROW LEVEL SECURITY;

ALTER TABLE membership       FORCE ROW LEVEL SECURITY;
ALTER TABLE department       FORCE ROW LEVEL SECURITY;
ALTER TABLE event            FORCE ROW LEVEL SECURITY;
ALTER TABLE job              FORCE ROW LEVEL SECURITY;
ALTER TABLE schedule         FORCE ROW LEVEL SECURITY;
ALTER TABLE audit_event      FORCE ROW LEVEL SECURITY;
ALTER TABLE action_invocation FORCE ROW LEVEL SECURITY;
ALTER TABLE idempotency_key  FORCE ROW LEVEL SECURITY;
ALTER TABLE app_licence      FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON membership FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON department FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON event FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON job FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON schedule FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON audit_event FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON action_invocation FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON idempotency_key FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON app_licence FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ⚠️ THE ONE ADDITIONAL POLICY, and why it is not optional. Login must find the ONE
-- (app_user, tenant) pair for a verified subject before any tenant is known (CONTRACT §9:
-- the tenant comes from membership, never a header). The agent pattern for this is a
-- SECURITY DEFINER function, but FORCE ROW LEVEL SECURITY binds the table OWNER too, so a
-- function owned by conveyor_migrate sees nothing — and this estate has no BYPASSRLS role
-- (S2-D3). Instead the app first reads app_user (no RLS), sets the user id with a
-- transaction-local GUC, and this SELECT-only policy exposes exactly that user's own
-- membership rows. It is scoped TO conveyor_runtime: it never applies to the owner, and it
-- adds no write path. Reads of any other table remain confined by tenant_isolation.
CREATE POLICY membership_self_read ON membership FOR SELECT TO conveyor_runtime
    USING (user_id = app_current_user());

-- ── Runtime grants ───────────────────────────────────────────────────────────────────────
-- ORDER MATTERS: the schema-wide GRANT first, then the audit_event UPDATE/DELETE revoke.
-- The migrate subcommand re-applies scripts/grants-runtime.sql after the last migration so
-- a table added by a later slice is not invisible to the runtime role.
GRANT USAGE ON SCHEMA public TO conveyor_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO conveyor_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO conveyor_runtime;
REVOKE ALL ON _sqlx_migrations FROM conveyor_runtime;
REVOKE UPDATE, DELETE ON audit_event FROM conveyor_runtime;

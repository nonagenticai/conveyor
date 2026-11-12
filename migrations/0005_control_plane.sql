-- 0005_control_plane.sql — the six S06 comms tables, forced RLS and the runtime grants.
--
-- Column-for-column from 02-DATA-ARCHITECTURE §2.6. Forward only: never edited after this
-- slice closes (CONTRACT §2). Every tenant-owned table carries `created_at`; the mutable
-- aggregates additionally carry `updated_at` and `version bigint` (CONTRACT §2).
-- `message` and `audience_member` are append-only facts and carry `created_at` alone:
-- a message is written once per send and a membership is inserted/deleted, never edited.
--
-- `message` is the outbound content, once per send; `delivery` fans out from it, one row per
-- (message, channel, recipient). The unique key `(message_id, channel_kind, recipient_ref)`
-- is what makes "delivered once" a key rather than a convention (§3.6) — a retried send
-- cannot create a second row.
--
-- `delivery` retention never prunes `pending`/`sent` (data-arch §6, S06 trap 6); the partial
-- index `delivery_live_idx` is the drainer's access path and covers exactly those two states.
-- The expiry/prune job is planned to use the same index.
--
-- `channel_binding.config`, `channel_preference.quiet_hours`, `message.meta` and
-- `audience.member_rule` are jsonb because each is read whole with its parent and never
-- filtered, sorted, joined or constrained (data-arch §5). A secret is stored by reference
-- only; no credential column exists here.
--
-- `audience` and `audience_member` are PENDING-S0 in the plan and ship here so S06 owns one
-- migration rather than a second later one.

-- ── channel_binding ─────────────────────────────────────────────────────────────────────
-- Per-tenant channel config; absorbs fr channels + telegram_forum_topics. `route_key` is a
-- project key or 'default'. The unique key is the natural key `channels.bind` upserts and
-- `channels.unbind` deletes, so a bind never needs a read first.
CREATE TABLE channel_binding (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    channel_kind text NOT NULL
                 CHECK (channel_kind IN ('web', 'telegram', 'mcp')),
    route_key    text NOT NULL DEFAULT '',
    config       jsonb NOT NULL DEFAULT '{}'::jsonb,
    active       boolean NOT NULL DEFAULT true,
    version      bigint NOT NULL DEFAULT 1,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT channel_binding_tenant_channel_route_key
        UNIQUE (tenant_id, channel_kind, route_key)
);

-- ── channel_preference ──────────────────────────────────────────────────────────────────
-- Absorbs fr notification_prefs + user_channel_prefs: one row per (user, channel kind).
CREATE TABLE channel_preference (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    user_id      uuid NOT NULL,
    channel_kind text NOT NULL,
    enabled      boolean NOT NULL DEFAULT true,
    quiet_hours  jsonb NOT NULL DEFAULT '{}'::jsonb,
    version      bigint NOT NULL DEFAULT 1,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT channel_preference_tenant_user_channel_key
        UNIQUE (tenant_id, user_id, channel_kind),
    CONSTRAINT channel_preference_user_fk FOREIGN KEY (user_id) REFERENCES app_user (id)
);

-- ── message ─────────────────────────────────────────────────────────────────────────────
-- The outbound content, once per send. Append-only: no updated_at, no version.
-- `correlation_id` links a send to the run/approval that caused it.
CREATE TABLE message (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    channel_kind   text NOT NULL,
    sender_id      uuid,
    kind           text NOT NULL DEFAULT 'text',
    body           text NOT NULL,
    meta           jsonb NOT NULL DEFAULT '{}'::jsonb,
    correlation_id uuid,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT message_sender_fk FOREIGN KEY (sender_id) REFERENCES app_user (id)
);
CREATE INDEX message_correlation_idx ON message (tenant_id, correlation_id);

-- ── delivery ────────────────────────────────────────────────────────────────────────────
-- One row per (message, channel, recipient); the state machine of contract §8 / data-arch
-- §3.6. `attempts` is the send-attempt counter and `last_error` the last failure text.
CREATE TABLE delivery (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    message_id    uuid NOT NULL,
    channel_kind  text NOT NULL,
    recipient_ref text NOT NULL,
    state         text NOT NULL DEFAULT 'pending'
                  CHECK (state IN ('pending', 'sent', 'delivered', 'failed', 'expired')),
    attempts      int NOT NULL DEFAULT 0,
    last_error    text,
    sent_at       timestamptz,
    delivered_at  timestamptz,
    expires_at    timestamptz,
    version       bigint NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT delivery_message_channel_recipient_key
        UNIQUE (message_id, channel_kind, recipient_ref),
    CONSTRAINT delivery_message_fk FOREIGN KEY (message_id) REFERENCES message (id)
);
-- The drainer's access path. Partial on the two live states, so the index does not grow
-- with the 30-day retention tail (data-arch §6).
CREATE INDEX delivery_live_idx ON delivery (tenant_id, state, expires_at)
    WHERE state IN ('pending', 'sent');

-- ── audience ────────────────────────────────────────────────────────────────────────────
-- Multicast audience; replaces fr streams. `member_rule` is expanded at fan-out and has no
-- query path (data-arch §5).
CREATE TABLE audience (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    name         text NOT NULL,
    channel_kind text NOT NULL,
    member_rule  jsonb NOT NULL DEFAULT '{}'::jsonb,
    version      bigint NOT NULL DEFAULT 1,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT audience_tenant_name_key UNIQUE (tenant_id, name)
);

-- ── audience_member ─────────────────────────────────────────────────────────────────────
-- Replaces fr asset_stream_membership. A fact: inserted and deleted, never edited.
CREATE TABLE audience_member (
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    audience_id uuid NOT NULL,
    user_id     uuid NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT audience_member_pkey PRIMARY KEY (audience_id, user_id),
    CONSTRAINT audience_member_audience_fk FOREIGN KEY (audience_id) REFERENCES audience (id),
    CONSTRAINT audience_member_user_fk FOREIGN KEY (user_id) REFERENCES app_user (id)
);
-- The primary key starts at `audience_id`; "the audiences a user is in" needs the other
-- direction, and it is also the RLS predicate's first column.
CREATE INDEX audience_member_user_idx ON audience_member (tenant_id, user_id);

-- ── Row level security ──────────────────────────────────────────────────────────────────
-- Six tables, no exception. One FOR ALL policy each with USING (reads) AND WITH CHECK
-- (writes): a policy with only USING lets a forged tenant_id write escape. FORCE is not
-- optional — without it the schema owner reads every tenant and the isolation gate would
-- pass while production leaked (CONTRACT §2).
ALTER TABLE channel_binding    ENABLE ROW LEVEL SECURITY;
ALTER TABLE channel_preference ENABLE ROW LEVEL SECURITY;
ALTER TABLE message            ENABLE ROW LEVEL SECURITY;
ALTER TABLE delivery           ENABLE ROW LEVEL SECURITY;
ALTER TABLE audience           ENABLE ROW LEVEL SECURITY;
ALTER TABLE audience_member    ENABLE ROW LEVEL SECURITY;

ALTER TABLE channel_binding    FORCE ROW LEVEL SECURITY;
ALTER TABLE channel_preference FORCE ROW LEVEL SECURITY;
ALTER TABLE message            FORCE ROW LEVEL SECURITY;
ALTER TABLE delivery           FORCE ROW LEVEL SECURITY;
ALTER TABLE audience           FORCE ROW LEVEL SECURITY;
ALTER TABLE audience_member    FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON channel_binding FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON channel_preference FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON message FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON delivery FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON audience FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON audience_member FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── Runtime grants ───────────────────────────────────────────────────────────────────────
-- What THIS block does: makes the six new tables visible to `conveyor_runtime` (a
-- schema-wide GRANT) and revokes the migration ledger. It performs NO append-only revokes.
-- Those live in scripts/grants-runtime.sql — `audit_event`, `event` and `action_invocation`
-- lose UPDATE (and `tenant`/`app_user` lose DELETE) there — and `migrate` re-applies that
-- file after the LAST migration, so it also runs after this one. Do not read this block as
-- carrying them: a database migrated by any other path would then have the wrong grants.
GRANT USAGE ON SCHEMA public TO conveyor_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO conveyor_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO conveyor_runtime;
REVOKE ALL ON _sqlx_migrations FROM conveyor_runtime;

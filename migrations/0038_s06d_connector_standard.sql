-- 0038_s06d_connector_standard.sql — connector kinds become open, machine channels get
-- tenant-configurable action grants, interactions get a multicast dispatch ledger, and a
-- connector decision gets its own honest decider kind.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- What this migration deliberately does NOT do:
--   * it does not add a delivery state. The data model already carries the first-response rule:
--     `pending | sent → expired` is exactly how the losing recipients of a resolved approval are
--     closed (crates/domain/src/channel.rs), so no second state machine is introduced;
--   * it does not create a per-binding service account. The identity is per `(tenant, kind)` and
--     derived from a deterministic subject; `channels.bind` ensures it at runtime, and the
--     backfill below covers the Telegram bindings that existed before this slice.

-- ── connector kinds are an open vocabulary ───────────────────────────────────────────────
-- The code registry decides which names this build serves; the database guards only the shape,
-- so onboarding a connector never needs a migration just to widen a CHECK.
ALTER TABLE channel_binding DROP CONSTRAINT channel_binding_channel_kind_check;
ALTER TABLE channel_binding ADD CONSTRAINT channel_binding_channel_kind_format
    CHECK (channel_kind ~ '^[a-z][a-z0-9_]{1,31}$');

-- ── the tenant's grant overrides per connector kind ──────────────────────────────────────
-- Absent row = the kind's compiled default (`actions::channel::default_actions`). A stored row
-- is read once per request (machine credential) or per inbound message (binding), so a change
-- applies on the next call with no cache to invalidate.
CREATE TABLE channel_action_grant (
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    channel_kind text NOT NULL,
    action       text NOT NULL,
    allowed      boolean NOT NULL,
    version      bigint NOT NULL DEFAULT 1,
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT channel_action_grant_pkey PRIMARY KEY (tenant_id, channel_kind, action),
    CONSTRAINT channel_action_grant_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT channel_action_grant_kind_format
        CHECK (channel_kind ~ '^[a-z][a-z0-9_]{1,31}$')
);

ALTER TABLE channel_action_grant ENABLE ROW LEVEL SECURITY;
ALTER TABLE channel_action_grant FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON channel_action_grant FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── the multicast dispatch ledger ────────────────────────────────────────────────────────
-- One row per (approval, connector kind, recipient) that was offered a request. The primary
-- key IS the fan-out idempotency: a restarted/duplicated dispatcher tick inserts nothing new.
-- `recipient_ref` is the logical address (`binding:<uuid>` or `user:<uuid>`); the adapter
-- resolves it to a concrete chat/mailbox at send time. `binding_id` is nullable because the
-- web channel's recipients are users, not bindings.
CREATE TABLE interaction_dispatch (
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    approval_id        uuid NOT NULL,
    channel_kind       text NOT NULL,
    recipient_ref      text NOT NULL,
    binding_id         uuid,
    message_id         uuid NOT NULL,
    closed_notified_at timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT interaction_dispatch_pkey
        PRIMARY KEY (tenant_id, approval_id, channel_kind, recipient_ref),
    CONSTRAINT interaction_dispatch_approval_fk FOREIGN KEY (approval_id) REFERENCES approval (id)
        ON DELETE CASCADE,
    -- An unbound route must not erase the history of what was dispatched; the approval and
    -- message are the facts, the binding is a configuration pointer.
    CONSTRAINT interaction_dispatch_binding_fk FOREIGN KEY (binding_id) REFERENCES channel_binding (id)
        ON DELETE SET NULL,
    CONSTRAINT interaction_dispatch_message_fk FOREIGN KEY (message_id) REFERENCES message (id)
        ON DELETE CASCADE,
    CONSTRAINT interaction_dispatch_kind_format
        CHECK (channel_kind ~ '^[a-z][a-z0-9_]{1,31}$')
);
CREATE INDEX interaction_dispatch_open_idx
    ON interaction_dispatch (tenant_id, approval_id)
    WHERE closed_notified_at IS NULL;

ALTER TABLE interaction_dispatch ENABLE ROW LEVEL SECURITY;
ALTER TABLE interaction_dispatch FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON interaction_dispatch FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── a connector's decision is a service decision, not a rule ────────────────────────────
-- The old vocabulary forced `service → rule`, which hid which transport answered. The writer
-- now records `service` for a provisioned connector, `agent` for MCP and `human` for web.
ALTER TABLE approval_decision DROP CONSTRAINT approval_decision_decided_by_kind_check;
ALTER TABLE approval_decision ADD CONSTRAINT approval_decision_decided_by_kind_check
    CHECK (decided_by_kind IN ('human', 'triage_agent', 'rule', 'agent', 'service'));

-- ── backfill: every tenant with a Telegram binding gets its connector identity ──────────
-- The identity is one per (tenant, kind): `service:<kind>:<tenant_id>` with the synthetic
-- machine role. Until this slice Telegram acted as a human member named in the binding's
-- config; that impersonation is removed, and the deterministic subject is what makes the
-- identity idempotent and re-creatable without a column on the binding.
--
-- ⚠️ `membership` is FORCE RLS, so the migration owner is subject to `tenant_isolation` too and
-- an INSERT with `app.current_tenant` unset would be refused. The window below is inside this
-- migration's transaction; FORCE is restored immediately after (the same pattern as 0028).
ALTER TABLE membership DISABLE ROW LEVEL SECURITY;
ALTER TABLE membership NO FORCE ROW LEVEL SECURITY;

INSERT INTO app_user (id, oidc_subject, display_name, kind, status)
SELECT uuidv7(), 'service:telegram:' || t.id, 'Telegram', 'service', 'active'
  FROM tenant t
 WHERE EXISTS (
     SELECT 1 FROM channel_binding b
      WHERE b.tenant_id = t.id AND b.channel_kind = 'telegram'
 )
ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL DO NOTHING;

INSERT INTO membership (tenant_id, user_id, role)
SELECT DISTINCT b.tenant_id, u.id, 'kata_executor'
  FROM channel_binding b
  JOIN app_user u ON u.oidc_subject = 'service:telegram:' || b.tenant_id
 WHERE b.channel_kind = 'telegram'
ON CONFLICT (tenant_id, user_id) DO NOTHING;

ALTER TABLE membership ENABLE ROW LEVEL SECURITY;
ALTER TABLE membership FORCE ROW LEVEL SECURITY;

-- Grants are NOT repeated here: `apps/server`'s `migrate` re-applies
-- `scripts/grants-runtime.sql` after the last migration (0001's comment states the rule).

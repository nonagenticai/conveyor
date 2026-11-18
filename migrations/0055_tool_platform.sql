-- 0055_tool_platform.sql — the tool estate (SPEC_V2 §11.8; issue #76, T13).
--
-- Four tables and their one owner each (§11.1):
--
--   toolkit       one service's published bundle: an immutable `key@toolkit_version`, with the
--                 destination alias, the auth adapter and the licence app.
--   tool          one callable operation of a toolkit version: closed input schema, output
--                 schema, effect class (data, never inferred from the HTTP method), idempotency,
--                 timeout, redaction and the declared external action that dispatches it.
--   tool_binding  the tenant-scoped grant: a job binding OR a run, pinned to one toolkit version,
--                 narrowed by effect ceiling and credential alias. The ceiling narrows only.
--   tool_call     the ledger: one append-only row per call that reached a resolved tool
--                 (run/step, tool version, input hash, status, latency, bytes, cost). It is NOT
--                 an `action_invocation` and it never transitions a card (§11.8).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- The toolkit's semantic version is `toolkit_version`, not the row's optimistic-lock `version`:
-- every table in this schema carries the latter for If-Match, and the published bundle must not
-- reuse that column or a lock bump would look like a new release.

-- ── toolkit — a service's published bundle ────────────────────────────────────────────────
CREATE TABLE toolkit (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    key             text NOT NULL,
    toolkit_version bigint NOT NULL CHECK (toolkit_version >= 1),
    title           text NOT NULL,
    -- The destination alias the broker resolves through configuration (`CONVEYOR_EXTERNAL_<SERVICE>_URL`).
    -- A call never carries a URL (§11.3 step 5, the SSRF rule).
    service         text NOT NULL,
    auth_adapter    text NOT NULL DEFAULT 'bearer'
                    CHECK (auth_adapter IN ('bearer', 'none', 'sealed-profile')),
    licence_app     text,
    owner           text NOT NULL,
    status          text NOT NULL DEFAULT 'draft'
                    CHECK (status IN ('draft', 'published', 'retired')),
    meta            jsonb NOT NULL DEFAULT '{}'::jsonb,
    version         bigint NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT toolkit_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT toolkit_tenant_key_version_key UNIQUE (tenant_id, key, toolkit_version),
    CONSTRAINT toolkit_key_shape CHECK (key ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT toolkit_service_shape CHECK (service ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT toolkit_meta_is_object CHECK (jsonb_typeof(meta) = 'object')
);

CREATE INDEX toolkit_key_idx ON toolkit (tenant_id, key, toolkit_version DESC);

ALTER TABLE toolkit ENABLE ROW LEVEL SECURITY;
ALTER TABLE toolkit FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON toolkit FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── tool — one operation of a toolkit version ────────────────────────────────────────────
CREATE TABLE tool (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    toolkit_id     uuid NOT NULL,
    -- The operation key inside the toolkit (`jobs.create`). The canonical tool name a caller
    -- sends is `<toolkit.key>.<tool.key>` (§11.1).
    key            text NOT NULL,
    summary        text,
    input_schema   jsonb NOT NULL,
    output_schema  jsonb,
    effect         text NOT NULL
                   CHECK (effect IN ('read', 'write', 'spend', 'destructive')),
    idempotency    text NOT NULL DEFAULT 'optional'
                   CHECK (idempotency IN ('required', 'optional', 'forbidden')),
    timeout_secs   int NOT NULL DEFAULT 30 CHECK (timeout_secs BETWEEN 1 AND 300),
    redact         jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- The engine action that actually dispatches the destination call. It must be a DECLARED
    -- external action: the broker is a governed wrapper around the one external path, never a
    -- second control plane and never a generic launcher for conveyor actions (§11.3, §11.7).
    handler_action text NOT NULL,
    status         text NOT NULL DEFAULT 'published'
                   CHECK (status IN ('draft', 'published', 'retired')),
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tool_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT tool_toolkit_fk FOREIGN KEY (toolkit_id) REFERENCES toolkit (id),
    CONSTRAINT tool_tenant_toolkit_key_key UNIQUE (tenant_id, toolkit_id, key),
    CONSTRAINT tool_key_shape CHECK (key ~ '^[a-z][a-z0-9_.]*$'),
    CONSTRAINT tool_input_schema_is_object CHECK (jsonb_typeof(input_schema) = 'object'),
    CONSTRAINT tool_output_schema_is_object
        CHECK (output_schema IS NULL OR jsonb_typeof(output_schema) = 'object'),
    CONSTRAINT tool_redact_is_array CHECK (jsonb_typeof(redact) = 'array')
);

CREATE INDEX tool_toolkit_idx ON tool (tenant_id, toolkit_id, status);

ALTER TABLE tool ENABLE ROW LEVEL SECURITY;
ALTER TABLE tool FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tool FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── tool_binding — the grant, to a job binding or a single run ────────────────────────────
CREATE TABLE tool_binding (
    id                  uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id           uuid NOT NULL DEFAULT app_current_tenant(),
    job_binding_id      uuid,
    run_id              uuid,
    toolkit_id          uuid NOT NULL,
    -- The ceiling narrows the tool's own effect; it never raises it (§11.4).
    effect_ceiling      text NOT NULL DEFAULT 'read'
                        CHECK (effect_ceiling IN ('read', 'write', 'spend', 'destructive')),
    credential_alias    text,
    rate_limit_per_min  int CHECK (rate_limit_per_min > 0),
    enabled             boolean NOT NULL DEFAULT true,
    meta                jsonb NOT NULL DEFAULT '{}'::jsonb,
    version             bigint NOT NULL DEFAULT 1,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tool_binding_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT tool_binding_job_binding_fk FOREIGN KEY (job_binding_id) REFERENCES job_binding (id) ON DELETE CASCADE,
    CONSTRAINT tool_binding_run_fk FOREIGN KEY (run_id) REFERENCES run (id) ON DELETE CASCADE,
    CONSTRAINT tool_binding_toolkit_fk FOREIGN KEY (toolkit_id) REFERENCES toolkit (id),
    -- Exactly one grant target: a job binding (the durable path) or a run (one-off work).
    CONSTRAINT tool_binding_one_target CHECK ((job_binding_id IS NULL) <> (run_id IS NULL)),
    CONSTRAINT tool_binding_meta_is_object CHECK (jsonb_typeof(meta) = 'object')
);

CREATE UNIQUE INDEX tool_binding_job_toolkit_key
    ON tool_binding (tenant_id, job_binding_id, toolkit_id)
    WHERE job_binding_id IS NOT NULL;
CREATE UNIQUE INDEX tool_binding_run_toolkit_key
    ON tool_binding (tenant_id, run_id, toolkit_id)
    WHERE run_id IS NOT NULL;

ALTER TABLE tool_binding ENABLE ROW LEVEL SECURITY;
ALTER TABLE tool_binding FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tool_binding FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── tool_call — the ledger ───────────────────────────────────────────────────────────────
-- Append-only: no `updated_at` and no `version`. A retention prune may remove an old row with its
-- run (SET NULL keeps the row; the run link is what the run's retention removes).
CREATE TABLE tool_call (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    run_id         uuid,
    run_step_id    uuid,
    card_id        uuid,
    tool_id        uuid NOT NULL,
    -- The canonical name the caller used (`<toolkit>.<operation>`), kept readable after a toolkit
    -- is superseded; `tool_id` still pins the exact version row.
    tool_key       text NOT NULL,
    toolkit_key    text NOT NULL,
    toolkit_version bigint NOT NULL,
    input_hash     text NOT NULL,
    status         text NOT NULL
                   CHECK (status IN ('ok', 'precondition_failed', 'check_failed', 'pending',
                                     'backend_error')),
    http_status    int,
    latency_ms     int NOT NULL CHECK (latency_ms >= 0),
    request_bytes  int CHECK (request_bytes >= 0),
    response_bytes int CHECK (response_bytes >= 0),
    cost           numeric(18, 6) CHECK (cost >= 0),
    error_class    text,
    error_message  text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tool_call_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT tool_call_run_fk FOREIGN KEY (run_id) REFERENCES run (id) ON DELETE SET NULL,
    CONSTRAINT tool_call_run_step_fk FOREIGN KEY (run_step_id) REFERENCES run_step (id) ON DELETE SET NULL,
    CONSTRAINT tool_call_card_fk FOREIGN KEY (card_id) REFERENCES card (id) ON DELETE SET NULL,
    CONSTRAINT tool_call_tool_fk FOREIGN KEY (tool_id) REFERENCES tool (id),
    CONSTRAINT tool_call_input_hash_shape CHECK (input_hash ~ '^[0-9a-f]{64}$')
);

CREATE INDEX tool_call_run_idx ON tool_call (tenant_id, run_id, created_at DESC, id);
CREATE INDEX tool_call_tool_idx ON tool_call (tenant_id, tool_id, created_at DESC, id);

ALTER TABLE tool_call ENABLE ROW LEVEL SECURITY;
ALTER TABLE tool_call FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tool_call FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

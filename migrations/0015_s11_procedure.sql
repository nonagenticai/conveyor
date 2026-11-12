-- 0015_s11_procedure.sql — S11's three tables (02-DATA-ARCHITECTURE §2.12), plus the two `run` columns the
-- slice plan records as forward references from S03.
--
-- A procedure is a version family: rows share (tenant_id, key), each with a version and a status. At most one
-- row per key may be `published` — the partial unique index is the invariant, not a check in the handler.
--
-- Every table is ENABLE + FORCE RLS with one `tenant_isolation` policy, like every other domain table.

CREATE TABLE procedure (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    key            text NOT NULL,
    title          text NOT NULL,
    description    text NOT NULL DEFAULT '',
    status         text NOT NULL DEFAULT 'draft'
                   CHECK (status IN ('draft', 'published', 'retired')),
    version        bigint NOT NULL DEFAULT 1,
    card_type_id   uuid,
    owner_user_id  uuid,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT procedure_card_type_fk FOREIGN KEY (card_type_id) REFERENCES card_type (id),
    CONSTRAINT procedure_owner_fk FOREIGN KEY (owner_user_id) REFERENCES app_user (id),
    CONSTRAINT procedure_key_not_blank CHECK (btrim(key) <> ''),
    CONSTRAINT procedure_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT procedure_tenant_key_version_key UNIQUE (tenant_id, key, version)
);

-- One published version per key. A handler that tries to publish a second one gets a unique violation, which
-- is the whole point: the check is not advisory.
CREATE UNIQUE INDEX uq_procedure_published
    ON procedure (tenant_id, key) WHERE status = 'published';
CREATE INDEX procedure_status_idx ON procedure (tenant_id, status, key);

CREATE TABLE procedure_step (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    procedure_id   uuid NOT NULL,
    position       int NOT NULL CHECK (position >= 1),
    action         text NOT NULL,
    input_template jsonb NOT NULL DEFAULT '{}'::jsonb,
    gate_kind      text,
    on_error       text NOT NULL DEFAULT 'fail'
                   CHECK (on_error IN ('fail', 'continue', 'retry')),
    max_attempts   int NOT NULL DEFAULT 1 CHECK (max_attempts >= 1),
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT procedure_step_procedure_fk FOREIGN KEY (procedure_id) REFERENCES procedure (id),
    CONSTRAINT procedure_step_action_not_blank CHECK (btrim(action) <> ''),
    CONSTRAINT procedure_step_position_key UNIQUE (procedure_id, position)
);

CREATE TABLE run_step (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    run_id            uuid NOT NULL,
    procedure_step_id uuid,
    position          int NOT NULL CHECK (position >= 1),
    attempt           int NOT NULL DEFAULT 1 CHECK (attempt >= 1),
    action            text NOT NULL,
    status            text NOT NULL DEFAULT 'queued'
                      CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'skipped')),
    input             jsonb NOT NULL DEFAULT '{}'::jsonb,
    output            jsonb NOT NULL DEFAULT '{}'::jsonb,
    error             text,
    started_at        timestamptz,
    finished_at       timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT run_step_run_fk FOREIGN KEY (run_id) REFERENCES run (id),
    CONSTRAINT run_step_procedure_step_fk FOREIGN KEY (procedure_step_id) REFERENCES procedure_step (id),
    -- A retry APPENDS: the trace keeps every attempt, which is what S13's diff gate reads.
    CONSTRAINT run_step_attempt_key UNIQUE (run_id, position, attempt)
);
CREATE INDEX run_step_run_idx ON run_step (tenant_id, run_id, position);

-- The two `run` columns S03 did not create (02-DATA-ARCHITECTURE §8 records them as forward references).
ALTER TABLE run
    ADD COLUMN procedure_id uuid,
    ADD COLUMN inputs jsonb NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE run
    ADD CONSTRAINT run_procedure_fk FOREIGN KEY (procedure_id) REFERENCES procedure (id);
CREATE INDEX run_procedure_idx ON run (tenant_id, procedure_id) WHERE procedure_id IS NOT NULL;

-- Row level security: ENABLE + FORCE, one FOR ALL policy with USING and WITH CHECK each.
ALTER TABLE procedure      ENABLE ROW LEVEL SECURITY;
ALTER TABLE procedure      FORCE ROW LEVEL SECURITY;
ALTER TABLE procedure_step ENABLE ROW LEVEL SECURITY;
ALTER TABLE procedure_step FORCE ROW LEVEL SECURITY;
ALTER TABLE run_step       ENABLE ROW LEVEL SECURITY;
ALTER TABLE run_step       FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON procedure FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON procedure_step FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON run_step FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

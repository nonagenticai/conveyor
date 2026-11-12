-- 0004_board.sql — the nine S05 board satellites, forced RLS and the runtime grants.
--
-- Column-for-column from 02-DATA-ARCHITECTURE §2.5. Forward only: never edited after this
-- slice closes (CONTRACT §2). Every tenant-owned table carries `created_at`; the mutable
-- aggregates additionally carry `updated_at` and `version bigint` (CONTRACT §2). `card_label`
-- is a join fact (insert/delete only, like `trace_link`), so it carries `created_at` alone.
--
-- `board_membership` replaces Planka's board_membership + project_manager + board_subscription:
-- ONE role enum, one row per (board, user). `comment` is the ONE comment store. `custom_field`
-- flattens Planka's group into `group_name` + `group_position` (S5-D3) — no group entity, no
-- catalogue row: the group is a label, never joined.
--
-- There is deliberately no `card_membership` and no `attachment` (SPEC §2): a card's assignee
-- is `card.assignee_id`, and a cover attachment has no target column.
--
-- `position` is `numeric`, never a float: a midpoint insert `(a+b)/2` must stay exact
-- (data-arch §2.3, trap 2).

-- ── board_membership ────────────────────────────────────────────────────────────────────
-- One row per (board, user); the ONE role enum of the system.
CREATE TABLE board_membership (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    board_id    uuid NOT NULL,
    user_id     uuid NOT NULL,
    role        text NOT NULL DEFAULT 'member'
                CHECK (role IN ('owner', 'admin', 'member', 'viewer')),
    can_comment boolean NOT NULL DEFAULT true,
    watching    boolean NOT NULL DEFAULT true,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT board_membership_tenant_board_user_key UNIQUE (tenant_id, board_id, user_id),
    CONSTRAINT board_membership_board_fk FOREIGN KEY (board_id) REFERENCES board (id),
    CONSTRAINT board_membership_user_fk FOREIGN KEY (user_id) REFERENCES app_user (id)
);
CREATE INDEX board_membership_user_idx ON board_membership (user_id);

-- ── label ───────────────────────────────────────────────────────────────────────────────
-- Per-board label. Colour is free text (S05 §10 Q2 default: seed the named colours later).
CREATE TABLE label (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    board_id   uuid NOT NULL,
    name       text NOT NULL,
    color      text NOT NULL,
    position   numeric NOT NULL DEFAULT 0,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT label_tenant_board_name_key UNIQUE (tenant_id, board_id, name),
    CONSTRAINT label_board_fk FOREIGN KEY (board_id) REFERENCES board (id)
);
CREATE INDEX label_board_position_idx ON label (tenant_id, board_id, position);

-- ── card_label ──────────────────────────────────────────────────────────────────────────
-- The join. A fact: inserted and deleted, never edited.
CREATE TABLE card_label (
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    card_id    uuid NOT NULL,
    label_id   uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT card_label_pkey PRIMARY KEY (card_id, label_id),
    CONSTRAINT card_label_card_fk FOREIGN KEY (card_id) REFERENCES card (id),
    CONSTRAINT card_label_label_fk FOREIGN KEY (label_id) REFERENCES label (id)
);
CREATE INDEX card_label_label_idx ON card_label (label_id, card_id);

-- ── comment ─────────────────────────────────────────────────────────────────────────────
-- The ONE comment store (replaces fr comments + Planka comment). `edited_at` is the edit
-- marker; `version` carries the If-Match precondition `comments.update` needs.
CREATE TABLE comment (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    card_id    uuid NOT NULL,
    author_id  uuid,
    body       text NOT NULL,
    edited_at  timestamptz,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT comment_card_fk FOREIGN KEY (card_id) REFERENCES card (id),
    CONSTRAINT comment_author_fk FOREIGN KEY (author_id) REFERENCES app_user (id)
);
CREATE INDEX comment_card_created_idx ON comment (tenant_id, card_id, created_at, id);

-- ── task_list ───────────────────────────────────────────────────────────────────────────
-- A checklist container on one card.
CREATE TABLE task_list (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    card_id        uuid NOT NULL,
    name           text NOT NULL,
    position       numeric NOT NULL DEFAULT 0,
    hide_completed boolean NOT NULL DEFAULT false,
    show_on_front  boolean NOT NULL DEFAULT false,
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT task_list_card_fk FOREIGN KEY (card_id) REFERENCES card (id)
);
CREATE INDEX task_list_card_position_idx ON task_list (tenant_id, card_id, position);

-- ── task ────────────────────────────────────────────────────────────────────────────────
-- A checklist item. `done` is the only state.
CREATE TABLE task (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    task_list_id uuid NOT NULL,
    name         text NOT NULL,
    done         boolean NOT NULL DEFAULT false,
    position     numeric NOT NULL DEFAULT 0,
    version      bigint NOT NULL DEFAULT 1,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT task_task_list_fk FOREIGN KEY (task_list_id) REFERENCES task_list (id)
);
CREATE INDEX task_task_list_position_idx ON task (tenant_id, task_list_id, position);

-- ── custom_field ────────────────────────────────────────────────────────────────────────
-- Planka's group -> field -> value, with the group flattened into two columns (S5-D3).
-- `kind` is validated by the domain; the database refuses the vocabulary too.
CREATE TABLE custom_field (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    board_id       uuid NOT NULL,
    group_name     text NOT NULL DEFAULT '',
    group_position numeric NOT NULL DEFAULT 0,
    name           text NOT NULL,
    kind           text NOT NULL DEFAULT 'text'
                   CHECK (kind IN ('text', 'number', 'date', 'select', 'checkbox')),
    options        jsonb NOT NULL DEFAULT '[]'::jsonb,
    position       numeric NOT NULL DEFAULT 0,
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT custom_field_tenant_board_name_key UNIQUE (tenant_id, board_id, name),
    CONSTRAINT custom_field_board_fk FOREIGN KEY (board_id) REFERENCES board (id)
);
CREATE INDEX custom_field_board_position_idx
    ON custom_field (tenant_id, board_id, group_position, position);

-- ── custom_field_value ──────────────────────────────────────────────────────────────────
-- One value per (card, field). `content` is JSON so one column carries every field kind.
CREATE TABLE custom_field_value (
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    card_id         uuid NOT NULL,
    custom_field_id uuid NOT NULL,
    content         jsonb NOT NULL DEFAULT 'null'::jsonb,
    version         bigint NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT custom_field_value_pkey PRIMARY KEY (card_id, custom_field_id),
    CONSTRAINT custom_field_value_card_fk FOREIGN KEY (card_id) REFERENCES card (id),
    CONSTRAINT custom_field_value_field_fk
        FOREIGN KEY (custom_field_id) REFERENCES custom_field (id)
);
CREATE INDEX custom_field_value_field_idx ON custom_field_value (tenant_id, custom_field_id);

-- ── webhook_subscription ────────────────────────────────────────────────────────────────
-- Planka's webhook semantics without instance-admin scoping. `board_id` is nullable: a
-- tenant-wide subscription names no board. Delivery rows are a job, never a table here.
CREATE TABLE webhook_subscription (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    board_id          uuid,
    url               text NOT NULL,
    secret_ciphertext bytea,
    events            text[] NOT NULL DEFAULT '{}',
    excluded_events   text[] NOT NULL DEFAULT '{}',
    active            boolean NOT NULL DEFAULT true,
    version           bigint NOT NULL DEFAULT 1,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT webhook_subscription_board_fk FOREIGN KEY (board_id) REFERENCES board (id)
);
CREATE INDEX webhook_subscription_active_idx ON webhook_subscription (tenant_id, active);

-- ── Row level security ──────────────────────────────────────────────────────────────────
-- Nine tables, no exception. One FOR ALL policy each with USING (reads) AND WITH CHECK
-- (writes): a policy with only USING lets a forged tenant_id write escape.
ALTER TABLE board_membership   ENABLE ROW LEVEL SECURITY;
ALTER TABLE label              ENABLE ROW LEVEL SECURITY;
ALTER TABLE card_label         ENABLE ROW LEVEL SECURITY;
ALTER TABLE comment            ENABLE ROW LEVEL SECURITY;
ALTER TABLE task_list          ENABLE ROW LEVEL SECURITY;
ALTER TABLE task               ENABLE ROW LEVEL SECURITY;
ALTER TABLE custom_field       ENABLE ROW LEVEL SECURITY;
ALTER TABLE custom_field_value ENABLE ROW LEVEL SECURITY;
ALTER TABLE webhook_subscription ENABLE ROW LEVEL SECURITY;

ALTER TABLE board_membership   FORCE ROW LEVEL SECURITY;
ALTER TABLE label              FORCE ROW LEVEL SECURITY;
ALTER TABLE card_label         FORCE ROW LEVEL SECURITY;
ALTER TABLE comment            FORCE ROW LEVEL SECURITY;
ALTER TABLE task_list          FORCE ROW LEVEL SECURITY;
ALTER TABLE task               FORCE ROW LEVEL SECURITY;
ALTER TABLE custom_field       FORCE ROW LEVEL SECURITY;
ALTER TABLE custom_field_value FORCE ROW LEVEL SECURITY;
ALTER TABLE webhook_subscription FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON board_membership FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON label FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON card_label FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON comment FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON task_list FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON task FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON custom_field FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON custom_field_value FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON webhook_subscription FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── Runtime grants ───────────────────────────────────────────────────────────────────────
-- What THIS block does: makes the nine new tables visible to `conveyor_runtime` (a
-- schema-wide GRANT) and revokes the migration ledger. It performs NO append-only revokes.
-- Those live in scripts/grants-runtime.sql — `audit_event`, `event` and `action_invocation`
-- lose UPDATE (and `tenant`/`app_user` lose DELETE) there — and `migrate` re-applies that
-- file after the LAST migration, so it also runs after this one. Do not read this block as
-- carrying them: a database migrated by any other path would then have the wrong grants.
GRANT USAGE ON SCHEMA public TO conveyor_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO conveyor_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO conveyor_runtime;
REVOKE ALL ON _sqlx_migrations FROM conveyor_runtime;

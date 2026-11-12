-- 0002_domain.sql — the fourteen S03 aggregates, forced RLS and the runtime grants.
--
-- Column-for-column from 02-DATA-ARCHITECTURE §2.3 (board spine, 9) and §2.4 (governance
-- core, 5). Forward only: never edited after this slice closes (CONTRACT §2). A later slice
-- that needs `run.procedure_id`, `run.inputs` or `run.workflow_id` adds that column in its
-- own migration; they are deliberately absent here.
--
-- Every vocabulary is text + CHECK. A Postgres enum type would make a later label change
-- impossible in the same transaction as an index on it, and each migration file runs in one
-- transaction (data-arch §1.8).
--
-- The envelope (S03 binding decision 1): `created_at` on all fourteen; `updated_at` +
-- `version bigint NOT NULL DEFAULT 1` on the eleven mutable aggregates. `revision`,
-- `trace_link` and `approval_decision` are facts: created_at only, no version.

-- ── programme ────────────────────────────────────────────────────────────────────────────
-- Portfolio container; ships here because project is its first consumer.
CREATE TABLE programme (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    name          text NOT NULL,
    status        text NOT NULL DEFAULT 'active',
    owner_user_id uuid,
    version       bigint NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT programme_tenant_name_key UNIQUE (tenant_id, name),
    CONSTRAINT programme_owner_fk FOREIGN KEY (owner_user_id) REFERENCES app_user (id)
);

-- ── project ──────────────────────────────────────────────────────────────────────────────
-- The ONE project entity; absorbs fr projects + Planka project + RM pm_projects.
CREATE TABLE project (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    key               text NOT NULL,
    name              text NOT NULL,
    description       text NOT NULL DEFAULT '',
    status            text NOT NULL DEFAULT 'active'
                      CHECK (status IN ('draft', 'active', 'paused', 'closed', 'archived')),
    programme_id      uuid,
    project_type_code text,
    phase_code        text,
    owner_user_id     uuid,
    repo_url          text,
    meta              jsonb NOT NULL DEFAULT '{}'::jsonb,
    version           bigint NOT NULL DEFAULT 1,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT project_tenant_key_key UNIQUE (tenant_id, key),
    CONSTRAINT project_programme_fk FOREIGN KEY (programme_id) REFERENCES programme (id),
    CONSTRAINT project_owner_fk FOREIGN KEY (owner_user_id) REFERENCES app_user (id)
);
CREATE INDEX project_status_idx ON project (tenant_id, status, updated_at DESC, id);
CREATE INDEX project_programme_idx ON project (tenant_id, programme_id);

-- ── board ────────────────────────────────────────────────────────────────────────────────
-- One board per project today, but the FK is the contract, not a 1:1 assumption.
CREATE TABLE board (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    project_id uuid NOT NULL,
    name       text NOT NULL,
    position   numeric NOT NULL DEFAULT 0,
    settings   jsonb NOT NULL DEFAULT '{}'::jsonb,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT board_tenant_project_name_key UNIQUE (tenant_id, project_id, name),
    CONSTRAINT board_project_fk FOREIGN KEY (project_id) REFERENCES project (id)
);
CREATE INDEX board_project_position_idx ON board (tenant_id, project_id, position);

-- ── list ─────────────────────────────────────────────────────────────────────────────────
-- The card lifecycle vocabulary. `kind` is the only lane type in the system.
CREATE TABLE list (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    board_id   uuid NOT NULL,
    name       text NOT NULL,
    position   numeric NOT NULL,
    kind       text NOT NULL
               CHECK (kind IN ('intake', 'active', 'waiting', 'done', 'archive')),
    color      text,
    wip_limit  int CHECK (wip_limit IS NULL OR wip_limit > 0),
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT list_tenant_board_id_key UNIQUE (tenant_id, board_id, id),
    CONSTRAINT list_board_fk FOREIGN KEY (board_id) REFERENCES board (id)
);
CREATE INDEX list_board_position_idx ON list (tenant_id, board_id, position, id);

-- ── card_type ────────────────────────────────────────────────────────────────────────────
-- The join target of the project-type matrix. Deliberately not a catalogue_entry: a card
-- type carries its own `definition`, and data-arch §10 Q1 records the exception.
CREATE TABLE card_type (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    code       text NOT NULL,
    label      text NOT NULL,
    definition jsonb NOT NULL DEFAULT '{}'::jsonb,
    is_active  boolean NOT NULL DEFAULT true,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT card_type_tenant_code_key UNIQUE (tenant_id, code)
);
CREATE INDEX card_type_active_idx ON card_type (tenant_id, is_active, code);

-- ── card ─────────────────────────────────────────────────────────────────────────────────
-- The work item. There is no lifecycle column: list_id + archived_at are the lifecycle.
-- `board_id` is denormalised and every move keeps it equal to list.board_id (trap 8).
CREATE TABLE card (
    id                   uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id            uuid NOT NULL DEFAULT app_current_tenant(),
    board_id             uuid NOT NULL,
    list_id              uuid NOT NULL,
    card_type_id         uuid NOT NULL,
    parent_card_id       uuid,
    name                 text NOT NULL,
    description          text NOT NULL DEFAULT '',
    position             numeric NOT NULL,
    due_at               timestamptz,
    due_done_at          timestamptz,
    stopwatch_started_at timestamptz,
    stopwatch_seconds    int NOT NULL DEFAULT 0,
    archived_at          timestamptz,
    assignee_id          uuid,
    lease_expires_at     timestamptz,
    lease_token          uuid,
    claim_fence          bigint NOT NULL DEFAULT 0,
    prev_list_id         uuid,
    list_changed_at      timestamptz NOT NULL DEFAULT now(),
    meta                 jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_by           uuid,
    version              bigint NOT NULL DEFAULT 1,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    tsv                  tsvector GENERATED ALWAYS AS (
                             to_tsvector('english', name || ' ' || description)
                         ) STORED,
    -- The lease pair is set and cleared together: a token without an expiry never expires,
    -- and an expiry without a token cannot be fenced.
    CONSTRAINT card_lease_pair CHECK ((lease_expires_at IS NULL) = (lease_token IS NULL)),
    CONSTRAINT card_tenant_board_id_key UNIQUE (tenant_id, board_id, id),
    CONSTRAINT card_tenant_list_id_key UNIQUE (tenant_id, list_id, id),
    CONSTRAINT card_list_fk FOREIGN KEY (list_id) REFERENCES list (id),
    CONSTRAINT card_board_fk FOREIGN KEY (board_id) REFERENCES board (id),
    CONSTRAINT card_card_type_fk FOREIGN KEY (card_type_id) REFERENCES card_type (id),
    CONSTRAINT card_parent_fk FOREIGN KEY (parent_card_id) REFERENCES card (id),
    CONSTRAINT card_assignee_fk FOREIGN KEY (assignee_id) REFERENCES app_user (id)
);
-- The three partial indexes: two board scans (one grouped by list) and the lease sweep.
CREATE INDEX card_board_list_position_idx
    ON card (tenant_id, board_id, list_id, position, id) WHERE archived_at IS NULL;
CREATE INDEX card_board_position_idx
    ON card (tenant_id, board_id, position, id) WHERE archived_at IS NULL;
CREATE INDEX card_assignee_lease_idx
    ON card (tenant_id, assignee_id, lease_expires_at) WHERE assignee_id IS NOT NULL;
CREATE INDEX card_tsv_idx ON card USING gin (tsv);

-- ── run ──────────────────────────────────────────────────────────────────────────────────
-- One execution of a card through a gate/procedure/workflow. procedure_id, inputs and
-- workflow_id are added by S11/S13's migrations, FK in the same file.
CREATE TABLE run (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    card_id        uuid,
    project_id     uuid NOT NULL,
    status         text NOT NULL DEFAULT 'queued'
                   CHECK (status IN ('queued', 'running', 'waiting_approval', 'succeeded',
                                     'failed', 'cancelled')),
    attempt        int NOT NULL DEFAULT 1,
    error_class    text,
    error_message  text,
    correlation_id uuid,
    external_ref   text,
    started_at     timestamptz,
    finished_at    timestamptz,
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT run_card_fk FOREIGN KEY (card_id) REFERENCES card (id),
    CONSTRAINT run_project_fk FOREIGN KEY (project_id) REFERENCES project (id)
);
-- One live run per card, enforced by the database even if two API pods race.
CREATE UNIQUE INDEX uq_run_live
    ON run (tenant_id, card_id) WHERE status IN ('queued', 'running', 'waiting_approval');
CREATE INDEX run_queued_idx ON run (tenant_id, status, created_at, id) WHERE status = 'queued';
CREATE INDEX run_created_idx ON run (tenant_id, created_at DESC, id);
CREATE INDEX run_card_created_idx ON run (tenant_id, card_id, created_at DESC);

-- ── approval ─────────────────────────────────────────────────────────────────────────────
-- The gate. Dual control is required_decisions, never a second gate row.
CREATE TABLE approval (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    run_id             uuid,
    card_id            uuid,
    project_id         uuid,
    gate               text NOT NULL,
    kind               text NOT NULL,
    status             text NOT NULL DEFAULT 'pending'
                       CHECK (status IN ('pending', 'approved', 'rejected', 'expired', 'voided')),
    required_decisions int NOT NULL DEFAULT 1 CHECK (required_decisions >= 1),
    requested_by       uuid,
    expires_at         timestamptz,
    callback_url       text,
    correlation_id     uuid,
    meta               jsonb NOT NULL DEFAULT '{}'::jsonb,
    version            bigint NOT NULL DEFAULT 1,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT approval_run_fk FOREIGN KEY (run_id) REFERENCES run (id),
    CONSTRAINT approval_card_fk FOREIGN KEY (card_id) REFERENCES card (id),
    CONSTRAINT approval_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT approval_requested_by_fk FOREIGN KEY (requested_by) REFERENCES app_user (id)
);
-- At most ONE pending per (run, gate). This index, not check-then-insert, is the guarantee.
CREATE UNIQUE INDEX uq_appr_pending
    ON approval (tenant_id, run_id, gate) WHERE status = 'pending';
CREATE INDEX approval_pending_expiry_idx
    ON approval (tenant_id, status, expires_at) WHERE status = 'pending';
CREATE INDEX approval_card_created_idx ON approval (tenant_id, card_id, created_at DESC);

-- ── approval_decision ────────────────────────────────────────────────────────────────────
-- One row per approver per approval, immutable after insert (S3-D5: a decision is a fact).
-- decided_by_kind carries fr migrations 211/219 (SPEC D18): an agent decision must be
-- distinguishable from a human one.
CREATE TABLE approval_decision (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    approval_id     uuid NOT NULL,
    approver_id     uuid NOT NULL,
    decision        text NOT NULL CHECK (decision IN ('approve', 'reject')),
    note            text NOT NULL DEFAULT '',
    item_decisions  jsonb,
    decided_by_kind text NOT NULL DEFAULT 'human'
                    CHECK (decided_by_kind IN ('human', 'triage_agent', 'rule', 'agent')),
    via             jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),
    -- One person cannot satisfy dual control twice.
    CONSTRAINT approval_decision_approval_approver_key UNIQUE (approval_id, approver_id),
    CONSTRAINT approval_decision_approval_fk FOREIGN KEY (approval_id) REFERENCES approval (id),
    CONSTRAINT approval_decision_approver_fk FOREIGN KEY (approver_id) REFERENCES app_user (id)
);
CREATE INDEX approval_decision_approver_idx
    ON approval_decision (tenant_id, approver_id, created_at DESC);

-- ── governed_asset ───────────────────────────────────────────────────────────────────────
-- ONE lifecycle for requirements, principles and ADRs. Kind-specific fields live in meta.
CREATE TABLE governed_asset (
    id               uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id        uuid NOT NULL DEFAULT app_current_tenant(),
    kind             text NOT NULL CHECK (kind IN ('requirement', 'principle', 'adr')),
    key              text NOT NULL,
    title            text NOT NULL,
    body             text NOT NULL DEFAULT '',
    status           text NOT NULL DEFAULT 'draft'
                     CHECK (status IN ('draft', 'in_review', 'approved', 'superseded',
                                       'archived')),
    version          bigint NOT NULL DEFAULT 1,
    supersedes_id    uuid,
    superseded_by_id uuid,
    project_id       uuid,
    owner_user_id    uuid,
    phase_code       text,
    effort           numeric,
    reach            numeric,
    impact           numeric,
    confidence       numeric,
    acceptance       jsonb NOT NULL DEFAULT '[]'::jsonb,
    links            jsonb NOT NULL DEFAULT '{}'::jsonb,
    lens             jsonb NOT NULL DEFAULT '{}'::jsonb,
    meta             jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    tsv              tsvector GENERATED ALWAYS AS (
                         to_tsvector('english', key || ' ' || title || ' ' || body)
                     ) STORED,
    -- A CHECK is immediate, so the superseding UPDATE writes both pointers at once.
    CONSTRAINT governed_asset_superseded_pointer
        CHECK (status <> 'superseded' OR superseded_by_id IS NOT NULL),
    CONSTRAINT governed_asset_superseded_not_self
        CHECK (superseded_by_id IS NULL OR superseded_by_id <> id),
    CONSTRAINT governed_asset_tenant_kind_key_key UNIQUE (tenant_id, kind, key),
    CONSTRAINT governed_asset_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT governed_asset_owner_fk FOREIGN KEY (owner_user_id) REFERENCES app_user (id),
    CONSTRAINT governed_asset_supersedes_fk
        FOREIGN KEY (supersedes_id) REFERENCES governed_asset (id),
    CONSTRAINT governed_asset_superseded_by_fk
        FOREIGN KEY (superseded_by_id) REFERENCES governed_asset (id)
);
CREATE INDEX governed_asset_kind_status_idx
    ON governed_asset (tenant_id, kind, status, updated_at DESC, id);
CREATE INDEX governed_asset_project_idx ON governed_asset (tenant_id, project_id);
CREATE INDEX governed_asset_tsv_idx ON governed_asset USING gin (tsv);

-- ── trace_link ───────────────────────────────────────────────────────────────────────────
-- The only many-to-many between governed assets, cards and model elements. Polymorphic by
-- design: the domain function validates both ends in the same transaction.
CREATE TABLE trace_link (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    from_type  text NOT NULL,
    from_id    uuid NOT NULL,
    to_type    text NOT NULL,
    to_id      uuid NOT NULL,
    kind       text NOT NULL DEFAULT 'traces'
               CHECK (kind IN ('traces', 'realises', 'derives', 'satisfies', 'conflicts')),
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT trace_link_tenant_edge_key
        UNIQUE (tenant_id, from_type, from_id, to_type, to_id, kind)
);
CREATE INDEX trace_link_to_idx ON trace_link (tenant_id, to_type, to_id);
CREATE INDEX trace_link_from_idx ON trace_link (tenant_id, from_type, from_id);

-- ── catalogue_entry ──────────────────────────────────────────────────────────────────────
-- The ONE vocabulary table. `kind` deliberately has no CHECK: S03 fixes the vocabulary in
-- `domain::catalogue::CatalogueKind`, and a new kind is a code change, not a migration.
CREATE TABLE catalogue_entry (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    kind       text NOT NULL,
    code       text NOT NULL,
    label      text NOT NULL,
    meta       jsonb NOT NULL DEFAULT '{}'::jsonb,
    sort       int NOT NULL DEFAULT 0,
    is_active  boolean NOT NULL DEFAULT true,
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT catalogue_entry_tenant_kind_code_key UNIQUE (tenant_id, kind, code)
);
CREATE INDEX catalogue_entry_kind_sort_idx ON catalogue_entry (tenant_id, kind, sort, code);

-- ── revision ─────────────────────────────────────────────────────────────────────────────
-- The ONE history table. `seq` equals governed_asset.version for an asset (dense from 1);
-- append-only, so created_at only. Model revisions are pruned by a job to the last 50.
CREATE TABLE revision (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    entity_type  text NOT NULL,
    entity_id    uuid NOT NULL,
    seq          int NOT NULL CHECK (seq >= 1),
    label        text NOT NULL DEFAULT '',
    content      jsonb NOT NULL DEFAULT '{}'::jsonb,
    content_hash text NOT NULL,
    author_id    uuid,
    status       text NOT NULL DEFAULT 'draft'
                 CHECK (status IN ('draft', 'published', 'superseded')),
    created_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT revision_tenant_entity_seq_key UNIQUE (tenant_id, entity_type, entity_id, seq),
    CONSTRAINT revision_author_fk FOREIGN KEY (author_id) REFERENCES app_user (id)
);
CREATE INDEX revision_entity_seq_desc_idx
    ON revision (tenant_id, entity_type, entity_id, seq DESC);

-- ── andon_state ──────────────────────────────────────────────────────────────────────────
-- One lazily created row per tenant; the estate-wide stop. The primary key IS tenant_id.
CREATE TABLE andon_state (
    tenant_id   uuid PRIMARY KEY DEFAULT app_current_tenant(),
    engaged     boolean NOT NULL DEFAULT false,
    engaged_by  uuid,
    engaged_at  timestamptz,
    released_at timestamptz,
    reason      text NOT NULL DEFAULT '',
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT andon_state_engaged_by_fk FOREIGN KEY (engaged_by) REFERENCES app_user (id)
);
CREATE INDEX andon_state_engaged_idx ON andon_state (tenant_id) WHERE engaged;

-- ── Row level security ────────────────────────────────────────────────────────────────────
-- Fourteen tables, no exception. One FOR ALL policy each with USING (reads) AND WITH CHECK
-- (writes): a policy with only USING lets a forged tenant_id write escape.
ALTER TABLE programme         ENABLE ROW LEVEL SECURITY;
ALTER TABLE project           ENABLE ROW LEVEL SECURITY;
ALTER TABLE board             ENABLE ROW LEVEL SECURITY;
ALTER TABLE list              ENABLE ROW LEVEL SECURITY;
ALTER TABLE card_type         ENABLE ROW LEVEL SECURITY;
ALTER TABLE card              ENABLE ROW LEVEL SECURITY;
ALTER TABLE run               ENABLE ROW LEVEL SECURITY;
ALTER TABLE approval          ENABLE ROW LEVEL SECURITY;
ALTER TABLE approval_decision ENABLE ROW LEVEL SECURITY;
ALTER TABLE governed_asset    ENABLE ROW LEVEL SECURITY;
ALTER TABLE trace_link        ENABLE ROW LEVEL SECURITY;
ALTER TABLE catalogue_entry   ENABLE ROW LEVEL SECURITY;
ALTER TABLE revision          ENABLE ROW LEVEL SECURITY;
ALTER TABLE andon_state       ENABLE ROW LEVEL SECURITY;

ALTER TABLE programme         FORCE ROW LEVEL SECURITY;
ALTER TABLE project           FORCE ROW LEVEL SECURITY;
ALTER TABLE board             FORCE ROW LEVEL SECURITY;
ALTER TABLE list              FORCE ROW LEVEL SECURITY;
ALTER TABLE card_type         FORCE ROW LEVEL SECURITY;
ALTER TABLE card              FORCE ROW LEVEL SECURITY;
ALTER TABLE run               FORCE ROW LEVEL SECURITY;
ALTER TABLE approval          FORCE ROW LEVEL SECURITY;
ALTER TABLE approval_decision FORCE ROW LEVEL SECURITY;
ALTER TABLE governed_asset    FORCE ROW LEVEL SECURITY;
ALTER TABLE trace_link        FORCE ROW LEVEL SECURITY;
ALTER TABLE catalogue_entry   FORCE ROW LEVEL SECURITY;
ALTER TABLE revision          FORCE ROW LEVEL SECURITY;
ALTER TABLE andon_state       FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON programme FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON project FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON board FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON list FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON card_type FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON card FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON run FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON approval FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON approval_decision FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON governed_asset FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON trace_link FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON catalogue_entry FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON revision FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON andon_state FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── Runtime grants ───────────────────────────────────────────────────────────────────────
-- ORDER MATTERS: the schema-wide GRANT first, then the audit_event UPDATE/DELETE revoke.
-- `migrate` re-applies scripts/grants-runtime.sql after the last migration.
GRANT USAGE ON SCHEMA public TO conveyor_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO conveyor_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO conveyor_runtime;
REVOKE ALL ON _sqlx_migrations FROM conveyor_runtime;
REVOKE UPDATE, DELETE ON audit_event FROM conveyor_runtime;

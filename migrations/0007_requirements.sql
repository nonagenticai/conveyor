-- 0007_requirements.sql — the five S07 tables, pg_trgm and the three trigram indexes.
--
-- Column-for-column from 02-DATA-ARCHITECTURE §2.7. Forward only: never edited after this
-- slice closes (CONTRACT §2). The version gap is deliberate — 0006 is reserved for S06a,
-- which ships no migration of its own (the signed channel reuses `approval.callback_url`).
--
-- S07 CONSUMES the S03 governance tables (`governed_asset`, `revision`, `catalogue_entry`,
-- `trace_link`) and adds only what those do not carry. Its own five:
--
--   outline, outline_node   one tree per (project, kind); replaces RM's toc_trees,
--                           toc_settings, toc_nodes and fr's project_outlines. PENDING-S0:
--                           the tables ship, the API and screens are S08's. S07 leaves both
--                           empty and never writes them.
--   quality_evaluation      append-only score history; absorbs RM quality_evaluations. It is
--                           polymorphic (`entity_type`/`entity_id`) because S08 scores models
--                           with the same rubric; S07 writes only `entity_type='revision'`.
--   principle_conflict      persisted verdicts between two principles; absorbs RM
--                           principle_conflicts. `dismissed_by`/`dismissed_at` are the one
--                           mutation: a verdict is not edited, it is dismissed.
--   project_request         the CP→conveyor bridge intake; absorbs fr project_requests.
--                           `repository_url` is text, not an FK: `repository` ships in S09.
--
-- One deviation from §2.7, recorded here and in 00-INDEX: `project_request` carries
-- `version bigint NOT NULL DEFAULT 1`, because `project_requests.update` is an edit and
-- CONTRACT §3 requires it to honour `If-Match`. §2.7's column list omits `version`; without
-- it the contract cannot be satisfied. No other table here is version-addressed: outlines
-- have no action in S07, a conflict is dismissed once, and a score is never edited.
--
-- §4 puts the three trigram indexes here rather than in S03 (S03 ships only the FTS
-- indexes). They are on S03/S05 tables; this migration adds no column, key or constraint to
-- any of them.

-- pg_trgm is a trusted extension: `conveyor_migrate` (NOSUPERUSER, database owner) can
-- install it once the database grants CREATE. Verified on a throwaway postgres:18 with
-- scripts/db-roles.sql applied — `CREATE EXTENSION` succeeded as conveyor_migrate (1.6).
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- ── outline ──────────────────────────────────────────────────────────────────────────────
-- One outline per (project, kind). `project_id` is nullable: a tenant-wide outline has no
-- project, and the unique key treats NULLs as distinct (PostgreSQL default), so two
-- tenant-wide outlines of the same kind and title are allowed — the plan's key is verbatim.
-- `settings` is jsonb because it is read whole with the outline and never filtered.
CREATE TABLE outline (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    project_id uuid,
    kind       text NOT NULL DEFAULT 'wbs',
    title      text NOT NULL,
    settings   jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT outline_kind_check CHECK (kind IN ('wbs', 'ltp', 'toc')),
    CONSTRAINT outline_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT outline_tenant_project_kind_title_key UNIQUE (tenant_id, project_id, kind, title)
);

-- ── outline_node ────────────────────────────────────────────────────────────────────────
-- The ONE tree table. `position numeric` (never a float) is the sibling order, and the index
-- is the access path for "this parent's children in order" — the only read S08 needs.
-- `edge_kind` names the relation to the parent for the non-tree kinds; it is free text.
CREATE TABLE outline_node (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    outline_id uuid NOT NULL,
    parent_id  uuid,
    position   numeric NOT NULL DEFAULT 0,
    kind       text NOT NULL DEFAULT '',
    title      text NOT NULL,
    body       text NOT NULL DEFAULT '',
    meta       jsonb NOT NULL DEFAULT '{}'::jsonb,
    edge_kind  text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT outline_node_outline_fk FOREIGN KEY (outline_id) REFERENCES outline (id),
    CONSTRAINT outline_node_parent_fk FOREIGN KEY (parent_id) REFERENCES outline_node (id)
);
CREATE INDEX outline_node_tree_idx
    ON outline_node (tenant_id, outline_id, parent_id, position, id);

-- ── quality_evaluation ──────────────────────────────────────────────────────────────────
-- Append-only: a re-score inserts, it never updates. `score` is nullable because a rubric
-- run can be recorded with criteria only (a degraded evaluation), and `criteria` carries the
-- rubric version plus each raw and weighted value, which is what makes the number auditable.
-- No CHECK on `entity_type`/`evaluator_kind`: §2.7 fixes no vocabulary, and S08 adds entity
-- types this slice cannot enumerate.
CREATE TABLE quality_evaluation (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    entity_type    text NOT NULL,
    entity_id      uuid NOT NULL,
    score          numeric,
    criteria       jsonb NOT NULL DEFAULT '{}'::jsonb,
    evaluator_kind text NOT NULL DEFAULT 'heuristic',
    created_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX quality_evaluation_entity_idx
    ON quality_evaluation (tenant_id, entity_type, entity_id, created_at DESC);

-- ── principle_conflict ──────────────────────────────────────────────────────────────────
-- Two principles, one verdict. `uq (tenant_id, principle_a_id, principle_b_id)` makes the
-- pair unique but direction-sensitive: (a,b) and (b,a) are two rows. The store canonicalises
-- the pair (lower uuid first) before writing, so the pair is unique in BOTH directions —
-- recorded in store/src/quality.rs's `canonical_pair`, tested there.
CREATE TABLE principle_conflict (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id      uuid NOT NULL DEFAULT app_current_tenant(),
    principle_a_id uuid NOT NULL,
    principle_b_id uuid NOT NULL,
    verdict        text NOT NULL,
    dismissed_by   uuid,
    dismissed_at   timestamptz,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT principle_conflict_a_fk FOREIGN KEY (principle_a_id) REFERENCES governed_asset (id),
    CONSTRAINT principle_conflict_b_fk FOREIGN KEY (principle_b_id) REFERENCES governed_asset (id),
    CONSTRAINT principle_conflict_dismissed_by_fk FOREIGN KEY (dismissed_by) REFERENCES app_user (id),
    CONSTRAINT principle_conflict_pair_key UNIQUE (tenant_id, principle_a_id, principle_b_id)
);

-- ── project_request ─────────────────────────────────────────────────────────────────────
-- The intake. `payload` is the whole forwarded body (read whole, never filtered); `status`
-- and `external_ref` are columns because they are filtered and joined. Idempotency is the
-- unique key: a re-post updates the draft or reopens a rejected row, and a `promoted` row is
-- a conflict carrying its `project_id` (fr's contract, carried by S07 §5.4).
CREATE TABLE project_request (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    external_ref    text NOT NULL,
    external_origin text NOT NULL,
    payload         jsonb NOT NULL DEFAULT '{}'::jsonb,
    status          text NOT NULL DEFAULT 'draft',
    project_id      uuid,
    repository_url  text,
    version         bigint NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT project_request_status_check
        CHECK (status IN ('draft', 'approved', 'rejected', 'promoted')),
    CONSTRAINT project_request_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT project_request_tenant_external_ref_key UNIQUE (tenant_id, external_ref)
);
CREATE INDEX project_request_status_idx
    ON project_request (tenant_id, status, created_at, id);

-- ── Trigram indexes (data-arch §4) ──────────────────────────────────────────────────────
-- Trigram is not full-text: these serve `%`, `ILIKE` and `similarity()`; `tsv @@ tsquery`
-- keeps the `gin(tsv)` index S03 built. `similarity()` alone in an ORDER BY would scan the
-- table, so S07-2's search arms always carry `%` or the tsv predicate.
CREATE INDEX governed_asset_title_trgm_idx ON governed_asset USING gin (title gin_trgm_ops);
CREATE INDEX card_name_trgm_idx ON card USING gin (name gin_trgm_ops);
CREATE INDEX catalogue_entry_label_trgm_idx ON catalogue_entry USING gin (label gin_trgm_ops);

-- ── Row level security ──────────────────────────────────────────────────────────────────
-- Five tables, no exception. One FOR ALL policy each with USING (reads) AND WITH CHECK
-- (writes): a policy with only USING lets a forged tenant_id write escape. FORCE is not
-- optional — without it the schema owner reads every tenant and the isolation gate would
-- pass while production leaked (CONTRACT §2).
ALTER TABLE outline            ENABLE ROW LEVEL SECURITY;
ALTER TABLE outline_node       ENABLE ROW LEVEL SECURITY;
ALTER TABLE quality_evaluation ENABLE ROW LEVEL SECURITY;
ALTER TABLE principle_conflict ENABLE ROW LEVEL SECURITY;
ALTER TABLE project_request    ENABLE ROW LEVEL SECURITY;

ALTER TABLE outline            FORCE ROW LEVEL SECURITY;
ALTER TABLE outline_node       FORCE ROW LEVEL SECURITY;
ALTER TABLE quality_evaluation FORCE ROW LEVEL SECURITY;
ALTER TABLE principle_conflict FORCE ROW LEVEL SECURITY;
ALTER TABLE project_request    FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON outline FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON outline_node FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON quality_evaluation FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON principle_conflict FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON project_request FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── Runtime grants ───────────────────────────────────────────────────────────────────────
-- What THIS block does: makes the five new tables visible to `conveyor_runtime` (a
-- schema-wide GRANT) and revokes the migration ledger. It performs NO append-only revokes —
-- those live in scripts/grants-runtime.sql, which `migrate` re-applies after the LAST
-- migration, and that file is where `quality_evaluation` loses UPDATE/DELETE.
GRANT USAGE ON SCHEMA public TO conveyor_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO conveyor_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO conveyor_runtime;
REVOKE ALL ON _sqlx_migrations FROM conveyor_runtime;

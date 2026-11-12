-- 0019_s13_workflow.sql — the workflow engine's one table and the three trace deltas (S13 step 1).
--
-- Column-for-column from 02-DATA-ARCHITECTURE §2.13 plus S13 §5.1's four deltas. Forward only:
-- never edited after this slice closes (CONTRACT §2).
--
-- The design decisions this schema carries:
--
--   * **The whole graph is ONE document.** `definition` is `{schema:1, nodes, edges}` and nothing
--     filters on a node, so there are no node/edge tables. Publishing freezes the document into
--     S03's `revision` (`entity_type = 'workflow'`, `seq = version`) — the same one revision writer
--     every other family uses, so a verifier never needs a second hash implementation.
--   * **One published version per key, and it is a database rule.** `uq_workflow_published` is a
--     partial unique index, so two API pods that publish concurrently cannot both win; the publish
--     path retires the previous row first because the index is immediate.
--   * **The 256 KiB ceiling is a CHECK, not a convention.** `pg_column_size(definition)` is the
--     stored size, so a caller cannot slip a larger document past the validator by compressing it.
--   * **`run.workflow_id` is S03's forward reference, added here.** A NULL means the run was driven
--     by a procedure (S11); a non-NULL means a workflow definition owns it (S13 §5.3).
--   * **`run_step` becomes the trace.** The four columns are S13's; S11's `attempt` and
--     `UNIQUE (run_id, position, attempt)` are unchanged — a retry still APPENDS an attempt row, and
--     the diff gate reads every one of them.
--
-- Retention (data-arch §6): with the run (180 days). The rule is recorded here; its producer is not
-- part of S13's build order.

-- ── workflow ─────────────────────────────────────────────────────────────────────────────
-- `key` is the stable name (`cap.x`), `version` the dense fork counter, `status` the lifecycle.
-- A new `key` starts at v1; reusing a key creates the next version as a draft (S13 §5).
CREATE TABLE workflow (
    id                    uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id             uuid NOT NULL DEFAULT app_current_tenant(),
    key                   text NOT NULL,
    title                 text NOT NULL,
    status                text NOT NULL DEFAULT 'draft'
                          CHECK (status IN ('draft', 'published', 'retired')),
    version               bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
    definition            jsonb NOT NULL DEFAULT '{"nodes":[],"edges":[]}'::jsonb,
    published_revision_id uuid,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT workflow_key_not_blank CHECK (btrim(key) <> ''),
    CONSTRAINT workflow_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT workflow_published_revision_fk
        FOREIGN KEY (published_revision_id) REFERENCES revision (id),
    CONSTRAINT workflow_key_version_key UNIQUE (tenant_id, key, version),
    -- The 256 KiB ceiling S13's publish validator also enforces. `pg_column_size` is the stored
    -- size, so a compressed document cannot exceed the limit while reading as smaller in `length`.
    CONSTRAINT workflow_definition_size CHECK (pg_column_size(definition) <= 262144)
);

-- Exactly one published version per key. The publish path retires the previous row in the same
-- transaction, BEFORE this row enters `published`, because the index is immediate.
CREATE UNIQUE INDEX uq_workflow_published
    ON workflow (tenant_id, key) WHERE status = 'published';

-- RLS: enabled AND forced, so the schema owner is subject to the policy too (S03's rls_domain
-- test asserts exactly this shape).
ALTER TABLE workflow ENABLE ROW LEVEL SECURITY;
ALTER TABLE workflow FORCE  ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON workflow FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── run.workflow_id — S03's forward reference ─────────────────────────────────────────────
-- Nullable: a procedure-driven run does not name a workflow. The FK is added here because this
-- migration creates its target; a single-column FK does not enforce tenancy, so every store path
-- resolves the workflow inside the caller's scope first (CONTRACT §2).
ALTER TABLE run
    ADD COLUMN workflow_id uuid,
    ADD CONSTRAINT run_workflow_fk FOREIGN KEY (workflow_id) REFERENCES workflow (id);

-- The run list a workflow's page reads: newest first, workflow-driven runs only.
CREATE INDEX run_workflow_idx
    ON run (tenant_id, workflow_id, created_at DESC) WHERE workflow_id IS NOT NULL;

-- ── run_step — S13's four trace columns ─────────────────────────────────────────────────
-- All nullable: S11's procedure runs leave them NULL and keep working unchanged. `procedure_step_id`
-- stays NULL for workflow nodes (no procedure step exists to point at).
ALTER TABLE run_step
    ADD COLUMN node_id     text,
    ADD COLUMN input_hash  text,
    ADD COLUMN output_hash text,
    ADD COLUMN duration_ms int;

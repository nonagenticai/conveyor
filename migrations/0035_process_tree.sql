-- 0035_process_tree.sql — S13's process model (SPEC D6/D20/D22/D28/D29/D30, CONTRACT §21,
-- 02-DATA-ARCHITECTURE §2.12–§2.13).
--
-- One ordered process, not a graph: `procedure` is RENAMED `process` and `procedure_step` is
-- RENAMED `process_step`; the name `procedure` is then free for the SOP the step is judged
-- against. `process_map` is the user-facing DAG (declarative, never the executor). A run is a
-- tree: one live ROOT per card, one live child per (parent, step), depth ≤ 3.
--
-- Order matters in three places and each is commented below:
--   1. catalog names are freed BEFORE the SOP table claims them (index names are schema-global);
--   2. free-text values (`job.kind`, `approval.kind`, `subject_kind`) move with the rename;
--   3. a CHECK that gains a value must be dropped before the data is updated and re-added after.
--
-- Forward only. No down migration.

-- ── 1. the rename ─────────────────────────────────────────────────────────────────────────
ALTER TABLE procedure      RENAME TO process;
ALTER TABLE procedure_step RENAME TO process_step;

ALTER TABLE process_step RENAME COLUMN procedure_id TO process_id;
ALTER TABLE run          RENAME COLUMN procedure_id TO process_id;
ALTER TABLE run_step     RENAME COLUMN procedure_step_id TO process_step_id;

-- Catalog names travel with the TABLE, not the rename, and index/constraint names are
-- schema-global: free the `procedure*` namespace before the new SOP table uses it.
ALTER INDEX uq_procedure_published RENAME TO uq_process_published;
ALTER INDEX uq_procedure_draft     RENAME TO uq_process_draft;
ALTER INDEX procedure_status_idx   RENAME TO process_status_idx;
ALTER TABLE process RENAME CONSTRAINT procedure_tenant_key_version_key TO process_tenant_key_version_key;
ALTER TABLE process RENAME CONSTRAINT procedure_card_type_fk           TO process_card_type_fk;
ALTER TABLE process RENAME CONSTRAINT procedure_capability_fk          TO process_capability_fk;
ALTER TABLE process RENAME CONSTRAINT procedure_owner_fk               TO process_owner_fk;
ALTER TABLE process RENAME CONSTRAINT procedure_key_not_blank          TO process_key_not_blank;
ALTER TABLE process RENAME CONSTRAINT procedure_title_not_blank        TO process_title_not_blank;
ALTER TABLE process RENAME CONSTRAINT procedure_status_check           TO process_status_check;
ALTER TABLE process RENAME CONSTRAINT procedure_pkey                   TO process_pkey;

ALTER INDEX procedure_step_position_key RENAME TO process_step_position_key;
ALTER TABLE process_step RENAME CONSTRAINT procedure_step_procedure_fk  TO process_step_process_fk;
ALTER TABLE process_step RENAME CONSTRAINT procedure_step_position_check TO process_step_position_check;
ALTER TABLE process_step RENAME CONSTRAINT procedure_step_max_attempts_check TO process_step_max_attempts_check;
ALTER TABLE process_step RENAME CONSTRAINT procedure_step_pkey          TO process_step_pkey;

-- `on_error` becomes `on_result` (the typed outcome of a step, CONTRACT §21).
ALTER TABLE process_step RENAME COLUMN on_error TO on_result;
ALTER TABLE process_step DROP CONSTRAINT procedure_step_on_error_check;
ALTER TABLE process_step ADD CONSTRAINT process_step_on_result_check
    CHECK (on_result IN ('fail', 'continue', 'retry'));

-- A step's action is now nullable: an `agent` step runs the process agent, a `gate` parks and
-- a `call` starts a child — none of them names one action. A computed step still must.
ALTER TABLE process_step DROP CONSTRAINT procedure_step_action_not_blank;
ALTER TABLE process_step ALTER COLUMN action DROP NOT NULL;
ALTER TABLE process_step ADD CONSTRAINT process_step_action_not_blank
    CHECK (action IS NULL OR btrim(action) <> '');

ALTER TABLE run_step RENAME CONSTRAINT run_step_procedure_step_fk TO run_step_process_step_fk;
ALTER TABLE run RENAME CONSTRAINT run_procedure_fk TO run_process_fk;
ALTER INDEX run_procedure_idx RENAME TO run_process_idx;

-- ── 2. the new SOP (`procedure`) and the process map ──────────────────────────────────────
-- A procedure is the SOP a step is judged against: definition of ready/done, acceptance
-- criteria, the inspection (the judge) and the outputs. Versioned and reusable; one published
-- row per key, enforced by a partial unique index rather than a check-then-insert.
CREATE TABLE procedure (
    id                   uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id            uuid NOT NULL DEFAULT app_current_tenant(),
    key                  text NOT NULL,
    title                text NOT NULL,
    description          text NOT NULL DEFAULT '',
    status               text NOT NULL DEFAULT 'draft'
                         CHECK (status IN ('draft', 'published', 'retired')),
    version              bigint NOT NULL DEFAULT 1,
    lock_version         bigint NOT NULL DEFAULT 1,
    definition_of_ready  jsonb NOT NULL DEFAULT '{}'::jsonb,
    definition_of_done   jsonb NOT NULL DEFAULT '{}'::jsonb,
    acceptance_criteria  jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- `{kind: laya|eval|checker, threshold?, max_rejects?, rule?}` — the judge contract (D28).
    inspection           jsonb NOT NULL DEFAULT '{}'::jsonb,
    outputs              jsonb NOT NULL DEFAULT '[]'::jsonb,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT procedure_tenant_key_version_key UNIQUE (tenant_id, key, version),
    CONSTRAINT procedure_key_not_blank   CHECK (btrim(key) <> ''),
    CONSTRAINT procedure_title_not_blank CHECK (btrim(title) <> '')
);
CREATE UNIQUE INDEX uq_procedure_published ON procedure (tenant_id, key) WHERE status = 'published';
CREATE UNIQUE INDEX uq_procedure_draft     ON procedure (tenant_id, key) WHERE status = 'draft';
CREATE INDEX procedure_status_idx ON procedure (tenant_id, status, key);

ALTER TABLE procedure ENABLE ROW LEVEL SECURITY;
ALTER TABLE procedure FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON procedure FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- The user-facing DAG. Declarative: it cannot reorder a step; it is rendered with the
-- project's live position. Bound to a project type (`catalogue_entry(kind='project_type')` —
-- there is no `project_type` table).
CREATE TABLE process_map (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    key             text NOT NULL,
    title           text NOT NULL,
    status          text NOT NULL DEFAULT 'draft'
                    CHECK (status IN ('draft', 'published', 'retired')),
    version         bigint NOT NULL DEFAULT 1,
    lock_version    bigint NOT NULL DEFAULT 1,
    project_type_id uuid NOT NULL,
    definition      jsonb NOT NULL DEFAULT '{"nodes":[],"edges":[]}'::jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT process_map_project_type_fk FOREIGN KEY (project_type_id) REFERENCES catalogue_entry (id),
    CONSTRAINT process_map_tenant_key_version_key UNIQUE (tenant_id, key, version),
    CONSTRAINT process_map_key_not_blank   CHECK (btrim(key) <> ''),
    CONSTRAINT process_map_title_not_blank CHECK (btrim(title) <> '')
);
CREATE UNIQUE INDEX uq_process_map_published
    ON process_map (tenant_id, key, project_type_id) WHERE status = 'published';
CREATE UNIQUE INDEX uq_process_map_draft
    ON process_map (tenant_id, key, project_type_id) WHERE status = 'draft';
CREATE INDEX process_map_status_idx ON process_map (tenant_id, status, key);

ALTER TABLE process_map ENABLE ROW LEVEL SECURITY;
ALTER TABLE process_map FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON process_map FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 3. process / process_step columns ─────────────────────────────────────────────────────
-- `capability_id` already exists: 0020 moved the 17 catalogue rows and gave `procedure` (now
-- `process`) that column. Only the rest is new.
ALTER TABLE process
    ADD COLUMN list_kind       text,
    ADD COLUMN project_type_id uuid,
    ADD COLUMN outputs         jsonb NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE process
    ADD CONSTRAINT process_project_type_fk FOREIGN KEY (project_type_id) REFERENCES catalogue_entry (id),
    ADD CONSTRAINT process_list_kind_check CHECK (list_kind IN ('ready', 'doing', 'qa', 'done'));

-- `kind` is how a step executes; `actor` is who owns it; `procedure_id` is the SOP it is
-- judged against; `process_agent_id` the process agent that runs it; `child_process_key` the
-- process OR published workflow a `call` step starts (process-first, CONTRACT §21).
ALTER TABLE process_step
    ADD COLUMN kind               text NOT NULL DEFAULT 'computed'
                                  CHECK (kind IN ('agent', 'computed', 'gate', 'call')),
    ADD COLUMN actor              text NOT NULL DEFAULT 'tool_agent'
                                  CHECK (actor IN ('process_agent', 'tool_agent', 'system', 'human')),
    ADD COLUMN capability_id      uuid,
    ADD COLUMN process_agent_id   uuid,
    ADD COLUMN procedure_id       uuid,
    ADD COLUMN child_process_key  text;
ALTER TABLE process_step
    ADD CONSTRAINT process_step_capability_fk     FOREIGN KEY (capability_id) REFERENCES capability (id),
    ADD CONSTRAINT process_step_process_agent_fk  FOREIGN KEY (process_agent_id) REFERENCES capability_agent (id),
    ADD CONSTRAINT process_step_procedure_fk      FOREIGN KEY (procedure_id) REFERENCES procedure (id);
-- Existing dev rows whose gate_kind is set are gates; the rest stay computed.
UPDATE process_step SET kind = 'gate' WHERE gate_kind IS NOT NULL;

-- ── 4. the run tree ───────────────────────────────────────────────────────────────────────
-- `step_id` is the `process_step` that started the child (not the run_step attempt); the
-- partial indexes are the enforcement, not a check-then-insert.
ALTER TABLE run
    ADD COLUMN parent_run_id   uuid,
    ADD COLUMN step_id         uuid,
    ADD COLUMN depth           int NOT NULL DEFAULT 0 CHECK (depth BETWEEN 0 AND 3),
    -- The process version the run pinned (CONTRACT §21). The step pins its SOP's version.
    ADD COLUMN process_version bigint NOT NULL DEFAULT 1;
ALTER TABLE run
    ADD CONSTRAINT run_parent_fk FOREIGN KEY (parent_run_id) REFERENCES run (id),
    ADD CONSTRAINT run_step_fk   FOREIGN KEY (step_id) REFERENCES process_step (id),
    ADD CONSTRAINT run_parent_step_check CHECK (parent_run_id IS NULL OR step_id IS NOT NULL);

-- Root-only now: a child must never take the card's live slot (that was the coach's blocker).
DROP INDEX uq_run_live;
CREATE UNIQUE INDEX uq_run_live
    ON run (tenant_id, card_id)
    WHERE parent_run_id IS NULL AND status IN ('queued', 'running', 'waiting_approval');
DROP INDEX uq_run_project_live;
CREATE UNIQUE INDEX uq_run_project_live
    ON run (tenant_id, project_id)
    WHERE parent_run_id IS NULL AND card_id IS NULL
      AND status IN ('queued', 'running', 'waiting_approval');
-- One live child per (parent, step); one row per attempt.
CREATE UNIQUE INDEX uq_run_child_live
    ON run (parent_run_id, step_id)
    WHERE status IN ('queued', 'running', 'waiting_approval');
CREATE UNIQUE INDEX uq_run_child_attempt
    ON run (tenant_id, parent_run_id, step_id, attempt);

ALTER TABLE run_step
    ADD COLUMN judge jsonb,
    ADD COLUMN procedure_version bigint;

-- ── 5. capability kinds and agent roles (S15's tables, amended by the process model) ─────
ALTER TABLE capability
    ADD COLUMN kind text NOT NULL DEFAULT 'tenant' CHECK (kind IN ('system', 'tenant'));

ALTER TABLE capability_agent
    ADD COLUMN role        text,
    ADD COLUMN process_key text,
    ADD COLUMN toolset     text[] NOT NULL DEFAULT '{}';
-- The 17 catalogue agents predate the role; backfill before the CHECK exists.
UPDATE capability_agent SET role = 'process_agent', process_key = key WHERE role IS NULL;
ALTER TABLE capability_agent
    ALTER COLUMN role SET NOT NULL,
    ALTER COLUMN role SET DEFAULT 'process_agent';
ALTER TABLE capability_agent
    ADD CONSTRAINT capability_agent_role_check CHECK (role IN ('process_agent', 'tool_agent')),
    ADD CONSTRAINT capability_agent_process_key_check
        CHECK (role <> 'process_agent' OR process_key IS NOT NULL) NOT VALID;
ALTER TABLE capability_agent VALIDATE CONSTRAINT capability_agent_process_key_check;

-- ── 6. free-text values that named the old meaning ──────────────────────────────────────
-- `approval.kind='procedure'` meant "opened by a procedure run's gate"; the same gate now
-- belongs to a process. `job.kind` must move with the worker arm or a pending job stalls.
UPDATE approval SET kind = 'process' WHERE kind = 'procedure';
UPDATE job      SET kind = 'process.step' WHERE kind = 'procedure.step';

-- The prompt catalogue (D27) renders a process definition under the source kind whose name
-- followed the old table name. The code now writes `process`; the CHECK and the seed rows move
-- with it, or every seeded prompt refuses on `prompt_section_source_kind_check`.
UPDATE prompt_section SET source_kind = 'process' WHERE source_kind = 'procedure';
ALTER TABLE prompt_section DROP CONSTRAINT prompt_section_source_kind_check;
ALTER TABLE prompt_section ADD CONSTRAINT prompt_section_source_kind_check
    CHECK (source_kind IN ('system', 'process', 'capability', 'role', 'project_card', 'knowledge'));

-- ── 7. the bus cursor ─────────────────────────────────────────────────────────────────────
-- The outbox relay publishes unpublished rows by seq and records the stream id. The index is
-- what the relay reads; a Redis loss re-delivers from these rows (the bus is a nudge).
ALTER TABLE event
    ADD COLUMN bus_stream_id text,
    ADD COLUMN published_at  timestamptz;
CREATE INDEX event_unpublished_idx ON event (tenant_id, seq) WHERE published_at IS NULL;

-- ── 8. the S17/S19 subject pins: the PROCESS is the measured subject, not the SOP ────────
ALTER TABLE benchmark_run DROP CONSTRAINT benchmark_run_subject;
ALTER TABLE benchmark_run DROP CONSTRAINT benchmark_run_subject_kind_check;
ALTER TABLE benchmark_run RENAME COLUMN procedure_id TO process_id;
UPDATE benchmark_run SET subject_kind = 'process' WHERE subject_kind = 'procedure';
ALTER TABLE benchmark_run
    ADD CONSTRAINT benchmark_run_subject_kind_check
        CHECK (subject_kind IN ('suite', 'capability_agent', 'process')),
    ADD CONSTRAINT benchmark_run_subject CHECK (
        (subject_kind = 'suite' AND capability_agent_id IS NULL AND process_id IS NULL)
        OR (subject_kind = 'capability_agent' AND capability_agent_id IS NOT NULL)
        OR (subject_kind = 'process' AND process_id IS NOT NULL));

ALTER TABLE eval DROP CONSTRAINT eval_subject_kind_check;
UPDATE eval SET subject_kind = 'process' WHERE subject_kind = 'procedure';
ALTER TABLE eval
    ADD CONSTRAINT eval_subject_kind_check
        CHECK (subject_kind IN ('completions', 'copilot', 'process', 'external', 'agent_run'));

-- ── 9. S16/D23's caller kind moves with the rename ───────────────────────────────────────
-- The train's 0033 (D23) added `procedure_step` to this CHECK. That value names the process
-- step executor, so it moves with the table — the CHECK is dropped before the data update and
-- re-added after, exactly as the `subject_kind` moves above.
ALTER TABLE inference_call DROP CONSTRAINT inference_call_caller_kind_check;
UPDATE inference_call SET caller_kind = 'process_step' WHERE caller_kind = 'procedure_step';
ALTER TABLE inference_call
    ADD CONSTRAINT inference_call_caller_kind_check
    CHECK (caller_kind IN ('copilot','process_step','workflow_step','agent_run','eval','other'));

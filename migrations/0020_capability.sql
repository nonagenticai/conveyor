-- 0020_capability.sql — S15 steps 1–4: the capability aggregate (SPEC D20, CONTRACT §17,
-- data-arch §2.15).
--
-- Five tables: the aggregate and its relation edges, the configured executor (configuration, not a
-- loop — CONTRACT §17), the versioned knowledge binding, and hot work-package memory. The strata
-- client, `knowledge.query`, `memory.*` and the run-readiness seam are S15 steps 6–13 and add no
-- table here.
--
-- `run.capability_id` / `run.capability_agent_id` are already columns from 0021; this migration adds
-- only the foreign keys they point at. Do not add them again.
--
-- `capability` and `capability_agent` are mutable aggregates, so they carry `version` (CONTRACT §3).
-- `capability_knowledge` is a binding row (updated in place, no If-Match), `capability_relation` and
-- `work_package_memory` are append-only facts and carry `created_at` only.

CREATE TABLE capability (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    code        text NOT NULL,
    label       text NOT NULL,
    description text NOT NULL DEFAULT '',
    status      text NOT NULL DEFAULT 'draft'
                CHECK (status IN ('draft', 'active', 'retired')),
    meta        jsonb NOT NULL DEFAULT '{}'::jsonb,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT capability_code_not_blank  CHECK (btrim(code) <> ''),
    CONSTRAINT capability_label_not_blank CHECK (btrim(label) <> ''),
    CONSTRAINT capability_tenant_code_key UNIQUE (tenant_id, code)
);
CREATE INDEX capability_status_idx ON capability (tenant_id, status);

CREATE TABLE capability_agent (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    capability_id   uuid NOT NULL,
    key             text NOT NULL,
    status          text NOT NULL DEFAULT 'draft'
                    CHECK (status IN ('draft', 'active', 'retired')),
    model_ref       text NOT NULL,
    prompt_ref      text,
    params          jsonb NOT NULL DEFAULT '{}'::jsonb,
    allowed_actions text[] NOT NULL DEFAULT '{}',
    -- The slice's ceiling is 20 iterations and 300 seconds. The CHECKs are the database's copy of
    -- that ceiling; the domain still validates first so a bad input is a 422, not a 500.
    max_iterations  int NOT NULL DEFAULT 1
                    CHECK (max_iterations BETWEEN 1 AND 20),
    timeout_secs    int NOT NULL DEFAULT 300
                    CHECK (timeout_secs BETWEEN 1 AND 300),
    version         bigint NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT capability_agent_capability_fk FOREIGN KEY (capability_id) REFERENCES capability (id),
    CONSTRAINT capability_agent_key_not_blank CHECK (btrim(key) <> ''),
    CONSTRAINT capability_agent_model_ref_not_blank CHECK (btrim(model_ref) <> ''),
    CONSTRAINT capability_agent_tenant_key_key UNIQUE (tenant_id, key)
);
CREATE INDEX capability_agent_capability_idx
    ON capability_agent (tenant_id, capability_id, status);

CREATE TABLE capability_knowledge (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    capability_id uuid NOT NULL,
    pack_key      text NOT NULL,
    pack_version  int NOT NULL CHECK (pack_version > 0),
    min_score     numeric,
    required      boolean NOT NULL DEFAULT true,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT capability_knowledge_capability_fk FOREIGN KEY (capability_id) REFERENCES capability (id),
    CONSTRAINT capability_knowledge_pack_key_not_blank CHECK (btrim(pack_key) <> ''),
    CONSTRAINT capability_knowledge_unique UNIQUE (tenant_id, capability_id, pack_key, pack_version)
);

CREATE TABLE capability_relation (
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    from_capability_id uuid NOT NULL,
    to_capability_id   uuid NOT NULL,
    kind               text NOT NULL CHECK (kind IN ('requires', 'enables', 'concern')),
    created_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT capability_relation_pk PRIMARY KEY (from_capability_id, to_capability_id, kind),
    CONSTRAINT capability_relation_from_fk FOREIGN KEY (from_capability_id) REFERENCES capability (id),
    CONSTRAINT capability_relation_to_fk   FOREIGN KEY (to_capability_id) REFERENCES capability (id),
    CONSTRAINT capability_relation_not_self CHECK (from_capability_id <> to_capability_id)
);

CREATE TABLE work_package_memory (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    project_id      uuid NOT NULL,
    card_id         uuid NOT NULL,
    run_id          uuid,
    capability_id   uuid,
    kind            text NOT NULL CHECK (kind IN ('fact', 'attempt', 'decision', 'blocker', 'digest')),
    source          text NOT NULL CHECK (source IN ('agent', 'human', 'system')),
    body            text NOT NULL,
    supersedes_id   uuid,
    consolidated_at timestamptz,
    meta            jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT work_package_memory_project_fk    FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT work_package_memory_card_fk       FOREIGN KEY (card_id) REFERENCES card (id),
    CONSTRAINT work_package_memory_run_fk        FOREIGN KEY (run_id) REFERENCES run (id),
    CONSTRAINT work_package_memory_capability_fk FOREIGN KEY (capability_id) REFERENCES capability (id),
    CONSTRAINT work_package_memory_supersedes_fk FOREIGN KEY (supersedes_id) REFERENCES work_package_memory (id),
    CONSTRAINT work_package_memory_body_not_blank CHECK (btrim(body) <> '')
);
CREATE INDEX work_package_memory_card_idx ON work_package_memory (tenant_id, card_id, created_at);
CREATE INDEX work_package_memory_hot_idx  ON work_package_memory (tenant_id)
    WHERE consolidated_at IS NULL;

-- The default capability path: a card type may name its capability, a procedure may override it.
-- Both are nullable by the owner's 2026-11-09 ruling; no backfill is possible — the 17 capability
-- codes do not map cleanly onto the 74 seeded card-type codes, and a guessed binding is worse than a
-- NULL one. The readiness/admin step binds them.
ALTER TABLE card_type ADD COLUMN capability_id uuid;
ALTER TABLE card_type ADD CONSTRAINT card_type_capability_fk
    FOREIGN KEY (capability_id) REFERENCES capability (id);

ALTER TABLE procedure ADD COLUMN capability_id uuid;
ALTER TABLE procedure ADD CONSTRAINT procedure_capability_fk
    FOREIGN KEY (capability_id) REFERENCES capability (id);

-- ⚠️ `run.capability_id` / `run.capability_agent_id` are 0021's columns, and 0021 runs AFTER this
-- migration on a fresh database. Their foreign keys therefore live in `0027_run_capability_fks.sql`;
-- adding them here fails with 42703 on a fresh DB (measured 2026-11-09).

-- The 17 catalogue rows become capability rows, then the old vocabulary is removed.
-- ⚠️ `catalogue_entry` is FORCE-RLS. A migration that runs as the table owner (not a superuser)
-- would see zero rows here and delete zero — a silent no-op. Drop FORCE for the move only; the
-- migration is transactional, so a failure restores it.
ALTER TABLE catalogue_entry NO FORCE ROW LEVEL SECURITY;
INSERT INTO capability (tenant_id, code, label, description, status, meta)
SELECT tenant_id,
       code,
       label,
       coalesce(meta ->> 'description', ''),
       CASE WHEN is_active THEN 'active' ELSE 'retired' END,
       coalesce(meta, '{}'::jsonb) - 'description'
FROM catalogue_entry
WHERE kind IN ('capability');
DELETE FROM catalogue_entry WHERE kind IN ('capability');
ALTER TABLE catalogue_entry FORCE ROW LEVEL SECURITY;

-- FORCE is what makes the schema owner subject to the policy too (data-arch §2.11's rule).
ALTER TABLE capability           ENABLE ROW LEVEL SECURITY;
ALTER TABLE capability           FORCE  ROW LEVEL SECURITY;
ALTER TABLE capability_agent     ENABLE ROW LEVEL SECURITY;
ALTER TABLE capability_agent     FORCE  ROW LEVEL SECURITY;
ALTER TABLE capability_knowledge ENABLE ROW LEVEL SECURITY;
ALTER TABLE capability_knowledge FORCE  ROW LEVEL SECURITY;
ALTER TABLE capability_relation  ENABLE ROW LEVEL SECURITY;
ALTER TABLE capability_relation  FORCE  ROW LEVEL SECURITY;
ALTER TABLE work_package_memory  ENABLE ROW LEVEL SECURITY;
ALTER TABLE work_package_memory  FORCE  ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON capability FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON capability_agent FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON capability_knowledge FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON capability_relation FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON work_package_memory FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

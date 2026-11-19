-- 0058_pko.sql — the Procedural Knowledge Ontology (T16, issue #85; SPEC_V2 §5.6, §10.3)
--
-- The PKO is relational, conveyor-owned and tenant-editable. A `pko_release` is immutable: it
-- pins the `pko_entity` and `pko_relation` rows that belong to that release, layered
-- base → industry → department → job. A tenant edit never mutates a release; it writes a new
-- release with a new content hash. A procedure's published version pins one release.
--
-- Two spaces, never mixed (SPEC_V2 §5.6, D53): an entity is knowledge (name, label, kind,
-- attributes, synonyms) and never carries an executable step; the executable form is the
-- procedure's PDDL step, which references PKO terms by name.
--
-- Forward-only: no backfill, no rewrite of 0035's `procedure` table beyond the pin column.

-- ── 1. pko_release — one immutable release per layer ────────────────────────────────────────
CREATE TABLE pko_release (
    id               uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id        uuid NOT NULL DEFAULT app_current_tenant(),
    key              text NOT NULL,
    title            text NOT NULL,
    -- The layer the release belongs to. Resolution is narrowest-first: job → department →
    -- industry → base (a narrower layer overrides a broader one for the same term).
    layer            text NOT NULL DEFAULT 'base'
                     CHECK (layer IN ('base', 'industry', 'department', 'job')),
    -- The SHA-256 of the canonicalised release content (entities + relations + metadata).
    content_hash     text NOT NULL,
    -- draft → published → superseded. A published release is immutable; a correction writes a
    -- new release, never an update of the old one.
    status           text NOT NULL DEFAULT 'draft'
                     CHECK (status IN ('draft', 'published', 'superseded')),
    -- The governed asset (kind='sop_source') the release was converted from, when it came from
    -- an ingested source. A release authored directly carries no source.
    source_asset_id  uuid REFERENCES governed_asset (id),
    version          bigint NOT NULL DEFAULT 1,
    lock_version     bigint NOT NULL DEFAULT 1,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT pko_release_key_not_blank   CHECK (btrim(key) <> ''),
    CONSTRAINT pko_release_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT pko_release_hash_not_blank  CHECK (btrim(content_hash) <> ''),
    CONSTRAINT pko_release_tenant_key_version_key UNIQUE (tenant_id, key, version)
);
-- One published release per (key, layer); the draft and its published row coexist.
CREATE UNIQUE INDEX uq_pko_release_published
    ON pko_release (tenant_id, key, layer) WHERE status = 'published';
CREATE UNIQUE INDEX uq_pko_release_draft
    ON pko_release (tenant_id, key, layer) WHERE status = 'draft';
CREATE INDEX pko_release_layer_idx ON pko_release (tenant_id, layer, status);

ALTER TABLE pko_release ENABLE ROW LEVEL SECURITY;
ALTER TABLE pko_release FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON pko_release FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 2. pko_entity — the terms a release pins ───────────────────────────────────────────────
CREATE TABLE pko_entity (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    release_id  uuid NOT NULL REFERENCES pko_release (id) ON DELETE CASCADE,
    -- The term itself, unique within its release.
    name        text NOT NULL,
    label       text NOT NULL DEFAULT '',
    -- task | step | role | artefact | constraint | term (the ontology's kinds).
    kind        text NOT NULL DEFAULT 'term'
                CHECK (kind IN ('task', 'step', 'role', 'artefact', 'constraint', 'term')),
    attributes  jsonb NOT NULL DEFAULT '{}'::jsonb,
    synonyms    jsonb NOT NULL DEFAULT '[]'::jsonb,
    version     bigint NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT pko_entity_name_not_blank CHECK (btrim(name) <> ''),
    CONSTRAINT pko_entity_release_name_key UNIQUE (tenant_id, release_id, name)
);
CREATE INDEX pko_entity_release_idx ON pko_entity (tenant_id, release_id, name);

ALTER TABLE pko_entity ENABLE ROW LEVEL SECURITY;
ALTER TABLE pko_entity FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON pko_entity FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 3. pko_relation — the edges a release pins ─────────────────────────────────────────────
CREATE TABLE pko_relation (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    release_id         uuid NOT NULL REFERENCES pko_release (id) ON DELETE CASCADE,
    subject_entity_id  uuid NOT NULL REFERENCES pko_entity (id) ON DELETE CASCADE,
    predicate          text NOT NULL,
    object_entity_id   uuid NOT NULL REFERENCES pko_entity (id) ON DELETE CASCADE,
    -- many_to_one | one_to_many | many_to_many | one_to_one, when the predicate declares it.
    cardinality        text NOT NULL DEFAULT 'many_to_many'
                       CHECK (cardinality IN ('one_to_one', 'one_to_many', 'many_to_one', 'many_to_many')),
    version            bigint NOT NULL DEFAULT 1,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT pko_relation_predicate_not_blank CHECK (btrim(predicate) <> ''),
    CONSTRAINT pko_relation_distinct_ends CHECK (subject_entity_id <> object_entity_id),
    CONSTRAINT pko_relation_release_edge_key
        UNIQUE (tenant_id, release_id, subject_entity_id, predicate, object_entity_id)
);
CREATE INDEX pko_relation_release_idx ON pko_relation (tenant_id, release_id, predicate);

ALTER TABLE pko_relation ENABLE ROW LEVEL SECURITY;
ALTER TABLE pko_relation FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON pko_relation FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 4. procedure — the published version pins one PKO release ──────────────────────────────
-- SPEC_V2 §10.3: `procedure`/`procedure_step` extend with PDDL action text and a PKO pin. The
-- PDDL text and step preconditions/effects are the T16 execution half; the pin is the column
-- that makes a published procedure reproducible (the pinned release cannot be mutated).
ALTER TABLE procedure
    ADD COLUMN pko_release_id uuid REFERENCES pko_release (id),
    ADD COLUMN pddl_domain    text NOT NULL DEFAULT '';

-- A published procedure may name a release; a refused pin (a draft release) is a store rule,
-- because a check constraint cannot see the release's status without a trigger.
COMMENT ON COLUMN procedure.pko_release_id IS
    'The pinned PKO release; set only when the procedure is published and the release is published (T16, #85).';
COMMENT ON COLUMN procedure.pddl_domain IS
    'The PDDL domain the procedure''s steps are written against (T16, #85).';

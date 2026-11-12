-- 0030_prompt_catalogue.sql — the prompt catalogue and its section layout (SPEC D27, owner
-- ruling 2026-11-09).
--
-- A prompt is not a stored text blob: it is COMPILED at run time from the run's own sources —
-- the procedure or workflow (its steps and gates), the capability/agent configuration, the
-- project and card, the capability's knowledge packs, and the tenant's system section. `prompt`
-- names the recipe; `prompt_section` is its ordered layout.
--
-- The compiled text is deliberately NOT stored: `inference_call` already carries
-- `prompt_ref`/`prompt_version`/`prompt_sha256` for every call, and a second stored copy would
-- drift from the sources it was built from. A stored recipe plus the recorded hash is what makes
-- a past call reproducible.
--
-- CONTRACT §17 previously said `prompt_ref` resolved a built-in, compiled-in prompt with no
-- store. The owner ruled 2026-11-09 that prompts live in Postgres and are compiled from the
-- procedure and system/tenant/project sources; §17 is amended with this migration.
--
-- One active version per (tenant, key) is a partial unique index, not an application check: a
-- second activation fails even under a race. Older versions stay readable by exact pin (an
-- `inference_call` names the version that produced it), and a new run may pin any non-retired
-- version.
--
-- Grants are NOT repeated here: apps/server's migrate re-applies scripts/grants-runtime.sql after
-- the last migration, which is what makes a later slice's tables visible to `conveyor_runtime`
-- (0001's comment states the rule; 0013 has no grant block either).

CREATE TABLE prompt (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    key         text NOT NULL,
    version     int  NOT NULL CHECK (version >= 1),
    -- 'active' | 'retired'. Retiring is how a version stops being usable for a NEW run; the row
    -- itself stays, because an `inference_call` that names it must stay explicable.
    status      text NOT NULL DEFAULT 'active' CHECK (status IN ('active','retired')),
    title       text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT prompt_key_not_blank   CHECK (btrim(key) <> ''),
    CONSTRAINT prompt_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT prompt_tenant_key_version_key UNIQUE (tenant_id, key, version)
);

-- At most one active version per key. `status = 'retired'` rows are unconstrained, so the whole
-- history stays addressable by `(key, version)`.
CREATE UNIQUE INDEX uq_prompt_active ON prompt (tenant_id, key) WHERE status = 'active';

CREATE TABLE prompt_section (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    prompt_id   uuid NOT NULL,
    position    int  NOT NULL CHECK (position >= 1),
    -- What this section is rendered FROM. `system` is the only kind whose text is literal; the
    -- others are built from the run's facts, and a `required` one that cannot be resolved is a
    -- readiness refusal rather than an empty section.
    source_kind text NOT NULL
                CHECK (source_kind IN ('system','procedure','capability','project_card','knowledge')),
    required    boolean NOT NULL DEFAULT false,
    template    text NOT NULL DEFAULT '',
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT prompt_section_prompt_fk
        FOREIGN KEY (prompt_id) REFERENCES prompt (id) ON DELETE CASCADE,
    CONSTRAINT prompt_section_position_key UNIQUE (prompt_id, position),
    -- A `system` section with no text would render nothing and silently vanish from the prompt.
    CONSTRAINT prompt_section_system_has_text
        CHECK (source_kind <> 'system' OR btrim(template) <> '')
);

CREATE INDEX prompt_section_prompt_idx ON prompt_section (tenant_id, prompt_id, position);

-- Row level security: ENABLE + FORCE, one FOR ALL policy with USING and WITH CHECK each (0001's
-- rule). Without FORCE the schema owner reads every tenant's rows and an isolation test running
-- as the owner passes while production leaks.
ALTER TABLE prompt         ENABLE ROW LEVEL SECURITY;
ALTER TABLE prompt         FORCE  ROW LEVEL SECURITY;
ALTER TABLE prompt_section ENABLE ROW LEVEL SECURITY;
ALTER TABLE prompt_section FORCE  ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON prompt FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON prompt_section FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

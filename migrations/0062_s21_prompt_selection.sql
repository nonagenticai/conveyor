-- 0062_s21_prompt_selection.sql — S21 step 3 (issue #15): the recipe lifecycle, the selection
-- config and the memory source kind.
--
-- 1. `prompt.status` gains `draft`. The default stays `active`, so the built-in seed and every
--    existing row are unchanged; a tenant-authored version is inserted `draft` explicitly by
--    `prompt_recipes.create`. One draft and one active version per (tenant, key) are partial
--    unique indexes, so a second activation under a race is refused by Postgres, not by a
--    read-then-write check in the action.
-- 2. `prompt_section.selector` is the one selection config. It is a JSON object, never a second
--    template language: memory names `{kinds, limit, include_digest}`, knowledge names `{query}`.
--    The empty object means the source's default selection.
-- 3. `memory` joins the section kinds. It is rendered from the card's `work_package_memory` rows
--    (hot first, then one digest), never from `strata_rag` directly.
--
-- The compiled text is still never stored: `inference_call` keeps the recipe pin and the hash.
-- Grants are re-applied by migrate after the last migration, exactly as 0030 states.

ALTER TABLE prompt DROP CONSTRAINT prompt_status_check;
ALTER TABLE prompt ADD CONSTRAINT prompt_status_check
    CHECK (status IN ('draft', 'active', 'retired'));

CREATE UNIQUE INDEX uq_prompt_draft ON prompt (tenant_id, key) WHERE status = 'draft';

ALTER TABLE prompt_section DROP CONSTRAINT prompt_section_source_kind_check;
ALTER TABLE prompt_section ADD CONSTRAINT prompt_section_source_kind_check
    CHECK (source_kind IN
        ('system', 'process', 'capability', 'role', 'project_card', 'knowledge', 'memory'));

ALTER TABLE prompt_section ADD COLUMN selector jsonb NOT NULL DEFAULT '{}';
ALTER TABLE prompt_section ADD CONSTRAINT prompt_section_selector_object
    CHECK (jsonb_typeof(selector) = 'object');

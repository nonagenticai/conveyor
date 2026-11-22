-- 0069_programme_shared_config.sql — the programme's shared configuration (SPEC_V2 §8.2).
--
-- A programme carries `shared_config` beside its dependency graph, cadence and standing
-- backlog. The shape is `{ "values": { key: value }, "locked": [key, ...] }`; a flat object is
-- read as values with no locks. A locked key keeps the programme's value when a project
-- override is resolved (v1-p82-3), so the lock is a rule on the row, not a convention in prose.
ALTER TABLE programme
    ADD COLUMN shared_config jsonb NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE programme
    ADD CONSTRAINT programme_shared_config_shape
    CHECK (
        jsonb_typeof(shared_config) = 'object'
        AND (NOT shared_config ? 'values'
             OR jsonb_typeof(shared_config -> 'values') = 'object')
        AND (NOT shared_config ? 'locked'
             OR jsonb_typeof(shared_config -> 'locked') = 'array')
    );

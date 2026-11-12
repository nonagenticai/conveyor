-- 0031_s21_prompt_budget.sql — S21 step 1: the budgeted compile and the role component (SPEC D27,
-- slice S21 §4).
--
-- 0030 built the recipe; this migration gives it two things the compile pipeline needs:
--
--   1. A BUDGET the recipe itself names. `budget_chars` is a character ceiling, not a token one:
--      tokens are provider-dependent, so v1 measures characters deterministically and leaves the
--      provider's actual count to `inference_call.prompt_tokens`. NULL means unbounded, which is
--      what every seeded recipe stays — the mechanism is opt-in until a recipe names a ceiling.
--      Over budget is a REFUSAL carrying the measured size, never a truncation (slice §4.4).
--
--   2. The `role` source kind. The slice names role knowledge as the third component: the
--      perspective the agent works from, sourced from the run's capability binding (the
--      `capability_agent` row), never from the caller's scopes — scopes are an authorisation fact,
--      not context. The rendered text comes from `capability_agent.params->>'role'` in v1; a
--      first-class column would change the `capability_agents.*` action schemas, which this step
--      deliberately does not touch (no action, scope or route moves here).
--
-- Grants are NOT repeated: apps/server's migrate re-applies scripts/grants-runtime.sql after the
-- last migration (0001's rule, restated by 0030).

ALTER TABLE prompt ADD COLUMN budget_chars int;
ALTER TABLE prompt ADD CONSTRAINT prompt_budget_chars_positive
    CHECK (budget_chars IS NULL OR budget_chars > 0);

ALTER TABLE prompt_section ADD COLUMN budget_chars int;
ALTER TABLE prompt_section ADD CONSTRAINT prompt_section_budget_chars_positive
    CHECK (budget_chars IS NULL OR budget_chars > 0);

-- The inline CHECK in 0030 is auto-named prompt_section_source_kind_check; replace it so an
-- existing database and a fresh one carry the same constraint text.
ALTER TABLE prompt_section DROP CONSTRAINT prompt_section_source_kind_check;
ALTER TABLE prompt_section ADD CONSTRAINT prompt_section_source_kind_check
    CHECK (source_kind IN ('system', 'procedure', 'capability', 'role', 'project_card', 'knowledge'));

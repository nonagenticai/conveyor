-- 0032_s21_plan_outputs.sql — S21 step 2: declared step outputs and the plan projection (slice S21 §6).
--
-- The plan projection reads a published procedure as a dependency graph: a step's input may
-- reference an EARLIER step's output, its preconditions are that reference plus its gate, and its
-- effects are what it declares it exposes. Until now the DSL had no declaration, so "input
-- references an output no earlier step produces" had nothing to check against.
--
-- `outputs` is an array of dot-separated paths into the step's action output — for example
-- ["card.id", "list_id"], or ["id"]. An empty array is the default and means the step exposes
-- nothing: a later step that references it is refused at publish. A declared path must itself
-- resolve in the action's output schema, so a declaration cannot name a field the action never
-- returns.
--
-- Publish does not copy step rows (store::procedure::publish_tx flips the status only), so this
-- one column covers both a draft and its published version; the frozen revision snapshot carries
-- the declaration with the rest of StepInput.
--
-- The default is the strict reading: a step that does not name its outputs exposes none, so an
-- existing DRAFT that references a step output must add the declaration before it can be published.
-- Existing PUBLISHED versions are unaffected — the runtime resolution is unchanged, and publishing
-- an already-published version is a no-op.
--
-- No new action, scope or route. The served-spec change is confined to the six `procedures.*`
-- operations, whose StepIn/StepOut schemas gain the member; the lock is regenerated with the slice.
--
-- Grants are not repeated: apps/server's migrate re-applies scripts/grants-runtime.sql after the
-- last migration (0001's rule, restated by 0030 and 0031).

ALTER TABLE procedure_step
    ADD COLUMN outputs jsonb NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE procedure_step
    ADD CONSTRAINT procedure_step_outputs_is_array
    CHECK (jsonb_typeof(outputs) = 'array');

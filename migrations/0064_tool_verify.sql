-- 0064_tool_verify.sql — a write tool's declared verification path travels with its version
-- (SPEC_V2 §11.3; finding v1-w4a-2, conveyor#526).
--
-- A `write`/`destructive` tool must declare how the broker proves the write happened — a
-- `read_back` tool in the same toolkit or a `verdict` + `checks` pair on the synchronous answer.
-- Validation refuses a manifest that does not. The declaration is copied onto each tool row so
-- the broker reads it from the version it resolved, never from the manifest the publish wave
-- happened to carry.
--
-- `'{}'` on a read tool: the absence is data every reader can rely on, not a NULL each reader
-- must special-case.
--
-- Column only: the table's existing tenant policy and table-level grants cover the new column,
-- so no RLS or grant change is made here. Forward only: never edited after this slice closes
-- (CONTRACT §2).

ALTER TABLE tool
    ADD COLUMN verify jsonb NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE tool
    ADD CONSTRAINT tool_verify_is_object
    CHECK (jsonb_typeof(verify) = 'object');

COMMENT ON COLUMN tool.verify IS
    'The write verification declaration (§11.3): verdict + checks, or read_back naming a read tool in this toolkit. `{}` on a non-writing tool.';

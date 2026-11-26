-- 0077_process_step_tool_pin.sql — the step's tool pin (issue #778, T13 / v1-p116-1).
--
-- SPEC_V2 §11.6 item 3: a procedure step that needs an operation with no tool opens a
-- tool-request card; a development job implements it, publishes the version, and the step is
-- rebound to that pin. Until now `process_step` had nowhere to record a pin, so "rebound" had no
-- storage.
--
-- Two nullable columns, not a new table: a step that resolves to a declared engine action never
-- carries one, and a step whose operation has no tool yet carries none until the tool is
-- published. `tool_version` is the toolkit version the pin was taken at — recorded rather than
-- derived, because a later publication must not silently move a step a run already trusted.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

ALTER TABLE process_step
    ADD COLUMN tool_id      uuid REFERENCES tool (id),
    ADD COLUMN tool_version bigint;

COMMENT ON COLUMN process_step.tool_id IS
    'The published tool this step was rebound to (§11.6.3); NULL until a tool exists for its action.';
COMMENT ON COLUMN process_step.tool_version IS
    'The toolkit version the pin was taken at; NULL whenever tool_id is NULL.';

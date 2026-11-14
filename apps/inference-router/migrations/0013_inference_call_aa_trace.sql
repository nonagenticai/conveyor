-- inference_call: the AA-TRACE join keys and the prompt version (alembic 0013).
-- Ids and hashes only; no text is stored here. The CHECKs are a backstop — the router validates
-- every header before the write and stores NULL for a malformed one, which is strictly tighter.
SET LOCAL lock_timeout = '5s';

ALTER TABLE inference_call ADD COLUMN trace_id uuid;
ALTER TABLE inference_call ADD COLUMN card_id text;
ALTER TABLE inference_call ADD COLUMN project_id uuid;
ALTER TABLE inference_call ADD COLUMN parent_execution_id text;
ALTER TABLE inference_call ADD COLUMN prompt_version_id text;
ALTER TABLE inference_call ADD COLUMN prompt_body_sha256 char(64);

ALTER TABLE inference_call ADD CONSTRAINT ck_inference_call_card_id_len
    CHECK (char_length(card_id) <= 256) NOT VALID;
ALTER TABLE inference_call ADD CONSTRAINT ck_inference_call_parent_execution_id_len
    CHECK (char_length(parent_execution_id) <= 256) NOT VALID;
ALTER TABLE inference_call ADD CONSTRAINT ck_inference_call_prompt_version_id_len
    CHECK (char_length(prompt_version_id) <= 256) NOT VALID;
ALTER TABLE inference_call ADD CONSTRAINT ck_inference_call_prompt_body_sha256_hex
    CHECK (prompt_body_sha256 ~ '^[0-9a-f]{64}$') NOT VALID;

CREATE INDEX ix_inference_call_trace_id ON inference_call (trace_id)
    WHERE trace_id IS NOT NULL;
CREATE INDEX ix_inference_call_card_id ON inference_call (card_id)
    WHERE card_id IS NOT NULL;

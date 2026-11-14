-- inference_call: the exact request/response bytes and their hash (alembic 0011).
-- The payload partial index is rebuilt so its predicate covers the text columns too; a partial
-- index is only usable when the query predicate implies the index predicate.
SET LOCAL lock_timeout = '5s';

ALTER TABLE inference_call ADD COLUMN request_text text;
ALTER TABLE inference_call ADD COLUMN response_text text;
ALTER TABLE inference_call ADD COLUMN request_text_sha256 char(64);

DROP INDEX ix_inference_call_captured_created_at;
CREATE INDEX ix_inference_call_captured_created_at ON inference_call (created_at)
    WHERE request_json IS NOT NULL OR response_json IS NOT NULL
       OR request_text IS NOT NULL OR response_text IS NOT NULL;

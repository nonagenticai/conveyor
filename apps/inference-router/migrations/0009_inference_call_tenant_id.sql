-- inference_call.tenant_id and the payload partial index (alembic 0009).
ALTER TABLE inference_call ADD COLUMN tenant_id uuid;

CREATE INDEX ix_inference_call_captured_created_at ON inference_call (created_at)
    WHERE request_json IS NOT NULL OR response_json IS NOT NULL;

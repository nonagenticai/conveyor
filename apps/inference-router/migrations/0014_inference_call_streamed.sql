-- inference_call: the streaming columns and the request-id idempotency key (alembic 0014).
SET LOCAL lock_timeout = '5s';

ALTER TABLE inference_call ADD COLUMN request_id uuid;
ALTER TABLE inference_call ADD COLUMN streamed boolean DEFAULT false;
ALTER TABLE inference_call ADD COLUMN stream_status text;
ALTER TABLE inference_call ADD COLUMN response_json_valid boolean;

ALTER TABLE inference_call ADD CONSTRAINT ck_inference_call_stream_status
    CHECK (stream_status IN ('completed', 'client_disconnect', 'upstream_error', 'timeout')) NOT VALID;

CREATE UNIQUE INDEX ux_inference_call_request_id ON inference_call (request_id)
    WHERE request_id IS NOT NULL;

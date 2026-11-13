-- inference_call — the ledger row of every inference (alembic 0008).
CREATE TABLE inference_call (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    created_at timestamptz NOT NULL DEFAULT now(),
    correlation_id text,
    caller_worker text,
    requested_model text NOT NULL,
    served_provider text,
    served_model text,
    served_model_root text,
    temperature double precision,
    seed bigint,
    max_tokens integer,
    request_sha256 char(64) NOT NULL,
    response_sha256 char(64),
    latency_ms integer NOT NULL,
    status text NOT NULL,
    request_json jsonb,
    response_json jsonb
);

CREATE INDEX ix_inference_call_created_at ON inference_call (created_at);
CREATE INDEX ix_inference_call_correlation_id ON inference_call (correlation_id)
    WHERE correlation_id IS NOT NULL;

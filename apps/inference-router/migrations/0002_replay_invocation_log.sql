-- replay_invocation_log — one row per replay-mode inference (alembic 0002).
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE replay_invocation_log (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    correlation_id uuid NOT NULL,
    worker_name text NOT NULL,
    version integer NOT NULL,
    started_at timestamptz NOT NULL DEFAULT now(),
    latency_ms integer NOT NULL,
    prompt_tokens integer NOT NULL,
    completion_tokens integer NOT NULL,
    validation_passed boolean,
    judge_score numeric(6, 4)
);

CREATE INDEX replay_invocation_log_corr ON replay_invocation_log (correlation_id);
CREATE INDEX replay_invocation_log_worker ON replay_invocation_log (worker_name, started_at DESC);

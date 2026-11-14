-- prompt_canary_routing — the per-worker canary window (alembic 0001).
CREATE TABLE prompt_canary_routing (
    worker_name text PRIMARY KEY,
    active_version integer NOT NULL,
    canary_version integer,
    canary_percent integer NOT NULL DEFAULT 0,
    rollback_thresholds jsonb NOT NULL DEFAULT '{}'::jsonb,
    canary_started_at timestamptz,
    canary_ends_at timestamptz,
    CONSTRAINT ck_prompt_canary_routing_percent_range CHECK (canary_percent BETWEEN 0 AND 100)
);

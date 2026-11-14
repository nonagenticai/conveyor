-- canary_monitor_lease — one elected monitor (alembic 0005).
CREATE TABLE canary_monitor_lease (
    lease_name text PRIMARY KEY,
    holder_id text NOT NULL,
    acquired_at timestamptz NOT NULL DEFAULT now(),
    renewed_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);

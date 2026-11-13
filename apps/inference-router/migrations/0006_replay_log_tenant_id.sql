-- replay_invocation_log.tenant_id (alembic 0006).
-- The column is added with the unattributed default, then the default is dropped so every new
-- insert has to name its tenant explicitly.
ALTER TABLE replay_invocation_log
    ADD COLUMN tenant_id text NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';

ALTER TABLE replay_invocation_log ALTER COLUMN tenant_id DROP DEFAULT;

CREATE INDEX ix_replay_invocation_log_tenant_started_at
    ON replay_invocation_log (tenant_id, started_at DESC);

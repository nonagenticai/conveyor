-- replay payload columns (alembic 0003).
ALTER TABLE replay_invocation_log ADD COLUMN input jsonb;
ALTER TABLE replay_invocation_log ADD COLUMN expected jsonb;
ALTER TABLE replay_invocation_log ADD COLUMN actual jsonb;

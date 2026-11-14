-- replay version integer -> nullable text (alembic 0004).
ALTER TABLE replay_invocation_log
    ALTER COLUMN version TYPE text USING version::text,
    ALTER COLUMN version DROP NOT NULL;

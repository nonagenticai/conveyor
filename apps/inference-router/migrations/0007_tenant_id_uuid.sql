-- replay_invocation_log.tenant_id text -> uuid (alembic 0007).
--
-- The guard is kept verbatim: a non-uuid tenant fails the migration loudly instead of being
-- dropped or silently skipped. A fresh database has no rows, so it cannot trip; an existing
-- database either converts or refuses, never loses data.
DO $$
DECLARE
    bad_count bigint;
    sample text;
BEGIN
    SELECT count(*), min(tenant_id)
      INTO bad_count, sample
      FROM replay_invocation_log
     WHERE tenant_id IS NOT NULL
       AND tenant_id !~ '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$';
    IF bad_count > 0 THEN
        RAISE EXCEPTION
            'replay_invocation_log.tenant_id: % row(s) are not a uuid and cannot be cast (first offender: %). Triage these by hand - map each tenant NAME to its uuid in licenses.tenants and add an explicit backfill above this guard. Refusing to drop or skip rows.',
            bad_count, sample;
    END IF;
END $$;

ALTER TABLE replay_invocation_log ALTER COLUMN tenant_id TYPE uuid USING tenant_id::uuid;

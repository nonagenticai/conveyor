-- Row snapshot taken after each cron run. Two databases are reset from the SAME seeded
-- template before their run, so every timestamp the run did NOT write is byte-identical
-- between the Python and Rust databases. A timestamp the run DID write is detected by
-- compare.py as falling inside that run's [start, finish] window and normalized.
SELECT json_build_object(
  'license', (
    SELECT coalesce(json_agg(row_to_json(l) ORDER BY l.id), '[]'::json)
      FROM (
        SELECT id::text, status, revoked_at, updated_at, expires_at, deleted_at,
               metering_verified_at, token_budget_override, quota_override
          FROM license
      ) l
  ),
  'token', (
    SELECT coalesce(json_agg(row_to_json(t) ORDER BY t.id), '[]'::json)
      FROM (
        SELECT id::text, license_id::text, revoked, revoked_at, expires_at, last_used_at
          FROM token
      ) t
  )
);

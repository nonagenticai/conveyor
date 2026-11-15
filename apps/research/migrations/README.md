# research migrations

`0001_media_baseline.sql` — media-discovery schema (issue #118), squashed from reference alembic
revisions `efdf83adc2e4` -> `b4d2e8c17f39` -> `c7a1f4e9b820`; applied by `sqlx::migrate!("./migrations")`.
The reference has no RLS and no triggers; the baseline reproduces exactly that.

| table | row purpose |
|---|---|
| `media_collections` | a tenant's saved collection of media items |
| `media_items` | one discovered media item; the central catalogue row |
| `search_sessions` | one discovery/search run owned by a tenant |
| `collection_items` | collection -> item membership with `position` ordering |
| `session_items` | item surfaced by a session, with discovery method/adapter |
| `download_jobs` | async download of one item, carrying the submitter's `license_id` |
| `tenant_id_purge_audit` | audit of the legacy-tenant purge in `b4d2e8c17f39` |

Verified 2026-11-15 against the reference: `docker run -d --rm --name md-schema-verify -e POSTGRES_PASSWORD=postgres -p 127.0.0.1::5432 postgres:18`
then `PGPASSWORD=postgres psql "host=127.0.0.1 port=$PORT user=postgres dbname=postgres" -v ON_ERROR_STOP=1 -f apps/research/migrations/0001_media_baseline.sql`
then on both sides `pg_dump -s | grep -E '^(CREATE|ALTER) ' | sort` + `diff`, and a full `pg_dump -s` diff:
identical apart from the excluded `alembic_version`.

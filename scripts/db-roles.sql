-- scripts/db-roles.sql — create the two conveyor roles. SUPERUSER ONLY.
--
-- Local development and every test database run this; production's DBA does the equivalent
-- in S14. The migration does NOT create roles: the agent pattern's `DO` block swallowed
-- `insufficient_privilege`, so `migrate` passed with no runtime role and every GRANT was a
-- no-op. Here `migrate` asserts the roles exist and are unprivileged instead.
--
-- Passwords are LOCAL DEVELOPMENT values. A production deployment sets its own and never
-- checks this file in with a usable secret. The role names are the contract; the passwords
-- are not.
--
-- Pure SQL (no psql meta-commands) so both `psql -f` and the test harness can execute it.

-- ⚠️ EVERY STATEMENT BELOW THAT TOUCHES A SHARED CATALOG IS A CROSS-DATABASE RACE.
-- Each test process runs this script against its OWN database, but `pg_authid` (roles) and
-- the `pg_proc` row of the built-in `pg_catalog.ts_match_vq` are shared by ALL databases.
-- Two sessions updating the same row abort the loser with `XX000 tuple concurrently
-- updated` — measured 2026-11-11 on the run that switched CI to `cargo nextest` (one process
-- per test): 88 tests passed, then the harness died at this script.
--
-- ⛔ A `pg_advisory_xact_lock` CANNOT serialise them. Advisory locks are DATABASE-LOCAL —
-- measured the same day: a lock held on key 76010 in `postgres` does NOT conflict with the
-- same key in `db2`. The lock this file used to take serialised nothing across test
-- databases, and the per-process mutex in the harness cannot help either because nextest
-- runs every test in its own process.
--
-- The guard is the retry below: `XX000` is transient — the winner commits and the loser's
-- retry sees the committed row and succeeds. Every other SQLSTATE is re-raised.

DO $roles$
DECLARE
    attempt int := 0;
BEGIN
    LOOP
        attempt := attempt + 1;
        BEGIN
            -- A lost CREATE ROLE race lands here (duplicate_object); `insufficient_privilege`
            -- is NOT caught: that is the swallowed error that once let a migration pass with
            -- no runtime role and made every GRANT a no-op.
            BEGIN
                IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'conveyor_migrate') THEN
                    CREATE ROLE conveyor_migrate;
                END IF;
            EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL;
            END;
            BEGIN
                IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'conveyor_runtime') THEN
                    CREATE ROLE conveyor_runtime;
                END IF;
            EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL;
            END;

            -- Re-assert every attribute on every run: a hand-edited role must not stay
            -- privileged. This is the update that races; the retry covers it.
            ALTER ROLE conveyor_migrate
                LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS
                PASSWORD 'conveyor_migrate_dev';
            ALTER ROLE conveyor_runtime
                LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS
                PASSWORD 'conveyor_runtime_dev';
            EXIT;
        EXCEPTION
            WHEN OTHERS THEN
                IF SQLSTATE = 'XX000' AND attempt < 30 THEN
                    PERFORM pg_sleep(0.02 * attempt);
                ELSE
                    RAISE;
                END IF;
        END;
    END LOOP;
END
$roles$;

-- Per-database privileges. `current_database()` keeps this script usable against a test
-- database with a generated name.
DO $grants$
BEGIN
    EXECUTE format('GRANT CONNECT, CREATE ON DATABASE %I TO conveyor_migrate', current_database());
    EXECUTE format('GRANT CONNECT ON DATABASE %I TO conveyor_runtime', current_database());
END
$grants$;

-- The owner of the schema owns every table the migration creates, so runtime-grant and
-- policy management are the migrator's alone.
ALTER SCHEMA public OWNER TO conveyor_migrate;
GRANT USAGE ON SCHEMA public TO conveyor_runtime;

-- ── LEAKPROOF, without which no text-search index is usable under RLS ────────────────────
--
-- MEASURED on postgres:18 (2026-11-08). Every table here is FORCE ROW LEVEL SECURITY, and
-- the planner refuses an index whose qual is not leakproof while a policy applies. Four
-- operators a search would reach for are all `proleakproof = false` out of the box:
--
--     ts_match_vq          = false   -- tsvector @@ tsquery   (FTS)
--     texticlike           = false   -- ILIKE
--     similarity_op        = false   -- pg_trgm %
--     word_similarity_op   = false   -- pg_trgm <%
--
-- So `search.query` run as `conveyor_runtime` degraded to `Bitmap Index Scan on
-- ga_tenant` + a per-row filter — an O(rows-per-tenant) scan, silently. With the two
-- functions below marked leakproof the identical statement, role and policy produce
-- `BitmapOr -> Bitmap Index Scan on governed_asset_tsv_idx` and `..._title_trgm_idx`:
-- 1.6 ms over 50 000 rows instead of a full tenant sweep.
--
-- Marking them is a superuser decision and cannot live in a migration — `conveyor_migrate`
-- is NOSUPERUSER by the rule above, and that is deliberate. It lives here, in the file whose
-- entire contract is "run me as a superuser before migrating". Only these two are marked:
-- they are pure predicates that raise no error and take no data-dependent path, so the
-- leakproof promise is true. `texticlike` and `similarity_op` are left alone because nothing
-- in this schema indexes on them.
--
-- The assertion at the end is the point: a database provisioned without this step must FAIL
-- loudly here rather than serve search at O(n) forever.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

DO $leakproof$
DECLARE
    target  record;
    missing text;
    attempt int := 0;
BEGIN
    LOOP
        attempt := attempt + 1;
        BEGIN
            -- Only the functions NOT already leakproof: the first process on a fresh server
            -- changes them, every later one is a no-op. The `pg_proc` row of the BUILT-IN
            -- `pg_catalog.ts_match_vq` is shared by every database, so two processes that
            -- both see it unmarked still race — the retry below covers that.
            FOR target IN
                SELECT n.nspname, p.proname, pg_get_function_identity_arguments(p.oid) AS args
                  FROM pg_proc p
                  JOIN pg_namespace n ON n.oid = p.pronamespace
                 WHERE p.proname IN ('ts_match_vq', 'word_similarity_op')
                   AND NOT p.proleakproof
            LOOP
                EXECUTE format('ALTER FUNCTION %I.%I(%s) LEAKPROOF',
                               target.nspname, target.proname, target.args);
            END LOOP;
            EXIT;
        EXCEPTION
            WHEN OTHERS THEN
                IF SQLSTATE = 'XX000' AND attempt < 30 THEN
                    PERFORM pg_sleep(0.02 * attempt);
                ELSE
                    RAISE;
                END IF;
        END;
    END LOOP;

    SELECT string_agg(p.proname, ', ') INTO missing
      FROM pg_proc p
     WHERE p.proname IN ('ts_match_vq', 'word_similarity_op') AND NOT p.proleakproof;
    IF missing IS NOT NULL THEN
        RAISE EXCEPTION
            'still not LEAKPROOF after ALTER: % — text search cannot use a GIN index under RLS',
            missing;
    END IF;
END
$leakproof$;

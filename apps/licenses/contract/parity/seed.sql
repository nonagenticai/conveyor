-- Synthetic seed for the #45 P2b.1 Python-vs-Rust cron parity artifact.
-- No production identifiers: every uuid, name, realm, subject and token hash is invented.
-- All instants are RELATIVE to seed time, so the two databases are identical in shape and a
-- reset from the seeded template keeps every not-written-by-the-run timestamp byte-identical.
--
-- Apply after `conveyor-licenses migrate` against a fresh postgres:18 database:
--   psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f seed.sql
\set ON_ERROR_STOP on
BEGIN;

INSERT INTO tenant_owner (key, display_name) VALUES ('parity-owner', 'Parity Owner');

INSERT INTO tenants (id, name, keycloak_realm, created_at, byo_mode, kind, owner_key, budget_currency)
VALUES
  ('11111111-1111-4111-8111-111111111111', 'parity-alpha', 'parity-realm', now() - interval '400 days', 'local_allowed', 'customer', 'parity-owner', NULL),
  ('11111111-1111-4111-8111-222222222222', 'parity-money', 'parity-realm', now() - interval '400 days', 'local_allowed', 'customer', 'parity-owner', 'USD');

INSERT INTO app (id, key, name, revoked, created_at, audience)
VALUES ('22222222-2222-4222-8222-222222222222', 'parity-app', 'parity-app', false, now() - interval '400 days', 'platform');

INSERT INTO plan (id, app_id, code, name, revoked, created_at, rate_limit_rpm, rate_limit_burst, quota_monthly, token_budget_monthly)
VALUES ('33333333-3333-4333-8333-333333333333', '22222222-2222-4222-8222-222222222222',
        'parity-plan', 'parity-plan', false, now() - interval '400 days', 60, 10, 1000, 500000);

-- One subject per ACTIVE licence: uq_license_active_tenant_app_subject is partial on
-- status='active', and this seed deliberately exercises five active rows for tenant alpha.
INSERT INTO subject (id, keycloak_sub, keycloak_realm, tenant_id, created_at)
VALUES
  ('44444444-4444-4444-8444-444444444444', 'parity-sub-1', 'parity-realm', '11111111-1111-4111-8111-111111111111', now() - interval '400 days'),
  ('44444444-4444-4444-8444-444444444445', 'parity-sub-2', 'parity-realm', '11111111-1111-4111-8111-222222222222', now() - interval '400 days'),
  ('44444444-4444-4444-8444-444444444446', 'parity-sub-3', 'parity-realm', '11111111-1111-4111-8111-111111111111', now() - interval '400 days'),
  ('44444444-4444-4444-8444-444444444447', 'parity-sub-4', 'parity-realm', '11111111-1111-4111-8111-111111111111', now() - interval '400 days'),
  ('44444444-4444-4444-8444-444444444448', 'parity-sub-5', 'parity-realm', '11111111-1111-4111-8111-222222222222', now() - interval '400 days'),
  ('44444444-4444-4444-8444-444444444449', 'parity-sub-6', 'parity-realm', '11111111-1111-4111-8111-222222222222', now() - interval '400 days'),
  ('44444444-4444-4444-8444-44444444444a', 'parity-sub-7', 'parity-realm', '11111111-1111-4111-8111-111111111111', now() - interval '400 days'),
  ('44444444-4444-4444-8444-44444444444b', 'parity-sub-8', 'parity-realm', '11111111-1111-4111-8111-111111111111', now() - interval '400 days');

-- Licences cover every predicate leg: active-stale, active-live, perpetual, recently expired,
-- long expired, revoked, soft-deleted, expired with NULL expiry, stamped, override, money tenant.
INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, expires_at, created_at, updated_at,
                     revoked_at, tenant_id, metering_verified_at, deleted_at, delete_reason, deleted_from_status,
                     token_budget_override, quota_override)
VALUES
  ('aaaaaaaa-0000-4000-8000-000000000001', '44444444-4444-4444-8444-444444444444', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'active', now() - interval '40 days', now() - interval '10 days', now() - interval '40 days', now() - interval '40 days',
   NULL, '11111111-1111-4111-8111-111111111111', NULL, NULL, NULL, NULL, NULL, NULL),
  ('aaaaaaaa-0000-4000-8000-000000000002', '44444444-4444-4444-8444-444444444446', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'active', now() - interval '20 days', now() + interval '30 days', now() - interval '20 days', now() - interval '20 days',
   NULL, '11111111-1111-4111-8111-111111111111', NULL, NULL, NULL, NULL, NULL, NULL),
  ('aaaaaaaa-0000-4000-8000-000000000003', '44444444-4444-4444-8444-444444444447', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'active', now() - interval '19 days', NULL, now() - interval '19 days', now() - interval '19 days',
   NULL, '11111111-1111-4111-8111-111111111111', NULL, NULL, NULL, NULL, NULL, NULL),
  ('aaaaaaaa-0000-4000-8000-000000000004', '44444444-4444-4444-8444-444444444444', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'expired', now() - interval '50 days', now() - interval '5 days', now() - interval '50 days', now() - interval '5 days',
   NULL, '11111111-1111-4111-8111-111111111111', NULL, NULL, NULL, NULL, NULL, NULL),
  ('aaaaaaaa-0000-4000-8000-000000000005', '44444444-4444-4444-8444-444444444445', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'expired', now() - interval '90 days', now() - interval '60 days', now() - interval '90 days', now() - interval '60 days',
   NULL, '11111111-1111-4111-8111-222222222222', NULL, NULL, NULL, NULL, NULL, NULL),
  ('aaaaaaaa-0000-4000-8000-000000000006', '44444444-4444-4444-8444-444444444444', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'revoked', now() - interval '300 days', now() - interval '200 days', now() - interval '300 days', now() - interval '250 days',
   now() - interval '250 days', '11111111-1111-4111-8111-111111111111', NULL, NULL, NULL, NULL, NULL, NULL),
  ('aaaaaaaa-0000-4000-8000-000000000007', '44444444-4444-4444-8444-444444444444', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'revoked', now() - interval '60 days', now() - interval '20 days', now() - interval '60 days', now() - interval '5 days',
   NULL, '11111111-1111-4111-8111-111111111111', NULL, now() - interval '5 days', 'parity-soft-delete', 'active', NULL, NULL),
  ('aaaaaaaa-0000-4000-8000-000000000008', '44444444-4444-4444-8444-444444444444', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'expired', now() - interval '10 days', NULL, now() - interval '10 days', now() - interval '10 days',
   NULL, '11111111-1111-4111-8111-111111111111', NULL, NULL, NULL, NULL, NULL, NULL),
  ('aaaaaaaa-0000-4000-8000-000000000009', '44444444-4444-4444-8444-444444444448', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'active', now() - interval '5 days', now() + interval '60 days', now() - interval '5 days', now() - interval '1 day',
   NULL, '11111111-1111-4111-8111-222222222222', now() - interval '1 day', NULL, NULL, NULL, NULL, NULL),
  ('aaaaaaaa-0000-4000-8000-000000000010', '44444444-4444-4444-8444-444444444449', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'active', now() - interval '4 days', now() + interval '60 days', now() - interval '4 days', now() - interval '4 days',
   NULL, '11111111-1111-4111-8111-222222222222', NULL, NULL, NULL, NULL, NULL, NULL),
  ('aaaaaaaa-0000-4000-8000-000000000011', '44444444-4444-4444-8444-44444444444a', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'active', now() - interval '3 days', now() + interval '90 days', now() - interval '3 days', now() - interval '3 days',
   NULL, '11111111-1111-4111-8111-111111111111', NULL, NULL, NULL, NULL, 700000, NULL),
  ('aaaaaaaa-0000-4000-8000-000000000012', '44444444-4444-4444-8444-44444444444b', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333',
   'active', now() - interval '2 days', now() + interval '45 days', now() - interval '2 days', now() - interval '1 day',
   NULL, '11111111-1111-4111-8111-111111111111', now() - interval '1 day', NULL, NULL, NULL, NULL, NULL);

-- Tokens: two expired on L2, one live on L3, one already revoked, one perpetual, one expired on L1.
INSERT INTO token (id, license_id, token_hash, token_type, created_at, expires_at, revoked, revoked_at, granted_roles,
                   long_lived_owner, long_lived_reason)
VALUES
  ('bbbbbbbb-0000-4000-8000-000000000001', 'aaaaaaaa-0000-4000-8000-000000000002', 'parity-hash-expired-1', 'api', now() - interval '30 days', now() - interval '1 day',  false, NULL, '{}', NULL, NULL),
  ('bbbbbbbb-0000-4000-8000-000000000002', 'aaaaaaaa-0000-4000-8000-000000000002', 'parity-hash-expired-2', 'api', now() - interval '30 days', now() - interval '2 days', false, NULL, '{}', NULL, NULL),
  ('bbbbbbbb-0000-4000-8000-000000000003', 'aaaaaaaa-0000-4000-8000-000000000003', 'parity-hash-live-3',    'api', now() - interval '10 days', now() + interval '10 days', false, NULL, '{}', NULL, NULL),
  ('bbbbbbbb-0000-4000-8000-000000000004', 'aaaaaaaa-0000-4000-8000-000000000003', 'parity-hash-revoked-4', 'api', now() - interval '40 days', now() - interval '30 days', true, now() - interval '20 days', '{}', NULL, NULL),
  ('bbbbbbbb-0000-4000-8000-000000000005', 'aaaaaaaa-0000-4000-8000-000000000003', 'parity-hash-longlived-5', 'api', now() - interval '10 days', NULL, false, NULL, '{}', 'parity-owner', 'perpetual parity token'),
  ('bbbbbbbb-0000-4000-8000-000000000006', 'aaaaaaaa-0000-4000-8000-000000000001', 'parity-hash-expired-6', 'api', now() - interval '20 days', now() - interval '1 hour', false, NULL, '{}', NULL, NULL);

COMMIT;

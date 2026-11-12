---
slice: S02
title: Skeleton — workspace, tenancy, auth, jobs, SSE
status: planned
depends_on: [S00]
spec_sections: ["§5.1", "§5.2", "§5.3", "§5.4", "§5.6", "§5.9", "§7", "§8 S2", "§11 R6/R8"]
adds_tables: [tenant, app_user, membership, department, event, job, schedule, audit_event, action_invocation, idempotency_key, app_licence]
adds_actions: []
adds_events: []
adds_modules: [jobs, realtime]
parity_ids: PENDING-S0
---

⚠️ **Amended 2026-11-09 (process model).** `process.step` remains the one step driver (S11's `procedure.step` is renamed by `0033`); the `workflow.node` and `workflow.start` job kinds retire with the DAG. The `event` outbox gains `bus_stream_id text null` and `published_at timestamptz null` (migration `0033`); a relay in `apps/server` publishes unpublished rows to Redis Streams (topics `work.requested|succeeded|failed`, `tool.call|response`, `dead.letter`; consumer groups `process|tool`; dead letter after 5 deliveries). Compose gains a Redis service for dev/test. The bus is a nudge — Postgres `run`/`run_step`/`event` are the record and re-deliver after a Redis loss (SPEC §5.6, CONTRACT §7/§21).

# S02 — Skeleton

Programme project: S02 · issues #12

**Gate (SPEC §8 S2):** two tenants isolated (including RLS); login works.

## 1. Objective

A ten-member workspace (eight crates + two apps) and one binary (`serve | migrate | worker | admin`) over Postgres 18: eleven platform tables, forced RLS, OIDC/dev auth, a fail-closed licence gate, a fenced job runner, a replaying SSE stream.

## 2. Starting state

- `conveyor` is docs-only — no `Cargo.toml`, `crates/` or git repo. CONTRACT §13: `scripts/rename.sh` cannot run (`git ls-files`), so the workspace is hand-built from the template's patterns; no `.git`, `.github` or `new-rust-target.sh` this slice.
- S00 wrote `parity.yaml`, which S02 ignores. S01 recorded the UI ADR; `apps/web` is reserved but empty.
- `template-rust-service@0c7396a`, `agent@83b8de7`, `strata_rag@b9426c3` read locally.

## 3. Scope

**IN** — workspace, toolchain, `.dockerignore`, `compose.yaml`; `migrations/0001_platform.sql` (11 tables, RLS forced, roles, policies); `crates/store` pool/scope/migrator/outbox/jobs/audit/idempotency; `crates/auth` JWKS + dev principal + licence extractor; `crates/api` health, request id, problem+json, `/api/events`, `/metrics`; `apps/server` subcommands and `--print-openapi`.

**OUT** — domain aggregates and tables (S03); action registry (S04); board/run/approval endpoints (S03–S06); the Keycloak realm (S14; S02 consumes `CONVEYOR_OIDC_ISSUER` only); the licences service (D12); cluster deploy and pgbouncer config (S14); pgvector/search (S07); the UI (S12); `.github/` and git (CONTRACT §13).

**UI surface declared for S12** (contract §11 — declaration only, no UI code): `shell/login` — sign in through OIDC, sign out, tenant switch, and the `/api/me` read model (principal, memberships, active tenant).

## 4. Source material

| path | read this | carry (symbols) | do not carry |
|---|---|---|---|
| `agent` `crates/agent-api/src/tenancy.rs` | tenant-first tx scope | `begin_scoped`, `set_tenant`, `lock_tenant_row`, `TenantCtx`, `JobCtx`; tenant row locked first | `ACTIVE_TENANT_HEADER` — CONTRACT §9 forbids a client-supplied tenant |
| `agent` `crates/agent-api/migrations/0001_tenancy.sql` | the policy, function and grants | `app_current_tenant()`, `DEFAULT app_current_tenant()`, `tenant_isolation`, `NOBYPASSRLS`, `REVOKE ALL ON _sqlx_migrations` | the `DO` block swallowing `insufficient_privilege` — passes with no runtime role |
| `agent` `crates/agent-api/src/auth.rs`, `crates/agent-api/src/licensing/gate.rs` | JWKS cache and refusal paths | `JwksCache`, `unverified_iss`, `Verifier`, `JWKS_TTL` 600 s, `JWKS_MIN_REFRESH` 30 s, `VerdictGate`; `aud`+`azp`; exhaustive refusal | 8-role breadth, `resource_access`, synchronous licences call |
| `agent` `crates/agent-api/src/sse_hub.rs`, `crates/agent-api/src/conversations/hub.rs` | replay ring and keepalive | `Subscription`, `to_event`, `KEEPALIVE` 15 s, `MAX_REPLAY` 200, `Drop` unsubscribe | the Redis pub/sub bridge (SPEC §4) |
| `agent` `crates/agent-api/src/provision/outbox.rs`, `crates/agent-api/migrations/0035_provision_outbox.sql` | claim SQL and lease fencing | `OutboxRow`, claim SQL, `lease_until`/`lease_token`, send outside the tx | `target`/`op` CHECKs, supersede column |
| `agent` `crates/agent-api/migrations/0034_audit.sql`, `crates/agent-api/src/audit/` | append-only grant, scrub | `record(&mut conn, …)`, `vocab`, `scrub`, `MAX_DETAILS_BYTES`; `GRANT SELECT, INSERT` only | `platform_audit` |
| `template-rust-service` `Cargo.toml`, `rust-toolchain.toml`, `src/{lib,db,tenant,config,error,main}.rs` | OpenApiRouter, AppState, two-struct OpenAPI | version set; `OpenApiRouter`, `AppState`, `from_lookup`, `--print-openapi`; two-struct OpenAPI pattern | `query!`, `Router::merge`, trusted tenant header, template error body |
| template `Dockerfile`, `.dockerignore`, `tests/{db,http}.rs` | non-root image, test ladder | non-root 65532; fail-never-skip ladder | per-run container |
| `strata_rag` `src/tenant.rs`, `src/metrics.rs`, `tests/common/mod.rs` | tenant newtype, metrics, cleanup | `Tenant(Uuid)`, `OnceLock` metrics, atexit container cleanup | token scopes |
| `fr@6f7c509` `backend/src/lib/licence-introspect.ts`, `backend/src/lib/require-fr-license.ts`, `backend/src/workers/` | positive-only cache, timer inventory | `looksLikeJwt` (three non-empty dots), positive-only cache; the 14 `setInterval` names, as job kinds | the Redis cache, the timers |

## 5. Deliverables

```
Cargo.toml  rust-toolchain.toml  .dockerignore  Dockerfile  compose.yaml
migrations/0001_platform.sql
scripts/db-roles.sql                 # superuser only
crates/{domain,store,api,actions,actions-derive,auth,render,workflow}/   # actions|actions-derive|render|workflow: empty stubs
apps/server/{src,tests}/             # bin `conveyor`
apps/web/                            # Cargo.toml + empty src/lib.rs, zero deps
```

`[workspace.dependencies]` — exact pins; every crate is `#![forbid(unsafe_code)]`, edition 2024, toolchain 1.99.0:

```toml
[workspace.dependencies]
tokio = "1.53.3"
axum = "0.8.9"
sqlx = { version = "0.9.0", features = ["runtime-tokio", "tls-rustls", "postgres", "migrate", "macros", "json"] }
utoipa = "6.0.0"
utoipa-axum = "0.3.0"
clap = "4.6.7"
thiserror = "2.0.21"
serde = "1.0.229"
serde_json = "1.0.154"
tower = "0.5"
tower-http = "0.7.1"
# also: uuid, tracing, tracing-subscriber, metrics, metrics-exporter-prometheus
```

**Tables** — exactly data-architecture §2.1/§2.2, column for column: `tenant`, `app_user`, `membership`, `department`, `event`, `job`, `schedule`, `audit_event`, `action_invocation`, `idempotency_key`, `app_licence`. No others.

`migrations/0001_platform.sql` — envelope (the eleven bodies are §2.1/§2.2 verbatim):

```sql
CREATE FUNCTION app_current_tenant() RETURNS uuid LANGUAGE sql STABLE
  AS $$ SELECT nullif(current_setting('app.current_tenant', true), '')::uuid $$;
ALTER TABLE membership ENABLE ROW LEVEL SECURITY;
ALTER TABLE membership FORCE  ROW LEVEL SECURITY;          -- owner included
CREATE POLICY tenant_isolation ON membership FOR ALL
  USING (tenant_id = app_current_tenant())
  WITH CHECK (tenant_id = app_current_tenant());           -- forged writes refused
-- tenant_id uuid NOT NULL DEFAULT app_current_tenant()
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO conveyor_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO conveyor_runtime;
REVOKE ALL ON _sqlx_migrations FROM conveyor_runtime;
REVOKE UPDATE, DELETE ON audit_event FROM conveyor_runtime;  -- append-only by GRANT
```

Roles are superuser-created (`scripts/db-roles.sql` locally, DBA in S14): `conveyor_migrate LOGIN` (owner) and `conveyor_runtime LOGIN NOSUPERUSER NOBYPASSRLS`. `migrate` **asserts** `rolsuper = false AND rolbypassrls = false` and aborts otherwise.

`sqlx::migrate!` lives in `crates/store/src/migrate.rs` and must read **`sqlx::migrate!("../../migrations")`**: the macro resolves against the *defining crate's* manifest dir, so a bare `"./migrations"` silently means `crates/store/migrations`. A unit test asserts the embedded set is exactly `[(1, "platform")]`. `migrate` re-applies the grant block afterwards: a later table is otherwise invisible to the runtime role.

**Endpoints:** `GET /health`, `/health/live` (no DB), `/health/ready` (`SELECT 1` through the runtime pool), `/metrics`, `/api/me`, `/api/events?topics=…`. `/api` is one `OpenApiRouter` built with `.routes(...)`; `--print-openapi` emits its document and exits before config/DB. (A second crate's paths must be folded with `OpenApiRouter::merge`, never `Router::merge`; only `/mcp` uses plain `Router::merge`, outside the document.)

**Env:**

```bash
APP_ENV
BIND_ADDR=0.0.0.0:8930
DATABASE_URL                  # pgbouncer 127.0.0.1:6439, runtime role
DATABASE_MIGRATE_URL          # direct :5439, owner
DATABASE_LISTEN_URL           # direct :5439
CONVEYOR_DEV_AUTH
CONVEYOR_OIDC_ISSUER
CONVEYOR_OIDC_AUDIENCE=conveyor
CONVEYOR_OIDC_CLIENT_ID
LICENCES_BASE_URL
APP_KEY=conveyor
JOB_LEASE_SECS=60
```

**Tenancy.** `TenantId` wraps a private `Uuid`, parsed in the extractor (never a `String`), so an invalid uuid is a `401`, not a `500`. `begin_scoped` opens a tx then `SELECT set_config('app.current_tenant', $1, true)`; a session-level `SET` never appears. Every `store` function takes `TenantId` as its first argument after the connection: an unscoped query is a compile error.

**Auth.** One realm (`conveyor`); unlisted `iss` rejected before any JWKS fetch; per-issuer cache 600 s with a 30 s negative floor. `CONVEYOR_DEV_AUTH=1` accepts `Authorization: Bearer dev:<login>` **or the `conveyor_dev=<login>` cookie** (the browser transport), both resolved through `membership` to exactly one `(app_user, tenant)` — zero or several is a 401; `migrate --dev-seed` creates **`dev-a`/`alice`** and **`dev-b`/`bob`**, so `dev:alice` IS the dev-a principal (CONTRACT §10); startup exits non-zero if set with `APP_ENV=production`. Licence tokens are non-JWT; a JWT-shaped token that fails verification is a hard `401`, never retried as a licence token. `RequireLicence` reads `app_licence` only — absent, stale or unknown refuse — refreshed by `licence.sync`; core requests never call the licences service.

**Jobs.** Claim is `FOR UPDATE SKIP LOCKED` + fenced `lease_token` (data-arch §4 SQL); a stale token completes nothing. `schedule` materialises under `pg_advisory_xact_lock(hashtext('conveyor:schedule'))`. Kinds: `event.prune` (90 d, batched, per tenant — RLS forbids a cross-tenant delete), `job.prune`, `idempotency.sweep`, `licence.sync`.

**SSE.** `Last-Event-ID` is `event.seq`; replay is `seq > $last AND topic = ANY($topics) ORDER BY seq LIMIT 500` in the subscriber's tenant scope; 15 s keepalive; a bounded channel closes a slow client with `resync`. `pg_notify('conveyor_event', seq::text)` is a **doorbell**: the listener holds a dedicated **direct** connection (`DATABASE_LISTEN_URL`) because `LISTEN` is session-scoped and PgBouncer is transaction-pooled, and each subscriber re-queries its own rows, so no cross-tenant read path exists.

`compose.yaml`: service `db` (Postgres 18 + pgvector, `127.0.0.1:5439`), `pgbouncer` on 6439, `app` on 8930 — **same port inside and outside**. That is a real mechanism, not a wish: `db` starts as `command: ["postgres", "-p", "5439"]` (the image defaults to 5432) and pgbouncer's `listen_port` is 6439 inside the container. Integration tests start their **own** ephemeral container on a random host port (one per binary, fresh database each test); `TEST_DATABASE_URL` short-circuits; a container that cannot start fails, never skips.

## 6. Build order

1. Preflight: docker up, ports 8930/5439/6439 free, no `.git`. Never 8000.
2. Root `Cargo.toml`, `rust-toolchain.toml`, ten members with `#![forbid(unsafe_code)]`.
3. `crates/domain`: `TenantId`, `Id`, `Version`; nil rejected.
4. `migrations/0001_platform.sql` + `scripts/db-roles.sql`; apply roles as superuser, then the migration once.
5. `crates/store`: pool (10 conns, 5 s acquire), `migrate.rs`, `begin_scoped`.
6. `apps/server/tests/rls.rs` — the two-tenant gate as the runtime role, with the superuser control.
7. `crates/auth`: `from_lookup`, `JwksCache`, `Principal`, dev principal, production refusal.
8. `crates/api`: `ApiError` → problem+json, `X-Request-Id`, health, `/metrics` (OnceLock, 1 s gauge timeout).
9. `apps/server`: clap subcommands (`serve | migrate | worker | admin`); `--print-openapi` emits the merged
   paths and exits before config/DB. `migrate --dev-seed` inserts the two dev tenants `dev-a`/`dev-b` with one
   user each — `alice` in `dev-a`, `bob` in `dev-b` (idempotent; refused when `APP_ENV=production`) — so
   `Bearer dev:alice` is a dev-a request. Every later gate that reads
   `select id from tenant where slug='dev-a'` depends on this. The **catalogue** seed
   (`migrate --seed --tenant <slug>`) is S07's; S02 does not implement it.
10. Outbox writer + `/api/events`.
11. Direct-pool listener; prove with `pg_notify` and `curl -N`.
12. Job claim/finish/poller + `worker` subcommand; kill test.
13. Schedule materialiser; two concurrent runs ⇒ one job.
14. Prune and sync kinds with seeded `schedule` rows.
15. Audit + idempotency stores; append-only proven by a failing `UPDATE`.
16. `RequireLicence` + `licence.sync` against a local HTTP stub.
17. `apps/web` placeholder; fmt + clippy.
18. Docker build; `compose up app`; full gate.

Commands, in step order (from `$HOME/Movies/conveyor`):

```bash
source ~/.cargo/env                       # cargo/rustc are NOT on PATH (measured 2026-11-07)
# 1
rustc --version && docker info && for p in 8930 5439 6439; do lsof -nP -iTCP:$p -sTCP:LISTEN; done || true; test ! -e .git
# 2
cargo check --workspace
# 3
cargo test -p conveyor-domain
# 4 — roles as superuser; migration is step 5's command
psql "$DATABASE_MIGRATE_URL" -v ON_ERROR_STOP=1 -f scripts/db-roles.sql
# 5
cargo run -p conveyor-server -- migrate
# 6
cargo test -p conveyor-server --test rls -- --nocapture
# 7
cargo test -p conveyor-auth
# 8
cargo test -p conveyor-api
# 9
cargo run -p conveyor-server -- --print-openapi | jq -e '.paths'
# 10
cargo test -p conveyor-server --test events
# 11
psql "$DATABASE_LISTEN_URL" -c "select pg_notify('conveyor_event','1')" && curl -sN localhost:8930/api/events -H 'authorization: Bearer dev:alice'
# `/api/events` is licence-gated: run `migrate --dev-seed` first (it seeds an active app_licence row)
# 12
cargo test -p conveyor-store --test jobs
# 13
cargo test -p conveyor-store --test schedule
# 14
cargo test -p conveyor-server --test prune
# 15
cargo test -p conveyor-store --test audit_idempotency
# 16
cargo test -p conveyor-auth --test licence
# 17
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
# 18
cargo build --release --locked -p conveyor-server && docker compose up -d app && curl -sf localhost:8930/health/ready
```

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env
cd $HOME/Movies/conveyor
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p conveyor-server -- --print-openapi | jq -e '.paths["/api/events"]'
cargo run -p conveyor-server -- migrate
docker compose up -d --build && curl -sf localhost:8930/health/ready   # --build: a bare `up` tests the stale image
```

| gate | proof |
|---|---|
| Two tenants isolated | as `conveyor_runtime` scoped to A, `count(*) FROM event` returns 0 of B's rows; `INSERT … tenant_id=<B>` fails `WITH CHECK`; **superuser** sees both (RLS, not an empty table); **owner** also sees 0 (proves `FORCE`) |
| Login works | `Bearer dev:alice` → 200 `/api/me`; with `APP_ENV=production` the process exits non-zero |
| Job survives a kill | `JOB_LEASE_SECS=2`; `kill -9` mid-handler; restart ⇒ row reclaimed on lease expiry, `attempts` increments, stale token writes nothing |
| SSE replay | three events; reconnect with `Last-Event-ID: <e1.seq>` ⇒ e2, e3 once each in `seq` order; B's events absent |
| Audit append-only | `UPDATE audit_event` as runtime ⇒ permission denied |
| Licence fail-closed | no row ⇒ 403 problem+json; unknown status ⇒ 403 |

## 8. Traps

1. **RLS owner bypass.** Without `FORCE ROW LEVEL SECURITY` the owner reads everything, so an owner-role test passes while production leaks. Superusers bypass even with FORCE — run the test as `conveyor_runtime` and print `current_user`. A `FOR ALL` policy without `WITH CHECK` lets a forged `tenant_id` write.
2. **`sqlx::migrate!` path.** Resolves against the defining crate's manifest dir → `crates/store/migrations`. Point it at `../../migrations` and assert the embedded set.
3. **LISTEN vs PgBouncer.** Session-scoped; transaction pooling returns the connection between statements. Use a dedicated direct pool.
4. **`pg_notify` payload limit is 8,000 bytes.** Send `seq::text` only; a JSON payload fails exactly when events get large.
5. **`set_config` must be transaction-local.** A session `SET` survives COMMIT and poisons the next client of that backend (measured here).
6. **Ports.** 8930/5439/6439 free locally 2026-11-07; re-check before binding and in-cluster at S14. Never 8000.
7. **`.dockerignore`.** Without `target/`, `.git/`, `.env*` the context is multi-GB and secrets enter the image.
8. **Pooler vs migrator.** sqlx takes a **session** advisory lock; through a transaction pooler it cannot exclude a second replica. `DATABASE_MIGRATE_URL` is direct; `migrate` is never implicit in `serve`.
9. **`Router::merge` vs `OpenApiRouter::merge`.** Plain merge drops the second crate's paths from the document while the routes still serve.
10. **R6 — Redis removal.** Nothing here may add Redis: rate limits and caches are in-process or Postgres advisory locks; the only cache whose TTL matters is the licence cache. A measured need is a contract change.
11. **R8 — auth/tenancy is the highest-bug-surface area.** `TenantId` is typed into the store layer (a missing scope is a compile error), RLS is forced, and the isolation test runs as the runtime role with a superuser positive control — a passing test as the owner proves nothing.

## 9. Decisions closed here

| id | decision | rationale |
|---|---|---|
| S2-D1 | `TenantId` lives in `crates/domain`, re-exported as `auth::TenantId` | keeps `store → domain`; a store→auth edge inverts the chain |
| S2-D2 | Roles created by a superuser; `migrate` asserts them | the swallowed-`DO` failure is otherwise invisible |
| S2-D3 | Prune loops per tenant in scoped transactions | no `BYPASSRLS` role exists to leak across tenants |
| S2-D4 | `pg_notify` is a doorbell; subscribers re-query their own scope | seq-only payload plus RLS leaves no cross-tenant read path |
| S2-D5 | Migrations at root `migrations/`, embedded via `../../migrations` | CONTRACT §2/SPEC §5.2, guarded by the embedded-set test |
| S2-D6 | Dev principals resolve through `membership`, never a header | CONTRACT §9: tenant comes from membership |
| S2-D7 | `apps/web` is a dependency-free placeholder | S1 owns the stack; S12 builds it |

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Run PgBouncer locally? | yes, 6439 — pooling constraints are the point |
| 2 | Image tag for compose and tests | `pgvector/pgvector:0.8.7-pg18` — exists (2026-11-07): amd64 `sha256:2547711f871f`, arm64 `sha256:ed9dc4e391eb` |
| 3 | Dev tenants for the gate | **closed — it is a deliverable, not a question**: `migrate --dev-seed` inserts `dev-a`/`dev-b`, one user each (step 9) |
| 4 | Slug case | lowercased in Rust, plain `text` unique, no `citext` |
| 5 | Extensions in `0001` | none; S07 adds `vector` and `pg_trgm` |

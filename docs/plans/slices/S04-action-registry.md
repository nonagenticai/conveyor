---
slice: S04
title: Action registry — derive, manifest, OpenAPI, MCP, invocation, external ingest
status: planned
depends_on: [S02, S03]
spec_sections: ["§5.5", "§7", "§8 S4", "§3 D11"]
adds_tables: [action_registry]
adds_actions: [actions.list, actions.describe, me.get, tenants.get, tenants.update_settings, members.list, members.set_role, departments.list, departments.create, audit.list, jobs.list, jobs.retry, schedules.list, schedules.upsert, events.list, licences.get, fixtures.echo, fixtures.slow, fixtures.fail, fixtures.timeout]
adds_events: []
adds_modules: [actions]
parity_ids: PENDING-S0
---

# S04 — Action registry

Programme project: S04 · issues #13

**Gate (SPEC §8 S4):** 20 actions identical over HTTP and MCP.

## 1. Objective

One declaration per invocable operation, from which manifest, operation, MCP tool and route are generated, so HTTP and MCP cannot diverge. It replaces the 24 node packages and fr's 88 capability declarations; later slices add actions, never a surface.

## 2. Starting state

- S02: `begin_scoped`, `audit_event`, `action_invocation`, `idempotency_key`, `event`, `/api/me`.
- S03: the 14 domain aggregates and transitions.
- `crates/actions` is empty.

## 3. Scope

**IN** — `ActionSpec` + `#[action]`; the compile-time registry and boot assertion; `GET /api/actions`; `POST /api/actions/{name}/invocations` with idempotency, timeout, audit, outbox; `rmcp` at `/mcp`; the OpenAPI fold and lock; `openapiv3` ingest; `migrations/0003_action_registry.sql`; 16 internal + 4 `fixtures.*` actions.

**OUT** — domain actions (`cards.move`, …) are S05–S11's; `kind: workflow` (S13); channel-`mcp` delivery rows (S06); the signed `mcp_server` channel (S06-α).

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `20-fr` registry | `Capability` fields, `derive-{openapi,mcp,router}.ts`, `registry.ts` | field set, closed `effect`/`confirm`/`tenancy` vocabularies, `assertRegistryConsistent`, `served-spec.lock.json`, `specExempt` | Zod, `legacyHandler`, `frozen` |
| `agent/crates/agent-api/src/runtime/catalog.rs` | `OpDef`, `PinnedCatalog::from_rows`, `effective_level`, `outputs` | one catalogue; a compiled row beats a stored one | `OpSource::Template` |
| `30` §4 node resources | 14 fr resource names | the action-name vocabulary | `_legacy` |

Pinned 2026-11-07 (`git -C $HOME/Movies/<repo> rev-parse origin/main`, read-only): `agent` `83b8de7e`, `fr` `4a579e91` (source map: `6f7c5097`).

## 5. Deliverables

```
crates/actions-derive/src/lib.rs
crates/actions/src/{spec,registry,invoke,mcp,openapi,lock,ingest}.rs
crates/actions/tests/{registry,parity_http_mcp,idempotency,timeout,ingest_diff,external_actions}.rs
crates/actions/tests/fixtures/external/openapi.json          # the four fixture operations
crates/{api,store}/src/actions.rs · migrations/0003_action_registry.sql · served-spec.lock.json
```

`crates/actions-derive` is a 10th workspace member (`proc-macro = true`, CONTRACT §1): a proc macro cannot live in a normal crate, and a hand-written registry would name each schema twice — the drift surface.

### 5.1 The declaration

```rust
pub struct ActionSpec {
    pub name: &'static str,        // <plural resource>.<verb>, exactly one dot
    pub version: u32,              // not part of the name
    pub kind: ActionKind,          // Internal | External | Workflow
    pub input: fn() -> Schema,     // schemars
    pub output: fn() -> Schema,    // schemars
    pub scopes: &'static [Scope],
    pub idempotent: bool,
    pub timeout_ms: u32,           // default 30_000
    pub effect: Effect,            // Read | Write | Destructive
    pub confirm: Confirm,          // None | Card | CardTyped
    pub tenancy: Tenancy,          // Self | Tenant | Project | Platform
    pub handler: HandlerRef,
}
```

`#[action(name, version, kind, scopes, idempotent, timeout_ms, effect, confirm, tenancy)]` on `async fn(Json<In>) -> Result<Json<Out>, AppError>` emits, **from the same tokens**, the spec value and an `inventory::submit!`; `conveyor_actions::openapi::fold` turns the same `ActionSpec` into the path item at `/api/actions/<name>/invocations` and `api::app` adds the literal route. The per-action `#[utoipa::path]` route proved impractical; the fold is the allowed alternative, and the single source is still the `ActionSpec`.

Each projection reads the same value: manifest every field (ETag); operation `name`/`input()`/`output()`/`effect`/`scopes`; MCP tool the same with `.`→`_`; route `handler`/`timeout_ms`/`tenancy`/`idempotent`. The gate proves it: one `spec_hash` per action equals manifest, `$ref`-resolved operation and MCP tool.

**16 internal** (`kind: internal`):

| action | input → output | scope | idempotent | timeout |
|---|---|---|---|---|
| `actions.list` | `{}` → `ActionManifest` | `actions:read` | yes | 30 s |
| `actions.describe` | `{name}` → `ActionSpecView` | `actions:read` | yes | 30 s |
| `me.get` | `{}` → `MeView` | `me:read` | yes | 30 s |
| `tenants.get` | `{}` → `TenantView` | `tenants:read` | yes | 30 s |
| `tenants.update_settings` | `{settings}` → `TenantView` | `tenants:write` | no | 30 s |
| `members.list` | `PageIn` → `Page<MemberView>` | `members:read` | yes | 30 s |
| `members.set_role` | `{user_id,role}` → `MemberView` | `members:write` | yes | 30 s |
| `departments.list` | `{}` → `[DepartmentOut]` | `departments:read` | yes | 30 s |
| `departments.create` | `{name,parent_id?}` → `DepartmentOut` | `departments:write` | no | 30 s |
| `audit.list` | `{since?} + PageIn` → `Page<AuditView>` | `audit:read` | yes | 30 s |
| `jobs.list` | `{status?} + PageIn` → `Page<JobView>` | `jobs:read` | yes | 30 s |
| `jobs.retry` | `{id}` → `JobView` | `jobs:write` | no | 30 s |
| `schedules.list` | `{}` → `[ScheduleOut]` | `schedules:read` | yes | 30 s |
| `schedules.upsert` | `{kind,cron,enabled}` → `ScheduleOut` | `schedules:write` | yes | 30 s |
| `events.list` | `{topics?} + PageIn` → `Page<EventView>` | `events:read` | yes | 30 s |
| `licences.get` | `{}` → `LicenceView` | `licences:read` | yes | 30 s |

**S11 adds two scopes** to the vocabulary (the `procedures.*` actions are S11's, not this slice's): a read is
open to every role, a write is `member`/`admin`/`owner` plus the synthetic tenant-admin `kata_executor`
(S11 CCR 5) — a procedure is work on the tenant's own board, not platform administration.

| scope | granted to |
|---|---|
| `procedures:read` | every role |
| `procedures:write` | `member`/`admin`/`owner`/`kata_executor` |

`ActionManifest = {actions: [{name, version, kind, spec_hash, scopes, idempotent, timeout_ms}], generated_at}`
— the shape `GET /api/actions` returns and `actions.list` projects, so consumers (S12/S13/S14 gates) can assert
`.actions[]` without guessing; `ETag`/`If-None-Match` wraps it (304 on a match).

**Four external** (`kind: external`), ingested from `crates/actions/tests/fixtures/external/openapi.json` and served by the `external_actions.rs` stub at `CONVEYOR_EXTERNAL_FIXTURES_URL`:

| action · in → out | stub behaviour | proves |
|---|---|---|
| `fixtures.echo` · `{message}` → `{message,service}` | 200 | happy path; HTTP/MCP parity |
| `fixtures.slow` · `{delay_ms}` → `{waited_ms}` | sleeps 600 ms then 200; `timeout_ms=200` pinned at ingest | timeout: rollback + the `outcome='timeout'` row |
| `fixtures.fail` · `{status}` → `{status}` | returns the requested 404/422 | 4xx → RFC 9457 `action-external-refused`, never a 500 |
| `fixtures.timeout` · `{}` → `{status}` | returns **504** every time | 5xx → `502 action-upstream-failed`; five in 30 s open the breaker (`503 action-circuit-open`, no upstream call) |

### 5.2 Registry

- **Compile-time:** `inventory` collected once into `Registry`; `assert_registry_consistent` runs in `serve`, `worker` and every test, failing on a duplicate `name`, an MCP-form collision (`cards.move` vs `cards_move`), an MCP name outside `^[a-z0-9_]{1,64}$`, a handler-less internal action, or an unknown `Scope`.
- **DB:** `action_registry` verbatim from §2.14, `kind ∈ {external, workflow}` only; `spec_version` + `schema_hash` pin the ingest, `timeout_ms` and `idempotent` override it. The ingester refuses an external name that already exists compiled (`action-name-conflict`).
- **Enable/disable is `approved_at`.** The highest **approved** `spec_version` per `(tenant_id, name)` serves; a refresh writes an unapproved row alongside.
- **Startup fails if the DB names an action the binary lacks** — an approved `workflow` row with no handler, or an `external` row whose `service` is unconfigured: exit 1 before bind.

### 5.3 Invocation and MCP

- `POST /api/actions/<literal name>/invocations` → `{input}` → `{output}`. A `spec_exempt` fallback at `/api/actions/{name}/invocations` returns `action-not-found`.
- `Idempotency-Key` scoped `(tenant_id, <action name>, key)`, claimed by `INSERT … ON CONFLICT DO NOTHING`; the loser replays the winner's status and body.
- **One transaction** writes the mutation, `action_invocation` and `audit_event` (`record(&mut PgConnection)`) — S04 declares no events, so it writes no outbox row (S05+ actions add their own `event` in the same transaction); `SET LOCAL statement_timeout = <timeout_ms>` + `tokio::time::timeout`.
- A timeout rolls that transaction back and cannot record itself: a **separate** short transaction writes `action_invocation(outcome='timeout')` + `audit_event(status='error')`.
- `kind: external` goes through a per-action circuit breaker (5 failures/30 s); the four fixture services are unauthenticated (a real service's credential is S09's), so no tenant token is sent; `kind: workflow` returns `501` until S13.
- **MCP:** `rmcp` streamable HTTP at `/mcp`, one session per tenant; `tools/list` comes from the same `Registry`, scope-filtered so a refused tool is **absent**. Credentials fail closed: an unresolvable tenant is 401; a JWT-shaped bearer that fails verification is never retried as a licence token.
- **`nonagenticai_mcp_server` is not this server** — it is the agent gateway; the conveyor sits behind it through S06-α's HMAC channel (SPEC §4.5).

### 5.4 OpenAPI and external ingest

`openapi()` folds a literal path item per action into the spec built from the `OpenApiRouter` exports. `--print-openapi --write-lock` regenerates `served-spec.lock.json` (per-operation sha256 of the `$ref`-resolved operation); `--check-lock` exits 1 on any added, removed or changed operation.

`openapiv3` reads a service's `openapi.json`, derives `kind: external` actions (`operationId` → `<resource>.<verb>` where parseable, else `(tag, path, method)`), stores `spec` + `schema_hash`. A refresh diffs: added-optional auto-approves; a removed or retyped input field, a newly required one, a removed output field or path/method is **breaking** — refused (`422`, exit 2) until `conveyor admin actions approve <name> --version <n>`.

## 6. Build order

1. Preflight: `cargo test --workspace`; `lsof -nP -iTCP:8930 -sTCP:LISTEN` empty; `docker compose up -d db`. Never 8000.
2. `crates/actions-derive`: `proc-macro = true`, attribute parser; reject a non-`<plural>.<verb>` name at expansion. `cargo test -p conveyor-actions-derive name_grammar`
3. Emit spec + `inventory::submit!` + the `openapi::fold` path item + the literal route; `cargo expand -p conveyor-actions`, read one end to end.
4. `spec.rs` + `registry.rs`: vocabularies, `validate_name`, `from_inventory`, `assert_registry_consistent` (six checks: name grammar, duplicate, MCP collision, MCP charset, handler-less, unknown scope), `spec_hash`. `cargo test -p conveyor-actions spec::vocabularies`
5. The 16 internal actions over S02/S03 store calls. `cargo test -p conveyor-actions --test registry`
6. `invoke.rs`: tenancy scope → idempotency claim → deadline → tx composition. `cargo test -p conveyor-actions --test idempotency`
7. `store/src/actions.rs`: the five `_tx` helpers (idempotency claim/finish, invocation, audit, event), none opening a transaction. `cargo test -p conveyor-store actions_tx`
8. `migrations/0003_action_registry.sql` from §2.14; RLS `ENABLE`+`FORCE`+policy; grants. `cargo run -p conveyor-server -- migrate && cargo run -p conveyor-server -- migrate`
9. `api/src/actions.rs`: manifest with `If-None-Match` → 304, plus the fallback. `cargo test -p conveyor-api manifest_304`
10. `mcp.rs`: `/mcp` session, per-tenant, scoped `tools/list`, `tools/call` → `invoke`. `cargo test -p conveyor-actions --test parity_http_mcp`
11. `openapi.rs` + `lock.rs` + `ingest.rs`: the fold, `--check-lock`, load/diff/approve; fixtures for a live `coding-*` spec and a removed-field one, plus the four `fixtures.*` operations. `cargo test -p conveyor-actions --test ingest_diff && cargo test -p conveyor-actions --test external_actions`
12. Boot wiring before `TcpListener::bind`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo run -p conveyor-server -- --print-openapi --check-lock` into `rust-service.yml`.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
B=http://127.0.0.1:8930
docker compose up -d db && cargo run -p conveyor-server -- migrate --dev-seed   # the DB and the dev principals (S02)
cargo test -p conveyor-actions
cargo run -p conveyor-server -- --print-openapi --check-lock          # exit 0
! CONVEYOR_TEST_DUPLICATE=members.set_role cargo run -p conveyor-server -- serve   # exit 1, before bind
CONVEYOR_DEV_AUTH=1 cargo run -p conveyor-server -- serve &
trap 'kill %1 2>/dev/null || true' EXIT
sleep 2
curl -s -X POST $B/api/actions/members.set_role/invocations -H 'content-type: application/json' -H 'If-Match: "1"' \
  -H 'authorization: Bearer dev:alice' -d '{"input":{}}' | jq -e '.type and .errors'   # RFC 9457: required fields missing
kill %1 2>/dev/null || true
```

| # | property | proof |
|---|---|---|
| a | HTTP and MCP cannot diverge | all 20 (16 internal + 4 `fixtures.*`) both ways; stripping `request_id`/`duration_ms`, `http.body == mcp.structuredContent` |
| b | one value, three projections | `spec_hash`: manifest == resolved operation == MCP tool |
| c | duplicates fail closed | exit 1 on a duplicate name, an MCP collision, an out-of-charset name |
| d | schema-invalid input | RFC 9457 with populated `errors[]`, never a 500 |
| e | idempotent replay | two identical POSTs, one key: same status and body, one `action_invocation` |
| f | the lock catches drift; breaking ingest refused | a renamed field makes `--check-lock` exit 1 naming the operation; the removed-field fixture exits 2 (`422`) while the approved version serves |

## 8. Traps

1. **Four artifacts drifting is the fr lesson.** Any projection reading a *second* source is the bug.
2. **`Router::merge` drops the second crate's paths from the spec** — routes work, the contract loses them. Only `OpenApiRouter::merge`.
3. **MCP tool names are not action names.** `.`→`_` is not injective and the charset is narrower; assert the mapping at boot.
4. **A timeout is the caller's, never the handler's.** A handler that swallows cancellation leaves a query running; `statement_timeout` belts `tokio::time::timeout`.
5. **Idempotency and the mutation must share a transaction.** Committed before, the key replays rolled-back work; after, the window races.
6. **Circular actions.** Carry `invocation_depth`, hard limit 8, then `409 action-recursion-limit`; else A→B→A recurses until the pool is gone.

## 9. Decisions closed here

| id | decision | rationale |
|---|---|---|
| S4-D1 | `crates/actions-derive` is a 10th member (§5) | a proc macro needs its own crate |
| S4-D2 | one literal route per action + a `spec_exempt` fallback | one operation per path; uniform errors |
| S4-D3 | enable/disable is `approved_at` | §2.14 has no `enabled` column |
| S4-D4 | an external name colliding with any name is refused at ingest | one namespace, no precedence |
| S4-D5 | idempotency scope is the action name | contract §5's `action_or_route` |
| S4-D6 | scope-filtered MCP tools; a refused tool is absent | fail-closed, not noisy |
| S4-D7 | `kind: workflow` → 501 until S13 | enum now, engine later |

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Is `GET /api/actions` role-filtered? | yes — an uncallable manifest is a second contract |
| 2 | One 30 s default, or 10 s for external? | 30 s compiled, 10 s at ingest |
| 3 | Generated TS client committed or CI artefact? | CI artefact; the lock is committed |

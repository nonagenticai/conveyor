# 01 — CONTRACT

**Authority:** this file is the single source of cross-slice rules. Slice plans may add detail but may not
redefine anything here. A slice that needs a different rule returns a `CONTRACT-CHANGE REQUEST` in its plan;
only the main session may amend this file.

Sources: `docs/SPEC.md` §3 (decisions D1–D20), §5, §6, §7. Written 2026-11-07, before any slice plan.

---

## 1. Workspace

```
crates/domain     pure aggregates, state machines, invariants — no IO, no sqlx, no axum
crates/store      queries + migrations + outbox — the ONLY crate that speaks SQL
crates/api        axum routers, utoipa, problem+json
crates/actions    ActionSpec, registry, invocation, MCP projection, external OpenAPI ingest
crates/actions-derive  the `#[action]` proc macro; `[lib] proc-macro = true`; depended on only by `crates/actions`,
                  which re-exports it. Added 2026-11-07 on S04's CONTRACT-CHANGE REQUEST: a proc macro cannot live
                  in a normal crate, and the derive is what makes the four artifacts share one field set.
crates/auth       OIDC/JWKS, TenantId, licence extractor, RLS context
crates/render     pure model → SVG / PNG / PDF / XMI / markdown
crates/bus        the care-bus port: envelope, Redis Streams, consumer groups, dead letter (S13)
crates/workflow   RETAINED (owner ruling 2026-11-09): the workflow engine — the tool-sequence executor a
                  process `call` step falls back to. Never a top-level executor
apps/server       composition root: serve | migrate | worker | admin
apps/web          the Rust UI (stack decided by S1)
```

Modules, not crates: `jobs`, `realtime`, `search`, `integrations` (live inside the crate that owns them).
The workspace is **11 members** (nine from the SPEC + `actions-derive` + `crates/bus`). **Amended 2026-11-09,
corrected as built 2026-11-10:** `crates/bus` is added and the count is **11**, not 10. The earlier claim that
`crates/workflow` "retires and the count stays 10" is superseded — **owner ruling 2026-11-09: the workflow engine
is retained** as the **tool-sequence executor** (`crates/workflow`, the `workflow` table,
`workflows.*`/`workflow_runs.*` and the `workflow.node`/`workflow.start` job kinds stay). A process `call` step
resolves `child_process_key` **process-first, workflow-second**. The process is still the top-level executor — an
ordered list of steps (see §21) — and the retained engine is never a second one.
Every crate: `#![forbid(unsafe_code)]`, edition 2024, toolchain from `rust-toolchain.toml` (1.99.0).

**Dependency direction is enforced by the compiler:** `api → actions → store → domain`; `domain` depends on
nothing internal; `render` depends on `domain` only. `store` never depends on `api` or `actions`.
**Amended 2026-11-08 (S08 Part B2):** `api → render` and `actions → render`. Both terminate at `render → domain`,
so the graph stays acyclic and `render` stays a leaf — this file never said who may consume `render`, and the
renderer is useless if only a crate that may not exist yet can call it. `render` still never depends on `store`,
`actions` or `api`.

## 2. Schema and migrations

- Migrations live at `migrations/`, embedded with `sqlx::migrate!`. Forward only, never edited after the slice
  that ships them closes, no down migrations.
- **Ownership:** S2 ships `0001_platform.sql`; S3 ships `0002_domain.sql`; **every later slice ships its own
  migration** (`NNNN_<slice>_<desc>.sql`). S3 must not pre-create a later slice's tables.
- `tenant_id uuid NOT NULL` on every tenant-owned table; `ENABLE` **and `FORCE` ROW LEVEL SECURITY** + policy
  `tenant_isolation` using `current_setting('app.current_tenant', true)::uuid` (FORCE is not optional — without it
  the owner reads everything, and S03's gate counts 14 `FORCE` lines).
- Runtime role `conveyor_runtime NOSUPERUSER NOBYPASSRLS`; migration role separate, owns the schema.
- ⛔ **FORCE RLS makes every text-search index unusable unless its operator is LEAKPROOF, and none of them
  ship that way.** Measured 2026-11-08 on `postgres:18`: `ts_match_vq` (`tsvector @@ tsquery`),
  `texticlike` (`ILIKE`), `pg_trgm`'s `similarity_op` (`%`) and `word_similarity_op` (`<%`) all have
  `proleakproof = false`, and the planner will not use an index on a non-leakproof qual while a policy
  applies. `search.query` silently degraded to a per-tenant btree scan and a row filter; marking
  `ts_match_vq` and `word_similarity_op` leakproof turned the same statement into a `BitmapOr` over
  `governed_asset_tsv_idx` and `…_title_trgm_idx` (1.6 ms over 50 000 rows). The `ALTER FUNCTION` needs a
  superuser, so it lives in **`scripts/db-roles.sql`** — the file whose contract is "run me as superuser
  before migrating" — which now creates `pg_trgm`, marks both, and raises if either is still not leakproof.
  Any future slice that indexes text under RLS inherits this; do not re-derive it, and never "fix" it by
  relaxing FORCE.
- Naming: lowercase singular tables (`card`, not `cards`); `app_user` not `user`; `position` not `order`;
  `timestamptz` everywhere, UTC.
- **The eleven S02-owned tables** (data-arch §2.1 identity/tenancy + §2.2 platform): `tenant`, `app_user`,
  `membership`, `department`, `event`, `job`, `schedule`, `audit_event`, `action_invocation`,
  `idempotency_key`, `app_licence`. S3 owns none of them; SPEC §6's "platform tables" names the seven in §2.2,
  which is not this list (2026-11-07: `department` was missing here while S02 shipped it).
- Every tenant-owned table carries `created_at`; mutable aggregates also carry `updated_at` and `version bigint`. Append-only/immutable facts (`revision`, `trace_link`, `approval_decision`) carry `created_at` only — S3-D5 says a decision is a fact.
- **A single-column FK does not enforce tenancy.** Referential-integrity checks bypass RLS, so a `tenant_id=A` row can name tenant B's parent id and the FK accepts it (measured on S03's schema, 2026-11-08). Every store function that takes a parent id resolves that id **inside the caller's tenant scope in the same transaction** and fails on a miss — never rely on the FK. A composite `(tenant_id, parent_id)` FK would be a data-arch change, not a slice decision.

## 3. Identity and concurrency

- Primary keys are uuid v7. **No external id is ever a primary key** (no `planka_card_id`).
- Every mutable aggregate has `version bigint NOT NULL DEFAULT 1`.
- **Edits** (PATCH/PUT/DELETE and action invocations that replace an existing representation — `*.update`,
  `cards.move`, `cards.archive`, `*.delete`, `cards.claim`) require `If-Match: "<version>"`; a mismatch is
  `412 Precondition Failed` with the current representation in the problem body. No blind writes.
- **State transitions are status-guarded, not version-guarded.** An action that flips a documented state by
  matching the expected status in its `WHERE` — `approvals.decide`, `procedures.publish`, `workflows.publish`,
  `runs.start`/`.finish`/`.cancel`, `procedure_runs.*`, `andon.engage`/`.release` — takes **no** `If-Match` and no
  `version` input; a zero-row update because the state moved is `409` with the current state in the problem body.
  The reason is not convenience: `andon_state` is a per-tenant singleton with no read action to learn a version
  from, and an approval is decided by a quorum count, not by replacing the row.
- Creation returns `201` + `ETag`.
- **A run is a tree (added 2026-11-09).** `run.parent_run_id`, `run.step_id` and `run.depth` (`0 ≤ depth ≤ 3`).
  One live **root** run per card — `uq_run_live` is partial on `parent_run_id IS NULL` (and `uq_run_project_live`
  likewise) — and one live child per `(parent_run_id, step_id)`, with one row per attempt. A live parent may start
  child runs for a capability call or the coach; cancel cascades from parent to children; a child's failure does not
  re-fire the parent's `err.card`. The old card-wide index is dropped by `0035_process_tree.sql`.

## 4. The list is the state

- `list.kind ∈ {intake, active, waiting, done, archive}`. A card's lifecycle is `list_id` + `archived_at`.
  **There is no `card.state` column.**
- Card-type × list legality is a domain-invariant function (`domain::board::may_move`), never a DB trigger and
  never a per-flow rule.
- A claim is one conditional `UPDATE` on the card row (assignee + `lease_expires_at` + `version`), not a second
  lease store.

## 5. API

- `/api/<resource>`, unversioned. Never `/api/v1/...`.
- Errors: RFC 9457 `application/problem+json` with `type` (stable slug), `title`, `status`, `detail`,
  `instance`, `request_id`, and `errors[]` for validation. No ad-hoc error envelopes.
- `X-Request-Id` is accepted or minted, echoed on every response, and recorded in `audit_event`.
- Pagination: opaque cursor (`?cursor=&limit=`), default 50, max 200. Cursors are base64 of
  `(sort_key, id)` and must never be documented as stable.
- POST accepts `Idempotency-Key`, scoped `(tenant_id, action_or_route, key)`, 24 h TTL; replay returns the
  original status and body.
- Health: `/health`, `/health/live`, `/health/ready`. `/health/ready` checks Postgres.

## 6. Actions

- Name grammar: `<resource>.<verb>`, resource plural, verb imperative (`cards.move`, `projects.create`,
  `approvals.decide`). The integer `version` is part of the spec, not the name.
- **Three shared input envelopes**, so no slice has to redefine them: `IdIn = {id}`, `PageIn = {cursor?, limit?}`
  (default 50, max 200 — §5), `Page<T> = {items: T[], next_cursor?}`. A slice names them; it never re-specifies
  their fields.
- One declaration produces: the manifest entry, the OpenAPI operation, the MCP tool, and the invocation route.
  MCP tool name = action name with `.` → `_` (`cards_move`).
- `GET /api/actions` → manifest. `POST /api/actions/{name}/invocations` → `{input}` → `{output}`.
- **An action's output is JSON, so an artefact travels inside it.** A rendered or exported document is carried
  as `content_b64` (binary: SVG/PNG/PDF) or as the text itself (XMI), always with `media_type`,
  `content_sha256` and `byte_len`, and always capped — over the cap is a `422`, never a truncated document.
  A `GET` route that returns raw bytes is **not** an action: it exists because a browser `<img>`/download cannot
  POST and cannot send a bearer header, it shares the same pure function so the two can never disagree, and it
  has no MCP projection. (Established by S08's `models.render`/`export_xmi`; S09's documents inherit it.)
- Every invocation writes `action_invocation` (actor, action, input hash, outcome, duration, trace id) **and**
  an `audit_event` in the same transaction as its mutation.
- `kind: internal | external`. External actions are ingested from a service `openapi.json`; timeout
  and circuit breaker are per-action fields; a schema refresh that removes or retypes a field needs explicit
  approval before it is applied.
- Default timeout 30 s; actions declare `idempotent: bool`.
- **`If-Match` on an internal invocation.** Over HTTP it is the header (§3). A process (procedure) step —
  which invokes the same action internally — carries the identical value as the reserved input member `_version`; the
  executor strips it before schema validation and passes it as the precondition. A template resolves it at step
  time (`"{{card.version}}"`, inside the step transaction), so it is the version read in that transaction, and a
  concurrent change is a `412` to the step — never a blind write and never a second body field on the action.

## 7. Events and jobs

- Outbox row written in the same transaction as the mutation:

```
event(seq bigserial pk, id uuid not null default uuidv7(), tenant_id uuid not null,
      topic text not null, type text not null, aggregate_type text not null, aggregate_id uuid not null,
      payload jsonb not null default '{}', correlation_id uuid, causation_id uuid, created_at timestamptz)
```

- `pg_notify('conveyor_event', seq::text)` in the same transaction. SSE consumes `seq` as the event id —
  `Last-Event-ID` replay is `seq > $last ORDER BY seq`, filtered by the subscribed topics.
- Delivery is at-least-once. Consumers must be idempotent on `id`.
- **Event `type` grammar: `<aggregate>.<past-tense verb>`, exactly one dot** (`card.moved`, `governed_asset.created`,
  `conversation_turn.created`) — the aggregate is the table that owns the emitter, matching §6's one-dot action rule.
- **A read action may emit — one named exception.** `models.render` and `models.export_xmi` emit
  `model.exported` (an export is an auditable act: who took a copy of which model, in which format). Every
  other read emits nothing. This is recorded rather than re-derived because "reads are pure" is a rule a later
  reader will assume; if a second read-emitting action appears, it belongs here, not in a handler comment.
- Topic grammar: `<aggregate>:{uuid}` (`board:…`, `run:…`) or `<aggregate>` for tenant-wide.
- `job(kind, dedupe_key)` is unique per tenant; workers claim with `FOR UPDATE SKIP LOCKED` and a fenced
  `lease_until` + `lease_token`; `schedule` rows materialise jobs under one advisory lock. No `setInterval`.
- **The bus (added 2026-11-09; S13).** The `event` row is the publish source: `event.bus_stream_id text null` and
  `event.published_at timestamptz null` are added by the implementation migration; a relay reads unpublished rows by
  `seq` (woken by `pg_notify`, polling as the fallback), XADDs the envelope
  `{id, topic, from, correlation_id, procedure, tenant_id, job, payload}` to Redis Streams, and records the stream
  id. Consumers use groups `process` and `tool`, `XREADGROUP`/`XACK` after the handler's transaction commits, and
  `XAUTOCLAIM` for entries a dead consumer left pending; after 5 deliveries an entry is XADDed to `bus:dead` with
  its reason and the original envelope. **The bus is a nudge, never the record:** `run`/`run_step`/`event` hold the
  truth, every handler is idempotent on the envelope id and its natural key, and a Redis loss is recovered by
  re-publishing the unacked rows. Topic grammar: `work.requested|succeeded|failed`, `tool.call|response`,
  `dead.letter`. **As built (2026-11-10):** the cursor columns are added by S13's `0035_process_tree.sql`;
  `crates/bus` implements the envelope, the two groups, `XAUTOCLAIM` and `bus:dead` after 5 deliveries; the relay
  reads `event` by `seq` and records `bus_stream_id`/`published_at`; the `process.step` job is the Redis-absent
  fallback (a run still executes from Postgres with Redis down); and CI starts a dind Redis from the archetype's
  `redis_image`. The `workflow.node` and `workflow.start` job kinds are **retained** with the workflow engine
  (owner ruling 2026-11-09, §21) — they are the tool-sequence executor's job kinds, never the process executor's.

## 8. Channels

- One `delivery` row per (message, channel, recipient). State machine:
  `pending → sent → delivered | failed | expired`.
- Channel kinds: `web`, `telegram`, `mcp`. A reply resolves to the same `approval` / `comment` / `cards.move`
  action regardless of channel — there is no per-channel mutation path.
- **One MCP surface (D19).** The app's own `rmcp` server at `/mcp` projects the action registry as tools — S04
  ships the transport, S10 the projection. Consumers differ by **principal**, not by protocol: the `agent` app
  authenticates with a licence bearer and acts as an `app_user(kind='agent')`; other machine consumers use their
  own principal. There is no signed channel and no five-command grammar.
- ✅ **CONTRACT CHANGE ACCEPTED — §8, "two MCP surfaces" → one (2026-11-08).** The 2026-11-07 rule ("the two
  surfaces must never be conflated; a token on `/mcp` is not a substitute for the signed channel") is withdrawn by
  SPEC **D19**. One surface, with the consumer boundary stated by principal. **S06b acts on this rule.** This block
  previously read "S06b may not act on this until the request is accepted"; that condition is discharged by D19, and
  S06a — which the withdrawn rule described — is not built.
- **S06d — one connector standard.** Web, Telegram and MCP are instances of one class; a connector kind
  is a registry entry (typed config + schema, named sealed secrets, default grants, authenticate/parse/send).
  `POST /api/channels/{binding_id}/inbound` is the one inbound path and `crates/actions/src/channel/command.rs`
  is the one text grammar; an adapter never authorizes. A binding's `config` stores non-secret values and
  **secret names**, never values; config and credentials are entered through the web UI and live in Postgres.
- A provisioned connector has one machine identity per `(tenant, kind)`
  (`app_user(kind='service', oidc_subject='service:<kind>:<tenant>')`, membership `kata_executor`). Its
  authorization is the tenant's `channel_action_grant` set for that kind, never the role's scope surface;
  `channels.policy_*` is the tenant-admin's matrix. `approval_decision.decided_by_kind`: a connector decision is
  `decided_by_kind='service'`, MCP is `'agent'`, web is `'human'`; `via` carries the channel and the
  transport sender reference.
- **Multicast and first response.** A pending approval is offered to every eligible channel
  (`interaction_dispatch`, one row per recipient; the PK is the idempotency). The approval's status guard
  arbitrates; `interaction.close` expires every live sibling delivery with `last_error='superseded:
  resolved via <channel>'` and sends one closure notice per dispatch. A question **is** an approval.
- **The machine floor cannot be granted.** `ActionSpec::machine_forbidden` refuses every `evals:approve`
  action, the held-out reads (`eval_results.get`, `eval_runs.export`), `eval_signoffs.request`, the
  administrative resources (`actions, catalogues, channels, departments, invites, jobs, licences,
  members, roles, schedules, secrets, tenants, webhooks`), `boards.set_member` (access control) and
  `workflows.*` authoring to every machine channel. The shared decision writer refuses a
  non-human or non-`evals:approve` decision on an `approval` with `eval_id IS NOT NULL`.
- Telegram inbound is authenticated by the binding's sealed secret-token header (not a deployment-wide
  secret); the deployment-wide `TELEGRAM_BOT_TOKEN`/`TELEGRAM_WEBHOOK_SECRET` vars are retired.
  An approval **and a question** are the same row: a question is an approval.

## 9. Auth and licences

- `Authorization: Bearer` only. **One identity, many memberships, one active tenant per request.** A person
  may hold a `membership` row in several tenants (the table is unique per `(tenant_id, user_id)`); the request
  selects one with the `X-Active-Tenant` header, **resolved only against the caller's own rows** (exact tenant
  id, then exact slug). A header naming a tenant the caller has no row in is `403 active_tenant_not_permitted`
  — indistinguishable from a tenant that does not exist. A caller holding more than one membership that names
  none is `400 active_tenant_required`: the conveyor never guesses a tenant, because an audit row attributed to
  a guessed tenant is not a proof. **The one exception is `GET /api/me`**, which answers `200` with the
  caller's membership list and `active_tenant: null` when no tenant was named — it is how the shell draws the
  tenant switcher, and it acts in no tenant. A caller with exactly one membership needs no header. A licence bearer's
  tenant is the credential's; a header may repeat it, never contradict it (`403`). There is **no** super-admin
  "tenant-admin everywhere" rule: the role is the selected row's.
- Keycloak realm `conveyor`; UI = public PKCE, backend = confidential service account. OIDC only.
- Machine tokens: licence tokens are non-JWT; resolution is fail-closed with a short-TTL in-process cache. A
  JWT-shaped token that fails verification is **never** retried as a licence token.
- App licence: `app_licence` synced by a job from `GET /licenses/active?tenant_id=&app_key=conveyor`; one
  `RequireLicence` extractor; core requests never call the licensing service synchronously.

## 10. Local development and tests

- `compose.yaml`: `postgres:18` + pgvector, published `127.0.0.1:5439`, database `conveyor_dev`. The app binds
  `0.0.0.0:8930`; **the same port inside and outside a container** — which for Postgres requires the service to
  start it as `command: ["postgres", "-p", "5439"]` (the image defaults to 5432, so a plain `-p 5439:5432`
  mapping would break the rule), and pgbouncer listens on `6439` inside and out.
- Integration tests start their **own** ephemeral `postgres:18` container on a random host port and remove it at
  teardown. Never `5432`; never the compose database; never a shared cluster database.
- `DATABASE_MIGRATE_URL` points directly at Postgres; `DATABASE_URL` points at PgBouncer. Startup `migrate`
  runs as a separate subcommand, not implicitly in `serve`.
- No mocked transport for behaviour: tests run against real Postgres and a real HTTP server.
- **Dev-auth mode.** `CONVEYOR_DEV_AUTH=1` accepts `Authorization: Bearer dev:<login>` instead of OIDC.
  `<login>` resolves through `membership` by §9's rule: one membership needs no header, several need
  `X-Active-Tenant`, and zero is a `401`.
  The seeder (`migrate --dev-seed`) creates **`dev-a`/`alice`** and **`dev-b`/`bob`**, so `dev:alice` is the
  dev-a principal. Refused when `APP_ENV=production` (a serving process exits non-zero at startup; a worker authenticates nobody and need not refuse). **Every local gate that calls the
  API** starts `serve`/`worker` with the flag and sends that header; a k6 harness reads it from
  `DEV_AUTH_TOKEN`. A headerless request to an authenticated route is a 401, so a gate that omits it is red,
  not anonymous. **The browser needs a cookie, not a header**: in dev-auth mode the app also accepts a
  `conveyor_dev=<login>` cookie (same resolution, same refusals), and the UI shows a dev sign-in only while the
  flag is on. S12's Playwright suite carries that cookie in `storageState`; a spec never drives real OIDC.

## 11. UI

- The stack is decided by S1 (Leptos CSR is the prior; SSR only if measured TTI > 2 s or auth forces cookies).
- The spike lives in `spikes/web/` — **never a workspace member**, deleted after S1 records its decision.
- No slice may assume a UI stack before S1 closes; UI work in S5–S11 is expressed as screens + data
  requirements, not framework code.

## 12. `parity.yaml`

One entry per used feature — **nine keys, all required** (`area` and `evidence_class` were added
2026-11-07 on S00's CONTRACT-CHANGE REQUEST; both are needed because the classification rules rank
evidence and S14 gates per area):

```yaml
- id: P-0001
  app: fr | rm | planka | n8n
  area: board | control-plane | requirements | modelling | documents | channels | copilot | admin | procedures | workflows
  surface: <route, action or flow id>
  source: <path or query>
  evidence: <count / log query / DB row>
  evidence_class: A | B | C | D | E   # A flow-JSON/call-site, B Loki sample, C execution, D row count, E catalogue
  acceptance: <executable check that proves the conveyor does it>
  status: used | rare | unused | dropped
```

Owned by S0, consumed by every slice (`parity_ids`) and by S14 (100 % of `used` items pass).

## 13. Bootstrap without git

`template-rust-service/scripts/rename.sh` uses `git ls-files` and cannot run in a directory that is not a repo.
The workspace is therefore built by hand from the template's **patterns** (Dockerfile shape, `config.rs`,
`error.rs`, tenant extractor, health route, `utoipa` wiring, test harness). `gitops-production/scripts/new-rust-target.sh`
is used later, when the repo is created. Do not `git init` before then.

### 13.1 Working rules in the bootstrap tree (added 2026-11-08, measured)

With no VCS there is no branch, no diff and no recovery, so the tree can hold **one writer at a time**. These
rules exist because three sessions edited this directory in one day: one session's `cargo fmt --all` reformatted
another's files, a half-written `mcp.rs` made the workspace unbuildable for both, and a peer's red was read as a
regression.

- **One writer, one lease.** The current lease is a row in `00-INDEX.md` §Tree lease —
  `session | slice | module dirs | started | released`. Its owner opens it before the first edit and releases it
  after the gate. A peer may **read** anything; it may not edit while the lease is open. A second writer is
  arranged by message, and by splitting the tree by module — never by hoping.
- **Format by crate, never by workspace.** `cargo fmt -p <crate>` (or `-- <paths>`). `cargo fmt --all` is the one
  command that rewrites files you do not own.
- **A gate measures a tree, not a change.** Never run `cargo test --workspace` while another lease is open: it
  compiles that session's half-written files and the result is attributed to whoever ran it.
- **`target/` is shared.** Never `cargo clean` — the rebuild is paid by everyone. Disk relief is
  `target/debug/incremental` only, and it is reported rather than silently taken.
- **No recovery exists.** Read a file in this session before editing it (an overwrite is final), keep edits inside
  the module directories your slice owns, and never "clean up" a file that belongs to another slice's work.
- **The handoff is the only history.** `00-INDEX.md` is updated by the lease owner at the end of a slice, with the
  measured numbers — not with intentions.

## 14. Slice-plan template (every `slices/S*.md` uses exactly this)

Front-matter, then ten sections. No plan may add a section; a plan that needs one returns a
`CONTRACT-CHANGE REQUEST`.

```yaml
---
slice: S05
title: Board
status: planned          # planned | ready | superseded
depends_on: [S03, S04]
spec_sections: ["§5.3", "§6", "§8 S5"]
adds_tables: []
adds_actions: []
adds_events: []
adds_modules: []
parity_ids: PENDING-S0
---
```

1. **Objective** — 2–3 sentences: what exists at the end that does not exist now.
2. **Starting state** — what is already built when this slice starts (≤3 bullets).
3. **Scope** — IN / OUT, explicit; OUT names where the spec puts it instead.
4. **Source material** — table `path | read this | carry | do not carry`, paths verified to exist,
   pinned SHA, symbols not line numbers.
5. **Deliverables** — exact file tree, table DDL (or "none — see 02-DATA-ARCHITECTURE §X"),
   action names with input/output types, endpoints, screens.
6. **Build order** — 10–20 numbered steps, each ≤1 day, each independently testable, each with its command.
7. **Gate** — executable acceptance commands and what must be green. SPEC §8's wording is the floor.
   The block opens with `set -euo pipefail` and **every line is an assertion that exits 0 only when its
   property holds**: a probe for absence is `test -z "$(…)"` or `! grep -q …`, never a command that merely
   prints (a `find | wc -l` that prints `0` asserts nothing, and a bare `lsof` exits 1 when the port is free —
   the desired state). A gate whose exit status cannot go red is not a gate. Where a command legitimately
   returns non-zero in the passing case, the line is rewritten as an assertion; `# fail-open:` is reserved for
   the residue and must be zero.
8. **Traps** — slice-specific, from SPEC §11 plus the measured source traps.
9. **Decisions closed here** — the answers this slice owns, each with a one-line rationale; contract-level
   changes go in a `CONTRACT-CHANGE REQUEST`, never applied.
10. **Open questions** — ≤5, each with a default answer.

Caps: 1,000–1,500 words, hard 1,800; **S06b, S08, S13 and S14 hard 2,400** (each holds two or more phases —
channel in/out, governance vs modelling, engine vs editor, deploy/seed/rehearsal/decommission). **Caps count
narrative prose only**: YAML front-matter, fenced code blocks **and markdown tables** are excluded — the rule
exists because this contract prefers detail in tables ("tables and bullets over prose"), so a table cannot be
both the preferred form and the counted one (2026-11-07: a raw `wc -w` put five plans over a cap that none of
them exceeded in prose; and a later count including tables condemned plans whose whole detail lives in a table).
Measure with: strip front-matter, strip fenced blocks, drop lines starting with `|`, then `wc -w`. Never
restate the SPEC.

`adds_modules` names the **crate/module area the slice fills** (a label for the reader). §1's module list says
where code lives; it is not a whitelist for this field.

**S12 is exempt from the 1,000-word floor** — it is a derived checklist (screens → owning slice → read model →
Playwright spec) and is deliberately thin; it may not invent a screen, action or table.

**The one permitted extension:** S00 is a measurement protocol, not a build slice. It carries two extra
sections — `Classification rules` and `Feature IDs and areas` — because they are the rules by which its output
is judged. They sit **after §8 Traps** as §9 and §10, shifting `Decisions closed here` to §11 and `Open
questions` to §12; the first eight sections keep their contract order. Every other plan uses the ten sections
above exactly.

## 15. Change control

- A slice plan that needs a different cross-slice rule states `CONTRACT-CHANGE REQUEST: <rule> → <proposal> →
  <why>` and does not act on it.
- Names are defined once: a table, action or event appears in exactly one slice's `adds_*` front-matter.
  Later slices reference, never redefine.

## 16. Toolchain and local environment (measured 2026-11-07; **build host changed 2026-11-09**)

- ⛔ **Rust builds and tests run on the build host `m1`, never on this machine (2026-11-09).** `~/.local/bin/cargo`
  and `~/.local/bin/rustc` are **shims that refuse inside any Cargo project** — the refusal is deliberate (the
  workspace's `target/` reached 84 GiB on a 228 GiB volume). Do not work around it, and do not add a
  machine-level `~/.cargo/config.toml`: cargo gives `config.toml` precedence over the repo's `Cargo.toml`.
  Use **`rbuild -- <cargo args>`** (e.g. `rbuild -- test --locked`), **`rbuild --gate`** for the LANE gate
  (`fmt --all -- --check && clippy && test --locked`), and `--detach` + `--status` / `--logs` for long runs.
  A one-off may override with `ALLOW_LOCAL_RUST=1`. Full detail: memory `remote-rust-builds-on-m1.md`.
- ⛔ **Therefore every gate block in this set is wrong as written, and a slice that copies one blindly runs
  nothing.** Two mechanical translations: (a) a line that invokes `cargo` becomes `rbuild -- <args>`, and the
  opening `source ~/.cargo/env` is removed (the shim is what must be reached, not bypassed); (b) `rbuild` never
  runs on this machine, so **it cannot satisfy a local `psql` assertion in the same script**. A gate may still
  probe the local compose database, but then it must say how that database was migrated — the binary's `migrate`
  is itself a cargo command and therefore a build-host action. ⚠️ The clean form is to assert in **Rust**
  (a `#[test]` that queries) rather than in `psql`, so the assertion travels to the host with the tests.
- **Toolchains:** the host runs **rustup 1.99.0**, the exact pin in `rust-toolchain.toml`; `rbuild` invokes the
  rustup path by absolute path and refuses on a version mismatch. Homebrew's rust (1.99.1) ignores
  `rust-toolchain.toml` and must never be what compiles this tree. `wasm32-unknown-unknown`, `trunk` and
  `cargo-leptos` are **not installed** — S01 installs them.
- **Tests get their own Postgres on the host:** `rbuild` starts a throwaway native cluster (`initdb -A trust`
  under `/tmp/rbuild-pg.*`, port 55432+) and points `TEST_DATABASE_URL` at it, tearing it down afterwards.
  `-A trust` is required: both harnesses rewrite the URL with literal `postgres/postgres` superuser credentials
  when creating each test database (§10). ⛔ Do **not** go back to a `--tmpfs` container — a full run died twice
  with `could not write to file base/<oid>/PG_VERSION: No space left on device`, once presenting as a bare
  HTTP 500 from `projects.create`.
- **One build at a time per repo** — a `mkdir` lock on the host (`~/build/<repo>/.rbuild/lock`); a second run
  exits **75** and names the owner. macOS has no `flock`; do not add one.
- **Ports:** app `8930`, compose Postgres `5439`; both free locally on 2026-11-07 (`lsof` empty). Never 8000.
  A cluster binding is re-checked at S02 before use.
- **Docker** 29.9.x is up (measured 29.9.0 on 2026-11-07; an earlier draft froze 29.9.2, a different patch —
  do not chase the patch level). Compose is the **local dev** database only; since 2026-11-09 the *test*
  database is the host's throwaway cluster above, not a container. There is **no bare-metal Postgres on this
  machine** and none may be started (2026-10-21).
- **Disk:** the 84 GiB `target/` is gone and this volume now sits at **22 % used (88 GiB free, 2026-11-09
  measured)** — the reason the build moved. The space pressure that remains is the *source* tree plus wasm and
  Playwright; `df -h` before a heavy download, disk full ⇒ STOP.
- **`check-jsonschema`, `k6` and `sqlx-cli` are not installed.** S00's gate installs the first once with
  `uv tool install check-jsonschema`; S05's load gate installs `k6` (`brew install k6`). **Migrations always
  run through the binary** — `cargo run -p conveyor-server -- migrate`, i.e. `rbuild -- run -p conveyor-server --
  migrate` — never `sqlx migrate run`: the binary's `migrate` re-applies the runtime grants after the last
  migration (S02 §5), and `sqlx-cli` is not on this box.
  `npx`, `ruby`, `yq` and `jq` are present (S12's Playwright and this set's YAML/JSON checks).
- **A pinned SHA is a snapshot, not the tip.** `fr@4a579e91` / `fr@6f7c5097`, `nonagenticai_mcp_server@dd2cabf6`
  and `template-rust-service@0c7396a0` are what the plans were written against; `origin/main` moves. Re-read a
  pinned file at its SHA (`git show <sha>:<path>`) or re-capture the tip at the slice's step 0 (S06b does).
  A difference between the tree and a pin is not a plan defect.

## 17. The executor model (added 2026-11-09; implements SPEC D22)

A **work executor** drives a `run`. Two kinds exist (amended 2026-11-09); the process is the slot:

| executor | drives | steps | recorded as |
|---|---|---|---|
| `process` | S11/S13 — a `process` whose steps are `agent \| computed \| gate \| call` | an agent step runs the bounded loop; a computed/tool step invokes one action; a gate parks; a call starts a child run | `run.process_id`, `run_step`, `run.parent_run_id`/`step_id` |
| `external` | an agent principal over `/mcp` (D19) running a project as a task (D21) | not in this app | `run.external_ref` = task id; `action_invocation.actor`; `approval_decision.decided_by_kind`/`via` |

- **Two agent roles per capability (added 2026-11-09).** A `capability_agent` row is a **process agent** (one per
  process / card type — the worker that decides, holding the process's domain knowledge) or a **tool agent** (one
  per toolset — the executor that makes the tool calls). The process agent's `allowed_actions` is the tool surface it
  may ask for; the tool agent's is the exact set it executes. The role is a column, not a second table.

- **One persisted spec, resolved before the loop.** `capability_agent` (S15) is the only record of `model_ref`,
  `prompt_ref`, `params`, `allowed_actions`, `max_iterations`, `timeout_secs` for a model-driven executor. S15's run
  wiring stores `run.capability_agent_id` at run creation; the step executor (`process.step`; the job kind S11's `procedure.step` is renamed to by `0035`) resolves that
  row — the run's **process agent** — into `{model, messages, tools, max_turns, capability_agent_id, prompt_ref}` and
  passes it as the `agent.run` step input. **`agent.run` is a pure loop and never queries configuration itself.** A
  step input may lower a cap but may not raise it above the row, and the row may not exceed this section's ceiling.
- **Caps are hard, validated, and typed.** Defaults/maxima: 8/20 turns, 20/50 tool calls, 300 s, agent→agent depth 0.
  `capability_agents.create|update` refuses `max_iterations > 20` or `timeout_secs > 300`; the executor fails typed
  (`agent.turn_limit`, `agent.tool_limit`, `agent.timeout`) — never silent truncation.
- **Scope is the intersection (model-driven executor).** An action outside `allowed_actions ∩ principal scopes` is
  refused **before** execution (`action-not-found` — corrected 2026-11-09 by owner ruling: S04-D6's non-disclosure wins, because a distinct
`action-not-permitted` tells the caller the tool exists. The registry, `agent_surface` and the copilot all return
`action-not-found`; this sentence was the only place the other name appeared); it is never attempted and audited after the fact. The S10
  projection and `/mcp` `tools/list` apply the same rule. An external executor has no `allowed_actions`; its scope is
  its principal's — since S06d, the tenant's `channel_action_grant` set for the principal's connector
kind (`mcp` for the agent's licence principal), never a hard-coded name list.
- **Prompt discipline.** `prompt_ref` names a **versioned recipe** `<name>@<version>` in the tenant's Postgres
  catalogue (D27, owner ruling 2026-11-09). The step executor **compiles** it at run time from the run's sources —
  the procedure/workflow and its step, the capability/agent, the project and card, the bound knowledge packs — and
  carries exactly one system message plus `{capability_agent_id, prompt_ref, model_ref}` in the `agent.run` step
  input, so `action_invocation.input_hash` and the trace pin what ran, and `inference_call.prompt_sha256` pins the
  compiled text. A required source the surface lacks is `409 capability_not_ready` naming the pin and the section,
  never an empty or ad-hoc prompt. No handler concatenates prompt text ad hoc; the copilot and the prompt audit
  resolve through the same catalogue, and the S21 design (selection + budgets + the `plan` projection) extends this
  renderer without changing the contract.
- **Readiness before execution.** A run must not start when a binding is unusable: a `required` pack has no resolvable
  leaves, an `allowed_actions` entry is not a registered action, or the `prompt_ref` does not resolve. The refusal is
  `409 capability_not_ready` with `blocking[]`, **before** the first step. `knowledge_degraded` remains the mid-run
  equivalent for a pack that disappears after start. `capabilities.get` returns the same `{ready, blocking[]}`
  projection.
- **One memory writer.** `memory.remember` is the only **action** that writes `work_package_memory`; the
  `memory.consolidate` job may append one `kind='digest'` row and stamp `consolidated_at`, and nothing else. A copilot
  turn or an `agent.run` step records what it learned through `memory.remember`, never by SQL. Hot rows are the
  record; the digest is derived.
- **The copilot is not a work executor.** S10's turn runner is an interactive, user-scoped loop over the same action
  surface. It uses the tenant default model via `completions.create`, records model/tool calls on
  `conversation_turn.meta`, and needs no `capability_agent`. A card-scoped turn may read the card's capability for
  knowledge; it does not execute the card's run.
- **No second runtime.** The conveyor hosts no model and no embeddings. The care-bus Redis Streams layer (§7) is
  transport between the conveyor's own workers, not a second runtime and never a system of record. `agent` is an
  external executor (D22), never a dependency; a proposal to make the conveyor call it as an internal runtime is a
  change to a taken decision.

## 18. The inference record (added 2026-11-09; implements SPEC D23)

Every model call the conveyor makes leaves one `inference_call` row. It is an **operational audit record, not a
billing source** — the router meters the same call into `billing_service`, which already stores both cache classes,
so a second emission would double-count.

- **The capture seam is the engine, not the caller.** An external `ActionSpec` that expects model usage declares
  `captures_usage = true`; `invoke_external` parses the response's OpenAI `usage` object and writes
  `inference_call` **in the same transaction as `record_bookkeeping`**, so a call and its usage cannot diverge and
  a rolled-back invocation leaves no usage row. Two actions carry it: `completions.create` (S10, **built**) and
  `inference.chat` (S13, **declared — set when that slice lands**). No handler parses `usage` itself, and no second
  writer exists.
- **Cache classes are defined, not inferred.** `cache_status ∈ hit | write | miss | unknown`:
  `cache_read_input_tokens > 0` ⇒ `hit`; `cache_read = 0 ∧ cache_creation > 0` ⇒ `write` (a deliberate warm,
  priced ≈1.25×, **not** a miss); both zero ⇒ `miss`; no `usage` in the response ⇒ `unknown`. **`inference-router`
  has no response cache** — a report or a screen must never call these "inference cache" without the provider
  qualifier, because the only cache in the path is the model provider's prompt cache.
- **Hashes and counts, never bodies.** The row carries `prompt_ref`, `prompt_version` and `prompt_sha256` (of the
  rendered prompt) and no prompt or response text. The router keeps the text; the conveyor keeps the key. A caller
  that needs a verbatim snapshot attaches an S09 `document` id instead.
- **One correlation key.** The engine sends `X-Correlation-Id` on every external model call, and the value is
  stored on the row. Neither side reads the other's database: reconciliation is per-call through that id, and in
  aggregate through billing.
- **Join keys are the caller's job.** `caller_kind` is required, and exactly one of `run_id` / `run_step_id` /
  `conversation_turn_id` accompanies it for `process_step` / `agent_run` / `copilot`. A row
  with no join key is legal only for `other`, so "whose call was this" is always answerable.
- **The audit proposes; it never applies.** `prompt_audit_finding.status` moves `open → accepted | rejected → applied`,
  and **every transition is a person's** (`prompt_findings.decide`); the `applied` stamp is written only after an
  ordinary `procedures.update` on a draft. The audit holds `inference:read` and `inference:write`, never
  `procedures:write` — it must not be able to edit the surface it audits.
- **The audit is a procedure, not a runtime.** `prompt_audits.start` enqueues `prompt_audit.run`: deterministic
  inventory and signal scan, then **one bounded `completions.create`** call against the built-in versioned prompt
  `prompt_audit@1`. The prompt version is stored on the audit row, so every finding is attributable to the prompt
  that produced it (§17's `prompt_ref` rule, applied here first). No tool loop, no `capability_agent`, no new
  executor.
- **Two triggers, one policy.** A tenant configures *after N executions* in `tenant.settings.prompt_audit`
  (`{enabled, after_executions, surfaces, target_model}`) through the existing `tenants.update_settings`. The policy
  is enforced **at capture time**: after the engine writes an `inference_call`, it counts the tenant's rows since
  the last audit (excluding the audit's own `prompt_audit@1` call) and, when the threshold is reached, inserts the
  queued audit and its one job inside the same transaction — **the ledger is the counter**, so the trigger cannot
  drift from the thing it counts. `target_model` is required for an event-created audit (the row's `target_model`
  is NOT NULL); an enabled policy without one is not armed and the capture seam logs and skips, never guesses.
  `prompt_audits.start` is the manual trigger for a person or the kata. A draft is not on the cadence at all: it is
  checked **at publish** (`prompt_audits.check`), which refuses a `high`-confidence finding with
  `422 procedure_audit_findings` unless the publisher sends `acknowledge_findings: true`, and that override is
  recorded on the publish's `audit_event`.
- **Kata needs no new surface.** Kata's prompt artifacts are procedures in the kata tenant; the audit takes a
  tenant scope like any other, and `prompt_audits.start` is the seam the kata's change/replay loop calls. The
  **kata-executor is the consumer** of a finding: it reads `prompt_findings.list` and starts the next audit itself,
  which is why the findings are a feed rather than a report nobody reads. The kata domain stays outside this spec
  (`02-DATA-ARCHITECTURE §7`).
- **What this asks of the two services, and what it does not.** `inference-router` (at its Rust rewrite) must
  **preserve the `usage` object** — `prompt_tokens`, `completion_tokens`, `total_tokens`,
  `cache_creation_input_tokens`, `cache_read_input_tokens` — and **keep accepting `X-Correlation-Id`**. That is the
  whole ask; it needs no cache endpoint, no new response field and no tenant delegation, and the conveyor never
  reads its ledger. `billing_service` needs **nothing**: it already stores both cache classes on
  `billing_cost_events`, and §4's "never absorbed" stands. ⚠️ Stated as a consequence, not a requirement: the
  router's own `inference_call` ledger carries **no token columns**, so a cross-service reconciliation is per-call
  by `correlation_id` only, and in aggregate through billing — do not plan a report that joins the two ledgers on
  numbers.

## 19. The benchmark harness (added 2026-11-09; implements SPEC D24)

`conveyor-bench` is **absorbed**, not integrated: the conveyor is the only harness — seed, produce, sandbox,
grade, compare, display. There is no Python in the product, no external runner and no second service.

- **The dataset is a hashed seed artifact**, never a network call at run time. `seed/benchmark/<slug>.json`
  carries the pinned dataset identity (`dataset`, `dataset_revision`, `seed_sha256`), the image-lock hash, the
  `swebench_version` whose semantics are reproduced, and one row per instance **including the `eval_script`**,
  generated at seed time by the conveyor's own Rust port of swebench 5.0.2's `make_test_spec` (a byte-for-byte
  transform; a golden test pins it to the harness, which runs once in development as the oracle and never in the
  build or the image).
  A suite is immutable: re-seeding the same slug with different hashes is `409 suite_seed_conflict`.
- **Answers are grader-only.** `gold_patch`, `test_patch`, `FAIL_TO_PASS`, `PASS_TO_PASS` and `eval_script` live
  on `benchmark_instance` and are read by the grader. A producer reads only `problem_statement`, `repo`,
  `base_commit` and `oracle_files`; no action returns the answer fields, and no benchmark action is
  grantable to a machine channel. Nothing that runs model code can read the answer key.
- **The grader is the only authority.** `resolved` is computed by the conveyor's port of swebench's rule for the
  suite's pinned semantics — never by the producer, never by a person. The ladder, in order: apply-fail marker
  before the eval marker ⇒ `apply_failed`; no eval marker ⇒ `infra_error`; timeout marker ⇒ `eval_timeout`;
  otherwise parse the test statuses; every `FAIL_TO_PASS` passed and every `PASS_TO_PASS` still passes ⇒
  `resolved`, else `unresolved`. No statuses parsed **and** no end-of-test marker ⇒ `infra_error` (the eval was
  cut off); no statuses with the end marker ⇒ `unresolved` (the code emitted none). An infra fault is never
  published as a model failure.
- **Parser parity is a test, not a claim.** The banked eval logs must reproduce the banked verdicts exactly, and
  the banked control runs their known answers (gold 50/50, empty 0/50). A parser that cannot match fails the
  build. One parser per repo in the suite; the parser version is part of the suite identity.
- **A run is `arm × subject`.** `arm ∈ gold | empty | baseline | conveyor`; `subject_kind ∈ suite |
  capability_agent | procedure`, and a `conveyor` run pins the executor version it measured. `conveyor` is
  refused with `409 arm_not_ready` until S13/S15 land. Gold and empty are the controls that make any number
  publishable: `control_ok=false` is recorded and shown, the run is never deleted, and its number is never
  presented as a score.
- **`no_attempt` is never a model failure.** It is withheld from the graded denominator, exactly as the old
  harness did; an outage must not be published as a model result.
- **Compare is exact and refuses.** Two-sided McNemar over the paired graded instances
  (`p = min(1, 2·Σ_{i≤min(b,c)} C(n,i)/2^n)`); `comparable:false` (200, with the reason) when the suite, oracle
  or prompt differ — **not the pipeline, which is what is being compared** — or when either run is a control
  that missed. Missing pairs are reported (`missing_in_one`), never silently dropped; a differing model is
  disclosed, not refused.
- **One driver.** `benchmark.run.poll` is the only writer of a run's progress — it starts up to `concurrency`
  cases, polls running sandbox Jobs, captures logs, grades and finalises, then re-enqueues the next tick as a
  new job (`max_attempts = 1`, per-tick dedupe key). `benchmark.reap` fails a run whose
  `last_polled_at` is stale and deletes its orphaned Jobs. A stall is a dead driver, not 30 minutes of case
  silence: a 50-instance run legitimately takes hours.
- **Sandbox isolation.** Untrusted patches run only in a `batch/v1` Job in the sandbox namespace: no service
  account token, no host mounts, CPU/memory/ephemeral-storage caps, a pod deadline, and the instance image by
  digest from the lock. The conveyor never executes test code in its own pod. Job and ConfigMap names are
  deterministic (`swb-<run8>-<instance-slug>`), so a repeated create is a reattach, not a second job.
- **The sandbox is fail-closed.** No configured namespace or client ⇒ `503 benchmark_sandbox_not_configured`
  before any case starts. A missing image, an over-1-MiB ConfigMap or an unreachable API is a typed refusal,
  never a silently empty patch.
- **Kata is a consumer, not a second runtime.** The improvement loop invokes exactly the actions a person would
  (start → observe `benchmark_run.changed` / `benchmark_runs.get` → `compare` → keep or revert); `promote` marks a
  run as the suite's baseline. The loop's verdict is data, not a hidden state machine.
- **No bodies in events.** `benchmark_run.changed` carries status and counts; `benchmark_case.recorded` carries
  the instance and verdict. Patches are bounded rows (256 KiB, over is a typed 422) and logs are S09 documents.

## 20. The eval record (added 2026-11-09; implements SPEC D25/D26)

An **eval** measures one Claude-powered flow. It is the native form of the `claude-api` `build-eval`,
`eval-audit` and `eval-hillclimb` methods, exactly as §18 is the native form of `prompt-audit`; S19 builds the
instrument and the report, S20 builds the climb.

- **One eval, one flow.** A second flow is a second eval, never a second case set on the same row. A grand unified
  benchmark is the method's named failure.
- **The subject is a registered action.** `eval.subject_kind ∈ completions | copilot | procedure | external |
  agent_run`; the runner invokes the action through the ordinary engine and records the
  `action_invocation_id` (and the `run_id`, when the subject produced one). It never reconstructs a model call:
  re-implementing the app measures a second implementation, which is the one thing an eval must not do.
  `agent_run` is declared and refused `409 subject_not_ready` until S13/S15.
- **The instrument is versioned.** `eval` is the logical flow; `eval_version` freezes its case set, metrics,
  graders, prices and harness paths under one `digest`; `eval_run` names a version. **A comparison across two
  versions, graders or splits is refused** (`409 not_comparable`) — scores from before and after a grader change
  are not comparable.
- **Two sign-offs are `approval` rows**, `gate='eval_inputs'` and `gate='eval_grading'`, carrying `eval_id` and
  `approval.meta.artifact_sha256`. `eval_runs.start` fails closed `409 eval_not_ready` naming every missing gate.
  **Editing the instrument after a sign-off voids it** — the digest no longer matches, and the gate reopens.
  §9's one-primitive rule is why there is no second escalation store; `approval_id` inherits the web, Telegram and
  MCP decision paths and the `decided_by_kind` audit for free.
- **`uq_appr_eval_pending` is the concurrency guarantee**, not a pre-check: `UNIQUE (tenant_id, eval_id, gate)
  WHERE status='pending' AND eval_id IS NOT NULL`. `uq_appr_pending` cannot cover this case — `run_id` is NULL for
  an eval gate and Postgres treats NULLs as distinct, so a `meta`-only gate would admit two pending sign-offs on
  one eval.
- **`evals:approve` is separate from `evals:write`.** Building is not signing. The sign-off **decision**, the run
  start, and a `test`-split trace read all require it, and **none of them can be granted to a machine
  channel** (`ActionSpec::machine_forbidden`; the S06d connector standard replaced `AGENT_TOOLS`). It is granted to
  `owner`/`admin` and **not** to `kata_executor` — a role held only by a machine principal cannot be a sign-off
  scope, or §9's "a sign-off a machine can give is not a sign-off" becomes false one role at a time. Requesting a
  sign-off needs only `evals:write`; deciding it needs `evals:approve` (and no machine may request one). The held-out wall is data access rather
  than a prompt instruction.
- **The answer key is structurally unreachable.** `expected` is write-only: no action returns it, the subject-input
  type has no such field (so a handler cannot pass it by accident), and no answer-key action is agent-reachable.
  This is the §17 readiness rule's sibling — correctness enforced by a type, not by discipline.
- **One `job` per `(case, rep)`**, `dedupe_key = <run_id>:<case_id>:<rep>`. A failed attempt writes
  `eval_attempt_error` — the `errors.jsonl` sidecar — with its failure class
  (`refusal|harness|serving|timeout|served_model_mismatch|grader`), `attempt`, `retry_count` and, when the call
  completed, its `model`/`usage`; it **never** writes `eval_case_result`, because a row at that slot would make
  resume skip the case forever and would score plumbing as a model failure.
- **Every result carries `model` from the response**, `usage`, and `grade` as an object keyed by metric id
  whenever more than one quality metric is declared. `status ∈ ok | truncated`; a clipped answer is counted,
  excluded from the mean, and never scored as wrong. The per-metric mean, the CI and the per-tag breakdown are
  **computed in Rust from the rows** — `grade` is never a SQL aggregate target. `trajectory/scores.tsv` in the
  skill is a derived artifact for the same reason and has no equivalent column here.
- **Nothing stores cost.** It is derived at read time from `inference_call` (`caller_kind='eval'`,
  `eval_run_id`) plus the judge's own `judge_model`/`judge_usage`, with cache writes at 1.25× input and reads at
  0.1×. A maintained counter would drift from the rows it counts, and D23 already forbids emitting cost.
- **The split is drawn once**, at random and stratified by `tags[0]` — never by baseline score — and locked by
  `eval_version.split_sha256`. Re-drawing is `409 split_locked`. A train slice selected for low scores
  guarantees regression to the mean and a flat held-out number, which is the failure the split exists to prevent.
- **The analyzer sees train transcripts only; the headline is test.** `eval_climbs.round` records the trace ids
  it handed over, and a round citing a `test` id is void. The wall is enforced at the scope, not in the prompt.
- **A grader fix is a regrade, never an edit in place.** `eval_runs.regrade {source_run_id, change_md?}`
  (`evals:approve`) creates a **new** `eval_run` under the eval's **current version** and enqueues one
  `eval.regrade` job per stored slot. The job reads the source `eval_trace`, grades the captured `output`
  against the new version's metrics and **never invokes the subject** — so `usage` is empty on the new result
  (the call was billed on the source run) and `judge_usage` carries the regrade's own cost. Slots are matched
  by `eval_case.ordinal` (a version clones cases to new ids and keeps ordinals); an ordinal whose `input`
  changed is counted `not_regradable`, never graded. The source's own version is refused
  (`409 version_unchanged`), because a regrade exists to change the instrument. Since a comparison across two
  versions is refused, a grader change means **regrade every arm, then compare new-vs-new** — a new grade is
  never compared against the run it replaced.
- **The transcript carries the output it graded.** The assistant turn stores `content` **and** the raw
  `output`, so a later regrade grades the same value the original run did. A transcript without it (written
  before this capture, or too large to store) is not regradable: the slot is skipped and counted, never
  guessed.
- **The climb's report is a column, not a derived view**: `eval_climb.narrative_md` (migration `0029`) is
  rewritten by `eval_climbs.stop` — status table, the four-part summary and the named winner. A cost goal
  names its **cheapest kept** round; a quality goal names its **highest held-out** kept round.
- **Traces are one row per result**, `eval_trace.turns` as the skill's exact
  `system|user|assistant|tool_call|tool_result` list. No base64: a binary payload is an S09 `document` and an
  `attachments.ref`. `turns` is capped, and case text is untrusted input — it is data until a view renders it,
  and no view may render it as HTML.
- **Retention:** `eval_trace` at 90 days (it is the bulk); `eval`/`eval_version`/`eval_case`/`eval_metric`/
  `eval_case_result`/`eval_audit` live with the eval, because a re-run against a pruned case set measures
  nothing.


## 21. Processes, steps, SOPs and the process map (added 2026-11-09; amended as built 2026-11-10; implements SPEC D6/D20/D22/D28/D29/D30)

The DAG is **not the top-level executor**: execution is an ordered list of steps, and the DAG returns only as the
**user-facing process map**. The workflow engine (`crates/workflow`, the `workflow` table) is **retained** (owner
ruling 2026-11-09) as the **tool-sequence executor** a process `call` step can name; it is not a second top-level
executor.

- **Storage (migration `0035_process_tree.sql`).** S11's `procedure` is **renamed `process`** and `procedure_step` → `process_step`;
  a **new `procedure` table is the SOP** (definition of ready / done / acceptance / inspection), so the word
  *procedure* means the SOP, never the sequence. `process_map` is a fourth, versioned document table. `0035` also
  moves `inference_call.caller_kind` `procedure_step` → `process_step`. S13's second migration is
  `0036_s19_output_grade.sql`. **The train's `0033_d23_eval_caller` and `0034_s18_kata_arm` keep their numbers, and
  there is NO drop migration:** the workflow engine is retained — `workflow`, `run.workflow_id`, `workflows.*`,
  `workflow_runs.*` and its job kinds stay.
- **A step** (`process_step`) is one of four kinds: `agent` (a process agent runs the bounded `agent.run` loop),
  `computed` (one action), `gate` (an approval parks the run in `waiting_approval`; a gate names no action, and an
  approval records the decision as the step's output), `call` (a child run, depth ≤3 — `child_process_key`
  resolves **process-first, workflow-second**, so the retained engine is the tool-sequence fallback).
  `actor ∈ process_agent | tool_agent | system | human`; `capability_id` names the owning capability;
  `process_agent_id` the worker; `procedure_id` the SOP; `on_result ∈ continue | fail | retry`, bounded.
  **As built (2026-11-10):** both starts run on the executor's transaction (`process_runs.start` for a process;
  `start_published_tx`'s child branch for a workflow), the child id is stored in `run_step.output`, the poll
  re-reads the child by id under its own `call` tag and counter, a succeeded child's last step output is judged,
  a failed/cancelled child applies `on_result`, cancelling a run cancels its descendants, and `err.card` is
  root-only (a child's failure is its parent step's business).
- **A procedure (SOP)** is versioned and reusable: `definition_of_ready` (inputs/preconditions), 
  `definition_of_done` (outputs/checks), `acceptance_criteria`, `inspection` (the judge and its config) and
  `outputs`. A step's output is **judged before the step is done**.
- **A process** is versioned, tenant-authored or from the default catalogue, bound to a project type (and a card
  type/list or phase), and declares its **user-facing `outputs`** — what a person sees when it completes. System
  capabilities (`project_management`, `kanban_coach`, `kata`) are fixed; every other capability, process, SOP and
  map is tenant data (D29). The 280 `(card_type, list)` processes are the default catalogue. **As built
  (2026-11-10):** 70 type files / 280 processes / 350 SOPs, loaded by `crates/store/src/process_seed.rs`
  (`serde_yaml` + `include_dir`; `apply_all` after `migrate --seed`, `apply_type` for the dev fixture's start
  process), with `seed/processes/manifest.yaml` as the generated 280-entry index.
- **The process map** (`process_map`) is a versioned document `{schema, nodes:[{id, kind, ref, label, position}],
  edges:[{from, to, label?}]}` with `kind ∈ process | capability | phase | milestone`, bound to a project type.
  It is **declarative**: it does not execute and cannot reorder a step; the PM plans from it and the UI renders
  the project's live position. A prescribed sequence of external API calls is a process whose steps are tool
  calls, not a second execution model.
- **The judge (D28).** A step's inspection names `laya`, an S19 eval, or a `checker`. The judge is an external action against the estate's laya service at `https://api.nonagentic.ai/laya`,
  authenticated with the conveyor's laya licence by a typed client — never a plain unauthenticated HTTP call; the request carries the step output, the SOP's acceptance
  criteria and the card's facts. The verdict is `{score, accepted, reason}` (the confidence margin is the signal,
  not the label). **On reject the same step reruns with laya's reason added** (at most 2 rejects), then the card
  goes **BLOCKED → the coach**. **A laya outage waits** — never fails open, never counted as a reject — and the
  wait is **bounded**. The verdict
  is recorded on `run_step.judge` (decision, score/margin, reason, criteria, output hash), so acceptance is
  auditable and replayable. **As built (2026-11-10):** the judge runs inside the step executor as
  `checker | laya | eval`; a reject reruns the same position with the reason (the SOP's `max_rejects`, default 2),
  an exhausted budget fails the run (→ `err.card` → the coach), and an outage **parks** the step and re-enqueues
  it — never fails open, never counts as a reject. Past `JUDGE_MAX_PARKS` (64) parks the step is recorded failed
  with `judge_unavailable` and its ordinary `on_result` applies: `fail` (the default) fails the run, so the card
  goes BLOCKED and the coach is notified; `continue` skips the step. That is a failure **closed**, not an open
  one — no unjudged output is ever accepted.
- **Publish validation:** steps ordered and contiguous; every computed/tool step names a registered action; every
  agent step names a `capability_agent(role='process_agent')` whose `allowed_actions` are registered; every gate
  names a gate code; every call names a published process and depth ≤3; every step's SOP resolves; every map node
  resolves to a published process/capability/phase; a system capability's process cannot be deleted or reordered.
  A draft that fails is `422`, never a run that silently skips a step.
- **The run tree:** `run.parent_run_id`, `run.step_id` (the `process_step` that started it), `run.depth` (0..3).
  One live **root** run per card (`uq_run_live ... WHERE parent_run_id IS NULL`); one live per project when
  card-less; one live **child** per `(parent_run_id, step_id)` and one row per attempt. A live parent may start a
  child on the same card — the coach and `cap.*` calls. Cancel cascades; a child's failure does not re-fire the
  parent's `err.card`.
- **The computed set stays in Rust:** `projects.create` creates the project + board + start card
  (`project_start_request_analysis`, intake) in one transaction and emits `card.created` (the
  `conveyor.start` webhook is retired). It starts a root run **only when the tenant has published a
  start process**: there is no universal tenant seed (a tenant may run accounting, marketing or
  human project management). The move step is a computed action; `wip.pull` reacts to `card.moved` and
  keeps `may_move` as the guard; `err.card` runs on `run.failed` — BLOCKED lane + comment + a coach root run;
  approval routing is the gate resume. The hook route and `CONVEYOR_HOOK_SECRET` are deleted.
- **The bus (§7) is the step transport.** A step attempt is published as `work.requested`; a process agent
  consumes it, calls its tool agents (`tool.call`/`tool.response`), and answers `work.succeeded|failed`. Postgres
  `run`/`run_step` are the record; the outbox re-delivers after a Redis loss. **As built (2026-11-10):**
  `crates/bus` (envelope, groups `process|tool`, 5 deliveries → `bus:dead`, the relay from `event` by `seq` with
  `bus_stream_id`/`published_at`), with the `process.step` job as the Redis-absent fallback.
- **Pins.** S17/S18's subject is the **process** (`process_id`); the old `procedure_id` pin migrates. A procedure
  (SOP) is not a measured subject — the process is.

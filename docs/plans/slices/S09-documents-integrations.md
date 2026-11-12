---
slice: S09
title: Documents, repositories, secrets, workers UI, reports/metrics, billing display
status: planned
depends_on: [S03, S04]
spec_sections: ["§2", "§3 D12/D14", "§5.2", "§5.5", "§8 S9"]
adds_tables: [document, repository, project_repository, secret]
adds_actions: ['documents.create', 'documents.get', 'documents.list', 'documents.download', 'documents.update', 'documents.delete', 'documents.ingest', 'documents.report', 'repositories.create', 'repositories.get', 'repositories.list', 'repositories.delete', 'repositories.link', 'repositories.unlink', 'secrets.create', 'secrets.rotate', 'secrets.get', 'secrets.list', 'secrets.delete', 'workers.list', 'providers.list', 'graphs.list', 'artifacts.list', 'billing.summary', 'coding_jobs.create', 'coding_jobs.get', 'retrieval.query']
adds_events: ['document.created', 'document.updated', 'document.deleted', 'document.ingested', 'repository.created', 'repository.linked', 'secret.created', 'secret.rotated', 'secret.deleted']
adds_modules: [integrations]
parity_ids: PENDING-S0
---

# S09 — Documents, repositories, secrets, workers UI, reports/metrics, billing display

Programme project: S09 · issues none yet

**Gate (SPEC §8 S9):** feature-list items green.

## 1. Objective

S09 ships four tables and a **boundary**: bytes in MinIO behind an authorized stream, write-only secrets, repositories as references, nine adjacent services reached only as declared actions with timeout and breaker — never a second worker manager, billing engine or KAG.

## 2. Starting state

- S03: `project`, `trace_link`, `revision`.
- S04: registry, `/api/actions`, `action_registry`, OpenAPI + MCP.
- S02 (`02-DATA-ARCHITECTURE §2.1`): `tenant`, `event`, `job`, `audit_event`, `app_licence`, licence extractor.

## 3. Scope

**IN** §2.10's four tables, the front-matter's actions, the `integrations` module, read-only workers/billing/observability displays, report queries over local tables, the strata handoff.
**OUT** billing logic and metering writes (**`billing_service`, D12**), worker scheduling (**`worker_management`, D14**), artifact objects (**`artifacts`**), code analysis (**`coding-*`**), licence issuance (**`licenses`**), ingestion/embeddings (**`strata_ingest`/`strata_rag`**), screens (**S12**), `project_request.repository_url` (**S07**).
**UI surface declared for S12**: `admin` — `actions.*`, `jobs.*`, `audit.list`, `licences.get`, `workers.list`, `billing.summary`; read models `/api/actions`, `audit_event`, `app_licence`; no own table.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| SPEC §2, §3 D12/D14, §5.2, §5.5 | the nine-service list | `object_store`; strata ingestion | absorbing any, KAG included |
| 01-CONTRACT §6, §9, §14 | external-action fields | timeout + breaker; tenant from `membership` | — |
| 02-DATA-ARCHITECTURE §2.10, §5, §8 | the four tables' DDL | verbatim; `meta` read-whole rule | any fifth table or column |
| 02-source-map/20-fr `6f7c509792b3ca1fc1fb00c198aebcec83f0af29` `account`, `admin`, `operate`, `settings`, `integrations` | repo registry, secret store, reports/workers reads | that shape | `kag.ts`, licensing/pricing, billing sync, `project-requests` (S07) |
| 02-source-map/30 §4 | artifacts/observability nodes | they are external | the node packages |

## 5. Deliverables

```
migrations/0013_documents.sql · crates/store/src/{document,repository,secret}.rs
crates/api/src/{documents,repositories,secrets,reports,workers,billing}.rs
crates/api/tests/{documents_roundtrip,documents_limits,documents_attachment,documents_ingest,secrets_never_revealed,externals_declared,reports_match_sql,workers_readonly,repositories_crud,coding_jobs,billing_display}.rs
crates/actions/src/integrations/{object_store,external,breaker,ingest}.rs   # module, not a crate (CONTRACT §1)
```

### 5.1 The line — nine externals
| service | external | action gap it leaves | how the conveyor calls it |
|---|---|---|---|
| `artifacts` | yes | artifact objects | external `artifacts.list` from its `openapi.json` (`artifact_service`; routes `/api/artifacts`, `/projects/{project_id}/artifacts`); display only |
| `worker_management` | yes | workers/providers/graphs | external `workers.list`/`providers.list`/`graphs.list` (bare `/workers`, `/providers`, `/graphs`); **read-only** |
| `billing_service` | yes | cost/budget summaries | external `billing.summary` — `GET /api/billing/overview`; display only |
| `inference-router` | yes | model routing, generation | external invoke; no local model inventory |
| `licenses` | yes | issuance, validation | `GET /licenses/active?tenant_id=&app_key=conveyor` → `app_licence` |
| `coding-*` | yes | analysis, edit, dispatch | external action per operation; ref in `run.external_ref` |
| `strata_ingest` | yes **as a service** | chunking, embeddings | **internal** `documents.ingest`; `strata_ingest` out of band |
| `strata_rag` | yes | retrieval | external `retrieval.query` |
| observability | yes | panels, metrics | read-only display links; no ingest |

⛔ **`documents.ingest` is `kind: internal`** — registered once; the registry refuses a name declared both.

A call not in `action_registry` does not exist; `action_registry.service` names the backing service. 5 s read timeout, 30 s write; the breaker is process-local (opens after 5 failures/30 s).

⚠️ **Amended 2026-11-09 (D22).** `retrieval.query` is the **raw strata passthrough**; S15 adds `knowledge.query`
(capability-bound), which is the only retrieval path a capability run uses — keep the two distinct. ⚠️ The action's
`path` is `/query` while `strata_rag` serves **`/api/query`** (measured at the pinned sha `b9426c38`,
`src/api/mod.rs:71`), and the base URL is operator-set (`CONVEYOR_EXTERNAL_STRATA_RAG_URL`); a base without `/api`
makes this a 404 the stub-backed tests cannot see. S15's client posts `/api/query`; align this action or state the
base-URL contract where it is configured.

### 5.2 Documents — bytes backend

**`object_store` → MinIO `minio.example.com:8389`** (§5.2), bucket `conveyor-documents`, key `t/<tenant>/<document>/<version>`. Rejected: **`artifacts`** (a second authorizer) and **local disk** (bytes die with the pod; worker3 carries all 24 PVs).

Credentials are service-level: MinIO authorizes nothing; every byte streams through `/api/documents/{id}/content`, authenticated against `tenant_id` + membership. **No presigned URLs** (a time-bombed authorization decision).

Upload: multipart, **10 MiB** (10,485,760 B) cap; allowlist `pdf, text/plain, text/markdown, text/csv, json, png, jpeg, svg`; magic bytes must match the type, `application/octet-stream` refused. Over cap → `413`, bad type → `415`, problem+json. `content_hash` is SHA-256 while streaming, re-verified on download. Delete sets `status='deleted'` and enqueues `document.purge` to remove the object; the row stays a tombstone. `content_type`/`size_bytes` in `meta` (read whole) — no new column.

**Attachment:** `ArtifactRef {document_id, role}` is a domain value, not a table (the §2 table cap); queryable as a `trace_link` row (`from_type='document'`, `kind='traces'`), `role` in `document.meta.role`. A role-indexed join table is a `CONTRACT-CHANGE REQUEST`.

### 5.3 Repositories — references, not clones

`repository` holds `provider`, `url`, `default_branch`, `installation_ref` (GitHub App installation id, text) and `credential_secret_id → secret`; `project_repository` is the join with `role`. The conveyor clones nothing — coding work is `coding_jobs.create {repository_url, ref, task}` returning a ref in `run.external_ref`; `coding_jobs.get` polls.

### 5.4 Secrets — never a plaintext response

`secret` is per tenant and never returns a value. **Envelope:** 32-byte KEK from `CONVEYOR_SECRET_KEK` (pod env from a Kubernetes Secret, never the database, `tenant.settings` or repo); per-tenant key = HKDF-SHA256(KEK, salt = `tenant_id`); values sealed XChaCha20-Poly1305, `ciphertext = nonce ‖ ct ‖ tag`, `key_id = 'kek:v1'`.

`create`/`rotate` take `value` marked **`x-secret`** (fr's `SECRET_MARKER_KEY`) — write-only in OpenAPI and MCP; `get`/`list` return `{id,name,scope,key_id,version,created_at}`; **no reveal action**. Rotation: `UPDATE … SET ciphertext=$new, key_id=$kek, version=version+1`; `uq(tenant,name)` forbids history — the old ciphertext does not survive. Values resolve server-side at the outbound call; an external action never receives one.

### 5.5 Workers UI, reports, billing display

**Workers:** the page renders `worker_management`'s answer; no writes, no local table. Stopped service: last-known + banner.

**Reports** are `document(kind='report')` rows with `meta.params`/`meta.generated_at`; numbers are live queries, never rollups (`project_metrics` stays dropped).

| number | source |
|---|---|
| cards opened / reached done | `event` (`card.created`, `card.moved` where `to_kind='done'`) |
| WIP | `card` join `list`, `list.kind='active'`, `archived_at is null` |
| run outcomes | `run` by `status`, `error_class` |
| approval latency / pending age | `approval.created_at` → `min(approval_decision.created_at)` |
| delivery success | `delivery` by `state` |

**Billing display** reads `billing.summary` and `app_licence` (`status`, `expires_at`, `checked_at`), as of `checked_at`: no arithmetic, metering writes or budget mutation.

### 5.6 strata

`documents.ingest {id, store_id, source_id?, source_run?, options?}` (**internal**) hands it to `strata_ingest` and stores the returned job id in `document.external_ref`.

⚠️ **Built 2026-11-08 with one deviation from this plan, recorded rather than quietly taken:** the failure path does **not** set `status='failed'`. `fetch_bytes` — the one path `documents.download` and the byte route share — requires `status='active'`, so a transient upstream outage would make a perfectly good document unreadable. A failed push writes nothing to the row; the engine's `record_failure` records it on the audit trail, and the caller gets the `502`/`422`/`503` that names the cause. The field is `id`, not `document_id`, matching every other `documents.*` action.

⚠️ **The contract below was re-read from the upstream's own source on 2026-11-08 (`~/Movies/strata_ingest`), not assumed. The earlier note in this section was wrong in two ways and cost the action a week of being "unbuildable".**

- `POST /api/ingest/jobs`, `multipart/form-data` **or** `application/json`.
- **`Authorization: Bearer <token>` and the token IS the tenant** (`STRATA_INGEST_API_TOKENS` → tenant uuid; every route but `/health` is behind it — measured: an unauthenticated call is `401`). The conveyor therefore stores a per-tenant token as the sealed secret `strata_ingest.token` (S09's own envelope) and opens it per call; a tenant without one is `422 ingest_not_configured` **before a byte is read**. There is no plaintext token env and no shared token.
- **Multipart requires a `store_id` part before the first `file` part**, and the upstream validates it as a **leaf store of the caller's tenant** (`src/api/upload.rs:274-284` → `src/api/handlers.rs:187`). The conveyor has no notion of a strata store, so `store_id` is a **required action input**, passed through and never invented.
- `202 Accepted` body is `{"jobs":[{"job_id":<uuid>,"status":"queued"}]}`. A top-level `job_id` exists **only** for a single JSON `source` body (crawl/URL), which this action does not send — so the multipart answer has no top-level `job_id` and the client reads `jobs[0]`.
- Other answers the action must handle: `401` (bad token), `503 not_ready`/`busy` (retryable), `413`/`415` (the upstream's own limits), `400` (malformed part order).

**KAG is archived (George 2026-11-04)** — no client, no `asset_bindings`/`card_asset_binding`.

### 5.7 Events

Outbox rows, one per mutation, in its transaction (CONTRACT §7).

| type | emitter | payload keys |
|---|---|---|
| `document.created` | `documents.create`, `documents.report` | id, version |
| `document.updated` | `documents.update` (title/tags), `documents.report`, `document.purge` job | id, changed[] |
| `document.deleted` | `documents.delete` | id |
| `document.ingested` | `documents.ingest` | id, external_ref |
| `repository.created` | `repositories.create` | id, url |
| `repository.linked` | `repositories.link` | id, project_id |
| `secret.created` | `secrets.create` | id, name |
| `secret.rotated` | `secrets.rotate` | id, version |
| `secret.deleted` | `secrets.delete` | id |

## 6. Build order

**Part A — BUILT 2026-11-08** (steps 2–8: the four tables, the object store, documents create/get/list/download/
update/delete, the byte route, the secret envelope and `secrets.*`; 11 actions, migration `0013`, 4 scopes).
**Part B — BUILT 2026-11-08** (steps 9–15 except the strata push: `repositories.*` 6, `documents.report`, and the
8 external displays; 15 actions, 2 scopes). **26 of the front-matter's 27 actions are registered.**
⛔ **`documents.ingest` is the one that is not**: it pushes multipart to `strata_ingest` (`POST /api/ingest/jobs`
→ 202 `{job_id}`), which no local environment can exercise, so writing it would be unproven code. It is a
follow-up with a live target, not an omission.

1. MinIO: ephemeral container, random host port; no compose service. Teardown below.

   ```bash
   : "${MINIO_ROOT_USER:?}" "${MINIO_ROOT_PASSWORD:?}"
   docker run --rm -d --name conveyor-minio -p 127.0.0.1::9000 -e MINIO_ROOT_USER -e MINIO_ROOT_PASSWORD minio/minio server /data
   export MINIO_ENDPOINT="http://127.0.0.1:$(docker port conveyor-minio 9000/tcp | sed 's/.*://')"   # tests reuse it; bucket from the test setup; teardown: docker rm -f conveyor-minio
   ```
2. `0013_documents.sql`: four tables, FKs, `FORCE` RLS, indexes; `cargo run -p conveyor-server -- migrate`
   twice (the binary re-applies the runtime grants; `sqlx migrate run` would not — CONTRACT §16).
   ⚠️ Renumbered 2026-11-08: `0010` shipped as `s08_retention`, `0011` is the S00-verdict drop and `0012` the
   retention chain, so S09 takes **0013**.
3. `actions::integrations::object_store` put/get/delete/stat; `cargo test -p conveyor-actions integrations::object_store`.
4. `documents.create` streaming upload: cap, allowlist, magic bytes, hash — `cargo test -p conveyor-api --test documents_limits` (10,485,760 and 10,485,761 B).
5. `documents.{get,list,download,delete}`; Range; `Content-Disposition: attachment` + `nosniff`; purge job — `cargo test -p conveyor-api --test documents_roundtrip`.
6. `ArtifactRef` + `trace_link` attachment; cross-tenant test returns 0 rows — `cargo test -p conveyor-api --test documents_attachment`.
7. Secret envelope: KEK load, HKDF, seal/open, `key_id`; `cargo test -p conveyor-store secret_envelope`.
8. `secrets.*`; `x-secret` through utoipa + MCP; served-spec — `cargo test -p conveyor-api --test secrets_never_revealed`
   then `cargo run -q -p conveyor-server -- --print-openapi --write-lock` (there is **no** `served_spec_lock`
   test target; the lock is the binary's own `--check-lock`).
9. `repositories.*`; link/unlink; assert no git crate — `cargo test -p conveyor-api --test repositories_crud && ! cargo tree -p conveyor-api | grep -qi '^git '`.
10. External ingest for `workers`/`providers`/`graphs`/`artifacts`/`billing`: registry rows, 5 s timeout, breaker; workers screen + write-action absence test — `cargo test -p conveyor-api --test externals_declared --test workers_readonly`.
11. `coding_jobs.create|get` from the fleet spec; `run.external_ref` written in the run transition — `cargo test -p conveyor-api --test coding_jobs`.
12. Report queries + `documents.report`; SQL-vs-report test — `cargo test -p conveyor-api --test reports_match_sql`.
13. `billing.summary` + `app_licence` display; manifest has no billing write — `cargo test -p conveyor-api --test billing_display`.
14. `documents.ingest` → strata; `external_ref`, failure path — `cargo test -p conveyor-api --test documents_ingest`.
15. Gate; `cargo clippy -- -D warnings`; `cargo fmt --check`.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -q -p conveyor-server -- --print-openapi --check-lock
export DATABASE_MIGRATE_URL='postgres://postgres:postgres@127.0.0.1:5439/conveyor_dev'
cargo run -q -p conveyor-server -- migrate      # twice; the second must be a no-op
cargo test -p conveyor-api --test documents_roundtrip --test documents_limits \
        --test documents_ingest --test secrets_never_revealed \
        --test externals_declared --test reports_match_sql \
        --test repositories_crud --test s07_requests
# ⚠️ Corrected 2026-11-09: this block named four test targets that do not exist — `documents_attachment`,
# `workers_readonly`, `coding_jobs`, `billing_display`. `cargo test --test <missing>` is a hard error, so the
# gate could never have passed as written. Their properties are covered, and are named by the tests that
# actually prove them: the attachment/trace-link cross-tenant proof is
# `s07_requests::a_trace_link_is_visible_from_both_ends_and_never_across_tenants`; the read-only worker,
# coding-job and billing displays are `externals_declared` (declared external, `effect = read`, bounded) plus
# `registry`/`parity_http_mcp` for their wire shape.
# The object-store half of property (a) lives in `documents_roundtrip`, not here: integration tests run
# against their own ephemeral database (CONTRACT §10), so a psql lookup in this gate would see no row and an
# `mc stat` on an empty id passes vacuously. The test holds the key and asserts `object_store::head` → NotFound
# after delete; `mc alias set` is only for manual inspection (build step 1), and needs `MINIO_ENDPOINT`,
# `MINIO_ROOT_USER` and `MINIO_ROOT_PASSWORD` exported — the gate itself asserts nothing through it.
```

| # | property · proof |
|---|---|
| a | round-trip — upload 1 MiB, list, download: sha256 equals `content_hash`; delete → GET 404 **and the object key is gone** (`object_store::head` → NotFound, asserted in `documents_roundtrip`; `mc stat` is the manual cross-check) |
| b | cap — 10 MiB refused `413 application/problem+json` with stable `type`; 10 MiB−1 accepted |
| c | secret — rotate to v2; `GET` has no plaintext, `value` key or `ciphertext` |
| d | workers — page renders live rows; service stopped → banner, breaker open after 5 failures |
| e | reports — each number equals its SQL count on the same snapshot |
| f | externals — `GET /api/actions` shows `timeout_ms` on every external; zero `reqwest::` outside the executor |
| g | billing — no write action, no metering event in the manifest |

## 8. Traps

0. ⚠️ **The transport body limit is not the document cap.** axum's default `DefaultBodyLimit` is 2 MiB, so a
   10 MiB document (≈14 MB of base64 JSON) is refused by the router with its own 413 before the action's cap is
   ever consulted — measured while building Part A, fixed with a 16 MiB layer on the app. The rule stays the
   action's: the layer is an envelope, the cap is a check.
0b. ⚠️ **The purge must check the tombstone before deleting bytes.** `document.purge` deletes the object and
   then calls `clear_object_tx`, which refuses a non-deleted row — so without an explicit `status == 'deleted'`
   guard the object is gone and the pointer survives, leaving a live document with no bytes.
   `documents_purge::a_purge_of_a_live_document_removes_nothing` is that guard's test.
0c. ⚠️ **`documents/mod.rs` cannot contain `documents/documents.rs`** — clippy's `module_inception` is denied
   workspace-wide; the action module is `documents/actions.rs`.
1. **Bytes in Postgres.** `bytea` TOASTs the row, multiplies WAL and puts binaries in every dump; the cap settles it.
2. **A "secret" returned once, "for convenience".** One reveal endpoint is the whole leak; there is none.
3. **`x-secret` leaking into OpenAPI examples.** A `default`/`example` on the value field publishes the credential shape into every client; strip examples for marked fields and assert.
4. **Absorption by accretion.** One direct read of `worker_management`'s database and you own its schema and outages.
5. **Object-store credentials in the repo.** Keys from env/Secret only; a committed `.env` survives.
6. **Content-type trust.** A `text/plain` carrying SVG/HTML served inline is stored XSS: sniff, reject mismatch, always `attachment` + `nosniff`.
7. **Delete that drops the row and not the object** — orphaned bytes are invisible.

## 9. Decisions closed here

1. Bytes: `object_store` → MinIO, one bucket, tenant prefix; the conveyor is sole authorizer, no presigned URLs.
2. Cap, allowlist, magic-byte check, attachment disposition (§5.2).
3. `content_type`/`size_bytes` in `meta`; no column on `document`.
4. `ArtifactRef` is a domain value plus a `trace_link` row — no `artifact_ref` table.
5. Secrets: HKDF per tenant from an env KEK, AEAD, `key_id` per generation, rotation bumps `version`.
6. Repositories are references; the conveyor never clones.
7. Every external call is a declared action with timeout and breaker.
8. Reports are live queries plus a document row; `project_metrics` dropped.
9. Workers, billing, observability are read-only displays.
10. `documents.ingest` is internal; `strata_ingest` is a service, never an action.

## 10. Open questions

| # | question | default |
|---|---|---|
| 0 | **PROVEN 2026-11-09 — the S3 wire round-trips.** `scripts/prove-s3.sh` starts RustFS (`rustfs/rustfs`) on
`127.0.0.1:9000`, and `s3_wire::the_s3_wire_round_trips_over_http_with_sigv4` passes against it, printing
`S3_WIRE_PROVEN`: real endpoint, path-style addressing and SigV4, not the in-memory backend. ⚠️ This row said
"UNVERIFIED locally" while `00-INDEX` claimed the opposite; the script settled it. What is still unproven is the
**deployed** MinIO (`minio.example.com:8389`): credentials, bucket policy and the operator's base URL — not the wire.
| Prove the deployed leg (credentials + bucket policy) before S14. |
| 1 | MinIO has **no off-site backup** (gitops#4617): a disk loss keeps rows, loses bytes. | Accept; say so in the UI; `content_hash` checks |
| 2 | Is `artifacts` needed for pre-existing objects? | Reference-only; no migration (D7) — `artifact_service` serves `/openapi.json` |
| 3 | Do S08's PNG/PDF/XMI renders become `document(kind='export')` here? | Yes — same bucket, written by `render`, not `artifacts` |
| 4 | strata_ingest contract: bytes pushed or URL pulled? | **Resolved** — multipart push, `POST /api/ingest/jobs` → 202 `{job_id}` |
| 5 | Billing source: `billing_service` summary or licence data only? | **Resolved** — `GET /api/billing/overview` |

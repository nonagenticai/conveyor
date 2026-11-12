---
slice: S06a
title: mcp_server signed channel — both directions
status: superseded
depends_on: [S04, S05, S06]   # S05 ships `projects.*`, which both properties (b) and the grammar call
spec_sections: ["§4.5", "§8 S6-α", "§11 R11"]
adds_tables: []
adds_actions: []
adds_events: []
adds_modules: []
parity_ids: PENDING-S0
---

> **SUPERSEDED 2026-11-08 by `S06b-agent-direct-interface.md` (SPEC D19).** The agent now calls the conveyor's own
> `/mcp`; there is no HMAC channel, no five-command grammar and no shared secret. This file is kept as the measured
> record of the *old* seam — its field sets, signing algorithm and traps describe `fr`/`mcp_server`, not the
> conveyor. Nothing here is built or to be built.

# S06a — `mcp_server` signed channel

Programme project: S06a · issues none yet

Sources: `nonagenticai_mcp_server` @ `dd2cabf6`, `fr` @ `6f7c5097`. Authority: `02-source-map/30-rm-planka-mcp-n8n.md`
§3; emitter field set: `02-source-map/20-fr.md`.

## 1. Objective

`agent` reaches its backend only through `mcp_server`, and that binding is two HMAC-signed HTTP endpoints. This
slice makes the conveyor speak both byte-compatibly: same path, grammar and five response words inbound; same three
events, fields and 204 contract outbound. **D18 makes it permanent** — the conveyor is one more backend behind
`mcp_server`, so conformance is maintained, never proved once and retired — and the agent handles approvals **and
questions** through the same primitive: a question is an approval. `mcp_server` changes only `MCP_FR_BASE_URL` and
the secret; the conveyor's outbound destination is `MCP_EVENTS_URL`.

## 2. Starting state

- S04: registry, `POST /api/actions/{name}/invocations`, RFC 9457 errors.
- S05: `projects.*`; S06: `approvals.decide`, the channel dispatcher, `approval`, `delivery`, the
  `approval.closed.emit` job kind — every task input is an fr approval, questions included
  (`src/external/adapter.py:55`).
- S02: outbox `event`, `job`, HTTP client.

## 3. Scope

**IN** — both directions verbatim: `POST /api/webhooks/mcp/incoming` (receiver) and
`POST {MCP_EVENTS_URL}/api/channels/fr/events` (sender); HMAC; the five-command grammar (questions ride as
approvals — no second concept); the five response words; the three event types; inbound 200/400/401/413/503, outbound 204/401/413/503; the emitter job; a stub receiver and a suite replaying Python-captured
fixtures.

**OUT** — `agent` (untouched, SPEC §4.5); `mcp_server`'s 48 tools, catalog, runs and 36 tables (it stays); the
conveyor's own MCP surface (S10); every other fr caller (S13).

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `nonagenticai_mcp_server/src/external/signing.py` | `sign`, `verify`, `_digest`, `SKEW_SECONDS` | the algorithm exactly | nothing |
| `nonagenticai_mcp_server/src/external/fr.py` | `INBOUND_PATH`, `TEXT_LIMIT=4000`, `START_PASSTHROUGH`, `_send` | envelope, grammar, client codes | httpx, the tool layer |
| `nonagenticai_mcp_server/src/routers/fr_channel.py` | `EVENT_TYPES`, `MAX_BODY_BYTES=65536`, `_approval`, `_message`, `_record`, `events` | 204 semantics, tombstone, first-report-wins | `mcp_runs`, activity rows |
| `nonagenticai_mcp_server/tests/test_fr_adapter.py`, `test_fr_channel_events.py` | sign/verify roundtrip; 401/413/503/204; replay; tombstone | fixture shapes | pytest as our runner |
| `fr/backend/src/domain/engagement/adapters/mcp.ts`, `channels/mcp-signature.ts`, `channels/mcp.ts` | `approvalEvent`, `signMcpBody`, `MAX_TEXT` | sender's field set | the Node runtime |

## 5. Deliverables

```
crates/api/src/channel/mcp.rs                 sign + verify, both directions, grammar, emit shaping
crates/store/src/channel/mcp.rs              emitter outbox query; delivery state writes
crates/api/tests/mcp_channel.rs              verify/grammar/vocabulary/tenancy + outbound/delivery
crates/api/tests/mcp_channel_stub.rs         the stub mcp receiver (axum, ephemeral port)
crates/api/tests/mcp_channel_conformance.rs  both directions end to end
crates/api/tests/fixtures/mcp/               captured Python requests/responses + *.sig + README
```

No tables, actions, events. Uses `approval.callback_url`/`correlation_id` (S03), `message`/`delivery` (S06), and
**two job kinds whose ownership is split**: S06 ships `approval.closed.emit` (its 30 s closer, S06 §5.3),
**S06a ships `mcp.event.emit`** (`dedupe_key = event.id`) — the slice that owns the sender owns its drainer, so
there is no conditional dependency between the two plans and no migration is needed for either.

### 5.1 Inbound — `mcp_server` → conveyor

| item | exact |
|---|---|
| route | `POST /api/webhooks/mcp/incoming` — no other path |
| auth | `X-Channel-Timestamp` + `X-Channel-Signature: sha256=<hex>`, shared secret `FR_MCP_CHANNEL_SECRET` |
| envelope | `{address, text, correlation_id}` + optional `job`, `on_behalf_of`; compact JSON, `sort_keys=True` at the sender |
| grammar | `projects.create {json}`, `projects.get {"id"}`, `projects.close {"id"}`, `approve <approvalId> [comment]`, `reject <approvalId> [reason]` |
| answers | a decision only: `approve`/`reject <id>` + optional `comment`/`reason` — no other material crosses inward; a declined/cancelled task sends nothing, so the approval stays open (`src/external/fr.py:141-150,328`) |
| maps to | `projects.create` → S05 `ProjectCreateIn`: `title→title`, `project_type→project_type_code`, `repository_url→repo_url`, `description`/`problem_statement`/`desired_outcome` verbatim, `key` derived server-side (slug of `title`); the action's `ProjectOut.id` is returned as `result.project_id`. `approve <id> [comment]` → `approvals.decide {approval_id, decision:'approve', note:<comment?>}`; `reject <id> [reason]` → `{decision:'reject', note:<reason?>}` — the wire says `approved`/`rejected`, the action and `approval_decision.decision`'s CHECK say `approve`/`reject`. Both write `decided_by_kind='agent'` **and** `via = {channel:'mcp', agent_member_id, product}` (the identities resolved above), so D18's provenance is on the row, not only in the SPEC |
| `projects.create` | `title` + `project_type` required; passthrough `description`, `problem_statement`, `desired_outcome`, `repository_url`; `project_type` must match `^[a-z0-9_-]{1,128}$` |
| text cap | `MAX_TEXT = 4000` **characters** (`text.length`, UTF-16 units, not bytes) |
| identity | `agent:<tenant_id>` must equal the run's tenant, else `error`/`actor_tenant_mismatch`; it resolves through `membership(meta.product)` to that tenant's `app_user(kind='agent')`, which is the `approver_id`; the tenant's `channel_binding(channel_kind='mcp')` must exist and be active — the binding is **tenant-level** (`uq (tenant_id, channel_kind, route_key)`), not per agent; the decision is written with `decided_by_kind='agent'`. **`agent:` is the only address scheme**, in two forms — `agent:<tenant>` / `agent:<tenant>:<product>` (`agent_members.address LIKE 'agent:%'`, measured); anything else, or unresolvable, is `unknown-sender` |
| response | HTTP **200** `{status, result.project_id?, approvalId?, code?}`, `status ∈ result \| pending-approval \| no-action \| error \| unknown-sender`, `code` matching `^[a-z_]{1,64}$` |
| transport errors | 401 `invalid_signature`; 413 `payload_too_large` (> 65536 bytes); 400 `bad_request` (body not `{address,text,correlation_id}`, or `text.length > 4000` — measured on fr); nothing else is a 4xx |

The client must be able to map every answer: `unknown-sender` → `actor_unresolved`; `no-action`/`error` →
sanitized `code`; `pending-approval` + string `approvalId` parks an input; `result` + string `result.project_id`
starts. Non-JSON or non-object → `external_result_invalid`; 5xx → `fr_unavailable`.

### 5.2 Outbound — conveyor → `mcp_server`

| item | exact |
|---|---|
| route | `POST {MCP_EVENTS_URL}/api/channels/fr/events` |
| signing | identical to §5.3; the other side's name is `MCP_FR_CHANNEL_SECRET` |
| `approval.requested` | the full measured set (`fr/backend/src/domain/engagement/adapters/mcp.ts:34-67`): `type, address, approval_id, project_id, tenant_id, correlation_id, kind, description, change_summary, impact_analysis, proposed_answer, proposed_answer_note, allowed_decisions, deblock_actions, expires_at, text` — the mcp receiver reads only `approval_id`, `expires_at` and, via `_runs`, `project_id`/`correlation_id`; the rest is carried for parity and removing a field fr sends is a contract change |
| `approval.closed` | `type, address, approval_id, project_id, tenant_id, correlation_id, final_status` (same base, no requested-only fields) |
| `message` | `type`, `address`, `correlation_id`, `kind ∈ approval-request \| action-result \| action-error \| text \| report`, `text`, `meta{approvalId, actionName, result.project_id, outcome}` (`sendMessage` in the same adapter) |
| `correlation_id` | `approval.metadata.mcp_run_id` when set, else the approval's own — never synthetic |
| accepted | **204 with an empty body** for every accepted event, including malformed JSON and unknown `type` |
| errors | 413 `payload_too_large` (> 65536 bytes), 401 `invalid_signature`, 503 `channel_unconfigured` |
| close semantics | idempotent: a replayed close writes nothing; `_record`'s conditional `UPDATE` on `external_ref` means **first report wins**; the per-**run** tombstone drops a late `approval.requested` once that run's approval closed — never per approval id |

Sender duties: one `approval.requested` per approval at creation, one `approval.closed` per closure (including
`rejected`/`expired`/`voided`), one `message` per outcome a run waits on.

### 5.3 Signing, byte-exact

```
canonical = ascii_decimal(unix_seconds) || "." || raw_request_body_bytes
signature = "sha256=" || lowercase_hex(HMAC-SHA256(key = secret_utf8, message = canonical))
```

- Send `X-Channel-Timestamp: <ascii digits>`, `X-Channel-Signature: sha256=<hex>`, `Idempotency-Key: <uuid4>`;
  neither receiver validates the last.
- Verify the **raw bytes as received**; parse JSON only after verification succeeds.
- Header lookup is case-insensitive; values are not trimmed; the timestamp is ASCII digits only (no `+`, `-`,
  spaces, fraction).
- Refuse when `abs(now_seconds - timestamp) > 300`; exactly 300 is accepted (Python uses `>`).
- Compare in constant time; on failure answer the single word `invalid_signature` and log the discriminator
  (`skew` / `mismatch` / `bad_shape`) server-side only. An overflowing timestamp is a refusal, never a panic.

### 5.4 Config — the cutover values

| value | `mcp_server` side | conveyor side |
|---|---|---|
| inbound base URL | `MCP_FR_BASE_URL` (on `mcp-external-driver`; today `http://fr-backend.fr.svc.cluster.local:8888`) | — (the conveyor is the inbound host) |
| outbound events URL | — (the receiver) | `MCP_EVENTS_URL` — today `http://mcp.mcp.svc.cluster.local:8033/api/channels/fr/events`, set on fr's backend; moves to the conveyor unchanged |
| shared secret | `MCP_FR_CHANNEL_SECRET` | `FR_MCP_CHANNEL_SECRET` |

## 6. Build order

0. Re-capture the live contract first — another session is finishing `mcp_server`/`agent` elsewhere, so
   `dd2cabf6`/`6f7c5097` may be stale. Re-read `src/external/signing.py`, `src/external/fr.py`,
   `src/routers/fr_channel.py` and `agent/crates/agent-api/src/mcp_client/tasks.rs`; record the new SHAs in the
   Sources line via `git -C $HOME/Movies/nonagenticai_mcp_server rev-parse HEAD` and
   `git -C $HOME/Movies/agent rev-parse HEAD`. Measurement, not build.
1. Copy the two Python test modules into `crates/api/tests/fixtures/mcp/` as read-only reference.
   `cd $HOME/Movies/nonagenticai_mcp_server && python3 -m pytest tests/test_fr_adapter.py tests/test_fr_channel_events.py -q`
2. `sign`/`verify` + unit tests (roundtrip, 300/301, non-digit ts, missing header, overflow).
   `cargo test -p conveyor-api channel::mcp::sign`
3. Cross-language vectors: a Rust test writes `crates/api/tests/fixtures/mcp/vectors.json` (`id`, `body_hex`,
   `timestamp`, `secret`, `expected`); Python `_digest` must reproduce every hex.
   `cd $HOME/Movies/nonagenticai_mcp_server && python3 -c 'import json,sys;from src.external.signing import _digest;v=json.load(open("$HOME/Movies/conveyor/crates/api/tests/fixtures/mcp/vectors.json"));bad=[x["id"] for x in v if _digest(bytes.fromhex(x["body_hex"]),x["secret"],x["timestamp"])!=x["expected"]];print(bad);sys.exit(1 if bad else 0)'`
4. Golden fixtures: capture real Python-signed requests/responses into `crates/api/tests/fixtures/mcp/`.
   `cargo test -p conveyor-api --test mcp_channel -- golden_python_verifies`
5. Inbound route: capped body read (stream, never trust `Content-Length`), verify, parse, dispatch.
   `cargo test -p conveyor-api --test mcp_channel inbound_path`
6. Grammar parser for five commands → S05/S06 actions; unknown text ⇒ `no-action`;
   `cargo test -p conveyor-api --test mcp_channel grammar`
7. Response shaper: five words, `result.project_id`, `approvalId`, sanitized `code`; tenant check.
   `cargo test -p conveyor-api --test mcp_channel vocabulary`
8. Identity: `agent:<uuid>` → the tenant's `channel_binding(channel_kind='mcp')`; every other scheme ⇒
   `unknown-sender` (§10). `cargo test -p conveyor-api --test mcp_channel tenancy`
9. Outbound sender: sign, POST, require 204 with an empty body; 401/413/503 permanent, 5xx retryable.
   `cargo test -p conveyor-api --test mcp_channel outbound_204`
10. `approval.requested` at creation (creating transaction's outbox); `approval.closed.emit` (S06, 30 s) writes
    the closed event; `mcp.event.emit` drains it, `dedupe_key = event.id`.
    `cargo test -p conveyor-store channel::mcp`
11. `message` for `action-result` / `action-error` / `text` / `report` from the reply path.
    `cargo test -p conveyor-api --test mcp_channel message_kinds`
12. Delivery: `delivery(channel_kind='mcp', recipient_ref=address)` → `delivered` on 204, `failed` + `last_error`
    on 401/413/503. `cargo test -p conveyor-api --test mcp_channel delivery_state`
13. Stub receiver `crates/api/tests/mcp_channel_stub.rs`: 204 empty, 401/413/503, tombstone, first-report-wins,
    queryable write log. `cargo test -p conveyor-api --test mcp_channel_stub`
14. Conformance suite: both directions, five words, three event types, four status codes.
    `cargo test -p conveyor-api --test mcp_channel_conformance`
15. Gate (§7) + `cargo clippy --workspace --all-targets -- -D warnings`.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
cargo test -p conveyor-api --test mcp_channel --test mcp_channel_stub --test mcp_channel_conformance
cargo clippy --workspace --all-targets -- -D warnings
# property c — a Rust-signed vector verifies with the REAL Python verifier, not a re-implementation
(cd $HOME/Movies/nonagenticai_mcp_server && python3 -c 'import json,sys;from src.external.signing import _digest;v=json.load(open("$HOME/Movies/conveyor/crates/api/tests/fixtures/mcp/vectors.json"));bad=[x["id"] for x in v if _digest(bytes.fromhex(x["body_hex"]),x["secret"],x["timestamp"])!=x["expected"]];print(bad);sys.exit(1 if bad else 0)')
# property b — the agent task's evidence must exist (S14 repeats it against the live deployment)
test -s docs/evidence/S6a-agent-task.md
```

| # | property | proof |
|---|---|---|
| a | Python → Rust | a captured Python-signed request verifies in Rust byte-for-byte, not after a JSON round-trip |
| b | `agent` end-to-end (SPEC §8 S6-α) | one real `agent` task via `agent`'s `mcp_client` against a locally run `mcp_server` (`MCP_FR_BASE_URL=http://127.0.0.1:8930`, `MCP_EVENTS_URL=http://127.0.0.1:8931/api/channels/fr/events`): task starts, `projects.create` parks `pending-approval`, approval answered through `mcp_server`, `approval.closed` received, project id on the run's `external_ref`, task `succeeded`. Evidence in `docs/evidence/S6a-agent-task.md` (§7.1): `run_id`, `project_id`, one approval round-trip, one 204 |
| c | Rust → Python | a Rust-signed body verifies with the real `src.external.signing.verify` |
| d | vocabulary | all five `status` words and `result.project_id` / `approvalId` / `code` match the fixtures |
| e | events | all three types' field sets match; `approval.closed` carries `final_status` |
| f | 204 | accepted, malformed JSON and unknown `type` all answer 204 with a zero-length body |
| g | replay | a replayed close writes nothing and the tombstone suppresses a late `approval.requested` |
| h | skew | a timestamp 1 s outside the window → 401 `invalid_signature` |
| i | re-serialisation | the same JSON re-encoded with a different key order → 401; the trap, tested deliberately |
| j | caps | 65536 bytes accepted, 65537 → 413; 4000 characters accepted, 4001 → 400 `bad_request` |
| k | tenancy | `agent:<other-tenant>` ⇒ `error`/`actor_tenant_mismatch`, zero mutations, zero `action_invocation` |

### 7.1 S14 re-run — the live deployment

S6-α proves the task locally (property b) and records it at `docs/evidence/S6a-agent-task.md` — `run_id`,
`project_id`, the approval round-trip and the one 204; the gate asserts that file exists, so the property is
evidence-backed rather than prose. S14 repeats it against the deployed conveyor and the live `mcp_server`
once `MCP_FR_BASE_URL` points at the conveyor: the same evidence, plus one activity row. SPEC §8 makes that re-run
the cutover gate, so the requirement is carried, not dropped.

## 8. Traps

1. **Re-serialisation.** A `serde_json::Value` round-trip reorders keys and changes spacing, so the MAC fails on a
   valid request; take `Bytes`, never `Json<T>`.
2. **Header casing.** `X-Channel-Signature` arrives lowercased from some clients; Python lowercases the whole map.
   A hand-rolled map keyed by the literal name 401s every real request.
3. **Seconds vs milliseconds.** `as_millis()` puts every timestamp ~56 years ahead and refuses everything, looking
   like a secret mismatch.
4. **The timestamp is inside the MAC.** Sign `ts "." body`; on receive recompute with the *received* `ts`. Body-only
   verification lets a captured body replay forever.
5. **`MAX_TEXT` counts characters, not bytes.** Match fr's `text.length` (UTF-16 units), not `.len()`; `.len()`
   rejects valid multi-byte text at a fraction of the real limit.
6. **A syntactically valid body of the wrong shape still gets 204.** `{"type":"approval.requested"}` with no
   `approval_id` is accepted, logged and ignored. This endpoint has four status codes; a 400 on shape breaks the
   client's `raise_for_status`.
7. **The tombstone is per run, not per approval id.** `close(tenant, run, approval)` closes one run's input;
   another live run of the same approval must still receive it.
8. **The secret has two names and one value.** `MCP_FR_CHANNEL_SECRET` and `FR_MCP_CHANNEL_SECRET`; rotate both or
   both directions 401 with no other symptom. Keep it a secret reference, never in `channel_binding.config`.
9. **A 401 must not say which half failed.** One body, one code, one timing path; the expected digest is never
   logged.
10. **204 means no body** — `{}` is a protocol violation the stub must reject.

## 9. Decisions closed here

1. **Verify-then-parse on raw bytes, both directions** — no re-serialisation anywhere.
2. **`approval.requested` at creation, `approval.closed` from the 30 s job** — one event per transition, keyed on
   the outbox `event.id`, so a re-drive cannot duplicate.
3. **Emit fr's full measured field set**, including `tenant_id`, `kind`, `impact_analysis` — dropping a field fr
   sends is exactly what R11 forbids.
4. **The conveyor enforces `agent:<tenant_id>` server-side** even though `mcp_server` pre-checks: a client-side
   check binds only that client.
5. **Channel failures are delivery failures, not command failures** — a 401/503 lands `delivery.state='failed'`
   with `last_error`; the originating action is never retried because a notification failed.
6. **One deployment-wide secret**, matching fr; not per tenant.
7. **The stub implements the receiver contract, not a mock** — 204/401/413/503, tombstone and first-report-wins, so
   a wrong assumption fails here, not at S14.
8. **`MCP_EVENTS_URL` is the conveyor's own outbound var.** It lives today on fr's backend as
   `http://mcp.mcp.svc.cluster.local:8033/api/channels/fr/events` (measured 2026-11-07); it moves to the conveyor
   unchanged. S14 repoints `MCP_FR_BASE_URL` (on `mcp-external-driver`) at the conveyor and rotates one secret.
9. **The agent decides as `decided_by_kind='agent'`, and the channel's answer vocabulary is frozen at decision +
   comment/reason** — it is an external contract (R11), so adding a word is a contract change.

## 10. Open questions

| # | Question | Default |
|---|---|---|
| 1 | Is `voided` sent as `approval.closed` with `final_status='voided'`? | Yes — `START_ENDINGS` has it. |
| 2 | One event per approval or per recipient address? | Per approval: `mcp_server` fans out by `correlation_id` → `project_id` → approval id. |
| 3 | Emitter retry budget — fr does not retry. | 5 attempts, `min(2^n·5 s, 900 s)`, then `failed`; idempotent on the outbox id. |
| 4 | "Augment human escalations": does the agent **respond** (decision + comment — supported today) or **attach analysis to a pending gate without deciding** (needs `mcp_server` + `agent` changes, out of scope for this project)? | **Responder** — the supported path; nothing new to build. The other reading is a proposal to change two external services, not a conveyor slice. |

The two questions that used to sit here are **closed measurements, not open ones**, and now live where they are
used: the `agent:`-only address scheme is in §5.1's identity row (two forms, `agent:<tenant>` and
`agent:<tenant>:<product>`), and fr's **400** on `text.length > 4000` is in §5.1's transport-errors row.

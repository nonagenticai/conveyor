---
slice: S10
title: Copilot / AG-UI — one action surface, proposals, conversations
status: planned
depends_on: [S02, S03, S04, S06]
spec_sections: ["§5.5", "§5.7", "§5.8", "§8 S10", "§11 R3", "§3 D14/D17"]
adds_tables: [conversation, conversation_turn]
adds_actions: [completions.create]
adds_events: [conversation_turn.created]
adds_modules: [copilot]
parity_ids: PENDING-S0
---

# S10 — Copilot / AG-UI

Programme project: S10 · issues none yet

**Gate (SPEC §8 S10):** a scripted agent session passes.

## 1. Objective

An agent operates the app through the S04 registry: the tool list is a **projection** of `ActionSpec`s filtered by the caller's scopes, a destructive tool call becomes an S06 `approval`, and the conversation is two tables plus an SSE stream. fr's 46-file AG-UI surface becomes a query, a proposal shape and a turn runner — not a second action system.

## 2. Starting state

- S04: `ActionSpec`, `Registry`, scoped MCP `tools/list` at `/mcp`, invocation route, `action_invocation`, idempotency.
- S06: `approval` + `approval_decision`, `approvals.decide`, SSE `approval:{id}` with `Last-Event-ID` replay.
- S02/S03: outbox `event`, `audit_event`, tenant RLS; `approval.run_id` is nullable.

## 3. Scope

**IN** — `conversation` + `conversation_turn` (§2.11 verbatim); the turn runner; the agent-surface projection and coverage test; proposals as S06 approvals; the execution job; SSE `conversation:{id}` with replay; `completions.create`; AG-UI adapter only on S0 evidence; screens as data requirements (S12 renders).

**OUT** — `inference-router` (D14); `/mcp` transport and scope filtering (S04); the approval state machine (S06); fr's 34 `agui/actions/` modules, `agent-core`, `HeuristicIntentResolver`; the agent's tool contract and principal (S06b); chat UI rendering (S12, contract §11).

**UI surface declared for S12** (declaration only, no UI code): `copilot conversation` — the turn stream over SSE `conversation:{id}` with `Last-Event-ID` replay, the proposal card (`approval_pending` → `approvals.decide`), and the read models `conversation_turn` + `approval`; routes `POST /api/copilot/conversations`, `POST …/{id}/turns`, `GET …/turns`.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `02-source-map/20-fr.md` §AG-UI (`fr` `6f7c5097`) | `registry.ts`, `register-all-actions.ts` (`ACTION_MODULE_LOADERS`), `agent-surface.ts` (`PROJECT_AGENT_ACTIONS/CORE/SEARCHABLE`, `PROJECT_BINDING`, `SurfaceRefusal`), `rbac.ts`, `coverage-check.ts`, `proposal-shape.ts`, `routes/agui*` | the surface model, the proposal shape, RBAC-as-filter, `coverage-check` as a build gate | the 34 action modules, the second registry, per-module approval metadata |
| `02-source-map/20-fr.md` §`agent-core` | `agentCore`, `setIntentResolver`, `session-store` | session = ordered turns | `HeuristicIntentResolver` |
| `02-source-map/10-agent-patterns.md` §`runtime/catalog.rs` (`agent` `83b8de7e`) | `OpDef`, `PinnedCatalog::from_rows`, `effective_level` | one catalogue; a declaration beats a stored row | `OpSource::Template` |
| `02-source-map/10-agent-patterns.md` §`sse_hub.rs`/`conversations/hub.rs` | `KEEPALIVE` 15 s, `MAX_REPLAY` 200, `EventHub`, `feed` tenant filter | framing and the filter | the Redis bridge, `LocalHub` |
| `02-source-map/10-agent-patterns.md` §`mcp_client` | client-side cache, refusal-rotate | — | not used; `/mcp` is the server side |

AG-UI event names — **measured 2026-11-07** in `fr/backend/src/agui/protocol.types.ts` (`AguiEventSchema`,
`z.discriminatedUnion('type', …)`): eight types, `tool_call`, `tool_result`, `tool_error`, `approval_pending`,
`approval_resolved`, `stream_delta`, `error`, `done` (snake_case, no dots). Step 15 captures golden frames and
pins each payload shape; the *names* are no longer open.

## 5. Deliverables

```
migrations/0011_copilot.sql
crates/domain/src/copilot.rs · crates/store/src/copilot.rs
crates/actions/src/{agent_surface,external/inference}.rs
crates/api/src/copilot/{mod,conversations,turns,proposal,agui}.rs
crates/store/src/jobs/kind/copilot.rs
crates/api/tests/{copilot_session,copilot_surface,copilot_replay,copilot_turn,copilot_proposal,copilot_approve,copilot_reap,agui_parity}.rs   # copilot_replay = replay of a recorded proposal/approval sequence (§7 d)
crates/actions/tests/copilot_coverage.rs
```

### 5.1 Tables — `02-DATA-ARCHITECTURE §2.11` verbatim, two only

`conversation` · `conversation_turn`; tool calls stay in `action_invocation`, referenced from `turn.meta`. One partial unique index is added: `uq_approval_copilot_turn` on `approval (tenant_id, (meta->>'turn_id')) where kind='copilot.action' and status='pending'` — `(tenant_id, run_id, gate)` cannot help.

### 5.2 The surface, not a fork

| fr construct | becomes |
|---|---|
| `PROJECT_AGENT_ACTIONS` | `agent_surface(principal)` = registry actions where `scopes ⊆ principal.scopes` ∧ `kind ∈ {internal, external}` ∧ `tenancy` satisfiable |
| `PROJECT_AGENT_CORE` / `SEARCHABLE` | `AgentHints.tier`, a defaulted `ActionSpec` field declared with the action |
| `PROJECT_BINDING` / `SurfaceRefusal` | the projection **omits**; `/mcp` answers `action-not-found`, never `403` (S04-D6) |
| `rbac.ts` | the S04 `Scope` check — no copilot authorization module |
| `coverage-check.ts` | `crates/actions/tests/copilot_coverage.rs`: every mutating UI operation resolves to an action name or the declared transport exemption. |

`GET /api/copilot/actions` returns that set plus `AgentHints`. The chat transport is a route, not an action — it is the channel, and streaming does not fit an invocation. That exemption is declared and asserted.

### 5.3 Proposals

A tool call whose action has `effect = destructive`, `confirm != none`, or `AgentHints.proposal_required` is not executed: the turn ends `outcome='proposed'` and the same transaction creates an approval through S06's `approval::create_tx` (`kind='copilot.action'`, `run_id` NULL, `requested_by` = caller) — one event owner.

```json
{"proposal_id":"<approval_id>","conversation_id":"…","turn_id":"…","action":"cards.move",
 "effect":"destructive","input":{"id":"…","list_id":"…","_version":7},
 "diff":[{"path":"/list_id","from":"<active>","to":"<done>"}],
 "summary":"Move CARD-12 from Active to Done","expires_at":"…","decide_via":["web","telegram","mcp"]}
```

`input` is the exact action input, schema-validated, pinning the version observed at proposal time (contract §3): a card that moved underneath yields `412`, never a silent overwrite. Execution is `copilot.proposal.execute`: one conditional `UPDATE … WHERE status='approved' AND meta->>'executed_invocation_id' IS NULL RETURNING`, `Idempotency-Key: proposal:<approval_id>`, under the proposer's re-checked scopes.

### 5.4 Conversations

`conversation` is per user (project optional). The assistant turn is inserted at start (`meta.status='streaming'`) and updated to terminal in one transaction that also writes `conversation_turn.created`. Its `meta` is the record:

```json
{"status":"completed|proposed|refused|failed|interrupted",
 "model":{"action":"completions.create","model":"…","request_id":"…"},
 "tool_calls":[{"action":"cards.move","invocation_id":"…","outcome":"ok"}],
 "error":{"type":"/problems/inference-unavailable","status":503,"detail":"…"}}
```

`seq` is allocated under `SELECT … FOR UPDATE` on the conversation row; `uq (conversation_id, seq)` is the backstop. SSE `GET /api/events?topics=conversation:{id}` streams deltas live; only committed turns are durable, so `Last-Event-ID` replays `event.seq > last`, and a mid-turn reconnect receives the turn once. `copilot.turn.reap` (60 s) marks a `streaming` turn older than 300 s `interrupted`.

### 5.5 The MCP channel and model access

`/mcp` (S04) is the machine channel; the copilot UI is the web channel; both call the same action and, for a proposal, the same `approvals.decide`. This is D17: **channels are equivalent** — a decision over Telegram, web or MCP closes the identical approval row and unlocks the identical execution job.

The `/mcp` surface this slice completes (S04's transport, S10's agent-surface projection) serves machine consumers **by principal**. The `agent` app is one of them: it calls this surface with a licence bearer and acts as `app_user(kind='agent')` (SPEC D19, slice S06b). There is one MCP surface; do not build a second agent-facing gateway.

The conveyor does not absorb `inference-router` (D14). `completions.create` is `kind: external` (`scopes: [models:invoke]`, 60 s timeout, per-action circuit breaker, tenant licence token). If it is down, times out, or the breaker is open, the turn ends `failed` with an RFC 9457 problem and **zero** tool invocations: the assistant never emits a fabricated tool call or a heuristic substitute. **S16 adds one declaration to this action** — `captures_usage = true` — and the engine then writes an `inference_call` row with `caller_kind='copilot'` and `conversation_turn_id` in the same transaction as the invocation's bookkeeping (CONTRACT §18). No handler parses `usage`, and nothing in the turn runner changes.

### 5.6 AG-UI wire contract

Kept only if S0's `agui*` parity rows (with ingress counts on `/api/agui*`, `agui-session`, `agui-threads`) show a live consumer: then `agui.rs` serializes the same turn events into fr's wire names, golden fixtures replayed against the native stream; otherwise native SSE only, no adapter, `parity.yaml` carrying the `dropped` rows.

## 6. Build order

1. Preflight: `cargo test --workspace`; `lsof -nP -iTCP:8930 -sTCP:LISTEN` empty; `docker compose up -d db`.
2. `yq '[.[] | select(.surface|test("agui";"i"))]' parity.yaml` — no live row ⇒ native SSE.
3. `0011_copilot.sql`: two tables, RLS, `uq_approval_copilot_turn`; `cargo run -p conveyor-server -- migrate` twice.
4. Domain: conversation transitions, turn seq, proposal validation; `cargo test -p conveyor-domain copilot`.
5. Store: `append_turn_tx`, `turn_seq_tx`, `claim_proposal_execution_tx`, `turns_page`; `cargo test -p conveyor-store copilot`.
6. `AgentHints` + defaulted derive argument + `agent_surface`; `cargo test -p conveyor-actions agent_surface`.
7. `GET /api/copilot/actions`; equality with `/mcp` `tools/list` for a high- and low-scope principal; `cargo test -p conveyor-api --test copilot_surface`.
8. `completions.create` + breaker + tenant token; fixture transcript; `cargo test -p conveyor-actions --lib inference`.
   ⚠️ Corrected 2026-11-09: this line said `cargo test -p conveyor-actions external_inference` — a filter, not a
   target, and it matches **0 tests**. The two inference tests are `--lib` tests.
9. Turn runner: model → validate → invoke or propose → repeat (max 8 rounds); `cargo test -p conveyor-api --test copilot_turn`.
10. Proposal path through `approval::create_tx` + terminal turn, one transaction; `cargo test -p conveyor-api --test copilot_proposal`.
11. `copilot.proposal.execute` + `copilot.proposal.sweep`; `cargo test -p conveyor-api --test copilot_approve`.
12. SSE `conversation:{id}` (keepalive, `MAX_REPLAY` 200, `Last-Event-ID`) + `copilot.turn.reap`; `cargo test -p conveyor-api --test copilot_replay --test copilot_reap`.
13. Coverage test; `cargo test -p conveyor-actions --test copilot_coverage`.
14. AG-UI adapter or its recorded absence; fixtures; `cargo test -p conveyor-api --test agui_parity`.
15. Gate (§7), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo run -p conveyor-server -- --print-openapi --check-lock`.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
cargo test -p conveyor-api --test copilot_session --test copilot_surface --test copilot_replay --test copilot_turn
cargo test -p conveyor-api --test copilot_proposal --test copilot_approve --test copilot_reap --test agui_parity
cargo test -p conveyor-actions --test copilot_coverage
cargo clippy --workspace --all-targets -- -D warnings
```

| # | property | proof |
|---|---|---|
| a | scripted session | open conversation → "move card C to Done" → proposal frame + pending approval, zero `action_invocation` |
| b | approval gates execution | approve as an authorized member → exactly one `action_invocation`, card moved, turn `outcome='completed'` with `tool_calls[0].invocation_id` |
| c | approving twice | second `approvals.decide` is a no-op/`409`; invocation count 1; key replay returns the first body |
| d | replay | disconnect after the user turn, reconnect with `Last-Event-ID`: the turn arrives once, no duplicate `seq` or invocation |
| e | scope-absent tool | low-scope principal's `/api/copilot/actions` **and** `/mcp` `tools/list` both omit it; calling it is `action-not-found`, not `403` |
| f | inference outage | breaker open ⇒ `503` problem+json, turn `failed`, zero `action_invocation`, no `cards.move` |
| g | coverage | every mutating UI operation maps to an action or the declared exemption; an injected unmapped operation fails by name |

## 8. Traps

1. **34 modules copied as code** is the failure this slice prevents. Approval metadata, tier lists or tool descriptions outside `ActionSpec` are a second registry; the surface and coverage tests must fail on them.
2. **A copilot that writes the DB directly.** Every tool call goes through `invoke`; no `sqlx::query` in `crates/api/src/copilot`, and every executed tool has an `action_invocation` row.
3. **Approving twice.** Three layers: `WHERE status='pending'`, the execution claim, the idempotency key. Two workers, one effect.
4. **A turn written outside the transaction that records the action.** Record `invocation_id` only after the action commits; commit the terminal turn and its event together; a crash is `interrupted`, never a turn claiming an uncommitted move.
5. **AG-UI ordering under retry.** A model retry is an attempt inside one turn, not a new turn; event ids are `event.seq`, monotonic per conversation; a reordering client sees a turn before its results.
6. **The proposal's `version` is the only defence against lost updates.** Re-reading state at execution silently overwrites a concurrent `cards.move`.

## 9. Decisions closed here

| id | decision | rationale |
|---|---|---|
| S10-D1 | the agent surface is a registry query; `AgentHints` is a defaulted `ActionSpec` field | one declaration → HTTP, MCP, copilot; no second table |
| S10-D2 | a proposal is an `approval(kind='copilot.action')` created via S06's `approval::create_tx`; one pending per turn | the gate exists; no proposal table |
| S10-D3 | execution is a job with a conditional claim, `Idempotency-Key: proposal:<id>`, scopes re-checked | restart-safe, exactly-once effect |
| S10-D4 | turns are durable, deltas are not; the event commits with the terminal update | replay never fabricates a partial turn |
| S10-D5 | `completions.create` is external (D14), fail closed, no heuristic resolver | an unavailable router must not invent a tool call |
| S10-D6 | chat transport routes are the declared coverage exemption | the channel is not an operation |
| S10-D7 | AG-UI is a serializer over the native stream, kept only on S0 evidence | R3 |

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Does any live consumer use AG-UI today? | No evidence ⇒ native SSE; adapter behind a flag until S14. |
| 2 | Who chooses the model? | `inference-router` by tenant policy; the turn records the model. |
| 3 | How much history enters the prompt? | Last 20 turns, 32k-token cap. |
| 4 | Is Telegram a copilot channel? | No — proposals are decidable there (S06), conversation is web + MCP. |

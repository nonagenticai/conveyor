---
slice: S06
title: Control plane — claims, runs, approvals, andon, comms and notifications
status: planned
depends_on: [S03, S04, S05]
spec_sections: ["§4.5", "§5.6", "§5.8", "§6", "§7", "§8 S6"]
adds_tables: [channel_binding, channel_preference, message, delivery]
adds_actions: [cards.claim, cards.release, cards.renew, runs.create, runs.start, runs.finish, runs.cancel, runs.list, approvals.decide, approvals.list, andon.engage, andon.release, messages.send, notifications.list, notifications.update, channels.bind, channels.unbind]
adds_events: [card.claimed, card.released, card.lease_expired, run.created, run.started, run.succeeded, run.failed, run.cancelled, approval.requested, approval.approved, approval.rejected, approval.expired, approval.voided, andon.engaged, andon.released, message.sent, delivery.changed]
adds_modules: [channel]
parity_ids: PENDING-S0
---

⚠️ **Amended 2026-11-09 (process model).** The computed system set is code, not workflows: `projects.create` creates the project + board + start card + root run in one transaction (the `conveyor.start` hook and `CONVEYOR_HOOK_SECRET` retire); the move step is the existing `cards.move`; `wip.pull` runs on board events; `err.card` runs on `run.failed`; approval routing is the gate resume. The `workflow.start` kind and the `schedules.upsert` payload retire. The run tree and the bus are S13's; `runs.create`/`runs.cancel` stay.

# S06 — Control plane

Programme project: S06 · issues #14

<!-- 2026-11-08: `audience` and `audience_member` were built by 0005 and DROPPED by migration 0011 — S00 day 2
measured zero usage (no `streams` node operation in any of the 348 flows, fr.streams 0 rows, no `/api/streams`
route). `messages.send` therefore takes an explicit recipient list only; its `audience_id` field is gone. -->

**Gate (SPEC §8 S6):** golden card-type×list path; dual-control approval; retry.

## 1. Objective

Claims, runs, approvals, andon and notifications become one durable control plane: one conditional `UPDATE` per claim, one `job` row per period, one `message` plus N `delivery` rows per notification.

## 2. Starting state

- S03: `card` (claim columns, `claim_fence`), `run`, `approval`, `approval_decision`, `andon_state`.
- S04: registry + invocation route; S05: `comment`, `cards.move`, `board:{id}` SSE.
- S02: `job`, `schedule`, `event`, `audit_event`, SSE replay.

## 3. Scope

**IN** — the 6 tables of `02-DATA-ARCHITECTURE §2.6`; 17 actions; job kinds for 11 of fr's 14 timers (§5.3); SSE `run:{id}`, `approval:{id}`, `tenant`; Telegram in/out; andon and its gauge.

**OUT** — the signed mcp_server channel (`S06a`: HMAC, payloads, ±300 s); MCP tool surfacing (`S10`); `project_request.timeout` (`S07`); UI (`S12`: screens only, §5.6); `audience` screens (PENDING-S0).

**UI surface declared for S12:** `run queue` — `runs.list` (status, card, attempt, age, assignee) over `run`, on the queued partial index.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `SPEC.md` §4.5; `02-source-map/20-fr.md` (fr `6f7c5097`) Workers, `approvals`/`conveyor`/`admin`/`schedules` | both signed directions; the 14 `runOnce` handlers; `andon_state`; `wip_slot`; `engagement_deliveries` | handlers as job kinds; seam shape | the timers; the HMAC (S06a) |
| `02-source-map/10-agent-patterns.md` (`agent` `83b8de7e`) `lease.rs`, `provision/outbox.rs` | conditional `UPDATE … RETURNING 1`; `lease_until`+`lease_token`; enqueue in-tx, send outside | the fence pair | per-process `owner()` |
| `02-source-map/30-rm-planka-mcp-n8n.md` §1 `pm_projects` | `_is_fr_close` | — | the fr-only close case |

## 5. Deliverables

```
migrations/0005_control_plane.sql
crates/domain/src/{claim,run,approval,andon,message}.rs
crates/store/src/{claim,run,approval,andong,channel,delivery}.rs
crates/actions/src/control/{cards,runs,approvals,andong,messages,channels}.rs
crates/actions/src/channel/{mod,telegram,web,reply}.rs
crates/api/src/{runs,approvals,events,telegram}.rs   apps/server/src/worker.rs
crates/store/src/jobs/{runner,schedule,kind}.rs
crates/api/tests/{golden_path,claim_race,run_retry,approval_dual,approval_expire,andon,delivery_retry,telegram_reply,notifications,sse_control}.rs
```

### 5.1 Tables — §2.6 verbatim, six only

`channel_binding` · `channel_preference` · `message` · `delivery` · `audience` · `audience_member` (PENDING-S0).

### 5.2 Actions

| action | input → output |
|---|---|
| `cards.claim` `.release` `.renew` | claim `{card_id,ttl_s?}` → `{claim_fence,version,lease_expires_at}`; renew `{card_id,claim_fence}` → the same; release `{card_id,claim_fence?}` → the same. The expected `version` is the request's `If-Match` (CONTRACT §3), bound as `$5` — never a body field |
| `runs.create` `.start` `.finish` `.cancel` `.list` | `{card_id,correlation_id?}` → `RunOut` (added later: `procedure_id` by S11, `workflow_id` by S13); `IdIn`; `{id,outcome,error_class?}`; `{id,reason?}`; `{status?,card_id?} + PageIn` → `Page<RunOut>` |
| `approvals.decide` `.list` | `{approval_id,decision,note?,item_decisions?}` → `ApprovalOut`; `{status?} + PageIn` → `Page<ApprovalOut>` |
| `andon.engage` `.release` | `{reason}` → `AndonOut`; `{}` → `AndonOut` — per-tenant singleton, no id (`andon_state` keys on `tenant_id`) |
| `messages.send` | `{recipients[],channel_kind,body,kind?,meta?,audience_id?}` → `{message_id,delivery_ids[]}` |
| `notifications.list` `.update` | `{unread?,cursor?}` → caller's deliveries; `{channel_kind,enabled?,quiet_hours?}` → pref |
| `channels.bind` `.unbind` | `{tenant_id,channel_kind,route_key,config?}` → `ChannelBindingOut`; `{tenant_id,channel_kind,route_key}` → 204 |

### 5.3 Job kinds — every timer replaced

| fr timer → job kind (cadence) | dedupe key |
|---|---|
| `scheduled-trigger-tick` → `schedule.materialise` 60 s | `schedule:<cron-minute>` |
| `mcp-approval-closed` → `approval.closed.emit` 30 s | `<approval_id>:<final_status>` |
| `approval-expiry` → `approval.expire` 60 s | `approval:<id>` |
| `approval-redelivery` → `approval.redeliver` 900 s | `approval:<id>` |
| `deblock-callback-redelivery` → `deblock.redeliver` 900 s | `run:<id>` |
| `ac-coverage-redelivery` → `coverage.redeliver` 300 s | `card:<id>` |
| `project-delivered-reconcile` → `project.reconcile` 300 s | `project:<id>` |
| `project-request-analysis-timeout` → `project_request.timeout` **S07** | `project_request:<id>` |
| `scheduled-trigger-health` → `schedule.health` 60 s | `schedule` |
| `idempotency-sweep` → `idempotency.sweep` 3600 s | `hour:<yyyy-mm-ddThh>` |
| `scheduled-trigger-run-prune` → `job.prune` 86 400 s | `day:<date>` |
| `analytics-rollup`, `signup-onboarding-tick`, `workflows-cache` → **deleted** | — (rollups derive from `event`) |

Also `card.claim`, `card.lease_sweep` (60 s), `delivery.send`, `run.execute`, `run.reap` (60 s). Handlers are idempotent on `(kind, dedupe_key)`; periods are `schedule` rows under `pg_advisory_xact_lock(hashtext('conveyor:schedule'))`.

**The claim** — one statement, no fallback arm (`02-DATA-ARCHITECTURE §4`, verbatim):

```sql
UPDATE card
   SET assignee_id = $3, lease_expires_at = now() + $4::interval,
       lease_token = uuidv7(), claim_fence = claim_fence + 1,
       version = version + 1, updated_at = now()
 WHERE tenant_id = $1 AND id = $2 AND version = $5
   AND (lease_expires_at IS NULL OR lease_expires_at < now() OR assignee_id = $3)
RETURNING claim_fence, version;
```

`$5` is the caller's `If-Match: "<version>"` (CONTRACT §3) — the body carries no version, and a mismatch is the 412 path, never a grant. Zero rows = not granted; a DB error is a refusal, never `granted: true`. **ABA:** `claim_fence` is monotonic, so a holder past its TTL compares the fence before every write; a changed fence means the card moved — fr's `card_phase_lease` window, closed by a column. **Waiter:** a blocked claimant enqueues `job(kind='card.claim', dedupe_key=<card_id>)`; the partial unique index on `(tenant_id, kind, dedupe_key) WHERE status IN ('pending','running')` collapses N into one. **Holder death:** the lease expires; `card.lease_sweep` clears assignee and token, emits `card.lease_expired`, and the waiter's predicate already treats an expired lease as free.

### 5.4 Approvals

At most one pending per `(run, gate)`: the partial unique index, not a handler. Dual control is `required_decisions` (fr's `required_approvers` folds in); `uq (approval_id, approver_id)` stops one person twice. `approvals.decide` is `pending → approved` iff `count(approve) >= required_decisions`, `pending → rejected` on any reject, one transaction. It records `decided_by_kind` (`human` from web/Telegram, `agent` from `/mcp` — D19) and `via` (`channel`, `agent_member_id`, `product`). **First-response-counts** closes the *delivery* set (other recipients `expired`), not the gate: with `required_decisions > 1` it stays pending. `approval.expire` runs at 60 s. A close enqueues `delivery(channel_kind='mcp')`.

### 5.5 Andon

`andon.engage` writes `andon_state` and emits `andon.engaged`; `runs.create` refuses `409 andon-engaged`; the poller skips engaged tenants; `release` reverses both. **Safety signal:** S06 exposes `conveyor_andon_engaged{tenant_id}` and `conveyor_andon_engaged_at_seconds` from the **worker's** scrape path, read from `andon_state` with a 1 s timeout. S14 alarms on `conveyor_andon_engaged == 1` **and** `absent()`: a dead exporter is an alarm, not a silence; never a request-driven cache (fr's gauge had no sample after a restore).

### 5.6 Comms

`messages.send` writes one `message` and N `delivery` rows, each enqueuing `delivery.send` on `dedupe_key = delivery.id`. The drainer claims with `FOR UPDATE SKIP LOCKED` + `lease_token`, sends **outside** the transaction, writes `pending → sent → delivered | failed | expired`; `attempts >= max_attempts` lands `failed`, backoff `min(2^attempts · 5 s, 900 s)`. **Reply path:** Telegram inbound (`POST /api/channels/telegram/{binding_id}`, secret-token header) resolves `(chat_id, topic_id) → channel_binding → tenant → sender` and calls the *same* `approvals.decide`/`cards.comment` the web does — no per-channel path. Web is `delivered` at SSE ack; `mcp` hands off to S06a.

**SSE.** `run:{id}` and `approval:{id}` are aggregate topics; `tenant` is the tenant-wide topic (inbox, andon banner). Every mutation writes its `event` in-tx and `pg_notify`s it.

**17 events:**

| type | emitter | payload keys |
|---|---|---|
| `card.claimed` | `cards.claim` | card_id, claim_fence |
| `card.released` | `cards.release`, `card.lease_sweep` | card_id, claim_fence |
| `card.lease_expired` | `card.lease_sweep` | card_id, claim_fence |
| `run.created` | `runs.create` | run_id, card_id, attempt |
| `run.started` | `runs.start` | run_id, attempt |
| `run.succeeded` | `runs.finish` | run_id, outcome |
| `run.failed` | `runs.finish`, `run.reap` | run_id, error_class, attempt |
| `run.cancelled` | `runs.cancel` | run_id, reason |
| `approval.requested` | run → `waiting_approval` | approval_id, run_id, gate |
| `approval.approved`, `approval.rejected` | `approvals.decide` | approval_id, run_id, actor |
| `approval.expired` | `approval.expire` | approval_id, run_id |
| `approval.voided` | `runs.cancel` | approval_id, run_id |
| `andon.engaged` | `andon.engage` | reason, actor_id |
| `andon.released` | `andon.release` | actor_id |
| `message.sent` | `messages.send` | message_id, channel_kind |
| `delivery.changed` | `delivery.send` | delivery_id, message_id, state |

## 6. Build order

1. Preflight: `cargo test --workspace`; port 8930 free; `docker compose up -d db`.
2. `0005_control_plane.sql`: 6 tables, FKs, §2.6 indexes, forced RLS. `cargo run -p conveyor-server -- migrate` twice
3. Domain: `run::transition`, `approval::decide`, `andon::apply`, `claim::may_claim`. `cargo test -p conveyor-domain`
4. `store/src/claim.rs`: `claim_card_tx` (above), `release` (fence optional), `renew` fenced by `claim_fence`. `cargo test -p conveyor-store claim`
5. Job runner — claim + fenced `finish`. `cargo test -p conveyor-store jobs::runner`
6. Schedule materialiser — one advisory lock. `cargo test -p conveyor-store jobs::schedule`
7. `worker` subcommand draining any kind. `cargo run -p conveyor-server -- worker & sleep 2; kill %1`
8. `card.claim` waiter + `card.lease_sweep` + the lease events. `cargo test -p conveyor-api --test claim_race`
9. Runs: 5 actions, `run.execute`, `run.reap`; one-live-run invariant. `cargo test -p conveyor-api --test run_retry`
10. Approvals: 2 actions, dual control, `approval.expire`. `cargo test -p conveyor-api --test approval_dual`
11. Human vs agent: one approval decided twice, `decided_by_kind` `human` then `agent`. `cargo test -p conveyor-api --test approval_dual -- decided_by_kind`
12. Approval events + `approval.closed.emit` (payload only). `cargo test -p conveyor-api --test approval_expire`
13. Andon: 2 actions, poller pause, gauge + timeout. `cargo test -p conveyor-api --test andon`
14. `messages.send`, fan-out, `delivery.send` + backoff. `cargo test -p conveyor-api --test delivery_retry`
15. `channel`: preferences, web delivery, Telegram webhook + reply→action. `cargo test -p conveyor-api --test telegram_reply`
16. `notifications.list|update`. `cargo test -p conveyor-api --test notifications`
17. SSE: 3 topics, replay bound, keepalive. `cargo test -p conveyor-api --test sse_control`
18. `channels.bind`/`channels.unbind` (platform-admin, audited). `cargo run -p conveyor-server -- --print-openapi | jq -e '.paths["/api/actions/channels.bind/invocations"]'`
19. Gate (§7), clippy, served-spec lock. `rg -n 'setInterval' crates apps` empty

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
cargo test -p conveyor-api --test golden_path --test claim_race --test run_retry \
  --test approval_dual --test approval_expire --test andon --test delivery_retry \
  --test telegram_reply --test notifications --test sse_control
test -z "$(rg -n 'setInterval' crates apps || true)"            # empty
```

| # | property | proof |
|---|---|---|
| a | golden path (`golden_path.rs`) | card → claim → run → gate → decide → `cards.move(done)`: run `succeeded`, approval `approved`, one `audit_event`+`action_invocation`+`event` each |
| b | dual control | `required_decisions=2`: one `approve` leaves `pending`; the same approver again `409`; a second closes |
| c | retry | `kill -9` mid-run: `run.reap` → `failed`/`crashed`, next `runs.create` is `attempt=2`; two rows, never two live |
| d | andon | engaged: `409`, a queued `run.execute` stays pending 60 s; released: runs within one poll |
| e | delivery retry | stub returns 500 → `attempts=5`, `state='failed'`, one row |
| f | no timers | `rg` empty; every cadence is a `schedule` row |
| g | human vs agent | `required_decisions=2`: one decision from a human, one from a seeded `app_user(kind='agent')` — both persist with different `decided_by_kind`, driven through `approvals.decide` (S06a depends on S06, not the reverse) |

## 8. Traps

1. **Approval expiry must not deliver after close.** Decision and expiry race on `WHERE status='pending'`; the loser emits nothing — re-read status in-tx.
2. **A delivery row per recipient, not per message** — else "delivered once" is a convention.
3. **Andon must not depend on the process holding the cord.** State is a row; the gauge scrapes.
4. **Handlers idempotent on `(kind, dedupe_key)`.** The index covers `pending`/`running`, so a `dead` job never blocks re-enqueue.
5. **Claim `version` vs a legitimate renew.** The claim's version is the caller's `If-Match` (CONTRACT §3), bound to `$5`, never a body field. `cards.renew` takes `{card_id,claim_fence}` — the fence from the claim response — and re-checks it, never `version`: an unrelated `cards.update` bumps the version. `cards.release` takes `{card_id,claim_fence?}` and checks the fence when one is supplied.
6. **`delivery` retention never prunes `pending`/`sent`** — terminal only, 30 days.

## 9. Decisions closed here

1. **Claim stays on the card row**; `claim_fence` is the ABA witness.
2. **A waiter is a deduped `card.claim` job**, not a table.
3. **Run liveness is the driving job's `lease_until`** — no `run.heartbeat_at`; reaped, retried at `attempt+1`.
4. **`approval.expire` at 60 s**, not 900 s.
5. **First-response-counts closes deliveries, not the gate** (§5.4).
6. **Andon pauses the poller and refuses `runs.create`**; never scales replicas.
7. **Telegram is the first channel**; web and mcp share the rows.
8. **The callback URL is on `approval`; the signed transport is S06a.**
9. **Three timers deleted** (`analytics-rollup`, `signup-onboarding-tick`, `workflows-cache`).

## 10. Open questions

| # | Question | Default |
|---|---|---|
| 1 | Does andon pause `delivery.send` too? | No — except `delivery.send` and `approval.closed.emit`. |
| 2 | `message.channel_kind` is single-valued while a message fans out. | Originating channel; fan-out lives on `delivery`. |
| 3 | `audience`/`audience_member` are PENDING-S0. | Ship tables; no UI until S0. |
| 4 | `runs.cancel` on a `waiting_approval` run. | Voids it, same transaction. |
| 5 | `schedule.health` cadence. | **60 s** — fr's measured tick (`workers/scheduled-trigger-health.ts`, 2026-11-07), not 300 s. |

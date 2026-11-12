---
slice: S16
title: Inference ledger and prompt audit
status: built 2026-11-09
depends_on: [S02, S03, S04, S10, S11]
spec_sections: ["§3 D23", "§5.7", "§6", "§8 S16", "§9"]
adds_tables: [inference_call, prompt_audit, prompt_audit_finding]
adds_actions:
  [
    inference_calls.list,
    inference_usage.report,
    prompt_audits.start,
    prompt_audits.check,
    prompt_audits.get,
    prompt_audits.list,
    prompt_findings.list,
    prompt_findings.decide,
  ]
adds_events:
  [inference_call.recorded, prompt_audit.started, prompt_audit.completed, prompt_audit_finding.decided]
adds_modules: [inference, prompt_audit]
parity_ids: PENDING-S0   # net-new: this slice ports no old surface, so S0 has no id for it
---

# S16 — Inference ledger and prompt audit

Programme project: S16 · issues #20

## 1. Objective

Every model call the conveyor makes becomes a durable, tenant-scoped row carrying the prompt's identity, the token
counts and the provider's prompt-cache classes, so "what did this cost and was it cached" is a query rather than a
guess. On top of that record, a **prompt audit** inventories the three prompt surfaces the conveyor owns —
procedure step templates, every action's description, and the copilot's system prompt — classifies them against the
`claude-api` `prompt-audit` method's four anti-pattern groups, and reports findings with confidence and a proposed
diff. Both are ordinary actions, so both are user-invokable over HTTP and machine-invokable over `/mcp`.

## 2. Starting state

- The action engine already owns the external call and writes `action_invocation` + `audit_event` in one
  transaction (`crates/actions/src/invoke.rs`); the two model-call actions are `completions.create` (**built**,
  S10) and `inference.chat` (**declared by S13, not built** — it declares `captures_usage` when it lands).
- `inference-router` already returns `usage` with both cache classes and already accepts `X-Correlation-Id`; the
  conveyor drops both on the floor.
- S11's `procedure`/`procedure_step`/`run_step` and S10's `conversation_turn` exist and are the join targets.

## 3. Scope

**IN.** The three tables (§2.17); the engine capture seam; the **eight** actions; the two scopes and four events;
the `prompt_audit.run` and `prompt_audit.sweep` jobs with the built-in prompt; the **two triggers** — a manual
`prompt_audits.start`, and a configurable *after N executions* policy the sweep enforces against `inference_call`
itself; the **draft check that becomes part of publishing** (`prompt_audits.check`, called by S11's publish); the
two screens **declared** for S12.

**OUT.** Building any UI — the screens belong to S12 and are listed here so S12 may not invent them. Absorbing
`inference-router` or `billing_service`: the conveyor reads neither database, adds no cache endpoint and emits no
cost event (SPEC §4 still says *never absorbed*). A tenant-authored prompt store — `prompt_ref` stays the built-in
versioned reference of CONTRACT §17. Any AI-driven *fixing* of prompts: the audit proposes, a person decides, and
`procedures.update` applies. Kata-specific tables: kata's prompts are procedures and the kata domain stays outside
this spec — kata **consumes** findings through `prompt_findings.list`, which is already on its allowlist. A
configuration table or action for the cadence: the policy rides `tenant.settings.prompt_audit` through the existing
`tenants.update_settings`, so the trigger is configurable without widening this slice.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `…/claude-api/shared/prompt-audit.md` (the installed skill) | Steps 0–7, the four groups, the keep list, the Step 5 report columns | the **method and the column set verbatim** | its file-editing affordances — the conveyor's surfaces are rows, not files |
| `…/claude-api/shared/prompt-caching.md` | §Silent invalidators, §Automatic vs explicit breakpoints | the invalidation signals Group 4 measures | the API-level advice; this slice calls the router, not a provider |
| `inference-router/src/models/schemas.py` | the `Usage` model (its two `cache_*_input_tokens` fields) and `ChatCompletionResponse.usage` | the exact field names | any assumption of a response cache — there is none |
| `crates/actions/src/invoke.rs` | `invoke_external`, `record_bookkeeping` | the transaction shape the capture joins | its breaker/idempotency logic, unchanged |
| `crates/actions/src/spec.rs` | `ActionSpec`, `AGENT_TOOLS`, `Scope` | the declaration pattern | — |
| `crates/api/src/copilot/turns.rs` | `SYSTEM_PROMPT` (`:51`) | the text as an audit surface | the turn runner |

## 5. Deliverables

`migrations/00NN_s16_inference_audit.sql` (next free number at start; `0024` at time of writing), DDL exactly as
`02-DATA-ARCHITECTURE §2.17`. Modules: `crates/domain/src/{inference,prompt_audit}.rs`,
`crates/store/src/{inference,prompt_audit}.rs`, `crates/actions/src/inference/{calls,usage}.rs`,
`crates/actions/src/prompt_audit/{audits,findings,scan}.rs`, `crates/actions/src/prompts/prompt_audit_v1.md`
(`include_str!`, the built-in `prompt_audit@1`). Plus `crates/actions/src/spec.rs` (`captures_usage` field, two new
scopes — **`inference:read` and `inference:write`**, `Scope::ALL` 52 → 54 — and six `AGENT_TOOLS` entries — all but `prompt_findings.decide`, a human's, and `prompt_audits.check`, publish's),
`crates/actions-derive/src/lib.rs` (the attribute + scope table),
`crates/actions/src/invoke.rs` (the capture), `apps/server/src/worker.rs` (the `prompt_audit.run` arm).

| action | input | output | scope |
|---|---|---|---|
| `inference_calls.list` | `{caller_kind?, run_id?, model?, cache_status?, since?, page}` | `Page<InferenceCallView>` | `inference:read` |
| `inference_usage.report` | `{since, until, group_by: day\|model\|caller_kind\|cache_status\|prompt_sha}` | `{rows:[{bucket, calls, prompt_tokens, completion_tokens, hits, writes, misses, unknown, hit_ratio}]}` | `inference:read` |
| `prompt_audits.start` | `{scope:{surfaces:[procedures\|actions\|copilot], procedure_ids?:[uuid]}, target_model}` | `PromptAuditView` | `inference:write` |
| `prompt_audits.check` | `{procedure_version_id}` | `{findings:[FindingView], blocking:bool}` | `inference:write` |
| `prompt_audits.get` | `{id}` | `{audit, findings:[FindingView]}` | `inference:read` |
| `prompt_audits.list` | `{page}` | `Page<PromptAuditView>` | `inference:read` |
| `prompt_findings.list` | `{audit_id?, status?, confidence?, page}` | `Page<FindingView>` | `inference:read` |
| `prompt_findings.decide` | `{finding_id, decision: accept\|reject, note?}` | `FindingView` | `inference:write` |

Screens declared for S12: **#11 Inference usage** (`inference_usage.report`, `inference_calls.list`;
`inference-usage.spec.ts`) and **#12 Prompt audit** (`prompt_audits.*`, `prompt_findings.*`, and the cadence in `tenants.update_settings`;
`prompt-audit.spec.ts`) — the screen that shows a kata what to fix, and the one place a person sees a finding.
Jobs: `prompt_audit.run`, `inference_call.prune`,
`prompt_audit.prune`. **There is no `prompt_audit.sweep` job:** the *after N executions* trigger is an event at
capture time (owner 2026-11-09, answering §10 Q2), so the audit is started by the call that crosses the threshold.

## 6. Build order

1. Migration + `crates/store/src/migrate.rs` touch (the macro embeds at compile time). apply migration 0024 to the dev database (a m1-built binary against the compose DB; CONTRACT §16).
2. `domain::inference`: `Usage` parsing, `cache_status()` with its four-way rule, and the bucket arithmetic. `rbuild -- test -p conveyor-domain inference`.
3. `store::inference`: the insert (called from the engine), the keyset list, the aggregate. `rbuild -- test -p conveyor-store inference_`.
4. `spec.rs` + derive: `captures_usage`; declare it on `completions.create`. `rbuild -- test -p conveyor-actions registry`.
5. `invoke.rs`: parse `usage` from a successful response and write the row in `record_bookkeeping`'s transaction,
   sending `X-Correlation-Id`. `rbuild -- test -p conveyor-actions inference_capture`.
6. `inference_calls.list` + `inference_usage.report` over `store::inference`. `rbuild -- test -p conveyor-actions inference`.
7. Scopes + `AGENT_TOOLS` (six: the read/start actions, not `prompt_findings.decide` and not `prompt_audits.check`) + the count assertion + `served-spec.lock.json`. `--print-openapi --write-lock`.
8. `prompt_audit` tables' store + the state machine in `domain::prompt_audit`. `rbuild -- test -p conveyor-store prompt_audit`.
9. `scan.rs`: inventory (procedure steps, `actions.list` descriptions, `SYSTEM_PROMPT`) + the group signals + the
   cache-order probe (a prompt whose stable prefix reports `creation > 0` with `read = 0` over N calls).
10. The built-in prompt `prompt_audit@1` and the `prompt_audit.run` worker arm: one bounded `completions.create`,
    findings persisted, `prompt_audit.completed`. `rbuild -- test -p conveyor-server prompt_audit_run`.
11. `prompt_audits.*` + `prompt_findings.*`. `rbuild -- test -p conveyor-actions prompt_audits`.
12. **The two triggers.** The *after N executions* policy is an **event at capture time** (owner 2026-11-09):
    after the engine inserts an `inference_call`, it counts the tenant's rows since the last `prompt_audit.created_at`
    (excluding `prompt_ref = 'prompt_audit@1'`) and, at the threshold, inserts the queued audit
    (`requested_by IS NULL`, `ON CONFLICT DO NOTHING` against `prompt_audit_auto_live_idx`) and its
    `prompt_audit.run` job in the same transaction. `tenant.settings.prompt_audit` carries
    `{enabled, after_executions, surfaces, target_model}`; an enabled policy without a `target_model` is not armed.
    Then `prompt_audits.check`: deterministic signal scan of ONE draft version, no LLM, synchronous.
    `rbuild -- test -p conveyor-server prompt_audit_triggers`.
13. **The publish seam.** S11's `procedures.publish` calls `prompt_audits.check` on the draft it is publishing and
    refuses with `422 procedure_audit_findings` when a `high`-confidence finding remains; the publisher re-sends
    with `acknowledge_findings: true` to override, and the override is recorded on the publish's `audit_event`.
    `rbuild -- test -p conveyor-actions procedure_publish_audit_gate`.
14. The two pruners on `schedule` rows. `rbuild -- test -p conveyor-server pruners`.
15. Whole gate below — `rbuild --gate` (fmt + clippy + `test --locked`) on m1 (CONTRACT §16).

## 7. Gate

```bash
set -euo pipefail
# CONTRACT §16: Rust runs on m1 through rbuild; mini3's cargo refuses inside a Cargo project.
# 1. the three tables exist with RLS forced (a policy without FORCE reads zero rows and proves nothing)
test "$(psql "$DATABASE_URL" -tAc "SELECT count(*) FROM pg_class WHERE relname IN
  ('inference_call','prompt_audit','prompt_audit_finding') AND relrowsecurity AND relforcerowsecurity")" -eq 3
# 2. a REAL call end to end: one `completions.create` through the router, then the stored row compared field-for-field
#    with the router's own `usage` (SPEC §9 — a fixture cannot prove the capture seam; this is the only line that can)
rbuild -- test -p conveyor-actions --test inference_capture -- --exact a_real_call_records_the_router_usage
# 3. the derivation is exhaustive — read>0 hit, creation-only write, both-zero miss, no usage unknown (unit, no router)
rbuild -- test -p conveyor-domain --test inference_usage
# 4. the deterministic scan reproduces: two runs over the same draft differ in no finding's
#    (pattern, location, confidence). The LLM run is proved with the scripted model (CCR 6).
rbuild -- test -p conveyor-actions --test prompt_audits -- --exact the_same_draft_scans_identically
# 5. the audit cannot edit what it audits: no action in this slice carries `procedures:write`.
#    A compile-time registry assertion — `action_registry.scopes` does not exist (CCR 3).
rbuild -- test -p conveyor-actions --test registry -- --exact no_prompt_action_holds_procedures_write
# 6. the cache definition is a DB invariant, not a convention: the constraint exists, and a Rust test against
#    the runtime role proves a 'hit' with zero read tokens is refused (psql would otherwise fail for the wrong
#    reason — a missing app.current_tenant — and the line would pass vacuously)
test "$(psql "$DATABASE_URL" -tAc "SELECT count(*) FROM pg_constraint
  WHERE conname = 'inference_call_cache_status' AND contype = 'c'")" -eq 1
rbuild -- test -p conveyor-store --test inference_constraints -- --exact cache_status_check_refuses_a_hit_with_no_read
# 7. the served surface and the lock agree — the repo's own checker, not a hand-rolled count
rbuild -- run -q -p conveyor-server -- --print-openapi --check-lock
rbuild --gate
```

## 8. Traps

- **`sqlx::migrate!` embeds the set at compile time** — a new `.sql` is invisible until the crate rebuilds and the
  command exits 0 either way. Touch `migrate.rs` and verify by the ledger, never by the migrate log line.
- **Usage capture must never fail the call.** The row is written after a 2xx; a response with no `usage` stores
  `unknown`, and a parse failure logs and continues. A model call that succeeded must not become an error because
  bookkeeping did — but it *is* in the same transaction, so a genuine DB failure rolls the call's audit row back with
  it. That is the honest trade: one transaction, no divergence, and the failure is visible as a missing row.
- **Derive `cache_status` from the response, never the request.** The request says nothing about caching; a caller
  that asked for a cache breakpoint still misses on a cold prefix.
- **Do not emit cost.** The router meters this same call into billing, whose `billing_cost_events` already carries
  both cache classes; a second emission double-counts a tenant's bill.
- **`AGENT_TOOLS` is a fixed-size array** (`[&str; 22]`): adding six entries changes the type, the MCP allowlist
  test and `/mcp`'s `tools/list` snapshot together.
- **The audit reads the live registry, not a copy.** `actions.list` is the writer of record for a description; an
  audit against a checked-in copy would report findings about text that no longer ships, and would itself become the
  staleness problem it exists to find.
- **The join keys are not foreign keys, and that is deliberate.** An FK's RI check runs as the table owner and
  bypasses RLS, so it would accept another tenant's `run_id`; the engine resolves each id in the caller's tenant
  first and the `inference_call_join_key` CHECK keeps the row answerable without one. Copying the FK idiom from
  `conversation_turn_conversation_fk` without this step is the defect.
- **`prompt_audit@1` is versioned, not edited.** Changing the audit prompt means `prompt_audit@2`; an audit row
  names the version that produced its findings, and rewriting v1 would make every past finding unreproducible.

## 9. Decisions closed here

- **`inference_call` is written by the engine, not by a handler** — the call site already owns the response and the
  transaction; a handler-side writer would be a second path for the same fact. Contract-level: CONTRACT §18.
- **Cache is three states plus unknown, never two.** A write is a deliberate warm priced above a miss's saving, so
  folding it into "miss" would make every optimisation look like a regression. SPEC D23.
- **The audit is a procedure with one bounded model call**, not `agent.run`: it needs no tool loop, and D22's
  executor slot must not grow a further kind for a reporting feature.
- **Findings are decided by a person; the audit holds no write scope over its own surfaces.** CONTRACT §18.
- **Screens are declared here and built in S12** — S12 may not invent a screen, action or table.
- **Contract change taken:** three tables beyond §2.16's ceiling, with D23 (2026-11-09).
- **Two triggers, and the second one is configurable** (George 2026-11-09): a manual `prompt_audits.start`, and an
  *after N executions* policy in `tenant.settings.prompt_audit` that fires **as an event on the Nth captured call**
  (owner decision 2026-11-09, §10 Q2 — the slice's original sweep default is superseded). **`inference_call` is the
  counter** — the rows since the tenant's last audit, excluding the audit's own call — so the trigger adds no counter
  column and cannot drift from the ledger it is counting. The policy names the `target_model` the event-created audit
  will call; without one it is not armed.
- **The cache report groups by `prompt_sha` from day one** (George). The whole point is to see *which prompt* is
  cache-hostile; model and caller alone cannot answer the Group 4 finding it exists to evidence.
- **A draft is audited at publish, not on a cadence** (George: *"audit of a draft procedure should become part of
  the publishing of a procedure version"*). `procedures.publish` runs `prompt_audits.check`; a `high`-confidence
  finding refuses with `422 procedure_audit_findings`, and `acknowledge_findings: true` overrides it and is
  recorded on the publish's `audit_event` — a scoped, logged override, never a silent one.
- **The findings feed the kata-executor** (George 2026-11-09): kata reads `prompt_findings.list` and starts audits
  with `prompt_audits.start`, both already on its allowlist. S16 adds no kata surface and no kata table.
- **Retention: 90 days** (George) — a cache ratio over a month is noise, over a quarter it is a signal.

## 10. Open questions

| # | question | answer (owner, 2026-11-09) |
|---|---|---|
| 1 | Is `served_model` ever different from `model` for our provider set? | **Store both, report on `served_model`, fall back to `model`.** |
| 2 | Is the *after N executions* check a periodic sweep or an event on the Nth call? | **An event on the Nth call.** The counter is still the ledger (rows since the last audit, excluding `prompt_audit@1`), so there is no separate counter to drift; `prompt_audit.sweep` is not built. |
| 3 | Does a `high`-confidence finding from `prompt_audits.check` block publishing of a *first* draft, or only of a version that replaces a published one? | **Both.** A first publish is exactly where a dated prompt enters the system. |

## 11. Build amendments (CCRs, 2026-11-09)

1. **`prompt_audits.check` takes `procedure_id`**, not `procedure_version_id` — no such identity exists; publish
   already uses `procedure_id`.
2. **`check` is pure.** A finding cannot be persisted without an audit (`audit_id` FK NOT NULL, `target_model` NOT
   NULL), so `check` returns `CheckFindingView`s and persists nothing. Only `prompt_audit.run` persists findings,
   under a real audit.
3. **Gate item 5's SQL is invalid** (`action_registry.scopes` does not exist; internal actions are never rows
   there). Replaced by a compile-time registry assertion that no `prompt_*` action holds `procedures:write`.
4. **`prompt_findings.list` was not on `AGENT_TOOLS`** despite §9's claim; S16 adds it plus the other five
   read/start tools (22 → 28 on S16's baseline).
5. **The event trigger needs a model:** `tenant.settings.prompt_audit` gains `target_model`; without it the policy
   is not armed and the capture seam logs and skips.
6. **Reproducibility is asserted on the deterministic scan**, not on the model call: an LLM cannot be
   bit-reproducible. `prompt_audit.run` is proved with the scripted model harness plus one live rehearsal.
7. **The `actions` surface is declared but empty:** `ActionSpec` carries no authored description (the OpenAPI
   description is generated boilerplate). Procedure templates and the copilot `SYSTEM_PROMPT` are the audited
   surfaces until specs carry descriptions.
8. **`note` is not accepted on `prompt_findings.decide`:** no column persists it, so it is refused rather than
   silently discarded.

## 12. Build record (2026-11-09)

**Built.** Migration `0024_s16_inference_audit.sql` (three tables, FORCE RLS, the cache CHECK, the
cadence index and the auto-audit race guard); `domain::{inference,prompt_audit}`;
`store::{inference,prompt_audit}` with the capture seam (`capture_and_maybe_audit_tx`, two
savepoints) and the event-on-Nth-call trigger; the eight actions; the deterministic scanner and
the versioned `prompt_audit@1` prompt with its `prompt_audit.run` worker arm; the publish gate; the
two pruners' worker arms. Owner decisions and the eight build amendments are in §9–§11.

**Evidence.** Domain 165 tests; store 18 (`s16_schema`, `inference_`, `prompt_audit`, `s16_capture`
4, `s16_prune`, `jobs`); actions S16 targets 16 (registry 8, inference 1, inference_capture 2,
prompt_audits 3, publish gate 2); server `prompt_audit_run` 2; `parity_http_mcp` green at 216 with
all eight S16 samples; `clippy --workspace --all-targets -D warnings` clean. S16 measured **216
actions · 57 scopes · 28 AGENT_TOOLS · 237 ops**; the combined tree after S17 measures **224 · 59 ·
245**, lock sha256 `35fb4c05…`. The one workspace-suite run at the handover saw four failures, all
from S17's eight actions landing mid-run (count assertions + its parity samples), none in S16 code.

**Pending, deliberately:** the live-router capture rehearsal (needs `CONVEYOR_EXTERNAL_INFERENCE_URL`
and a real `completions.create`; a fixture cannot prove the live seam) and the two prune schedule
rows (`store/src/schedule.rs` was S17's).

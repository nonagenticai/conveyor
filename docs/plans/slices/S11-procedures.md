---
slice: S11
title: Processes, steps and SOPs — executable documentation
status: built 2026-11-09 (steps 1–12); amended 2026-11-09 (process/step/SOP split — migration 0033)
depends_on: [S03, S04, S06, S07]   # S07 owns `migrate --seed`: the card_type and gate codes this gate reads
spec_sections: ["§3 D10", "§4", "§6", "§8 S11"]
adds_tables: [process, process_step, procedure, process_map, run_step]
adds_actions: [processes.create, processes.update, processes.publish, processes.retire, processes.get, processes.list, process_runs.start, procedures.create, procedures.update, procedures.publish, procedures.retire, procedures.get, procedures.list]
adds_events: [process.published, process.retired, procedure.published, procedure.retired, run_step.changed]
adds_modules: []
parity_ids: PENDING-S0
---

⚠️ **Amended 2026-11-09 (process model).** The artifact is a **process** — an ordered list of **steps**, no graph. Migration `0033` **renames `procedure` → `process` and `procedure_step` → `process_step`**, and adds a **new `procedure` table as the SOP** plus **`process_map`** (the user-facing DAG). A step carries `kind ∈ agent|computed|gate|call`, `actor ∈ process_agent|tool_agent|system|human`, `capability_id`, `process_agent_id` (the worker), `procedure_id` (the SOP) and `child_process_key` (a call). A **procedure (SOP)** is versioned: `definition_of_ready`, `definition_of_done`, `acceptance_criteria`, `inspection` (laya/eval/checker) and `outputs`. A **process** declares its **user-facing outputs** and is tenant-authored or from the default catalogue. Publish validation adds: every computed/tool step names a registered action; every agent step names a `capability_agent(role='process_agent')` whose `allowed_actions` are registered; every step's SOP resolves; every call names a published process and depth ≤3; every map node resolves. The per-type 280 default processes, the run tree, the judge and the bus are S13's.

# S11 — Procedures (redesigned)

Programme project: S11 · issues #15

**Gate (SPEC §8 S11):** author → publish → execute a procedure run; §7 adds the gate, refusal, retry and publish-time proofs.

## 1. Objective

A published procedure drives one card: ordered steps, each a registry action with a resolved input template and an optional approval gate, run as a `run` plus one immutable `run_step` per attempt. The 280 card-type × list flows become vocabulary data plus a few procedures, not flows.

## 2. Starting state

- S03: `run`, `approval`, `card_type`, `catalogue_entry`, `revision`; `run::transition`, `board::may_move`.
- S04: registry, `GET /api/actions`, invocation route, JSON-Schema validation.
- S06: `runs.*`, `approvals.decide|list`, job runner, SSE `run:{id}`.

## 3. Scope

**IN** — the 3 tables of `02-DATA-ARCHITECTURE §2.12`; two `run` columns; 9 actions; publish validation; one resolver; job kind `procedure.step`; the seed.

**OUT** — matrix legality (`S03`), approval transport (`S06`, `S06a`), sub-procedures/engine/editor (`S13`), screens (`S12`), PDL (never).

**UI surface declared for S12** (declaration only): `procedures` — `procedures.*`, `procedure_runs.start/cancel`, read models `procedure` + `procedure_step` + `run_step`; SSE `run:{id}`.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `SPEC.md` §3 D10, §4, §6 | no PDL; `Procedure 1—* ProcedureStep`; three states | the model | procedure_management's 54 endpoints |
| `02-DATA-ARCHITECTURE.md` §2.12, §3.2, §3.5, §8 | bodies; run transitions; publish invariant; forward refs | DDL + one published per key | — |
| `02-source-map/10-agent-patterns.md` (`agent` `83b8de7e`) `runtime/orchestrator.rs` | `ProcedureSource`, `choose`, `refusal` | shape only: refusal returned, steps from a pinned source | `identity_level`, the agent ops |
| `02-source-map/30-rm-planka-mcp-n8n.md` §1, §4 (`rm` `9ee72ab4`) | `pm_catalogue.py` routers | `catalogue_entry` kinds + seed source | the 20-table sprawl |

## 5. Deliverables

### 5.1 Schema — `migrations/0015_s11_procedure.sql`

`02-DATA-ARCHITECTURE §2.12` verbatim — a checklist of what this migration creates, all of it already recorded in §2.12 and §8 (not a delta list; a delta would need a `CONTRACT-CHANGE REQUEST`):

1. `procedure`: the partial `unique (tenant_id, key) where status='published'` (§2.12, enforced by §3.5) — this migration creates it.
2. `procedure_step`: `max_attempts int nn d=1 check(max_attempts >= 1)`. **Default 1, CHECK ≥ 1, application cap 10** — the cap lives in `domain::procedure::publish` and the retry predicate, never in DDL. Stated here, in `02-DATA-ARCHITECTURE §2.12` and in §10.
3. `run_step`: `attempt int nn d=1`; `uq (run_id, position, attempt)` replaces `uq (run_id, position)` — a retry appends, never overwrites the trace.
4. `run`: `procedure_id uuid null` — FK `→ procedure`, added by **this** migration; **S03 does not create it** (§8 records it as a forward reference). Plus `inputs jsonb nn d='{}'`. `run.workflow_id uuid null` stays S13's and is likewise not created by S03.

### 5.2 Actions

| action | input → output |
|---|---|
| `procedures.create` | `{key,title,description?,card_type_code?,steps?}` → `ProcedureOut`; new key = v1, existing forks `max(version)+1`, second draft = 409 `procedure.draft_exists` |
| `procedures.update` | `{procedure_id,title?,description?,steps?}` → `ProcedureOut`; draft only, `steps` replaces the list, `If-Match` |
| `procedures.publish` | `{procedure_id}` → `{version,revision_id,content_hash}`; re-publish returns the same |
| `procedures.retire` | `{procedure_id}` → `ProcedureOut`; live runs unaffected |
| `procedures.get` / `.list` | `{procedure_id?/key?,version?}` / `{status?,card_type_code?,cursor?,limit?}` → `ProcedureOut` / page |
| `procedure_runs.start` | `{card_id,procedure_id?/procedure_key?,inputs?}` → `RunOut`; `Idempotency-Key`, one live **root** run per card; a child may start under a live parent (`parent_run_id`/`step_id`, 2026-11-09) |

`StepIn = {action, input_template, gate_kind?, on_error?, max_attempts?}`, `on_error ∈ fail|continue|retry` (default `fail`). `procedure_runs.start` composes S06's `runs.create` + `runs.start`; **cancel is S06's `runs.cancel`** (extended to skip the run's queued `run_step` rows and announce each) — no second run state machine and no second cancel surface. `procedure_runs.complete_step` is **dropped** (step 9, 2026-11-09): the executor is the only writer of a run step, and an external "mark this complete" would let a caller set a step result without executing it; no consumer existed. HTTP is the registry route; no screens (S12). Files: `crates/domain/src/{procedure,template}.rs`, `crates/store/src/{procedure,run_step}.rs`, `crates/actions/src/procedure/`, `crates/api/src/procedures.rs`, `crates/api/tests/procedure_*.rs`. **Events:** `procedure.published`/`procedure.retired` from publish/retire (`{id,key,version}`), `run_step.changed` on every `run_step` transition (`{run_id,position,attempt,status}`).

### 5.3 Model and publish

A procedure is a version family: rows share `(tenant_id, key)`, each with `version` and `status ∈ draft|published|retired`. Publish validates in one transaction: action in the live manifest; `gate_kind` in `catalogue_entry(kind='gate')`; template roots/paths legal and `steps.<n> < position`; `retry` only on idempotent actions; no `procedure_runs.*` step. It writes the frozen `revision` (`entity_type='procedure'`, `seq=version`, `content` = the steps, `content_hash=sha256(canonical JSON)`), flips the key's previous published row to `retired` and this one to `published`, emits `procedure.published`. `procedure_step` is immutable after publish; an edit copies it into v+1.

### 5.4 Input template

| root | allowed paths |
|---|---|
| `run` | `id`, `card_id`, `project_id`, `correlation_id`, `inputs.<path>` |
| `steps` | `steps.<n>.output.<path>` — 1-based, `n <` current position |
| `card` | `id`, `name`, `description`, `list_id`, `list_kind`, `card_type_code`, `assignee_id`, `version`, `meta.<path>` |
| `procedure` | `procedure.key`, `procedure.version` |

Syntax `{{` _path_ `}}`, optional spaces. A whole-string reference substitutes the value **with its JSON type preserved**; an embedded reference interpolates **scalars only** (object or array refuses). `\{{` emits a literal `{{`; no other escapes, filters, conditionals or functions.

**A mutating step carries its precondition as the reserved member `_version`** — `"{\"id\":\"{{card.id}}\",\"_version\":\"{{card.version}}\"}"` — which the executor strips before schema validation and passes as the invocation's `If-Match` (CONTRACT §6). The action itself gains no `version` input: a stale value is a `412` to the step, handled by `on_error`.

One resolver: `crates/domain/src/template.rs::resolve(&Value, &TemplateCtx) -> Result<Value, TemplateError>`, once per step inside the step transaction. Missing root, unknown path, out-of-range index, illegal interpolation, or `steps.<n> >= position` is a **refusal** — 422 `procedure.template_unresolved` with the JSON pointer in `errors[]`, never an empty string. A path existing with value `null` resolves to `null` (absent ≠ null). The result is validated against the action's schema and persisted as `run_step.input`.

### 5.5 Execution

- **Start** (one tx): card matches the procedure's `card_type` (else 409 `procedure.card_type_mismatch`), procedure `published`; insert `run` with `inputs`; one `run_step(position, action, status='queued', attempt=1)` per step; `queued→running`; enqueue `procedure.step` with `dedupe_key="<run_id>:<position>:<attempt>"`.
- **Step**: claim `UPDATE run_step SET status='running' WHERE id=$1 AND status='queued'` (0 rows ⇒ stored result); resolve, validate, invoke; the record tx writes result, `action_invocation`, `audit_event`, `run_step.changed`. The call sits between the transactions with `Idempotency-Key` = uuidv5 of `run_id:position:attempt`.
- **Gate**: `gate_kind` K creates `approval(gate=K, kind='procedure', required_decisions/expires_at` from `catalogue_entry(kind='gate').meta)` in the `run_step` tx, transitions `running→waiting_approval`, enqueues nothing; `approved` enqueues the step and resumes, `rejected|expired` applies `on_error`. A gate authorises the step, never its outcome.
- **Retry and cancel**: `failed` + `on_error='retry'` + `attempt < max_attempts` inserts `run_step(position, attempt+1)` and enqueues — only the failed position re-enters; `fail` ends the run, `continue` marks `skipped`. Cancel is `running|waiting_approval → cancelled`, approval `voided`, open steps `skipped`.

### 5.6 The card-type × list matrix — what is a procedure

| what | where | slice |
|---|---|---|
| which `card_type` may sit in / move to which `list.kind` | `catalogue_entry(kind='project_type').meta.matrix` + `may_move` | S03 |
| phases, gates, recipes, project types, card types | `catalogue_entry(kind=…)`, `card_type` | S03 |
| the process executed on one card | `procedure` + `procedure_step` | **S11** |
| the 280 flows | matrix rows × event, one lookup | S03 |

A **process** (stored as `procedure` before migration `0033`, renamed to `process` by it) is an ordered sequence of **steps** on one card of one `card_type`, what one of the 51 capability flows did ("development phase": claim → work → gate → move to done). ⚠️ **Superseded 2026-11-09 by the amendment above:** a *procedure* now means the **SOP** attached to a step; the sequence is the process. The 280 flows are the **default catalogue**: one process per `(card_type, list)`, tenant-overridable, and matrix data for the moves.

`migrate --seed` writes `card_type` from `pm_card_types`, `catalogue_entry` from the `pm_catalogue` routers and the matrix from `pm_project_type_card_types`. **The procedure seed is DEFERRED (George 2026-11-09)** — the S11 slice closes with zero seeded procedures. The follow-up data task is: one procedure per S0-evidenced `cap.*` flow, steps = the actions that flow calls in order, derived from `parity/evidence/executions.tsv` (C1 evidence) plus a read-only census of the live n8n `workflow_entity.nodes`, recorded as `parity/evidence/cap-flow-census.tsv`, then a `seed/procedures.json` imported by `migrate --seed`. Zero-evidence procedures are not seeded; RM counts are estimates and must be re-counted.

## 6. Build order

1. `migrations/0015_s11_procedure.sql`; `cargo run -p conveyor-server -- migrate`; `\d+ run_step`.
2. `domain::procedure` types + the five publish checks; `cargo test -p conveyor-domain procedure_publish`.
3. `domain::template::resolve` + a case table (object, interpolation, escape, missing path, forward ref, unknown root); `cargo test -p conveyor-domain template`.
4. `store::{procedure,run_step}`: create/fork, copy-forward, materialise, claim, record, append attempt; `cargo test -p conveyor-store procedure_crud run_step`.
5. Six `procedures.*` actions + problem+json codes; `cargo test -p conveyor-actions procedures`.
6. Publish: manifest lookup, gate lookup, revision freeze, old→retired in one tx; `cargo test -p conveyor-actions --test procedure publish`.
7. Start: card/type checks, run insert, materialisation, job enqueue; `cargo test -p conveyor-actions --test procedure_start`.
8. Step executor + `procedure.step` job: claim → resolve → invoke → record; `cargo test -p conveyor-actions --test procedure_step`.
9. Gate, retry, continue, cancel (built 2026-11-09). **Gate representation:** the gated step stays `queued`; `gate_kind` opens an S06 `approval` (`kind='procedure'`, `gate=gate_kind`, `meta={position,attempt,procedure_step_id}`, quorum from `catalogue_entry(kind='gate').meta.dual_control`, `expires_at` from its `meta` when present) and the run parks `running → waiting_approval`. `approved` enqueues the same `(position,attempt)` as its own job (the `:gate` dedupe suffix keeps it from collapsing onto a still-running park job); `rejected|expired` applies `on_error` through the ordinary record-and-chain. **Cancel** is `runs.cancel` (voids the pending gate, skips the queued steps). Tests live in `crates/actions/tests/procedure_step.rs` (gate/retry/continue) and `procedure_cancel.rs`; a worker-level expiry path is in `apps/server/src/worker.rs::approval_expire`.
10. MCP projection + spec lock: `cargo run -p conveyor-server -- --print-openapi --check-lock`.
11. Seed: `cargo run -p conveyor-server -- migrate --seed`; then, **tenant-scoped** (the table is RLS+FORCE, so an unscoped count is always 0): `psql "$DATABASE_URL" -qAtc "BEGIN; SET LOCAL app.current_tenant='<dev-a uuid>'; select count(*) from procedure where status='published'; COMMIT;"`. **The procedure seed itself is deferred (George 2026-11-09)**: the count is 0 by decision, and the derivation path is in §5.6.
12. Gate rehearsal (§7); `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
docker compose up -d db && cargo run -p conveyor-server -- migrate --dev-seed --seed   # --dev-seed creates dev-a/alice; --seed alone seeds no tenants
CONVEYOR_DEV_AUTH=1 cargo run -p conveyor-server -- worker &
CONVEYOR_DEV_AUTH=1 cargo run -p conveyor-server -- serve &
trap 'kill $(jobs -p) 2>/dev/null || true' EXIT
API=localhost:8930/api/actions; P(){ curl -fsS -X POST "$API/$1/invocations" -H 'content-type: application/json' -H "authorization: Bearer dev:alice" -d "$2"; }
TENANT=$(psql "$DATABASE_URL" -tAc "select id from tenant where slug='dev-a'")          # `tenant` is the one table with no RLS
Q(){ psql "$DATABASE_URL" -qAtc "BEGIN; SET LOCAL app.current_tenant='$TENANT'; $1; COMMIT;"; }   # SET LOCAL inside one transaction, never a session-level SET
PAIR=$(Q "select e.code || '|' || ct.code from catalogue_entry e join card_type ct on e.meta->'matrix' ? ct.code and e.meta->'matrix'->ct.code->'active' ? 'done' where e.kind='project_type' and e.is_active and ct.is_active order by e.code, ct.code limit 1")
PT=${PAIR%%|*}; CT=${PAIR#*|}
[ -n "$PT" ] && [ "$PT" != "$PAIR" ] || { echo "seed has no project_type whose matrix allows active→done (S07 §5.4)" >&2; exit 1; }
ID=$(P procedures.create "{\"input\":{\"key\":\"dev-phase\",\"title\":\"Dev phase\",\"card_type_code\":\"$CT\",\"steps\":[
 {\"action\":\"cards.claim\",\"input_template\":{\"card_id\":\"{{card.id}}\",\"_version\":\"{{card.version}}\"}},
 {\"action\":\"cards.comment\",\"input_template\":{\"id\":\"{{card.id}}\",\"body\":\"run {{run.inputs.ticket}}\"},\"gate_kind\":\"human_approval\"},
 {\"action\":\"cards.move\",\"input_template\":{\"id\":\"{{card.id}}\",\"list_id\":\"{{run.inputs.done_list}}\",\"_version\":\"{{card.version}}\"}}]}}" | jq -r .id)
PRJ=$(P projects.create "{\"input\":{\"title\":\"s11 gate\",\"project_type_code\":\"$PT\"}}" | jq -r .id)
[ -n "$PRJ" ] || { echo "projects.create returned no id" >&2; exit 1; }
BOARD=$(Q "select id from board where project_id='$PRJ' order by position, id limit 1")
ACTIVE=$(Q "select id from list where board_id='$BOARD' and kind='active'")
DONE=$(Q "select id from list where board_id='$BOARD' and kind='done'")
[ -n "$BOARD" ] && [ -n "$ACTIVE" ] && [ -n "$DONE" ] || { echo "the project has no board or no active/done list" >&2; exit 1; }
CARD=$(P cards.create "{\"input\":{\"board_id\":\"$BOARD\",\"list_id\":\"$ACTIVE\",\"name\":\"S11 gate card\",\"card_type_code\":\"$CT\"}}" | jq -r .id)
P procedures.publish "{\"input\":{\"procedure_id\":\"$ID\"}}"
RUN=$(P procedure_runs.start "{\"input\":{\"card_id\":\"$CARD\",\"procedure_key\":\"dev-phase\",\"inputs\":{\"ticket\":\"T-1\",\"done_list\":\"$DONE\"}}}" | jq -r .id)
for i in $(seq 1 30); do [ "$(Q "select status from run where id='$RUN'")" = waiting_approval ] && break; sleep 1; done   # the worker advances asynchronously
test "$(Q "select status from run where id='$RUN'")" = waiting_approval          # waiting_approval
APPROVAL=$(Q "select id from approval where run_id='$RUN' and status='pending'")
P approvals.decide "{\"input\":{\"approval_id\":\"$APPROVAL\",\"decision\":\"approve\"}}"
for i in $(seq 1 30); do [ "$(Q "select count(*) from run_step where run_id='$RUN'")" = 3 ] && break; sleep 1; done
test "$(Q "select count(*) from run_step where run_id='$RUN'")" = 3        # 3 rows
for i in $(seq 1 30); do [ "$(Q "select status from run where id='$RUN'")" = succeeded ] && break; sleep 1; done   # the approved gate resumes the last step
test "$(Q "select status from run where id='$RUN'")" = succeeded
test "$(Q "select count(*) from run_step where run_id='$RUN' and status='succeeded'")" = 3
```

Green when three `run_step` rows are `succeeded` with non-empty `input`/`output`, the run is `succeeded`, one `approval` exists. Negatives:

- unresolved variable: `{{run.inputs.nope}}` returns **422** `procedure.template_unresolved` with a problem+json `type` — not `""`, not 500.
- unknown action: `{"action":"cards.nope"}` makes `procedures.publish` return 422 `procedure.action_unknown`; no run, no `run_step`.
- retry: step 2 failing once (`on_error=retry`) leaves position 1 = 1 row, position 2 = 2 rows, `action_invocation` for `cards.claim` = 1.

## 8. Traps

- **PDL creeping back.** No condition/effect/assertion field. Input legality is the action's JSON Schema; state legality is the matrix; "what next" is step order.
- **A template language with no escaping.** `\{{` is the only escape; unknown roots and bad indices refuse. No filters, `||` or truthiness.
- **A step that invokes itself.** Publish refuses any step naming `procedure_runs.*` and any `steps.<n>` at or after its position. Sub-procedures (S13) need `run.parent_run_id` and a depth cap first.
- **`run_step` outside the step's transaction.** Result, `action_invocation`, `audit_event` and event share one tx; the remote call cannot, so its `Idempotency-Key` derives from `(run_id, position, attempt)` and a crash retries into the same key. ⚠️ **As built (step 8), the internal composed call cannot share the record's tx**: `invoke` owns its own transaction (validation, idempotency claim, bookkeeping, handler, commit), so `action_invocation`/`audit_event` commit there and `run_step` + `run_step.changed` + the chain commit in a second transaction. The property is preserved by the replay rule instead: the resolved input is persisted at CLAIM, a crash re-invokes under the same Idempotency-Key, and the record is idempotent on `(run_id, position, attempt)`. Sharing one tx would require an `invoke_in_tx` engine entry point — deliberately not added here.
- **Publishing without freezing.** Status flip, `revision` and retired-previous are one tx; `procedure_step` is immutable after publish. Never re-read the procedure at step time.
- **A gate on a non-idempotent step.** A gate parks before execution so the step runs once per attempt, but `on_error='retry'` on a non-idempotent action is refused at publish — the retry has no second approval.
- ⚠️ **Executor scoping (D22, CONTRACT §17) — S11 seams it, S15 fills it.** As built (2026-11-09): migration
  **`0021_s11_run_capability`** adds the two forward-reference `run` columns (`capability_id`,
  `capability_agent_id`; no FK — S15's `0020` adds the targets and constraints), and `procedure_runs.start` calls one
  `resolve_start_context` + `ensure_start_ready` pair before anything is written. Today the resolver returns
  `StartContext::Unresolved` (no capability schema exists), so the readiness arm does not apply — it is not skipped;
  the enum makes that explicit. S15 fills the `Resolved` body (readiness, `409 capability_not_ready` with
  `blocking[]`, `allowed_actions` passed into the invocation) and must not add a second start path.
- **Who may author.** `procedures:write` is held by owner/admin/member (super-admin, tenant-admin, tenant-member)
  and by the **`kata_executor`** role — now a real fifth role (George 2026-11-09, CCR 5): a synthetic
  tenant-admin held only by a MACHINE principal (`app_user.kind` = `agent` | `service`; `members.set_role`
  refuses a human). It holds every tenant scope an `admin` does, because its job is to create and update the
  system's shared assets (procedures today; workflows S13, knowledge S15) and run project work. The step
  executor runs as `actor_kind='service'`, `Role::KataExecutor`. A viewer may read but not author. The
  publish-time invoke check (step 8) remains the control that stops a member from conferring an action they
  lack.

## 9. Decisions closed here

1. Three tables as §2.12; the §5.1 deltas are this slice's.
2. `on_error = fail|continue|retry` + `max_attempts`; `continue` replaces §2.12's old `skip` label — the column value is `continue`.
3. Gates are pre-step only; `gate_kind` names a `catalogue_entry(kind='gate')` row supplying `required_decisions` and `expires_at`.
4. Publish validates against the live manifest; template resolution is one pure function in `crates/domain`.
5. `run.inputs` lives on `run`; one published row per `(tenant_id, key)`; an edit forks v+1.
6. **Publishing runs a prompt audit of the draft (S16, George 2026-11-09).** `procedures.publish` calls S16's
   `prompt_audits.check` on the version it is publishing; a `high`-confidence finding refuses with
   `422 procedure_audit_findings`, and `acknowledge_findings: true` overrides it with the override written to the
   publish's `audit_event`. This is the one change S16 makes to a built action: a new optional input member, no new
   state and no change to the publish rules themselves.
7. **A procedure step is a prompt surface and a join target (S16).** `procedure_step.input` is what the prompt
   audit inventories, and a step whose action is `completions.create` produces an `inference_call` row carrying
   `run_step_id` and `caller_kind='procedure_step'`. Nothing in this slice changes to support it: the audit reads
   `procedure`/`procedure_step` through the ordinary read actions, and the capture happens in the action engine
   (CONTRACT §18), not in the step executor.

**CONTRACT-CHANGE REQUEST 1 — APPROVED 2026-11-07**, `02-DATA-ARCHITECTURE §2.12` amended: `run_step` gains `attempt int nn d=1` and `uq (run_id, position, attempt)` — a retry must append or S13's diff trace is destroyed.
**CONTRACT-CHANGE REQUEST 2 — APPROVED 2026-11-07**, §2.12 amended: `check(on_error in ('fail','continue','retry'))` plus `max_attempts int nn d=1 check(max_attempts >= 1)` (application cap 10, §5.1) — `retry(n)` was inexpressible.
**CONTRACT-CHANGE REQUEST 3 — raised 2026-11-08 while building the store**, §2.12 amended: `procedure` gains
`lock_version bigint nn d=1`, bumped by every mutation (migration `0017`). Reason: `procedure.version` is the
FAMILY version — `uq (tenant_id, key, version)` and "an edit forks v+1" give it that job — so it never changes
on an edit, and §5.2's `If-Match` had nothing to compare: a stale precondition would have passed. The two jobs
cannot share one column.
**Migration `0016`** adds `uq_procedure_draft` (`unique (tenant_id, key) where status='draft'`): the draft twin
of `0015`'s published index, so §5.2's "second draft = 409" is the database's rule rather than a check two
transactions can both pass. No contract change — §2.12's `procedure` already requires at most one draft.
**CONTRACT-CHANGE REQUEST 4 — raised 2026-11-09 while building step 8 and approved in the same pass**, this
file's front-matter and §5.2 amended: the step event is **`run_step.changed`**, not `run.step_changed`.
CONTRACT §7 (`01-CONTRACT.md:146`) requires `<aggregate>.<verb>` with exactly one dot and
`store::outbox::valid_event_type` enforces it, so the two-dot name would have made every emit roll its
transaction back. Same adjudication as S10's `conversation.turn.created` → `conversation_turn.created`.
Step 8 also records two interpretations taken while building: a gated step fails closed
(`procedure_gate_unimplemented`) until step 9 builds gates — **superseded 2026-11-09 by step 9's approval park
(§6 step 9)** — and `procedures.publish` refuses (403 `procedure.action_forbidden`) a step action the publisher's
role cannot invoke, because the executor runs as an internal `service` principal with the `kata_executor`
tenant-admin role (CCR 5) and `cards:write` is admin-only while `procedures:write` is member.

**CONTRACT-CHANGE REQUEST 5 — raised 2026-11-09 and approved by George the same day**, 02-DATA-ARCHITECTURE
§2.1 amended: `membership.role` gains **`kata_executor`** (migration `0022`). It is a synthetic tenant-admin:
`admin`-equivalent in `Scope::granted_to`, held only by a machine principal (`members.set_role` refuses a
human), and the principal the procedure step executor runs as. This supersedes the earlier "there is no fifth
role" decision in §8; the board-level `board_membership.role` vocabulary is unchanged.

## 10. Open questions

1. Rejected gate: fail the run or follow `on_error`? **Default:** follow `on_error` (`fail`).
2. Procedure-level input schema? **Default:** no; S04 validates each step's input.
3. Default `max_attempts`? **Default:** 1 — a single attempt; the cap is 10 (§5.1).
4. Per-step PATCH? **Default:** no; whole-list replace on a draft.

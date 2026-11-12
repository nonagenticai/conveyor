---
slice: S13
title: Process engine, judge, map and care-bus — the process is the executor, the DAG retained as the tool-sequence engine, the 348 flows re-authored
status: built (2026-11-10; session 2 closed W4/W5/D29, the catalogue E2E and the BYO token path; session 3 closed the step contract, gate/call seed content, the run lifecycle, the judge record, map authoring, the bus replay proof and the install path; the DAG plan is superseded; the engine is retained — owner ruling 2026-11-09)
depends_on: [S03, S04, S11, S15]
spec_sections: ["§2", "§3 D6/D20/D22/D28/D29/D30", "§4", "§5.2", "§5.6", "§6", "§8 S13", "§11 R4/R6"]
adds_tables: []
adds_actions: [laya.judge]
adds_events: []
adds_modules: [bus]
parity_ids: PENDING-S0
---

# S13 — Process engine, judge, map and care-bus

Programme project: S13 · issues #17

**Gate (SPEC §8 S13):** the execution-trace diff against the old flows, plus the run tree (a child under a live
parent), the judge (a reject loops, a laya outage waits) and the bus (a Redis loss re-delivers without a double
outcome). **R4 binds this plan:** the process model replaces the DAG as the executor, flows are re-authored as
processes/handlers (never converted), and the engine never depends on an editor.

## 1. Objective

Make the conveyor execute **processes**, not n8n graphs. A process is an ordered list of **steps** — an **agent**
task, a **computed** task, a **gate**, or a child **call** — stored as `process`/`process_step` (S11's tables,
renamed by migration `0035`). Every step has a **procedure (SOP)**: definition of ready, definition of done,
acceptance criteria and an **inspection**; the process declares its **user-facing outputs**. The step's output is
**judged** by the SOP's judge (the laya decision model, an eval, or a checker): accept → done; reject → rerun with
the judge's reason (max 2) → BLOCKED → coach; a laya outage **waits**.

A run is a **tree**: one live root per card, and a live parent may start a child (the coach, a `cap.*` call). Step
work is carried by the **care-bus** (Redis Streams, ported from `agent-api/src/runtime/bus.rs`, itself from
`care-bus-agents@96c608c`) with consumer groups and a dead-letter stream, while Postgres `run`/`run_step`/`event`
remain the system of record.

The **DAG returns only as the process map**: a versioned, declarative `process_map` per project type whose nodes
are processes, capabilities, phases or milestones — the user's view of what is happening, never the executor. The
**workflow engine is RETAINED** (owner ruling 2026-11-09): `crates/workflow`, the `workflow` table,
`workflows.*`/`workflow_runs.*` and the `workflow.node`/`workflow.start` job kinds stay as the **tool-sequence
executor** — a process `call` step resolves `child_process_key` **process-first, workflow-second** — and there is
**no drop migration**. What does retire is the hook seam: `POST /api/hooks/{hook}` and `CONVEYOR_HOOK_SECRET` are
deleted. The five named legacy flows are **absorbed**: `conveyor.start` → `projects.create`; `wip.pull` → a
`card.moved` handler; `err.card` → the `run.failed` handler; `ask_fr` and `kata.open` → processes.

## 2. Starting state

- S03/S04/S11 built: `run`, `run_step`, `procedure`, `procedure_step` (to be renamed), `run_step`, the six
  `procedures.*` actions, `procedure_runs.start`, the `procedure.step` executor, gates and cancel. S15 built:
  `capability_agent` with its readiness seam.
- The DAG exists and is proven (S13 sessions 1–9): `crates/workflow`, migration `0019`, the trace/diff gate, the
  348-row dispositions ledger. **It is RETAINED as the tool-sequence executor** (owner ruling 2026-11-09); its
  graph planner never drives a process step. Evidence: `docs/evidence/S13-ENGINE.md`.
- Measured 2026-11-07: 348 flows / 332 active — 280 card-type × list (70 types × 4 lists), 51 `cap.*`, 4 named,
  12 `zz.*`/dead, 1 `kata.open`. n8n history is 3 days; the three fixture traces are committed.
- `uq_run_live` is card-wide today — the reason the coach child cannot start (`crates/store/src/run.rs:199`).

## 3. Scope

**IN:** migration `0035_process_tree.sql` (the `procedure`→`process` and `procedure_step`→`process_step` renames, the new
`procedure` SOP table and `process_map`, the run tree, `capability.kind`, the agent roles, `run_step.judge`, the
event bus cursor, and `inference_call.caller_kind` `procedure_step`→`process_step`); migration
`0036_s19_output_grade.sql` (S19's per-output grade path); the process/SOP publish validation; the step executor
with gate/retry/call; the judge (checker/laya/eval); the run tree (root-only live index, depth ≤3, cascade cancel);
`crates/bus` + the relay + groups + DLQ; `projects.create` absorbing the start; `wip.pull`/`err.card` computed
handlers; the 70 type files / 280 default processes / 350 SOPs with their agents; the dispositions
rewrite; the trace export/diff over `run_step`. **The DAG is NOT deleted** — the workflow engine is retained
(owner ruling 2026-11-09) as the tool-sequence executor.

**OUT:** data migration (D7); the map/stage editor beyond a read/author form (S12); the `cap.*` prompts (S15/S21);
the benchmark/eval slices; **w2v/media production** (a later capability — the tenant-defined model is what will
carry it, D30); parallel fan-out (v1 is serial); any Python.

## 4. Source material

| path | read this | carry / do not carry |
|---|---|---|
| v4 `model.md` §3/§4, `workflows.md` §2, `capability-flows.md` §2 | macro/micro mantra; card-type → 4 list flows → 4 agents → atomics | the vocabulary; not n8n's execution model |
| v4 `README.md` R348(2), R359(1–4), R347(3) | the laya judge: `{score, accepted, reason}`, margin, reject → rerun with the reason (2), outage waits | the client is TypeSafe AI over `https://api.nonagentic.ai/laya` with the conveyor licence |
| `agent/crates/agent-api/src/runtime/bus.rs` (from `care-bus-agents@96c608c`) | envelope, topics, consumer groups, XACK/XAUTOCLAIM, DLQ, scope | port with attribution; no `env` |
| `agent/crates/agent-api/src/runtime/{case,orchestrator}.rs` | the case/phase machine and deadlines | the shape of a run that waits |
| S11 §5, S15 §5 | process/step, capability_agent, readiness | the storage and the executor seam |
| `docs/evidence/S13-ENGINE.md` | the DAG's measured history and the 348 ledger | evidence only; do not rebuild the DAG |

## 5. Deliverables

### 5.1 Migration `0035_process_tree.sql`

- Rename `procedure` → `process`, `procedure_step` → `process_step`; rename the FK columns accordingly
  (`run.procedure_id` → `run.process_id`, `run_step.process_step_id`).
- **New `procedure` (SOP)**: `{key, title, description, status, version, definition_of_ready jsonb,
  definition_of_done jsonb, acceptance_criteria jsonb, inspection jsonb, outputs jsonb}`; one published per key.
- **New `process_map`**: `{key, title, status, version, project_type_id, definition jsonb}` (nodes/edges); one
  published per `(key, project_type_id)`.
- `run`: `parent_run_id uuid → run(id)`, `step_id uuid → process_step(id)`, `depth int NOT NULL DEFAULT 0 CHECK
  (depth BETWEEN 0 AND 3)`, `CHECK (parent_run_id IS NULL OR step_id IS NOT NULL)`.
- Drop `uq_run_live` and `uq_run_project_live`; recreate both root-only; add `uq_run_child_live (parent_run_id,
  step_id)` and `uq_run_child_attempt (tenant_id, parent_run_id, step_id, attempt)`.
- **`0037_run_tree_workflow_step.sql` (2026-11-10, W5)**: `run.workflow_step_id uuid → run_step(id)` and the
  workflow pair of child indexes; `run_parent_step_check` replaced so a child names EXACTLY one step kind (a
  process parent's `process_step` or a workflow parent's `run_step`). Before this, a workflow's procedure node
  could not start a child at all: `step_id` is FK-bound to `process_step`, and a root start 409s on
  `uq_run_live` against the parent. `process_runs.start` takes `workflow_step_id`; the workflow node passes
  its `run_step` id.
- `process_step`: `kind ∈ agent|computed|gate|call`, `actor ∈ process_agent|tool_agent|system|human`,
  `capability_id`, `process_agent_id`, `procedure_id`, `child_process_key`, `on_result`.
- `process`: `capability_id`, `card_type_id`, `list_kind`, `project_type_id`, `outputs jsonb`.
- `capability.kind ∈ system|tenant`; `capability_agent.role ∈ process_agent|tool_agent` + `process_key`/`toolset`.
- `run_step.judge jsonb`; `event.bus_stream_id`/`published_at`.

### 5.2 The process/SOP definitions and publish validation

- One process per `(card_type, list)` — 70 × 4 = **280** — is the **default catalogue**, not a fixed system set;
  a tenant may instantiate, override or design its own (D29). **As built (2026-11-10):** 70 type files / 280
  processes / 350 SOPs, loaded by `crates/store/src/process_seed.rs` (`serde_yaml` + `include_dir`; `apply_all`
  after `migrate --seed`, `apply_type` for the dev fixture's start process); `seed/processes/manifest.yaml` is the
  generated 280-entry index. The loader test applies the whole catalogue **twice on a fresh database**, proving
  publish validation and idempotency. The `cap.*` processes, `ask_fr` and `kata.open` are
  the same shape. `project_management`, `kanban_coach` and `kata` are the fixed system capabilities.
- A **step** names its capability, worker (`process_agent_id`), tools, `procedure_id` (SOP), `on_result` and
  `max_attempts` (≤10). **No predicates, no loop nodes, no expression language**; a step's typed outcome is
  `continue | fail | retry`.
- **The step's action field (2026-11-10).** An `agent` step stores the primitive it runs
  (`agent.run`, domain `AGENT_STEP_ACTION`) so `run_step.action` is truthful and publish/seed/executor
  read one value; a `computed` step stores its registered action; `gate` and `call` steps store none.
  Publish requires exactly that, the seed loader applies the same shape rule, and the executor
  dispatches on the step's KIND. Before this the validator forbade the action the seed wrote, which
  made every catalogue process un-republishable through the API.
- An **SOP** has `definition_of_ready`, `definition_of_done`, `acceptance_criteria`, `inspection`
  (`{kind: laya|eval|checker, threshold?, max_rejects?}`) and `outputs`.
- A **process** declares its **user-facing outputs** and is versioned; a run pins the process/SOP versions it used.
- Publish validation refuses: no steps; duplicate/non-contiguous positions; a computed/tool step whose action is
  not registered; an agent step with no `capability_agent(role='process_agent')`; a step with no published SOP; a
  call to an unpublished process or depth >3; a map node that does not resolve; a reordering of a system
  capability's process.

### 5.3 The run tree

`process_runs.start` accepts `parent_run_id` plus **the parent's own step kind** —
`step_id` (a `process_step`, for a process `call`) or `workflow_step_id` (a `run_step`, for a
workflow's procedure node, 0037); naming both or neither is a `422`. A child may start under a live
parent **on the same card**, depth ≤3; one live child per `(parent, step)` and one row per attempt;
cancel cascades; a child failure does not re-fire the parent's `err.card`. The root-only indexes are
the enforcement; the store's `live_run_for_card_tx` filters roots, and `next_attempt_for_card_tx`
counts per `(parent, step)`. **As built (2026-11-10):** the root filter is
`uq_run_live ... WHERE parent_run_id IS NULL`; a workflow parent's child is proven by
`call_step::a_workflow_procedure_node_starts_a_child_under_the_parent`.

### 5.4 The step executor

`process.step` drives an attempt: load the run and step, write `run_step`, execute by `kind` (`agent` →
`agent.run` with the resolved process agent; `computed`/tool → the action via the tool agent; `gate` → open the
approval and park; `call` → start the child and poll), then **judge the output**, append the outcome and enqueue
the next step. Retry appends an attempt; timeout is `error_class=timeout`; every write is idempotent on
`(run_id, position, attempt)`.

### 5.5 The judge

- `laya.judge` is an **external action** against `https://api.nonagentic.ai/laya`, authenticated with the
  conveyor's laya licence by a typed client — never a plain unauthenticated HTTP call. Request: the step output, the SOP's `acceptance_criteria` and the
  card's measured facts. Response: `{score, accepted, reason}` — the confidence margin is the signal, not the label.
- **Reject → rerun the same step with `reason` added** (at most 2 rejects), then the card goes **BLOCKED → coach**.
  **Outage → wait**; never fail open, never count as a reject.
- The verdict is written to `run_step.judge` (decision, score/margin, reason, criteria, output hash) so acceptance
  is auditable and replayable. An `eval` inspection uses S19's `eval_runs` and records the same shape.
- **As built (2026-11-10):** the judge is a step-executor seam over the registered `laya.judge` action and the
  `checker`/`eval` inspection kinds. A reject reruns the same position with the reason, bounded by the SOP's
  `max_rejects` (default **2**); an exhausted budget fails the run (→ `err.card` → the coach); an outage **parks**
  the step and re-enqueues it.

### 5.6 The process map

`process_map` is a versioned document bound to a project type; nodes `process|capability|phase|milestone`, edges
flow. It is **declarative** (no execution, no reordering). The API serves it (`process_maps.get/list`), the PM plans
from it, and the UI overlays the project's live position (running/done/blocked) from the runs. A prescribed
sequence of external API calls is a process whose steps are tool calls, not a second execution model.
**UI surface declared for S12:** screen 7 (`processes` — steps, SOP, judge verdicts, user-facing outputs) and
screen 22 (`process map` — the DAG with the live position).

### 5.7 The bus

Port the envelope `{id, topic, from, correlation_id, procedure, tenant_id, job, payload}`; topics
`work.requested|succeeded|failed`, `tool.call|response`, `dead.letter`; groups `process|tool`; `XADD`/`XREADGROUP`/
`XACK`/`XAUTOCLAIM`; `max_deliveries=5` → `bus:dead`. The relay publishes unpublished `event` rows by `seq`
(woken by `pg_notify`), records `bus_stream_id`/`published_at`, and **re-delivers after a Redis loss** because the
step's outcome in Postgres is the truth. `FLUSHALL` + replay must reproduce the same run state. **As built
(2026-11-10):** `crates/bus` (envelope, groups `process|tool`, the relay and the DLQ) is in the workspace, and the
`process.step` job is the **Redis-absent fallback** — a run still executes from Postgres with Redis down. CI starts
a dind Redis from the archetype's `redis_image`.

### 5.8 The computed set (code, not processes)

`projects.create` creates project + board + start card in one transaction and emits `card.created`; the start
card is `project_start_request_analysis` (intake). It starts a root run **only when the tenant has published a
start process** — there is no universal seed, because a tenant may run accounting, marketing or human project
management (owner ruling 2026-11-10). The move
step is `cards.move`. `wip.pull` reacts to `card.moved` and keeps `may_move` as the guard. `err.card` runs on
`run.failed`, moves the card to BLOCKED, comments and starts the coach root run. Approval routing is the gate
resume. The hook route and `CONVEYOR_HOOK_SECRET` are deleted.

### 5.9 The dispositions ledger

`parity/n8n-dispositions.tsv` is rewritten: the 280 rows target `process:<card_type>:<list>`, the 51 `cap.*`
rows target `process:cap.<c>.<function>`, `ask_fr`/`kata.open` target processes, and the four computed flows
(`conveyor.start`, `wip.pull`, `err.card`, the move step) are `absorbed:...`. **No `delta:` rows.** Every old node
has a mapping row; the count is 348. **As built (2026-11-10):** the rewrite landed. Measured on disk: 348 rows, 333 distinct
`process:` targets (280 type + 53 named), 3 `absorbed:` rows, 12 `n8n-retire`, no `delta:` row, and
every target resolves against the seed catalogue. Column 3 still carries the DAG-era grouping
(`matrix`/`cap`/`named`/`dead`/`kata`); it is a label, and the test asserts the target column, which
is the claim.

## 6. Build order

**Done (2026-11-10):**

1. ✅ Migration `0035_process_tree.sql` (renames, SOP, map, run tree, kinds, roles, judge, bus cursor, the
   `caller_kind` move) + migration `0036_s19_output_grade.sql` + the domain/store types + publish validation;
   `cargo test -p conveyor-store process_tree`.
2. ✅ The step executor and the run tree (Postgres only); the coach child end to end;
   `crates/actions/tests/process_step.rs` + `call_step.rs`, `crates/api/tests/project_start.rs` +
   `computed_set.rs` (the plan's `crates/api/tests/process_run.rs` was never created — corrected
   2026-11-10).
3. ✅ The judge (`checker`/`laya`/`eval`) + the reject/rerun/block ladder and the outage wait;
   `crates/actions/tests/judge.rs` with a stub laya (a real call is the live gate).
4. ✅ `projects.create` + the start card + the root run; `crates/api/tests/project_start.rs`.
5. ✅ `wip.pull` and `err.card` as computed handlers; `crates/api/tests/computed_set.rs`.
6. ✅ `crates/bus` + the relay + consumer groups + DLQ; compose Redis; `crates/actions/tests/bus_delivery.rs`.
7. ✅ The default catalogue: 70 type files / 280 processes / 350 SOPs, with each file's agents;
   `seed/processes/**`; `seed/processes/manifest.yaml` (the generated 280-entry index); the loader test applies the
   whole catalogue twice on a fresh DB.

**Session 3 (2026-11-10) — the contracts between the parts, and install completeness:**

16. ✅ **One step contract.** `validate_publish` requires `action = agent.run` on an `agent` step
    (forbids it elsewhere), the seed loader applies the same shape rule, the executor dispatches on
    the KIND, and a per-step `process_agent_id` wins over the run-level fallback (readiness checks
    every bound agent; the prompt facts fall back to the step's agent). A fork of a catalogue
    process now publishes (D29), and an API-authored agent step now runs.
17. ✅ **Gate and call steps are expressible in the seed** (`StepSpec.gate_kind`,
    `.child_process_key`) and used: `ask_fr` parks at `human_approval`; the kanban coach calls
    `kata.open`. The loader refuses a call to a target not yet published.
18. ✅ **The run lifecycle has one writer.** `runs.finish` refuses a process run (`409
    run.process_owned`); cancel/fail/reap cascade to descendants' steps; the reaper matches the
    job's dedupe key by prefix, so a dead park/gate/call job terminates its step.
19. ✅ **Lease renewal (audit V1).** A claimed job renews while its handler runs; a stale token can
    neither renew nor finish.
20. ✅ **The judge record is complete and projected**: criteria names + output hash on
    `run_step.judge`; `RunStepOut.judge`; `RunOut.process_step_id|workflow_step_id`; a timed-out
    action fails the run with `error_class = timeout`; an `eval` pin that names a missing eval is a
    publish refusal.
21. ✅ **Maps are authorable** (`process_maps.create|update|publish|retire`), so the domain map
    validator is reachable through the API.
22. ✅ **The bus replay property is proved** (`FLUSHALL` + replay on a real run) and the gate resume
    announces `run_step.requested`; parks and call polls deliberately do not (they carry a backoff).
    `compose.yaml` gains the `worker` service.
23. ✅ **Install is complete**: `migrate --process-catalogue` installs the catalogue vocabulary
    (card types, project types, gate codes) first, proved from a fresh database.

**Session 2 (2026-11-10) — executor defects, catalogue E2E, BYO tokens:**

8. ✅ **W4** — `agent.run` resolves a bound run from its `capability_agent` row **first**; a conflicting
   model, node-supplied tools, a substituted prompt/executor, or a **raised** cap is a typed `422`
   (`agent.config_conflict` / `agent.cap_exceeds_bound`); a lowered cap is accepted.
   `crates/actions/tests/agent_run_resolved.rs`.
9. ✅ **W5** — migration `0037_run_tree_workflow_step.sql`: a workflow's procedure node starts its
   child with `workflow_step_id` (its `run_step`), not a second root on the card.
   `crates/actions/tests/call_step.rs`.
10. ✅ **D29 readiness** — a process step's `process_agent_id` is checked directly; the "exactly one
    active executor" rule applies only to unbound capability runs. `crates/api/tests/capability_readiness.rs`.
11. ✅ **The catalogue end to end** — `process_seed::apply_all` creates **333** processes (280 card-type +
    53 named), 403 SOPs and 3 maps; `bug_analysis.READY` executes agent → judge → move on a real card.
    `crates/actions/tests/process_catalogue.rs`.
12. ✅ **Per-tenant upstream tokens** — `inference.token` is an *optional* bearer (absent ⇒ no bearer,
    never a platform token); a sealed secret is either a raw BYO value or the licences-minted envelope;
    `conveyor admin upstream-token --tenant … --service … --mint|--token-stdin` fills/rotates the row.
    `apps/server/tests/upstream_tokens.rs`.

**Outstanding:**

13. `seed/processes/workflows/` — the reusable tool sequences. **Not blocked, not shipped.** The
    earlier note here claimed no `coding_jobs`/`workspaces` action existed; measured 2026-11-10 that
    is false (`coding_jobs.create|get` and the board/requirements/documents/artifacts families are
    registered). What is missing is the authoring, and the owner audit of 2026-11-10 excludes
    re-authoring the old flows. The `call` step's workflow limb is proved by `call_step.rs`.
14. The historical n8n trace fixtures. **They are in this repo** (`fixtures/{a,b,c}.old.jsonl`,
    `*.workflow.json`, `executions.json`, `SHA256SUMS`, exercised by `crates/workflow/tests/`). The
    earlier "not in this repo" note was wrong; the gate over `run_step` (`process_trace.rs`) is the
    process-side trace, and the old recordings drive the workflow-side diff.

⚠️ The **bus test is CI-only in this environment**: it needs a Redis reachable from the build host,
which has neither docker nor a Redis and cannot reach mini3. It fails closed; it never skips.

⛔ **No longer an item:** migration `0034`'s drop of `workflow`, and the deletion of `crates/workflow`,
`workflows.*`/`workflow_runs.*` and `fixtures/named/**`. **The workflow engine is retained as the tool-sequence
executor** (owner ruling 2026-11-09); only the hook route and `CONVEYOR_HOOK_SECRET` are deleted.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16
docker compose up -d db redis && cargo run -p conveyor-server -- migrate --seed
CONVEYOR_DEV_AUTH=1 cargo run -p conveyor-server -- worker &
trap 'kill $(jobs -p) 2>/dev/null || true' EXIT
# Postgres-only limb: a 4-step process with a gate, a retry, a judged step and a coach child on the parent's card.
cargo test -p conveyor-actions --test process_step
cargo test -p conveyor-api --test project_start
cargo test -p conveyor-api --test computed_set
# The judge: accept, reject → rerun with the reason, two rejects → BLOCKED, outage waits (stub laya).
cargo test -p conveyor-actions --test judge
# The run tree: root live twice refused; a child allowed; depth 4 refused; cancel cascades.
cargo test -p conveyor-store process_tree
# The bus: duplicate delivery one outcome; a kill re-delivers; a poison message dead-letters.
cargo test -p conveyor-actions --test bus_delivery
# The default catalogue and the ledger: the manifest is 280 type processes; the import is 333
# (280 card-type + 53 named); every old node mapped; no delta rows.
test "$(yq -r '.processes | length' seed/processes/manifest.yaml)" -eq 280
! grep -q 'delta:' parity/n8n-dispositions.tsv
test "$(wc -l < parity/n8n-dispositions.tsv)" -eq 348
# The trace gate over run_step.
cargo test -p conveyor-actions --test n8n_migration
# Session 2: the executor defects, the catalogue end to end, the BYO token path.
cargo test -p conveyor-actions --test agent_run_resolved
cargo test -p conveyor-actions --test call_step
cargo test -p conveyor-actions --test process_catalogue
cargo test -p conveyor-api --test capability_readiness   # one test is ignored: it needs the real strata
cargo test -p conveyor-server --test upstream_tokens
```

Green when the process limb passes with the gate parked and resumed and the coach child proving the run tree; the
judge proves accept, reject-rerun, the two-reject BLOCK and the outage wait; the bus proves duplicate idempotency,
re-delivery after `FLUSHALL` and the DLQ; the default catalogue is exactly 280; the dispositions have no `delta:`
row; and with Redis stopped the Postgres-only limb still passes.

## 8. Traps

- **The bus is not the record.** If a handler writes to Redis-only state, a Redis restart loses it. Every outcome
  is a Postgres row in the handler's transaction; the envelope is a nudge.
- **A child must not take the root slot.** `live_run_for_card_tx` must filter `parent_run_id IS NULL`; otherwise
  `runs.create` 409s while a coach child runs.
- **A retry must not duplicate a side effect.** Idempotency is the envelope id **and** `(run_id, position,
  attempt)`; the action's own `Idempotency-Key` is `uuidv5(run_id:step:attempt)`.
- **A judge reject is not a retry storm.** The rerun budget is the SOP's `max_rejects` (default 2); an exhausted
  budget is a block, never an infinite loop. A laya outage is not a reject.
- **The map must not become the executor.** It is a document; an edit to it changes no run. A process's order is
  its step positions.
- **The DAG must not become the process executor.** `crates/workflow` is retained as the tool-sequence executor,
  and `process.step` never calls its graph planner: a `call` step resolves `child_process_key`
  process-first, workflow-second. Do not add a process step kind that runs a graph.
- **RLS FORCE.** Seed/backfill as owner sees zero rows; every new table/column path stays tenant-scoped.
- **The 280 are a catalogue.** The seed has 74 card-type codes and must filter to the 70 active pairs; a tenant's
  process is a row, not a code constant.

## 9. Decisions closed here

1. Storage: `process`/`process_step` (renamed) + `procedure` (SOP) + `process_map`; migrations
   `0035_process_tree.sql` and `0036_s19_output_grade.sql`. **The workflow engine is retained** as the
   tool-sequence executor (owner ruling 2026-11-09); there is no drop migration.
2. Step kinds `agent | computed | gate | call`; actor `process_agent | tool_agent | system | human`; no graph, no
   predicates, no loop nodes; `continue | fail | retry` bounded.
3. The run tree: root-only live index, one live child per `(parent, step)`, depth ≤3, cascade cancel.
4. The judge is the SOP's inspection: laya (external action), an eval or a checker; `{score, accepted, reason}`;
   reject → rerun with the reason, max 2, then BLOCKED → coach; outage waits.
5. The process map is a declarative DAG per project type; the UI renders it; nothing executes from it.
6. System capabilities (PM, coach, kata) are fixed; the rest is tenant-defined; the 280 are the default catalogue.
7. The bus is the care-bus port (Redis Streams); Postgres is the record; the outbox re-delivers.
8. No `delta:` rows: the four named flows are absorbed, and the two remaining are processes.

## 10. Open questions

1. Does a map node carry the process's live status from the runs, or does the UI derive it per card? **Default:**
   derive per card; one `process_maps.get` plus the existing run reads.
2. Laya's request shape for the SOP criteria is not in a local wire schema (the TypeSafe AI node is installed, the
   schema is not in this repo). The first live call pins it; until then the stub fixes the conveyor side.
3. Does a tenant-authored process need its own map node, or can it be reached from a capability node? **Default:**
   both; a map node refs a process or a capability.

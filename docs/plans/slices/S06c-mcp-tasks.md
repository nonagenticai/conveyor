---
slice: S06c
title: MCP tasks — a long-running project asks for input
status: built 2026-11-09
depends_on: [S04, S05, S06, S06b]
spec_sections: ["§4.5", "§6", "§8 S06c", "§11 R11"]
adds_tables: []
adds_actions: [projects.execute]
adds_events: []
adds_modules: [mcp]
parity_ids: PENDING-S0
---

# S06c — MCP tasks

Programme project: S06c · issues none yet

**BUILT 2026-11-09.** `projects.execute` (task-capable), the run ↔ task mapping in
`crates/actions/src/mcp/tasks.rs`, the three hooks in `mcp.rs`, `enable_tasks()` in `get_info`, and
migration `0023_s06c_mcp_tasks.sql` — which holds the slice's two indexes: `uq_run_project_live`
(`(tenant_id, project_id) WHERE card_id IS NULL AND status live`) and `approval_project_idx`
(`(tenant_id, project_id)`), the project-scoped approval listing this slice reads. No table, no new event.

Build records / deviations, all deliberate:
- **The task id IS the run id.** `projects.execute` is create-or-return and answers **200** on both paths
  (one representation); only the creating call enqueues `run.execute` and emits `run.created`.
- **`run.execute` needed no change**: it was already card-agnostic (queued → running, never terminal). The
  REAPER did: `store::run::reap_dead_runs_tx` now requires `r.card_id IS NOT NULL` (trap 10).
- **`tasks/update` invokes `approvals.decide`** — the gate's own state machine, events, run resume and the
  `(approval_id, approver_id)` unique index all apply. A repeated answer is a replay (`decision_by_tx`).
- **`on_behalf_of` is not resolved to a human**: George's 2026-11-09 ruling says the conveyor need not know
  whether an answer is human or agent, so the decision records the authenticated agent principal
  (`decided_by_kind='agent'`). The `_meta` is accepted and ignored.
- **A blank-reason rejection leaves the gate open** (proven); the slice's "sets `statusMessage`" is not
  persisted — there is no task table to persist it into, and the next `tasks/get` still reports
  `input_required`. Recorded as a deviation, not hidden.
- **One `-32602`** for non-uuid, unknown, cross-tenant, expired and card-scoped ids (trap 7).
- **Agent tools**: `projects.execute` joins `AGENT_TOOLS`, plus `runs.cancel`/`runs.finish` (a task must be
  cancellable and completable by the platform that runs it). ⚠️ The build-order lines' `cargo test -p
  conveyor-actions mcp_tasks` matches **0 tests** (it filters test NAMES, not binaries); the real target is
  `--test mcp_tasks_contract`. Tests: `crates/api/tests/mcp_tasks.rs` (the §7
  property table over the real transport), `crates/actions/tests/mcp_tasks_contract.rs` (the wire seed and
  the single task-capable action), `crates/store/tests/transitions.rs` (create-or-return + the index).

Adds the MCP tasks extension (SEP-2663) to the conveyor's existing `/mcp` surface. S06b gives the agent a licence
principal and a tool list; this slice gives it a durable task for a project that can run for days and stop to ask a
human a question. `mcp_server` keeps its own task runtime for coding/care-bus/builder; the conveyor serves only its
own lifecycle.

## 1. Objective

At the end, an MCP client that declares `io.modelcontextprotocol/tasks` calls `projects.execute`, receives
`resultType:"task"`, polls `tasks/get` through a project that waits on an approval, answers it with `tasks/update`,
and reads a terminal result — with no Python service in the path and no new table.

## 2. Starting state

- S04: `/mcp` (rmcp 3.9.0) serves the action registry; `call_tool` invokes the same engine as HTTP.
- S06b: licence bearer → `app_user(kind='agent')`; every tool carries `_meta.operation`; `approvals.request`.
- S06: `run`, `approval`, `approval_decision`; `runs.create|start|finish|cancel`, `approvals.list|decide`.

## 3. Scope

**IN** — `enable_tasks()`; `get_task`/`update_task`/`cancel_task` backed by Postgres; `projects.execute`; a project
execution run; run↔task and approval↔input-request mapping; idempotent answers; cancel; TTL; actor provenance; the
protocol tests.

**OUT** — the agent's client-side support for tasks on an **external** MCP server (separate plan: today its gate is
`auth_kind=none`, `agent/.../connectors/runtime.rs:743-757`); decommissioning `mcp_server`; SSE/push; a progress
percentage (none exists and none is added); any new table.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `rmcp-3.9.0/src/model/task.rs`, `src/model/mrtr.rs`, `src/handler/server.rs` | `Task`, `DetailedTask`, `TaskPayload`, `InputRequests`, `CallToolResponse::Task`, the three hooks, `validate_tasks_capability` | the types and the gating | the in-memory `TaskManager` (Postgres is the store) |
| `nonagenticai_mcp_server/src/tasks/{extension,mapping,models}.py` | the wire shapes, status map, `-32602` rule | the contract, exactly | its run store and driver |
| `nonagenticai_mcp_server/src/storage/task_inputs.py` | idempotent update, reject-needs-reason, tombstone | the lenient semantics | the SQL |
| `agent/crates/agent-api/src/mcp_client/tasks.rs` | the client's status set and `-32602`→`lost` | the client's expectations | the platform binding |
| `crates/actions/src/mcp.rs` | `ServerHandler for McpServer`, `call_tool`, `tool_json`, `service` | the existing composition | the polling channel |
| `crates/store/src/run.rs`, `crates/store/src/approval.rs`, `crates/actions/src/control/runs.rs` | run lifecycle, `uq_run_live`, `open_gate_tx`, `runs.cancel` | the state machine | a second lease |

Pinned: `nonagenticai_mcp_server` @ `dd2cabf`, `agent` @ `83b8de7`. Re-capture before building.

## 5. Deliverables

```
crates/actions/src/mcp.rs                   enable_tasks(); call_tool → CallToolResponse::Task for a
                                            task-capable action; get_task/update_task/cancel_task
crates/actions/src/mcp/tasks.rs             run ↔ Task/DetailedTask; approval ↔ InputRequest; actor; -32602
crates/actions/src/board/projects.rs        projects.execute
crates/actions/src/spec.rs                  ActionSpec.task_capable (default false)
crates/actions-derive/src/lib.rs            the `task = true` attribute
crates/store/src/run.rs                     create_project_execution_tx; the live-execution index
crates/store/src/approval.rs                decide idempotent per (approval, approver); void_project_pending_tx
apps/server/src/worker.rs                   run.execute accepts a card-less run and leaves it running
crates/api/tests/mcp_tasks.rs               protocol tests with a declaring client
crates/actions/tests/mcp_tasks_contract.rs  wire shapes and the negative rules
```

No tables. `projects.execute` is the only new action; there is no new event.

| method | request | response |
|---|---|---|
| `tools/call` `projects.execute` | `{project_id}` | `{resultType:"task", taskId:<run uuid>, status:"working", createdAt, lastUpdatedAt, ttlMs:604800000, pollIntervalMs:5000, statusMessage}` |
| `tasks/get` | `{taskId}` | base task fields + `inputRequests` (one per pending approval, key = approval uuid) or `result` |
| `tasks/update` | `{taskId, inputResponses:{<approval uuid>:{action:"accept"\|"decline"\|"cancel", content:{decision:"approved"\|"rejected", reason?, comment?}}}, _meta:{"ai.nonagentic/actor":…}}` | `{resultType:"complete"}` |
| `tasks/cancel` | `{taskId}` | `{resultType:"complete"}` |

Status map, in this order: a terminal run maps directly (`succeeded`→`completed`, `failed`→`completed` with
`isError:true`, `cancelled`→`cancelled`); otherwise a pending approval in the project → `input_required`; otherwise
`queued`/`running` → `working`. `failed` is never emitted as a task status.

Every `GetTaskResult` also carries `statusMessage` from the run and, in `_meta["ai.nonagentic/conveyor"]`, the
project id, the run id and the last events on `run:<id>` — the shape the agent's relay already reads for fr
projects, under the conveyor's own key. A terminal task carries the tool-shaped `result` instead:
`structuredContent {run_id, project_id, status, result, error}`, with `isError` true for `failed` and `cancelled`.

## 6. Build order

1. Baseline and compose. `source ~/.cargo/env && cargo test --workspace`; `lsof -nP -iTCP:8930 -sTCP:LISTEN` is
   empty; `docker compose up -d db`.
2. `ActionSpec.task_capable` + the derive attribute + the four manual `ActionSpec` literals.
   `cargo test -p conveyor-actions --test duplicate`
3. `create_project_execution_tx` + the partial unique index `(tenant_id, project_id) WHERE card_id IS NULL AND
   status IN ('queued','running','waiting_approval')`; `cargo test -p conveyor-store run`.
4. `projects.execute` (create-or-return the live execution run, enqueue `run.execute`).
   `cargo test -p conveyor-actions projects`.
5. `run.execute` accepts a card-less run and marks it `running`; it never completes it.
   `cargo test -p conveyor-server worker`.
6. `enable_tasks()` in `get_info`; the three hooks return the task from Postgres, not `-32601`.
   `cargo test -p conveyor-actions --test mcp_tasks_contract`
7. `tasks.rs`: run→`Task`/`DetailedTask`; `GetTaskResult`; the `-32602 task not found` rule for non-uuid, unknown,
   cross-tenant and expired ids. `cargo test -p conveyor-actions mcp_tasks`
8. `inputRequests` from pending approvals of the project; key = approval id; `requestedSchema` with `decision`
   enum `["approved","rejected"]` plus `reason`/`comment`. `cargo test -p conveyor-actions mcp_tasks input_requests`
9. `tasks/update` → `approvals.decide`; idempotent per `(approval, approver)`; unknown keys ignored; a reject with
   no non-blank reason leaves the approval pending and sets `statusMessage`.
   `cargo test -p conveyor-actions mcp_tasks update`
10. `tasks/cancel` → `runs.cancel` + `void_project_pending_tx`, so card-level gates are not orphaned.
    `cargo test -p conveyor-actions mcp_tasks cancel`
11. Actor: `on_behalf_of` that resolves to a human member writes `decided_by_kind='human'` with
    `via={channel:'agent', agent:<principal>, job}`; otherwise the agent principal, `decided_by_kind='agent'`.
    `cargo test -p conveyor-actions mcp_tasks actor`
12. TTL: 7 days from `run.created_at`; recomputed on read; expired → `-32602`.
    `cargo test -p conveyor-actions mcp_tasks ttl`
13. Protocol tests with a client that declares the extension at `initialize`; a second client that does not gets
    `-32021`. `cargo test -p conveyor-api --test mcp_tasks`
14. Wire-contract tests: task result shape, status map, `inputRequests`, terminal `structuredContent`, `isError`.
    `cargo test -p conveyor-actions --test mcp_tasks_contract`
15. Regenerate the served-spec lock (one new action).
    `cargo run -q -p conveyor-server -- --print-openapi --write-lock`
16. Gate (§7) and clippy.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env
cargo test -p conveyor-store run
cargo test -p conveyor-actions projects
cargo test -p conveyor-actions --test mcp_tasks_contract
cargo test -p conveyor-api --test mcp_tasks
cargo test -p conveyor-api --test approval_dual
cargo test -p conveyor-actions --test parity_http_mcp
cargo clippy --workspace --all-targets -- -D warnings
cargo run -q -p conveyor-server -- --print-openapi --check-lock
# no task-capable action beyond projects.execute
test "$(grep -c 'task_capable: true' crates/actions/src/board/projects.rs)" -eq 1
```

Property table:

| # | property | proof |
|---|---|---|
| a | extension advertised | `initialize` returns `io.modelcontextprotocol/tasks` in `capabilities.extensions` |
| b | start | `tools/call projects.execute` returns `resultType:"task"` with a uuid `taskId` |
| c | wait | a pending approval appears as `inputRequests`, keyed by the approval id |
| d | answer | `tasks/update` decides it; the next `tasks/get` is `working` |
| e | terminal | the run's terminal status maps as §5, with `isError` on failure |
| f | idempotent | the same update twice writes one `approval_decision` |
| g | reject rule | a reject with a blank reason leaves the input open |
| h | cancel | `tasks/cancel` voids the project's pending approvals |
| i | tenancy | a task id from another tenant is `-32602`, zero mutations |
| j | durability | a server restart mid-wait still answers `tasks/get` |
| k | no worker held | a one-hour wait leases no job row |

## 8. Traps

1. **Terminal precedence.** Check the run first; a stale pending approval must not revive a finished task.
2. **`failed` never on the wire.** A failed run is `completed` + `isError:true` — the agent maps that to failure.
3. **`uq_run_live` does not dedupe NULLs.** A project execution has `card_id IS NULL`, so the live-execution index
   must be its own partial unique index, not a check-then-insert.
4. **`runs.create` derives the project from the card.** `projects.execute` writes the run directly; do not route it
   through the card path.
5. **Approvals are project-, card- or run-scoped.** `inputRequests` lists by `project_id`, not `run_id`, or
   card-level gates are invisible.
6. **`open_gate_tx` needs a running run.** It is for the engine's gate, not for `approvals.request` (S06b) or a
   project task's input.
7. **`-32602` for every ownership failure.** Non-uuid, unknown, cross-tenant, expired — one code, one message, no
   oracle.
8. **The client must declare the extension.** rmcp refuses a task result to a client that did not; the test client
   must declare it at `initialize`, not just send the header.
9. **`tasks/update` is lenient.** Unknown keys and terminal replays ack; a strict 4xx breaks the agent's retry.
10. **The reaper must skip project executions.** `run.execute` reaps dead card runs to `crashed`; a project run is
    long-lived by design.
11. **`_meta` only.** rmcp flattens a task and drops unknown top-level fields; `ai.nonagentic/conveyor` goes in
    `_meta`, never beside `taskId`.
12. **The protocol version is `2026-07-28`.** A client negotiating an older version does not get the methods.
13. **`statusMessage` is not progress.** There is no percentage and none is added: the card list can be empty or
    reordered, so a computed percent would be a second, false state beside the run's.

## 9. Decisions closed here

1. **Task = project execution run.** The run state machine is already the task lifecycle; no table, no second store.
2. **Input = approval.** One primitive; the answer is `approvals.decide`, not a second response channel.
3. **`projects.execute` is the only task-capable action.** Everything else returns a normal tool result.
4. **No decommissioning of `mcp_server`.** It keeps its task runtime for other backends; the conveyor owns only its
   own project lifecycle.
5. **Polling remains the fallback.** S06b's path still works when the client cannot run tasks; the methods are
   additive.

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Does a human's relayed answer record as human or agent? | human when `on_behalf_of` resolves; `via` names the agent |
| 2 | One live execution per project, or several? | one; a second `projects.execute` returns the same task |
| 3 | Is `statusMessage` free text or a typed phase? | free text from the run, populated by S13 |
| 4 | Does the conveyor emit `ai.nonagentic/conveyor`? | yes; the agent adds the key in its own plan |
| 5 | Should `runs.list` gain a `project_id` filter? | yes, in this slice — the task's activity needs it |

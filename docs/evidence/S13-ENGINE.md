# S13 engine — session 1: the definition model and the pure engine

> **Public flip (2026-11-12):** the recorded `fixtures/{a,b,c}.old.jsonl`, `fixtures/SHA256SUMS` and the trace-diff fixture tests were removed from the public tree. The n8n flows never ran successfully and that proof is retired; the projection code and the authored flow/expectation files remain.

> ⛔ **SUPERSEDED 2026-11-10 by the process model.** This receipt records the DAG era. The
> `POST /api/hooks/{hook}` seam and `CONVEYOR_HOOK_SECRET` are deleted; `err.card` is a computed
> handler on `run.failed` (no `workflow.start` job); `conveyor.start` is `projects.create`. The
> workflow engine itself is RETAINED as the tool-sequence executor. Current receipt:
> `docs/evidence/S13-PROCESS.md`.

**Date:** 2026-11-09 12:55–13:10 CET · **Session:** `s13-engine` · **Tree:** `$HOME/Movies/conveyor` (no `.git`)
**Lease:** `s13-engine` opened 12:55 in `00-INDEX` §Tree lease, released with this receipt. **Split tree by
message** with `inference-ledger-prompt-audit` (S16) and `build-eval-instrument` (S19 Part A): this session edited
only `crates/workflow/**` + this file + the index row/cell. No `spec.rs`, `lib.rs`, `migrate.rs`,
`s07_schema.rs`, migration or `served-spec.lock.json` was touched — **189 actions / 210 ops unchanged**.

## Files

| path | lines | what |
|---|---|---|
| `crates/workflow/src/definition.rs` | 1142 | the one-document model (`schema/nodes/edges`, five node kinds, closed predicate set) and `validate_publish` |
| `crates/workflow/src/engine.rs` | 644 | the **pure** executor: `advance`, `succeeded`/`failed`/`gate_decided`, deterministic serial planning |
| `crates/workflow/src/lib.rs` | 23 | the crate's public surface |
| `crates/workflow/tests/definition_validate.rs` | 354 | 14 publish-rule tests |
| `crates/workflow/tests/engine_advance.rs` | 329 | 11 executor tests |
| `crates/workflow/Cargo.toml` | 14 | adds `serde`, `serde_json`, `thiserror`; still `workflow → domain` only (CONTRACT §1) |

## What was run (real output, `rbuild` on m1 — local `cargo` is disabled by design)

| # | command | result |
|---|---|---|
| 1 | `rbuild --doctor` | ok — m1, toolchain 1.99.0, `initdb` present, slot0 36 GiB / slot1 2 GiB |
| 2 | `rbuild -- test -p conveyor-workflow` | **25 passed, 0 failed** (14 `definition_validate` + 11 `engine_advance`) |
| 3 | `rbuild -- fmt -p conveyor-workflow -- --check` | **exit 0** |
| 4 | `rbuild -- clippy -p conveyor-workflow --all-targets -- -D warnings` | **exit 0.** The first run was blocked by **S16's** in-flight `crates/domain/src/prompt_audit.rs:487` (`unnecessary_map_or`); the peer fixed it on message, and the re-run then reached this crate and caught one real lint of mine — an unused binding in `require_branches` — fixed, re-run green. |

## Reversal proofs — each is the proof, not the green run

| # | guard removed / changed | test | observed |
|---|---|---|---|
| A | the join guard in `ready_nodes` (all inbound edges must be decided) | `fan_out_runs_in_document_order_and_the_join_waits_for_every_edge` | **FAILED**: `left: ["start","b","join","c"]` vs `right: ["start","b","c","join"]` |
| B | the "graph minus loops must be acyclic" guard | `a_cycle_without_a_loop_is_refused` | **FAILED** at the `expect_err` (the two-node cycle validated) |
| C | `MAX_LOOP_ITERATIONS` 1 000 → 1 001 | `the_iteration_cap_is_enforced` | **FAILED** (1 001 accepted) |
| D | the ready-node comparator reversed | `fan_out_…` (same test) | **FAILED**: `left: ["start","c","b","join"]` vs `right: ["start","b","c","join"]` |

All four were reverted; the suite is green after the reverts (#2, #3 above).

## Corrections this session made to its own plan (recorded, not hidden)

1. **"One loop per cycle" cannot mean "one loop node per SCC".** Nested loops put *every* loop of the nest in
   one SCC, so the literal reading forbids exactly the nesting §5.2 permits (cap 3). The implemented rule:
   **remove the loop nodes and the remaining graph must be acyclic** — every cycle passes through a loop, and
   any number of nested loops is legal. A loop's body is its **`each`-reachable region** (the engine resets that
   region each iteration), and `loop_nesting` counts loops by body containment. Proven by
   `nested_loops_publish_up_to_the_cap_and_no_deeper` (3 publishes, 4 is refused with `depth: 4`).
2. **A loop's entry edge and its return edge must be told apart by the body, not by the SCC.** `L1.each → L2`
   enters the inner loop; `L3.done → L2` returns to it. Both are "into a loop in the same SCC". Counting both as
   returns gave the nested document **three start nodes** (measured: `MultipleStarts { count: 3 }`). The rule
   now: an edge into a loop is a cycle-closing return **iff its source is inside that loop's own body**.
3. **Template validation is roots-only** (`validate_template(template, usize::MAX)`). A graph's execution order
   is not fixed at publish time — a loop repeats its body, a branch may skip one — so `steps.<n>` cannot be
   checked against a static position. S11's positional check stays where it belongs (`procedure_step`, fixed
   order). A genuinely forward reference fails **typed at run time**, not at publish. Recorded as a scoped
   deviation, not as a gap to "fix" by inventing a positional rule for graphs.

## What is **not** built — steps 1, 3–17, stated plainly

- **Step 1** migration `0019_s13_workflow.sql` (the `workflow` table, `run.workflow_id` + FK, the four
  `run_step` trace columns, the partial index) + one line each in `store/src/migrate.rs` and
  `store/tests/s07_schema.rs`. Both ledger files are S16's right now; `0019` stays this lane's per its message.
- **Step 3** `crates/store/src/workflow/{workflow,run_step}.rs` (+ the `lib.rs` line).
- **Step 4** the 12 actions, `crates/api/src/workflows.rs`, registry 189 → 201, served-spec 210 → 222.
- **Steps 6–9**: scheduler (`job(kind='workflow.node')`, `DEFAULT_SCHEDULES`, `prune.rs`), nodes, agent caps,
  trace + export + `admin trace-diff`.
- **Steps 10–15b**: record the three old traces **before S14** (n8n history is 3 days), re-author A/B/C + the
  4 named flows + `kata.open`, the 348-row `parity/n8n-dispositions.tsv`, and **retire the legacy flows**
  (`PATCH {"active":false}`, assert API `data: []` **and** DB `count(*) where active = 0`).
- **Steps 16a–16d** the editor spike (`spikes/editor/`, never a workspace member) and the fallback decision.
- **Step 17** the workspace gate.

The engine currently **plans**; it does not yet invoke, persist or trace. Nothing in this slice can be read as
an executable workflow: the store, the actions and the scheduler are the next slices of this lane.

---

# S13 engine — session 2: migration 0019, the workflow store, six actions

**Date:** 2026-11-09 14:06– · **Session:** `s13-engine` · **Tree:** `$HOME/Movies/conveyor` (no `.git`)
**Lease:** `s13-engine` (session 2) opened 14:06 in `00-INDEX` §Tree lease. ⚠️ **No queue — owner ruling
2026-11-09: sessions run in parallel.** Targeted edits only on shared files, re-read before each. S17 released its
pass at ~14:14 and regenerated the lock; **S19-B registered 7 eval actions mid-session**, which moved the
registry under this session — every count below is the measured final one, not the planned one.

## Files

| path | change |
|---|---|
| `migrations/0019_s13_workflow.sql` | **new** — `workflow` (one document, partial published-unique, 256 KiB CHECK, RLS enabled **and** forced), `run.workflow_id` + FK + partial index, `run_step`'s four trace columns |
| `crates/store/src/migrate.rs` | the `(19, "s13 workflow")` ledger line |
| `crates/store/tests/s07_schema.rs` | the applied-set pin gains 19 |
| `crates/store/src/lib.rs` | `pub mod workflow;` |
| `crates/store/src/workflow/{mod,workflow}.rs` | **new** — error-by-name mapping, the size guard, create/fork, get by id/key, keyset list, draft update (`If-Match`), publish (retire-then-freeze), retire |
| `crates/store/tests/workflow_publish.rs` | **new** — 5 tests (fork, publish/retire, If-Match, the CHECK, RLS) |
| `crates/actions/src/spec.rs` | `WorkflowsRead`/`WorkflowsWrite` (59 → 61), `as_str`, `granted_to`, the vocabulary pin |
| `crates/actions-derive/src/lib.rs` | the macro's compile-time scope table gains the two spellings |
| `crates/actions/src/lib.rs` | `pub mod workflow;` |
| `crates/actions/src/workflow/{mod,actions,views}.rs` | **new** — the six actions, four events, error mapping, cursor |
| `crates/actions/src/board/events.rs` | the four `workflow.*` events join `S05_EVENT_TYPES` (22 → 26) |
| `crates/actions/tests/registry.rs` | `EXPECTED` 230 → 237 (S19-B's 7 landed mid-session), split `(224, 13)`, doc arithmetic |
| `crates/actions/tests/workflows.rs` | **new** — 4 tests (create+event, If-Match, publish/retire, list/get) |
| `crates/actions/tests/parity_http_mcp.rs` | 6 `workflows.*` samples + `seed_workflow` + prepare arms |
| `served-spec.lock.json` | regenerated at **258 ops** (S17 wrote it; this session verified `--check-lock` exit 0) |

## What was run (real output, `rbuild` on m1)

| # | command | result |
|---|---|---|
| 1 | `rbuild -- test -p conveyor-store --lib --test s07_schema --test workflow_publish` | **exit 0** — 5 passed; the embedded set and the applied ledger both carry 19 |
| 2 | `rbuild -- test -p conveyor-actions --lib --test registry --test workflows` | **46 / 8 / 4 passed, 0 failed** |
| 3 | `rbuild -- clippy -p conveyor-actions -p conveyor-store -p conveyor-actions-derive --all-targets -- -D warnings` | **exit 0** (4 lints fixed: `module_inception` allowed by name, three `collapsible_if` → let-chains) |
| 4 | `rbuild -- fmt -p conveyor-actions -p conveyor-store -p conveyor-actions-derive -- --check` | **this lane's files clean**; S19-B's `eval/**` files still had diffs — theirs to format, not reformatted here |
| 5 | `rbuild -- run -q -p conveyor-server -- --print-openapi --check-lock` | **exit 0** — **258 ops** |
| 6 | `rbuild --gate` (fmt `--all --check` + clippy + `test --locked`) | **stops at `fmt --all --check`** on two peers' files (`apps/web/src/state.rs`, `crates/domain/src/eval/grade.rs`) — not this lane's files, and not reformatted here |
| 7 | `rbuild -- test --locked --no-fail-fast` (14:44 CET) | **24 binaries green, 1 red target — `parity_http_mcp`**, exactly S19 Part B's 7 missing samples |
| 8 | count pins updated for the new actions | `crates/api/src/lib.rs` 224 → **237**; `portfolio_roundtrip` scopes 59 → **61** (this lane's two); `apps/server/tests/prune.rs` gains `eval.run.tick`/`eval.reap` (S19 Part B's two schedule rows). Re-run: api lib **29**, portfolio **2**, prune **13**, all green |

## Reversal proofs — each is the proof, not the green run

| # | guard reversed | test | observed |
|---|---|---|---|
| A | publish's retire-previous step (`AND status='published'` → `AND false`) | `workflow_publish` | **FAILED 4/5** — the second publish hit `uq_workflow_published` instead of retiring the first |
| B | create's Rust size guard (`ensure_definition_fits(&definition)?` → ignored) | `workflow_publish` | **FAILED 4/5** — the DB CHECK refused the document and the store mapped it to `Invalid`, not `DefinitionTooLarge` |
| C | the `(19, "s13 workflow")` ledger line removed | store `--lib` (embedded set) | **FAILED 1/36**, naming the embedded set |

All three were restored; runs #1–#2 are green after the restores.

## Scope corrections made this session (recorded, not hidden)

1. **The plan said 12 actions; the slice's build order says 6 at step 4.** `workflows.*` is step 4;
   `workflow_runs.*` (with the scheduler) is step 6, `workflow_traces.export` step 9, `agent.run`/`inference.chat`
   step 8. Six landed. Registry 224 → **230** from this lane (+7 from S19-B = **237**), scopes 59 → **61**, lock
   245 → **258**.
2. **`crates/api/src/workflows.rs` is not written.** No route is specified in SPEC, CONTRACT or S12; reads are
   already served by `workflows.get`/`workflows.list`, and inventing a REST facade would be a second
   implementation of the same rules. It belongs to whichever slice needs it (the editor, S13 step 16, or S12).
3. **`store/src/workflow/run_step.rs` is not written.** It lands with the trace (steps 6/9); writing it now
   would be untested code with no caller.
4. **`--copy-back` cannot copy a single file back** — `rbuild` passes a trailing slash to rsync, which fails
   with "Not a directory" for a file. The lock was verified on the local tree instead (S17 wrote it; `--check-lock`
   green). The `rbuild` doc line that advertises a file path is wrong.

## The workspace gate — measured, with the one external red named

`rbuild --gate` runs `fmt --all --check` first and stops there: two **peers'** files are unformatted
(`apps/web/src/state.rs`, `crates/domain/src/eval/grade.rs`). They were left alone — this lane formats only its
own packages (run #4). The suite was then run directly (`test --locked --no-fail-fast`, 13:44Z): **24 binaries
green, one red target, `parity_http_mcp`**, which requires one sample per registered action. S19 Part B's seven
(`eval_runs.{start,get,list,cancel}`, `eval_results.{list,get,label}`) have none; their owner reported the two
fixtures those need (an approved, digest-matching eval with a locked split; a filled `eval_result` slot with a
`pending_label` row) cannot land this session. **That red is theirs and is attributable, not hidden**; this
lane's six samples are present and compile.

⚠️ The tree moved after that run: `s15-capability-aggregate` landed migrations `0020`/`0027` and both ledger lines
at 14:53 CET, and at 14:59 its in-flight `CatalogueStoreError::CapabilityMoved` left `conveyor-actions`
non-exhaustive at `crates/actions/src/governance/catalogues.rs:57` (reported to that session). The next lane
re-runs the suite; **this receipt does not claim a green workspace gate.**

## What is **not** built — steps 6–17

- **Step 6** scheduler on `job(kind='workflow.node')`, `workflow_runs.start|get|cancel`, `DEFAULT_SCHEDULES`,
  `prune.rs`.
- **Step 7** action/procedure/gate nodes, retry, timeout, idempotency at the executor.
- **Step 8** `agent.run` (internal, capped) + `inference.chat` (external, `captures_usage = true`).
- **Step 9** trace columns' writers, `workflow_traces.export`, `admin trace-diff`, `run_step` store.
- **Steps 10–15b** old-trace recording before S14, the A/B/C + named flows + `kata.open`, the 348-row
  `parity/n8n-dispositions.tsv`, and retiring the legacy flows (API `data: []` **and** DB `active` count 0).
- **Steps 16a–16d** editor spike + decision; **step 17** the final workspace gate.

## §Session 3 — S13 steps 6 and 7 (scheduler + node execution)

**Session:** `s13-engine` (session 3), 2026-11-09 15:16 → the release below. **Lease:** released with the gate
recorded here. The tree is shared and had three live peers (`s15-capability-aggregate`, `s15-capability-runtime`,
`build-web-ui-wave2`); every shared-file edit was targeted and re-read first.

### What landed

| path | change |
|---|---|
| `crates/workflow/src/engine.rs` | `RecordedStep`/`RecordedStatus`, `Engine::resume` (trace replay, `TraceDiverged`), `waiting_node`/`running_node` |
| `crates/workflow/src/scheduler.rs` | **new** — pure `plan` / `apply` / `apply_gate`, `Positions`, `NodeRequest`, `HostResult`, `GateState` |
| `crates/workflow/src/nodes.rs` | **new** — template input, closed predicate set, loop items, run-time timeout cap, `uuidv5(run_id:node_id:attempt)` |
| `crates/workflow/tests/workflow_scheduler.rs` | **new** — the step-6 gate (9 tests: replay, in-flight resume, retry appends, gate wait/resume, fan-out order) |
| `crates/workflow/tests/workflow_nodes.rs` | **new** — the step-7 gate (6 tests: idempotency key, timeout cap, predicates, loop items, control-node rows) |
| `crates/store/src/workflow/run.rs` | **new** — run context, `run_step` start/finish/list, next position, the tick job (`workflow.node`, dedupe `<run_id>:tick`) |
| `crates/store/src/approval.rs` | workflow-gate readers: `workflow_gate_tx`, `approved_workflow_gate_tx`, `workflow_gate_status_tx` |
| `crates/actions/src/workflow/node.rs` | **new** — the executor: rebuild from rows, plan, invoke actions as `Role::KataExecutor`, start/poll procedure children, gates |
| `crates/actions/src/workflow/runs.rs` | **new** — `workflow_runs.start|get|cancel` |
| `crates/actions/src/control/approvals.rs` | the `kind == "workflow"` decide arm (approved → resume + tick; rejected → refused row + cancel) |
| `crates/actions/src/control/runs.rs` | `void_run_approvals` extracted; both cancels void pending gates |
| `crates/actions/src/procedure/step.rs` | `gate_requirements`/`GateRequirements` → `pub(crate)` (a workflow gate uses S11's catalogue rules, not a copy) |
| `apps/server/src/worker.rs` | the `workflow.node` arm + handler; `approval.expire` records a workflow gate's row before the generic cancel |
| `crates/actions/tests/registry.rs` | `EXPECTED` 237 → **240**; the split `(227 internal, 13 external)`; scopes unchanged by me (S15's 61→63) |
| `crates/actions/tests/workflow_runs.rs` | **new** — 4 tests, incl. the end-to-end run |
| `crates/actions/tests/parity_http_mcp.rs` | 3 samples + 2 harness fixes (below) |
| `served-spec.lock.json` | **261 operations**, sha256 `f29b050d53a9c0c3…`, `--check-lock` OK |

### What was run

| # | command | result |
|---|---|---|
| 1 | `test -p conveyor-workflow` | **40 passed** (`definition_validate` 14, `engine_advance` 11, `workflow_nodes` 6, `workflow_scheduler` 9) |
| 2 | `test -p conveyor-store --test s07_schema --test workflow_publish` | **3 + 5 passed** |
| 3 | `test -p conveyor-actions --test registry --test workflows --test workflow_runs` | **8 + 4 + 4 passed** |
| 4 | `test -p conveyor-actions --test parity_http_mcp` | **4 passed** — every registered action, both transports (was the tree-wide red) |
| 5 | `test -p conveyor-api --test approval_dual --test mcp_agent` | **4 + 2 passed** |
| 6 | `test -p conveyor-server --test approval_expire --test procedure_gate --test procedure_step --test procedure_reap --test jobs_kill` | **1+1+1+3+2 passed** |
| 7 | `clippy -p conveyor-workflow -p conveyor-actions --all-targets -- -D warnings` | **clean** |
| 8 | `fmt --check` on my files | formatted through the pinned toolchain; peers' unformatted files left alone |
| 9 | `--print-openapi --check-lock` | **lock OK — 261 operations** |

### Reversal proofs (each fired, then was restored)

1. `Engine::resume`'s planned-step assertion disabled → `resume_refuses_a_trace_that_is_not_the_planned_prefix`
   and `a_retry_appends_a_row_at_the_same_position_with_a_new_attempt` both fail (`NotRunning { node: "b" }`).
2. `nodes::idempotency_key` ignoring `attempt` → `the_idempotency_key_is_per_attempt_and_stable_across_a_replay` fails.
3. The run-status guard in `execute_node` removed → `cancel_stops_the_tick_without_advancing_the_run` fails.

### Corrections and findings while building

- **The lock is written on the build host.** `rbuild` syncs mini3 → m1 before every run, so a written lock is
  overwritten by the next `rbuild` call. Write, then copy back **immediately** (`scp m1:.../src/served-spec.lock.json`).
  `rbuild --copy-back <file>` treats the path as a directory and copies nothing.
- **`parity_http_mcp`: `workflows.get` was marked `deterministic`.** A deterministic sample builds its input twice
  (once per transport) from a one-row prepared queue → `workflows.get was not prepared before its input was built`.
  Dropped the flag. (This was the red S19-B attributed to S13.)
- **`parity_http_mcp`: the andon pair netted to ENGAGED.** The samples were declared release-then-engage and each
  prepare sets the opposite state, so after the pair the cord stayed pulled and every later run-starting sample
  (`workflow_runs.start`) got `409 andon-engaged`. Reordered engage → release; the pair nets to released.
- Peer reds seen and **not** fixed (recorded, attributable): `apps/server/src/eval_runner.rs:207` (`collapsible_if`,
  S19), `crates/store/tests/capability.rs:366` (`explicit_auto_deref`, S15), and the `fmt --check` reds in
  `apps/web/src/state.rs` + `crates/domain/src/eval/grade.rs` (S12/S19). `clippy --all-targets` over the whole
  workspace therefore cannot be green from this lane alone.

### Honest limits (scoped deviations, not softened)

- **The first `run_step` row is written by the first tick, not by `start`.** S13 §5.3 says `start` "creates the
  start `run_step`"; inserting it as `running` and later flipping it is an UPDATE, and the trace model appends one
  row per attempt. Start creates the run and its tick; the tick writes the row when it executes the node.
- **Gate rows:** an approved gate is recorded on the next tick after `approvals.decide` settles the run; a
  rejected or expired gate is recorded by the decide/expire path before the run is cancelled.
- **Procedure nodes need a card-scoped parent run** (`procedure_runs.start` requires `card_id`); a card-less
  workflow run fails that node typed (`workflow.nesting`/procedure refusal), never silently.
- **Step 8** (`agent.run`, `inference.chat` + caps) was built in session 4 (below). **Step 9** (trace export,
  `admin trace-diff`) is not built. Steps 9–17 remain.

---

## Session 4 — S13 step 8: `agent.run` + `inference.chat` + caps

**Date:** 2026-11-09 16:00– · **Session:** `s13-engine` session 4 (`scheduler-node-execution`) · **Tree:** same,
no `.git` · **Lease:** opened 16:00, released with the gate below. Own files only: `crates/workflow/**`,
`crates/actions/src/workflow/{agent,node,runs}.rs`, `crates/actions/src/integrations/inference.rs`,
`crates/store/src/workflow/run.rs`, the two new test binaries, and the two parity samples
(`s15-capability-crud-actions` handed the parity file over by message; the lock is their write).

### What was built

| file | change |
|---|---|
| `crates/workflow/src/agent.rs` | **new** — the pure capped loop: `Agent::start` / `Agent::apply` over `Plan::{CallModel, CallTool, Finished, Failed}`; 8/20 turns, 20/50 tool calls, 300 s, depth 0; typed `agent.turn_limit` / `agent.tool_limit` / `agent.timeout` / `agent.cap_invalid` / `agent.depth`; a cap is **refused, never clamped**, and the last allowed turn's tools are not executed |
| `crates/workflow/src/{lib,definition}.rs` | module export; the publish rule `DirectModelCall` — a workflow `action` node may not name `inference.chat`, the route is `agent.run` (S13 §8 trap "a second loop beside `agent.run`") |
| `crates/actions/src/integrations/inference.rs` | `inference.chat` declared beside `completions.create`: same router path, `captures_usage = true`, `model:write`, write effect, 60 s |
| `crates/actions/src/workflow/agent.rs` | **new** — `agent.run`'s declared action (its handler refuses a direct invocation: an internal action holds the invocation transaction and has no pool, so the loop cannot call `invoke` from a handler); the step-executor seam `resolve` (explicit input, or the run's `capability_agent` → `{model, allowed_actions→tools, max_turns, timeout_ms, prompt_ref}`) and `drive` (each model call and tool call through `invoke`, one `action_invocation` per call on `run:{run_id}`) |
| `crates/actions/src/workflow/{node,runs}.rs` | `workflow.node` special-cases `agent.run` and drives the loop; `workflow_runs.start` takes an optional `capability_agent_id` into `NewRun`; `WorkflowRunContext` carries the column |
| `crates/store/src/workflow/run.rs` | `capability_agent_id` in the run context read |
| `crates/actions/tests/{agent_run,agent_run_resolved}.rs` | **new** — the end-to-end proofs (two binaries because `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global) |

### What was run (all measured this session)

| # | command | result |
|---|---|---|
| 1 | `test -p conveyor-workflow --test workflow_agent` | **10 passed** |
| 2 | `test -p conveyor-workflow --test definition_validate` | **15 passed** |
| 3 | `test -p conveyor-actions --test agent_run` | **1 passed** — two model calls + one tool call, all three `action_invocation` rows on `run:{id}` |
| 4 | `test -p conveyor-actions --test agent_run_resolved` | **1 passed** — the run records its executor; the model comes from the row (`resolved-model`), not the node input |
| 5 | `test -p conveyor-actions --test workflow_runs` | **4 passed** (no regression from the executor branch) |
| 6 | `test -p conveyor-actions --test registry` | **8/8**, `EXPECTED` 259 (a peer folded this lane's two actions in during the shared pass) |
| 7 | `test -p conveyor-actions --test parity_http_mcp` | **4/4** — this lane's two samples plus the RBAC seven, once both landed |
| 8 | `test -p conveyor-actions --lib` | **61 passed** (incl. the four new seam tests) |
| 9 | `test -p conveyor-store --test workflow_publish --test s07_schema` | **5 + 3 passed** |
| 10 | `clippy -p conveyor-workflow -p conveyor-actions --all-targets -- -D warnings` | **exit 0**; `-p conveyor-store --lib` exit 0 (the `--all-targets` store red was a peer's `capability.rs:366`, reported and fixed by them) |
| 11 | `cargo fmt -p conveyor-workflow -p conveyor-actions -p conveyor-store -- --check` | **exit 0** |
| 12 | `run -p conveyor-server -- --print-openapi --check-lock` | **RED, one op, not this lane**: `changed: POST /api/actions/capabilities.bind_knowledge/invocations` (S15's action; handed over). Lock on disk is their write: 259 actions · 63 scopes · **280 ops**, sha `6c6906c4…` |

### Reversal proofs (each fired, then was restored)

| fix | reversal | observed |
|---|---|---|
| turn cap | the `turns >= max_turns` branch disabled | `the_turn_cap_fails_typed_and_does_not_run_the_last_tools` failed: `expected Failed, got CallTool(call-2)` |
| cap ceiling | `MAX_TURNS` 20 → 21 | `a_config_outside_the_ceiling_is_refused_never_clamped` failed: a 21-turn config started |
| publish guard | `DirectModelCall` arm disabled | `a_step_cannot_reach_a_model_without_the_agent_loop` failed: the `inference.chat` node published |
| run config | `capability_agent_id: None` restored in `workflow_runs.start` | `the_run_declares_its_executor…` failed: `the run records its executor — left: None` |
| trace proof | the tool invocation's `trace_id` dropped | `an_agent_node_runs_the_capped_loop…` failed: `["inference.chat","inference.chat"]` where three actions were expected |

### Decisions and deviations, stated plainly

- **The action's handler refuses; the executor drives the loop.** [`HandlerCtx`] carries no pool and an internal
  action already holds the invocation transaction, so a handler cannot `invoke` the model or a tool. Every
  executor in this codebase (`procedure.step`, `workflow.node`, `/mcp`) is the layer that invokes outside a
  transaction, so `workflow.node` special-cases `agent.run`. Consequence recorded, not hidden: **S11's
  `procedure.step` does not special-case it yet**, so a procedure step that names `agent.run` reaches the
  refusing handler (typed 409) until S11 adopts the same seam. Workflow nodes — S13's surface — are complete.
- **The renderer is this lane's, per CONTRACT §17** (the step executor resolves). S15 step 5 only reads
  `store::capability`; `s15-capability-crud-actions` confirmed no writer collision.
- **The built-in prompt catalogue does not exist.** A resolved executor with `prompt_ref` set and no system
  message in the node input is refused `409 capability_not_ready` / `prompt_unresolved` — fail-closed, rather
  than calling the model without the configured prompt. The explicit-input path is the whole gate (S13 §5.3
  "Sequencing"); a node can supply its system message today.
- **Idempotency:** model and tool calls are keyed under the node attempt's key (`…:turnN` / `…:toolN`), so a
  node retry replays stored responses instead of buying a second round of effects.
- **Tool allowlist:** the resolved `tools` array is the allowlist; a call the model names but the executor did not
  declare is refused before execution as `action-not-found` and fed back to the model — never audited after the
  fact.
- ⚠️ **Boundary slip, recorded:** `cargo fmt -p conveyor-store` also rewrote the peer-owned
  `crates/store/src/seed.rs` (it was already fmt-red on the shared tree). Formatting only, no semantic change;
  `web-ui-wave-2-rbac` owns that file and was told.
- **Not fixed, handed over:** `crates/api/tests/machine_login.rs` — two licence-tenant assertions return `Null`
  (`:189`, `:223`); identity lane, reported to `web-ui-wave-2-rbac`. And the one lock op above, to S15.

### Post-release re-measurement (16:45, both hands resolved)

- **`--check-lock` is exit 0 at 280 ops, sha `6c6906c4…`** — the one-op drift was S15 running its own reversal
  proof (a temporary scope change on `capabilities.bind_knowledge`) inside my sync window, not a bad lock.
- **`machine_login` is 6/6** — the two failures were web-ui-wave-2-rbac's `/api/me` returning
  `active_tenant: null` unconditionally in its tenant-switcher bootstrap; its owner fixed it.
- Final snapshot of this lane's own binaries after both: `workflow_agent` **10/10**, `definition_validate`
  **15/15**, `agent_run` **1/1**, `agent_run_resolved` **1/1**, `registry` **8/8**, `parity_http_mcp` **4/4**.

## §Session 5 — S13 step 9 (trace export + `admin trace-diff`)

**Session:** `trace-diff-export`, 2026-11-09 16:24 → the release below. **Lease:** released with the state
recorded here. The tree was shared with three live lanes (`eval-hillclimb-loop`, `web-ui-wave-2-rbac`,
`s15-capability-crud-actions`); step 8's peer had just released `node.rs`/`run.rs`, so its files were free.

### What landed

| path | change |
|---|---|
| `crates/workflow/src/trace.rs` | **new** — `TraceRecord`/`TraceKind`/`TraceOutcome`, `mask_value` (UUIDs, RFC 3339, keyed ids, `usage`), `hash_value` (`sha256:` + `content_hash(masked)`), `project_effect` (`true`/`false`, `each`/`done`, action name, `procedure:<key>`), `effects` (last attempt per position), `parse_jsonl`/`to_jsonl`, `diff_records`, `DiffReport::exit_code` |
| `crates/workflow/tests/{workflow_trace,trace_diff}.rs` | **new** — 7 + 7 pure tests |
| `crates/store/src/workflow/run_step.rs` | **new** — `RunTraceRow`, `RunTrace`, `run_trace_tx` (run→workflow join, `node_id IS NOT NULL`, ordered) |
| `crates/store/tests/workflow_trace.rs` | **new** — 2 tests: ordering/filter and two-tenant RLS |
| `crates/actions/src/workflow/trace.rs` | **new** — `workflow_traces.export` (`{run_id}` → `{jsonl}`), kind/effect projection from the frozen definition, stored-hash fallback |
| `crates/actions/tests/workflow_trace_export.rs` | **new** — 4 tests over the real registry (order/effects, masked-id invariance, live row, 404) |
| `apps/server/src/admin/{mod,trace_diff}.rs` + `lib.rs` + `main.rs` | **new** — pure `admin trace-diff --old --new`, exit 0/1/2, before any DB config |
| `crates/store/src/workflow/run.rs` | `NewWorkflowStep.input_hash`; `start_step_tx` writes it (and refreshes it on the conflict update) |
| `crates/actions/src/workflow/node.rs` | all three start sites pass the masked `input_hash`; the two output-hash sites now use `hash_value` (masked) |
| `crates/actions/tests/{registry,parity_http_mcp}.rs`, `crates/api/src/lib.rs` | registration: `workflow_traces.export` in `EXPECTED` (260), api count 260, one parity sample with `seed_workflow_trace` |
| `fixtures/trace-diff/{identical,mutated,bad}.jsonl` | the CLI proof files |

### What was run

| # | command | result |
|---|---|---|
| 1 | `rbuild -- test -p conveyor-workflow --test workflow_trace --test trace_diff` | **7 + 7 passed** |
| 2 | `rbuild -- test -p conveyor-store --test workflow_trace` | **2 passed** (ordering + two-tenant RLS) |
| 3 | `rbuild -- test -p conveyor-actions --test workflow_trace_export` | **4 passed** |
| 4 | `rbuild -- clippy -p conveyor-workflow -p conveyor-store --all-targets -- -D warnings` | **clean** |
| 5 | `cargo fmt --check` on this lane's files | **clean** |
| 6 | `rbuild -- run -p conveyor-server -- admin trace-diff --old … --new …` | identical **0**, one mutated hash **1**, n8n envelope **2** |
| 7 | `rbuild -- test -p conveyor-actions --test registry` | **8/8 passed** at `EXPECTED` **266** (S20's six registered; `workflow_traces.export` in the list) |
| 8 | `rbuild -- test -p conveyor-actions --test parity_http_mcp` | **4/4 passed** at **266 actions** — this lane's `workflow_traces.export` sample executes and matches over HTTP and MCP |
| 9 | `rbuild -- run -p conveyor-server -- --print-openapi --write-lock` | **wrote 287 operations** from a compiling tree; copied back to `served-spec.lock.json`, sha `47550345fe643d35…`; **`--check-lock` exit 0** after `s13-agent-inference-nodes` fixed its in-flight `prompt.rs` (a field named `source` collided with thiserror's cause; renamed to `kind`) |

### Reversal proofs (each fired, then was restored)

1. `hash_value` bypassing `mask_value` → `hashes_ignore_masked_values_but_not_real_ones` failed on two
   different UUIDs hashing differently.
2. `effects` keeping the first attempt (`or_insert`) instead of the last → the retry test saw `Failed`
   instead of `Succeeded`.
3. `diff_records` dropping the `output_hash` comparison → `a_mutated_output_hash_is_a_mismatch` failed.

### Corrections and findings while building

- **The plan named both binaries and both exist.** §6 step 9 says `workflow_trace`, §7's gate says
  `trace_diff`; both were written, so either command is meaningful.
- **Masking is the hash, not a display concern.** `usage` joins the mask list because an `agent.run`
  output's token counts vary between two runs of the same work (S13 step 8's owner, 2026-11-09); the
  execution trace and the diff would otherwise never agree on an agent node.
- **`TraceOutcome::Running` is a scoped deviation from §5.4's `succeeded|failed|skipped`.** A live run is
  exportable (the row is real) but `effects` refuses it as `Incomplete`, so a partial trace can never be
  diffed as if it were finished.
- **Input hashes are pinned at write time.** `start_step_tx` now stores the masked hash; the export falls
  back to hashing stored values for rows written before this change, so old traces stay comparable.
- **The registration and the lock are in place.** Live registry **266**, `EXPECTED` **266**, `registry`
  **8/8**, `parity` **4/4** (my export sample included), api lib **29/29** at 266, clippy clean across
  actions/api/workflow/store. S20 pushed the api count and its clippy fix, then handed the lock over; it was
  written from a compiling tree at **287 ops**, sha `47550345…`, and copied back immediately. The
  re-verification first hit `s13-agent-inference-nodes`' in-flight `prompt.rs:190` (`E0599`) and then
  passed — **`--check-lock` exit 0** — once it renamed the field (`source` is thiserror's cause). Its
  prompt work adds no actions or scopes, so 287 ops stays correct and it will not regenerate the lock.
- **Clippy is clean across actions/api/workflow/store after S20's `eval_final.rs` fix** (its `unused_mut`
  was the only remaining red).
- **Not run:** the full gate rehearsal (`gate5` live run, export over HTTP, n8n stopped). It needs the
  tree's workspace gate to be green first; that is the lane's next gate after S20 settles.

### Lane landing at release (recorded, because it is not obvious)

The shared checkout is on branch `lane/s20-eval-climb` (origin `2beb107`, local +6) with **115 uncommitted
files** from several lanes; `main` is `e58e1ff`, origin `lane/bootstrap` and `lane/s20-eval-climb` share
`2beb107`, and the only open PR is **#1** (bootstrap, red on the image build because the snapshot did not
compile the server). **There is no S13 branch, and this slice's files are untracked/uncommitted by design**
(the approved plan: the S13 lane's one branch/PR belongs to the last settling S13 session, and a branch cut
from `main` would not compile). Step 8's files are in the same state. `s13-agent-inference-nodes` declined
the PR; the landing step must base on a tree where the prerequisite lanes are committed, not on `main`.

## §Session 6 — step 8's two open items: the S11 seam and the compiled prompt (D27)

**Owner decisions this session (George, 2026-11-09):** the S11 issue gets a design *and* an implementation;
the prompt catalogue is built now as the full three-source compiler, **in Postgres**, and **all three prompt
surfaces** resolve through it; the 2026-2027 design goes on the spec as a preliminary plan (PDDL evaluated,
not prototyped); the `cargo fmt -p conveyor-store` slip is codified and recorded.

### What was built

| path | change |
|---|---|
| `crates/actions/src/workflow/agent.rs` | **+** `run_step` — the one resolve+drive seam both step executors share; the resolved path now compiles the system message from the catalogue (`agent.prompt_conflict` if the input also carries one) |
| `crates/actions/src/workflow/node.rs` | `run_agent` becomes a call to that seam (same behaviour, no second loop driver) |
| `crates/actions/src/procedure/step.rs` | **+** the `AGENT_ACTION` branch: a procedure step naming `agent.run` resolves the run's `capability_agent` and drives the loop (5xx retryable, typed 4xx a failed step so `on_error` applies) |
| `migrations/0030_prompt_catalogue.sql` | **new** `prompt` + `prompt_section` (RLS ENABLE+FORCE, one active version per key, `system` requires text) |
| `crates/domain/src/prompt.rs` | **new** the pure compiler: five source kinds, required-missing refusal, SHA-256 of the compiled text |
| `crates/store/src/prompt.rs` | **new** recipe reads by exact pin + the run/conversation fact readers |
| `crates/store/src/{seed.rs,prompts/**}` | **new** `prompts_for_tenant` (`copilot_system@1`, `prompt_audit@1`, `agent@1`), idempotent and additive; the prompt texts move here so they have one home |
| `crates/actions/src/prompt/mod.rs` | **new** `compile_for_run{,_tx}`, `compile_for_copilot`, `compile_for_audit`; every refusal names the ref and the reason |
| `crates/api/src/copilot/turns.rs` | the system message is `compile_for_copilot(…, "copilot_system@1")`, not a constant |
| `apps/server/src/prompt_audit_run.rs` | the audit instruction is `compile_for_audit(…, "prompt_audit@1")`; a missing/retired row fails the audit, a DB error retries |
| `crates/actions/tests/{procedure_agent,procedure_agent_resolved,prompt_compile}.rs`, `crates/store/tests/prompt_catalogue.rs` | **new** 7 tests across four binaries |
| `docs/{SPEC.md,plans/01-CONTRACT.md,plans/02-DATA-ARCHITECTURE.md,plans/00-INDEX.md}`, `docs/plans/slices/S21-message-construction.md` | D27 + the S21 design (PDDL evaluated against the step DSL, not adopted), §17 amended, §2.21 added, S13/S21 rows |

No new action, no new scope: the registry stays **266** and the lock **287 ops** (sha `47550345…`).

### What was run (real output, `rbuild` on m1)

| # | command | result |
|---|---|---|
| 1 | `test -p conveyor-domain --lib` | **209 passed** |
| 2 | `test -p conveyor-store --test prompt_catalogue` | **3 passed** |
| 3 | `test -p conveyor-store --test s07_schema` | **3 passed** — ledger through **30** |
| 4 | `test -p conveyor-actions --test procedure_agent --test procedure_agent_resolved` | **1 + 1 passed** |
| 5 | `test -p conveyor-actions --test prompt_compile` | **2 passed** |
| 6 | `test -p conveyor-actions --test registry --test parity_http_mcp --test agent_run --test agent_run_resolved --test procedure_step --lib` | **8 + 4 + 1 + 1 + 14 + 61 passed** |
| 7 | `test -p conveyor-api` (11 binaries: copilot_turn/rounds/crash_replay/approve/proposal/replay/reap/session/surface, agui_parity, mcp_agent) | **13 passed** |
| 8 | `test -p conveyor-server --test prompt_audit_run` | **2 passed** |
| 9 | `clippy -p conveyor-domain -p conveyor-store -p conveyor-actions -p conveyor-api --all-targets -- -D warnings` | **exit 0** |
| 10 | `rustfmt --check` on this lane's 10 touched files | **exit 0** |
| 11 | `run -p conveyor-server -- --print-openapi --check-lock` | **RED, not this lane** — `capabilities.get` drifted (S15 step 12, in flight); the lock file itself is 287 ops, sha `47550345…`, unchanged by this lane |

⚠️ `clippy -p conveyor-server` is red on S19's `apps/server/tests/eval_support/mod.rs` (three dead-code
errors under `-D warnings`) — pre-existing and untouched here.

### Reversal proofs — each is the proof, not the green run

1. **The S11 seam.** Flipping the branch to `false && row.action == AGENT_ACTION` makes
   `procedure_agent` fail with the handler's own 409: *"agent.run executes inside a run's step, driven by its
   executor…"*. Restored; the test passes only with the seam.
2. **The refusal is real.** `prompt_audit_run` first failed with
   `prompt "prompt_audit@1" is not in this tenant's catalogue` before its fixture seeded the built-ins —
   the name is produced by the resolver, not by the test.
3. **One active version is the database's rule.** The store test asserts the *constraint name*
   (`uq_prompt_active`), not just "the second insert failed".
4. **The RLS test was wrong the first time, and the reversal exposed it.** It used `db.super_pool` and the
   forged write was accepted — the superuser is the positive control, and a test that runs as it proves
   nothing. Re-run as `db.runtime`, the forged `tenant_id` is refused.
5. **The compile is live.** Renaming the card changes the compiled text and its SHA-256, so the recorded
   `prompt_sha256` is a provenance key rather than a snapshot of a template.

### The slip (owner decision 3)

`cargo fmt -p conveyor-store` in session 4 reformatted the peer-owned `store/src/seed.rs`. Recorded rather
than reverted: the file also carries 241 uncommitted lines of its owner's content, so a revert would destroy
their work, and the formatting is what the repo gate requires. **Rule, now part of the lane protocol: a
formatter runs on named files you own — never `--all` or `-p` on a shared tree. If a peer file was
formatted, record it, tell the owner, and do not revert.** This session formatted only its own files
(`rustfmt <paths>`, checked over exactly that set).

### One correction to this session's own work

The first `PromptCompileError::MissingRequired` named its field `source`, which thiserror reads as the
error's **cause** and requires to implement `std::error::Error` — `conveyor-domain` stopped compiling and
blocked every peer for a few minutes. Renamed to `kind`, with a comment so it does not regress. Reported to
`trace-diff-export` as soon as `cargo check -p conveyor-domain` was clean, because its `--check-lock` was
waiting on it.

### Not built

- **S21** is a design + preliminary plan only: selection/budgeting, the role section, the `plan` (PDDL-ish)
  projection and tenant-authored recipes are its steps.
- The S13 **steps 10–17** remain (live-trace recording, the 348-flow re-authoring, the editor, the final
  workspace gate).
- **The lane PR**: still none. The tree is uncommitted across several lanes; the S13 branch is the last
  settling S13 session's, per the approved plan.

### Addendum 17:4x — the two "not this lane" reds, re-measured

`capability-readiness-guard` reported S15 step 12 **closed and released**, consuming `get_prompt_tx`
unchanged (`None` and a retired row both → `prompt_unresolved`; a store error → 500). ⚠️ Its output-schema
change moved the **live** document, not the file — and that is the drift: `served-spec.lock.json` is still
**287** ops, sha `47550345fe643d35…`, with `capabilities.get`'s stored hash still `070d0c0a…`, so
`--check-lock` is exit-1 (`changed: POST /api/actions/capabilities.get/invocations`) **because the lock was
not rewritten**. It needs a **write**, and S20 owns it; an unchanged file does not mean the schema matched.

⚠️ `--check-lock` itself could not be re-run: the tree currently fails to compile `conveyor-actions` — a
duplicate `TraceRecord` import at `crates/actions/src/workflow/trace.rs:18`, which is the
`record-n8n-trace-diffs`/`s13-trace-fixtures` lane's in-flight edit (it claimed `trace.rs` and `n8n.rs` at
16:31). The last green lock measurement for this lane was 17:19's red on `capabilities.get`; treat that as
**history**, not current state, and re-measure when the trace lane releases.

## Session 7 — steps 10 and the n8n projection (`s13-trace-fixtures`, 2026-11-09 16:31Z)

Owner ruling this session: finish the S13 lane; the deploy lane belongs to `deploy-conveyor-service` and
S12 to the web lanes. Step 10 is time-critical — n8n keeps ~3 days of history (measured: 449 executions
across 29 flows, newest `2026-11-08 16:50 +01`).

### Recorded fixtures (step 10)

Read-only, direct on the primary — **no secret was extracted** (the k8s-secret route is hook-denied; the
database route needs only a superuser read):

```
ssh … admin@192.0.2.10 "sudo -u postgres psql -d n8n -At" <<'SQL'
select data from execution_data where "executionId" = $id;
SQL
```
`fixtures/<x>.old.jsonl` is that value unmodified (n8n's **flatted** array — 622/957/575 entries);
`fixtures/<x>.workflow.json` is `workflow_entity` (`id`, `name`, `versionId`, `nodes`) because an
execution carries results and **not node types**. Provenance and both hash sets: `fixtures/executions.json`
and `fixtures/SHA256SUMS`.

| fixture | shape | flow | execution | captured |
|---|---|---|---|---|
| a | linear (no `if`) | `cap.analysis.consult` `nJilzpQdRgF87YJ1` | 1218441 | `2026-11-08 16:41:20 +01` |
| b | condition | `wip.pull` `rAwDtIctiVtjhvFI` | 1218451 | `2026-11-08 16:50:16 +01` |
| c | agent | `cap.testing.consult` `u6qMvsAfx3p18MYx` | 1218449 | `2026-11-08 16:48:55 +01` |

⚠️ **C was chosen three times, and the reasons are the useful part.** `bug_analysis.DOING` has only
`status=error` executions. `project_start_request_analysis.DOING` executes a `planka` node with
`resource=comment, operation=list` — and there is **no `comments.list`** in the registry (`cards.comment`,
`comments.update`, `comments.delete` only), so its old side can never map completely. `cap.testing.consult`
has 40/40 successes and every executed node maps. **A flow with an unmappable node is not a fixture.**

### The projection (`crates/workflow/src/n8n.rs`)

`flatted_decode` resolves n8n's flat array (index references, cycle and depth bounded; a numeric string that
addresses nothing stays a literal). `project_execution(snapshot, execution, run_id)` emits one `TraceRecord`
per **effect**: order from `executionIndex`, attempt from the run array, kind from the node type (`if`/
`switch` → condition; `executeWorkflow` → procedure; else action), input hash from the **producing node's
item** via `source`, output hash from the node's first item, `duration_ms` from `executionTime`, outcome from
`error`/`executionStatus`. Not effects: `code`, `noOp`, `stickyNote`, `wait`, triggers, `lmChatOpenAi`,
`toolWorkflow` and every `*Tool` sub-node. An operation the effect map does not cover is returned in
`unmapped` and the CLI **refuses** — a diff over a shortened old side is a number with no meaning.

Measured: A → `projects.get, requirements.get, catalogues.list, agent.run`; B →
`true, projects.get, true, boards.get`; C → `projects.get, requirements.get, catalogues.list, agent.run`.

### Gate (this slice)

`rbuild -- test -p conveyor-workflow` **78 green**: lib 4, `definition_validate` 15, `engine_advance` 11,
`trace_diff` 7, `workflow_agent` 10, **`workflow_n8n` 9**, `workflow_nodes` 6, `workflow_scheduler` 9,
`workflow_trace` 7. `clippy -p conveyor-workflow --all-targets -D warnings` and
`clippy -p conveyor-actions --lib` clean; `clippy -p conveyor-server --lib --bins` clean; `rustfmt --check`
clean over exactly the touched files. ⚠️ `clippy -p conveyor-server --all-targets` is red on S19's
`apps/server/tests/eval_support/mod.rs:140` (`field version_id is never read`) — not this lane.

CLI, both paths proved:

```
admin trace-diff --old fixtures/a.old.jsonl --old-workflow fixtures/a.workflow.json --new fixtures/trace-diff/identical.jsonl
→ 4 named deltas, exit 1
admin trace-diff --old fixtures/b.old.jsonl --old-workflow <snapshot with comment/list> --new …
→ "1 executed node(s) have no mapped conveyor effect: \"Get board\" …", exit 2
```

### ⛔ The blocker steps 11–13 hit: hashes are not comparable across the two systems

S13 §5.4 passes a flow only when `(position, node_id, effect, input_hash, output_hash, outcome)` are equal.
The old node's payload is the *external system's* response: `cap.analysis.consult`'s `Project` node returns
the RM project row — **62 keys**, planka ids, `created_at`/`updated_at`, `planka_list_ids`, `it_inputs` — and
the conveyor's `projects.get` returns `ProjectView { project, boards }` (`crates/actions/src/board/views.rs:73`).
Masking removes uuids/timestamps/ids; it cannot make two different schemas one value. The same holds for
`planka` → `cards.*` (a Planka card object vs `CardOut`).

⇒ **A/B/C can be compared on effect, node id and outcome today; an exit-0 diff is unattainable under §5.4
as written** unless the owner decides how hashes should be normalised (three options: compare effects only
for migrated flows and keep hashes for same-system regressions; hash the *resolved parameters* under a
per-package normalisation; or store a recorded old hash and compare a projection digest). This is a
spec-level decision, not a lane edit — `diff_records` was **not** relaxed, and no hash was hand-edited.

### Not built (steps 11–17)

`fixtures/gate5.json` (the §7 harness body) does not exist and was not written; the three re-authored
`.flow.json` definitions and their zero-delta proof are blocked on the decision above; the 348-row
`parity/n8n-dispositions.tsv` close-out is step 15. The file now carries its first **three** rows (the three
fixtures), columns `workflow_id, name, kind, disposition, target`, no header — so step 15's `wc -l` = 348
stays the gate.

### One duplicate import, attribution corrected

This session introduced a duplicate `TraceRecord` import in `crates/actions/src/workflow/trace.rs` — a plain
`use` plus the new `pub use` (E0252). **`s13-agent-inference-nodes` did NOT fix it**: it measured the error at
17:35 and deliberately left the file alone because it is this lane's, naming it precisely so it would not be
mistaken for its own red. The file's mtime is 17:36:55, the plain `use` no longer carries `TraceRecord`, and
**which session removed it is not established** — the earlier version of this paragraph credited that peer, and
the peer corrected it. Removing a name from a plain `use` because a `pub use` added it is a correct fix; the
rule to keep is: adding a name to a `pub use` in a module that already imports it is the error, and the next
lane should check both lines before editing either.

The peer also measured `--check-lock` red on `capabilities.get` (S15's write, not this lane) — this slice
registers no action, scope or route, so it writes no lock.





## Session 8 — steps 11–13, the tick defect, and the 348-row ledger (2026-11-09, `s13-trace-gate`)

**Owner rulings.** (1) Finish this lane. (2) The §5.4 cross-system **payload-hash** comparison is
**thrown out** ("stupid ruling, throw it out"): no normaliser, no projection digest. A migrated
flow is compared on the ordered path — `(position, node_id, effect, outcome)` — with the hashes kept
in the trace for forensics only. The second limb (a reviewed `Expected` artefact + `end_state`) was
landed by `s13-trace-gate-behaviour` on top of this session's `DiffMode` and is recorded in
`docs/evidence/S13-TRACE-EXPECT.md`; this session's `n8n_migration` now asserts **both**.

**What landed and was proved**

| what | proof |
|---|---|
| `DiffMode::{SameSystem,Migration}` + `admin trace-diff` selecting Migration iff `--old-workflow` | `trace_diff` **11/11** (4 new: hash ignored, normalised id matched, effect/outcome still fail, non-ASCII id) |
| `fixtures/gate5.json` — read → condition → update → gate → move (plus a never-run false branch) | in-process `n8n_migration` **3/3**; §7 shell harness **GATE5_OK** |
| `fixtures/{a,b,c}.flow.json` reproducing the recordings | `n8n_migration`: A/C agent flows, B conditional + a reorder negative — against `Expected::compare` **and** the direct old-projection diff |
| §7 shell harness (fresh Postgres 5441, rbuild binary on 8938) | 5 effects, parks `waiting_approval`, resumes on `approvals.decide`, card lands in `done`, export = 5 records, same-system diff identical → **0**, one hash mutated → **1** |
| `parity/n8n-dispositions.tsv` | **348 rows** from the live n8n DB (2026-11-09): 280 matrix · 54 procedure · 2 platform-job · 12 retire |
| editor decision (step 16d) | `docs/decisions/0002-editor.md` — fallback A (Leptos/SVG), no egui dependency |

**⛔ The defect the harness found (fixed here): the driving tick never enqueued its successor.**
The tick job re-enqueued under the constant key `<run_id>:tick` while its **own** row was `running`,
and the live-dedupe index (`(tenant, kind, dedupe_key) WHERE status IN ('pending','running')`) counts
a running row as live — so the insert collided with the caller, became a no-op, and a deployed run
executed exactly **one** node then stalled. Measured: the §7 run had one `run_step`, one succeeded
`workflow.node` job, nothing pending, status `running` forever. The in-process tests missed it
because they call `execute_node` directly. Fixed: the key is `<run_id>:tick:<ordinal>`, the ordinal
being the `run_step` count read in the caller's transaction (`crates/store/src/workflow/run.rs`);
`crates/store/tests/workflow_tick.rs` proves the successor is inserted while the current job is live
(1/1); the shell harness re-proved the whole path end to end.

**Two §7 gate corrections — recorded, not faked**

1. The `attempt > 1` assertion is gone. A 5xx **defers** and spends no attempt (`node.rs` —
   `HostResult::deferred` → rearm); a 4xx retry cannot succeed without a fault-injection seam that
   does not exist. Retry-succeeds is `workflow_scheduler`'s test.
2. `cards.claim` and `cards.comment` are **unrunnable from a workflow node**: the node host executes
   as a service principal (`actor_id: Uuid::nil()`), and both resolve that actor as an `app_user`
   (`ensure_parent_tx(Parent::AppUser, …)`), so each fails a parent check. gate5 uses `cards.get`
   and `cards.update` instead; the write→gate→write path is unchanged.

**⛔ Steps 14–15 are blocked on missing plumbing, measured.** The four named flows and `kata.open`
were pulled from the live n8n DB (node counts: `conveyor.start` 32, `wip.pull` 14, `ask_fr` 14,
`err.card` 14, `kata.open` 15). Re-authoring them needs triggers the tree does not have: an inbound
webhook → procedure intake, a schedule → procedure job kind, a `run.failed` consumer, and
`fr.*`/coach actions. Writing procedures that silently drop those nodes would be a fake proof, so
they are not written. **15b** (retire the 348 legacy flows) is a live n8n mutation and S14 P1's
precondition; it is not run before cutover.

**Gate.** `rbuild -- test -p conveyor-workflow` **exit 0**: **88 passed / 0 failed / 1 ignored**
across 10 binaries — trace_diff **11**, trace_expect **6**, workflow_n8n **9**, workflow_scheduler
**9**, workflow_nodes **6**, workflow_agent **10**, engine_advance **11**, definition_validate
**15**, workflow_trace **7**, lib **4**. `clippy -p conveyor-workflow -p conveyor-store
-p conveyor-actions --all-targets -- -D warnings` **exit 0**. This session's files are clean
under the pinned `rustfmt` 1.99.0. The **workspace** gate is red on peers, not this lane:
`fmt --all --check` on `crates/actions/tests/{parity_http_mcp,registry}.rs`, and
`crates/server` test `rls.rs` does not compile (`AppState` gained `services`). No action, scope,
route or migration was added — no served-spec lock write.

## Session 9 (2026-11-09, `finish-s13-lane-steps`) — steps 14–15, 15b, 16, 17

Steps 14–15 are **built**: the four named flows and `kata.open` are re-authored at the conveyor's
model and driven end to end. The "blocked on missing plumbing" list from session 8 is closed:

| missing piece | built |
|---|---|
| inbound webhook → procedure | `POST /api/hooks/{hook}` (`crates/api/src/routes/hooks.rs`): three closed hooks (`conveyor.start`, `wip.pull`, `katas.open`), a deployment shared secret (`CONVEYOR_HOOK_SECRET`, fail-closed 503/401), body `{tenant_id, project_id, card_id?, inputs}`, `202 {run_id}`. It starts through the SAME `start_published_tx` the action uses. |
| schedule → procedure | a `workflow.start` worker kind, and `schedules.upsert` gains a `payload` (the runner already copies it into the job). `wip.pull` is a platform job (`"wip.pull"`) over `board::pull::wip_pull`, which selects intake cards up to the active lane's `wip_limit` and performs each move through the `cards.move` **action** (matrix, position and event included). |
| `run.failed` consumer | `store::run::enqueue_run_failed_trigger_tx` at every `run.failed` emitter (worker reap both halves, `procedure.step`, `workflow.node`, `runs.finish`) → one `workflow.start` job for `err.card`, deduped by run. A project-scoped run (no card) enqueues nothing. |
| `fr.*`/coach actions | not needed: `ask_fr` is a procedure over the conveyor's own `human_approval` gate; the coach is a recorded delta (below). |

- `cards.comment`, `cards.create` and `cards.duplicate` now record no author/creator for a service
  principal (the workflow host runs as `actor_id: nil`); a user/agent keeps the parent check. The
  flows that comment (`ask_fr`, `err.card`, `katas.open`) and `conveyor.start`'s card creation work
  as a service without inventing an `app_user`.
- One new action, `cards.list` (`cards:read`): a workflow can only call actions, and
  `conveyor.start` reads a lane. Registry **268 → 269** (255 internal + 14 external).
- Definitions: `fixtures/named/{conveyor.start,err.card,katas.open}.json` (workflows),
  `fixtures/named/ask_fr.procedure.json` (procedure), the 89-node coverage table
  (`fixtures/named/mapping.tsv` + `fixtures/named/old/*.nodes.json`). `parity/n8n-dispositions.tsv`
  stays **348** rows; the five targets are refined to `workflow:conveyor.start`,
  `procedure:ask_fr`, `schedule:wip.pull`, `run.failed:workflow:err.card`, `workflow:katas.open`.

**Two defects found by execution, one forced delta:**

1. The workflow `procedure` node sent `{"key": …}` to `procedure_runs.start`, which expects
   `procedure_key`; no test had exercised the node. Fixed in `crates/actions/src/workflow/node.rs`.
2. ⛔ **A workflow cannot start a child procedure on its own card.** `uq_run_live` permits one live
   run per card, and the parent is live, so the child start is a `409 LiveRunExists`. The coach
   nodes are therefore **not ported**; `mapping.tsv` records the delta, and the coach is started as
   its own run by the kata executor / its trigger (S14/S18 integration), not inside `err.card` or
   `katas.open`. Resolving it needs an inline execution of the child's steps or a `parent_run_id`
   exemption in the schema — an S13-open design decision, named here rather than silently dropped.
3. `projects.get` returns `{project, boards}`; the flow's identity check reads
   `steps.1.output.project.id` (recorded in the fixture, not assumed).

**Step 15b.** `scripts/retire-n8n-flows.sh` pages the n8n public API by cursor, asserts **348**, and
only with `--apply` flips every flow inactive, then asserts the API's active list is empty. The dry
run could not be executed in this environment: `N8N_API_KEY` is absent (the step-10 recording used a
key supplied then; the n8n DB stores only its hash, so it cannot be recovered there). The flip is
S14 P1 by design; the count of record is the 348-row dispositions ledger.

**Step 16.** `docs/decisions/0002-editor.md` records fallback **A**; the slice gate accepts the
recorded decision in place of the spike (S13 §7).

**Step 17 (gate).** Measured on this tree (`rbuild`, m1; `TEST_DATABASE_URL` on 55432):

- `crates/api/tests/named_flows.rs` **6/6** (conveyor.start create+move and its idempotent
  intake branch, wip.pull's cap and its second no-op run, ask_fr's ask→gate→answer, err.card's
  run.failed→waiting→comment, and the 89-node coverage assertion).
- `crates/api/tests/kata_open.rs` **1/1** (hook → card/board/prior-run read → comment).
- `conveyor-actions --test registry` **4/4** — **269** actions, **(255, 14)** internal/external.
- `conveyor-actions --test parity_http_mcp` **8/8** over all 269 actions on both transports,
  including the new `cards.list` sample.
- `conveyor-api --lib` **30/30** (the `every_action_has_a_documented_operation` 269 assertion).
- `conveyor-actions --test n8n_migration` **3/3** and `conveyor-workflow --test trace_diff`
  **11/11** — the A/B/C replays and the diff machinery still pass after the node/service changes.
- `--print-openapi --check-lock` **exit 0**.
- `clippy --workspace --all-targets -D warnings` **exit 0**. Two reds were closed on the way: the
  S12 wave-2 lane added `AppState.services` to its `rls.rs`/`events.rs` constructors (owner
  `land-list-run-steps-detail-tx`, formerly `build-measure-screens`), and this lane added a type
  alias for its diagnostic tuple (`clippy::type_complexity`).
- `fmt --all --check` **exit 1**, attributed: the remaining diffs are only
  `apps/server/src/eval_runner.rs` and `apps/server/tests/eval_*` (S19/S20 files). Every file this
  lane touched is rustfmt-clean under the pinned toolchain (`rustfmt --check` exit 0 on the set).




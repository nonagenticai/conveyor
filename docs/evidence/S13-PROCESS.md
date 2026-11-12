# S13-PROCESS — the process engine, judge, bus and catalogue, as built

Receipt for `lane/s13-process-engine` (2026-11-10). It records what is on disk, not what was planned; the plan is
`docs/plans/slices/S13-n8n-engine.md`.

## The executor model — the engine is RETAINED

**Owner ruling 2026-11-09: the workflow engine is retained.** `crates/workflow`, the `workflow` table,
`workflows.*`/`workflow_runs.*` and the `workflow.node`/`workflow.start` job kinds stay. It is the
**tool-sequence executor**. There is **no `0034` drop migration** and `run.workflow_id` is not removed. The
process is the top-level executor; the workflow engine is never a second one.

**The `call` step is wired (2026-11-10).** A `call` step names `child_process_key`; the executor resolves it
**process-first** (`process::published_by_key_tx`) and **workflow-second**
(`workflow::get_published_by_key_tx`). Both starts happen on the executor's own transaction: `process_runs.start`
(the same action an external caller uses) or `start_published_tx` with its `parent: Option<(run, step)>` branch,
which inserts a CHILD workflow run instead of a root. The child's id is written to the parent
`run_step.output` in that same commit, so a crash leaves either no child or a pollable id — never an orphan and
never a duplicate. Polling re-reads the child by id under the `call` dedupe tag and a `polls` counter of its
own; a succeeded child's last succeeded `run_step.output` is judged against the step's SOP, and a
failed/cancelled child takes the step's ordinary `on_result` (continue → skipped, retry → a new child, fail →
the run fails). A child never fires `err.card`; a cancelled parent cancels its live descendants.

The workspace has **11 members** — `crates/bus` is added and `crates/workflow` stays. CONTRACT §1's earlier
"`crates/bus` replaces `workflow`, the count stays 10" is superseded.

## Migrations

The train's `0033_d23_eval_caller` and `0034_s18_kata_arm` keep their numbers. S13 ships:

- **`0035_process_tree.sql`** — the `procedure`→`process` and `procedure_step`→`process_step` renames, the SOP
  `procedure` table, `process_map`, the run tree, `capability.kind`, the agent roles, `run_step.judge`, the event
  bus cursor, and `inference_call.caller_kind` `procedure_step`→`process_step`.
- **`0036_s19_output_grade.sql`** — S19's per-output grade path.

No migration was added for the `call` step, the maps or the park bound: the run tree, `run_step.output`/
`run_step.judge` and `process_map` already carry them.

## What is built

- **The bus** (`crates/bus`): envelope `{id, topic, from, correlation_id, procedure, tenant_id, job, payload}`,
  groups `process|tool`, 5 deliveries → `bus:dead`; the relay publishes from `event` by `seq` and records
  `bus_stream_id`/`published_at`. The `process.step` job is the **Redis-absent fallback**. CI starts a dind Redis
  from the archetype's `redis_image`.
- **The judge**, inside the step executor: `checker`/`laya`/`eval`. A reject reruns the same position with the
  reason (the SOP's `max_rejects`, default **2**) → an exhausted budget fails the run (→ `err.card` → the coach);
  an outage **parks** the step and re-enqueues it. `run_step.judge` records the verdict.
  **The park is bounded:** after `JUDGE_MAX_PARKS` (64) the step fails CLOSED with `judge_unavailable`, which
  takes the ordinary `on_result` path (`fail` — the default — fails the run, so the card goes BLOCKED and the
  coach is notified; `continue` skips the step). It never accepts an unjudged output and never spends a reject.
- **The `call` step** (see above): process-first, workflow-second, child poll, on_result, depth ≤3 enforced by
  `insert_child_run_tx`. `runs.cancel` and `cancel_run_tx` sweep descendants; `enqueue_run_failed_trigger_tx` is
  root-only, so a child's failure is its parent step's business.
- **The computed set**: `projects.create` creates project + board + start card
  (`project_start_request_analysis`, intake) in one transaction and emits `card.created`, and starts a root run
  **only when the tenant has published a start process**; `err.card`
  runs on `run.failed` (BLOCKED lane + comment + coach root run); `wip.pull` reacts to `card.moved`. The hook route
  and `CONVEYOR_HOOK_SECRET` are deleted.
- **⛔ No universal tenant seed (owner ruling 2026-11-10).** The 280 + 53 catalogue is software-development
  content, installed only by `migrate --process-catalogue --tenant <slug>`; `migrate --seed` never installs
  processes. A tenant may run accounting, marketing, software development or human project management, so the
  process a project starts is tenant content — its absence is not an error.
- **The default catalogue**: **70 type files / 280 processes / 350 SOPs**, loaded by
  `crates/store/src/process_seed.rs` (`serde_yaml` + `include_dir`); `apply_all` runs after `migrate --seed`, and
  `apply_type` applies the dev fixture's start process. `seed/processes/manifest.yaml` is the generated 280-entry
  index. Tenant-defined processes/SOPs/maps are rows (D29), not code constants.
- **The process maps**: `seed/processes/maps.yaml` carries **3** phase DAGs — `it`, `kata`, `py2rust` — one per
  project type whose `seed/catalogue.json` entry declares phases and card types. The loader resolves the project
  type, validates every node against `MapFacts` (published process keys, capability codes, that type's
  `meta.phases`, `milestone_kind` codes) and publishes version 1; a second apply inserts nothing.
  `kaizen`, `meta_procedure`, `meta_prompt` and `program` declare **zero phases and zero card types**, so no map
  is derived for them rather than inventing one.

## Tests

- **The catalogue** (`crates/store/tests/process_seed.rs`): applies the whole catalogue **twice on a fresh
  database** — publish validation and idempotency are proven; the manifest matches the type files exactly; the
  three maps publish, resolve every node, and the second apply is `SeedCounts::default()`.
- **The call step** (`crates/actions/tests/call_step.rs`): process-first (run-tree edge, one child, judged
  output), workflow-second (`workflow_id` set on the child), a failed child with `continue` (no `err.card`) and
  with `fail` (one root `err.card`), retry (a second child), a retired target (a recorded step failure, nothing
  started), and cancelling the parent (the live child is cancelled).
- **The judge** (`crates/actions/tests/judge.rs`): the accept/reject/rerun ladder, the outage wait, and the
  bound (64 parks → `judge_unavailable`, run failed, no 65th park job).
- **The start path and computed set** (`crates/api/tests/project_start.rs`, `computed_set.rs`,
  `process_run.rs`): project + board + start card, the root run only when a start process is published, gate
  park/resume, coach child, `wip.pull`.
- **The ledger** (`crates/actions/tests/n8n_migration.rs`): 348 dispositions, every `process:` target resolves.

## The gate

`rbuild --gate` (the CI equivalent: `fmt --all -- --check`, `clippy --all-targets --locked -- -D
warnings`, `test --locked`) is **green on this branch** (2026-11-10): **1,174 tests passed**
across 215 test results. The bus limb ran against a real Redis (`TEST_REDIS_URL`); it fails
closed, never skips.

**Session 2 local gate (2026-11-10):** `fmt --all -- --check` clean; `clippy --all-targets
--locked -- -D warnings` clean; `test --locked --workspace --no-fail-fast` ran **217 test
targets** — **216 `ok`**, and `bus_delivery` fails **closed** because this build host has neither
`TEST_REDIS_URL`/`REDIS_URL` nor a runnable `docker`
(`crates/actions/tests/support/redis.rs:75`: "integration tests fail closed"). No code failure was
reported; the bus limb's venue is CI's dind Redis.

## Session 2 (2026-11-10) — W4, W5, D29, the catalogue E2E and BYO tokens

A second pass over the same lane, driven by the 2026-11-10 plan audit. Measured, not quoted:

- **W4 (bound executor).** `crates/actions/src/workflow/agent.rs::resolve` now resolves a run that
  carries `capability_agent_id` from its **row first**. A step input may supply the messages and may
  **lower** a cap; a conflicting `model`, node-supplied `tools`, a substituted `prompt_ref`/
  `capability_agent_id`, or a **raised** `max_turns`/`timeout_ms` is a typed `422`
  (`agent.config_conflict` / `agent.cap_exceeds_bound`). The unbound explicit path stays, records
  `capability_agent_id = None`, and refuses a claimed executor. `agent_run_resolved`: 6/6.
- **W5 (workflow → process composition).** The run tree's `step_id` is FK-bound to `process_step`, so
  a workflow's procedure node could not name its step: `insert_child_run_tx` answered `StepNotFound`
  and a root start 409s on `uq_run_live`. Migration **`0037_run_tree_workflow_step.sql`** adds
  `run.workflow_step_id → run_step(id)`, its child-live/-attempt uniqueness pair, and a CHECK that a
  child names exactly one step kind. `process_runs.start` takes `workflow_step_id` (refusing both or
  neither); the node passes its `run_step` id. `call_step`: 10/10 (the new child test + cancel
  cascade included).
- **A failed node's class is now in the trace.** `scheduler::apply` recorded every failed step with
  `error = NULL`; the run only said `workflow_failed`. It now records the typed class
  (`agent.config_conflict`, a 4xx action code) on `run_step.error`.
- **D29 readiness.** `capability_readiness_tx` takes the process step's `bound_agent`; the bound
  agent is checked directly and the "exactly one active executor" rule applies only to unbound
  capability runs. A bound process starts with several active agents in its capability; an unbound
  capability with three still refuses `agent_ambiguous`. `capability_readiness`: 5 passed, 1 ignored
  (the ignored one needs the real strata service).
- **The catalogue executes end to end.** `crates/actions/tests/process_catalogue.rs` applies the
  shipped catalogue and runs `bug_analysis.READY`: the agent step (`agent.run`, bound executor,
  compiled `agent@1` prompt, stub inference) is judged accepted by a stub laya (0.9 ≥ 0.7), the
  computed `cards.move` moves the card intake → active, the run succeeds, two `run_step` rows are
  recorded, and the run pins its executor. Apply counts: **333 processes** (280 card-type + 53
  named), **403 SOPs**, 3 maps, 69 card types.
- **Per-tenant upstream tokens (BYO).** `inference.token` is an *optional* bearer on
  `inference.chat`/`completions.create` (`HandlerRef::External.auth_optional`): absent ⇒ no bearer,
  never a platform token, and the service's 401 is a recorded failure. A sealed secret may be the raw
  BYO value or the licences envelope `{token, expires_at, source, licence_id}`;
  `crates/actions/src/invoke.rs::bearer_from_secret` accepts both and refuses a malformed envelope.
  `conveyor admin upstream-token --tenant <slug> --service laya|inference --mint|--token-stdin`
  creates or rotates the row (the mint calls the licences bundle-token route with the existing S2S
  credential). `apps/server/tests/upstream_tokens.rs`: 3/3.
- **Store/migrations**: `0037` embedded and recorded; `s07_schema` 5/5, `process_tree` 3/3,
  `s11_process` 12/12, `transitions` 4/4. `process_trace` 7/7 and `n8n_migration` 1/1 (348
  dispositions, every `process:` target resolves).
- **Served-spec lock.** The lane's lock was stale against its own action surface — the process-model
  actions (`processes.*`, `process_runs.start`, `process_maps.*`, `laya.judge`, `eval_grades.*`)
  are in the served document, `procedure_runs.start` is gone — so `--check-lock` was red before any
  session-2 edit. Regenerated with `--print-openapi --write-lock`; `--check-lock` is green.
  Session 2 adds **no** action.

**Not run locally: the bus limb.** `bus_delivery` needs a real Redis; the build host has no docker
and no Redis and cannot reach mini3, so it fails closed (by design) rather than skipping. Its venue
is CI (dind Redis). Everything else above was executed.

## Outstanding (stated plainly — do not claim these)

- **No live `laya.judge`/`inference.chat` call has been made.** The token path is built and tested
  against stubs; a live call needs a tenant row filled (`upstream-token --token-stdin`) or the
  licences `bundle-token-minter` role granted to the secure-apps `conveyor-backend` client (not
  granted as of 2026-11-10). The `laya` request shape remains the first-live-call pin.

- **`workflow_step_id` is not on `RunOut`.** The column and the tree enforcement exist; the public
  run projection still exposes only `process_step_id`, so a workflow child's edge is visible in the
  database and in the node's output, not yet in the API read.

- **A live `call` child has no deadline.** The parent's poll backs off but never expires: a child
  parked on its own gate may legitimately wait for a person, and the judge bound covers judge
  outages only. A child that can never finish is a run-level problem (its live-run slot is the
  signal), not this step's.

- **The tool-sequence `workflows/` files.** No `coding_jobs`/`workspaces` action exists in this repo (the
  registry has `repositories.*` only), so a shipped sequence could not name a registered action. The
  workflow-second `call` limb is proven by a test-created workflow, not by seed YAML.
- **The n8n trace fixtures** — the historical recordings are not in this repo; the trace export/diff runs over
  `run_step`, which is the record that exists.

## Session 3 (2026-11-10) — the contracts between the parts

A third pass, driven by the owner's "full code completion" ruling and three read-only audits. The
session record is `docs/plans/slices/S13-n8n-engine.md` §6 (items 16–23); this section records what
was MEASURED and what is still not true.

### What was broken, and why the earlier receipts missed it

The spine was real (worker dispatch, judge, run tree, bus crate, a 333-process catalogue). The
defects were in the contracts BETWEEN those parts, and each was invisible to the test that covered
only one side:

| defect | proof it was real | closed by |
|---|---|---|
| `validate_publish` refused the action the seed writes on every `agent` step, and the seed loader bypassed validation — so a fork of ANY catalogue process answered `422 process.invalid_step` (D29 impossible through the API) | `process_catalogue` now forks `bug_analysis.READY` and publishes it as v2 | one step contract (`AGENT_STEP_ACTION`), same shape rule in the loader, dispatch on kind |
| an API-authored `kind: agent` step published and then invoked `""` | `process_agent` publishes and runs one | same |
| the seed could not express a `gate` or a `call` step (`StepSpec` had neither field; the loader hard-coded `None`) | `process_catalogue` parks the seeded `ask_fr` at `human_approval` and starts the coach's real `kata.open` child | `StepSpec.gate_kind` / `.child_process_key`, per-kind shape check, call-target check |
| `read_process` decoded `process_step.action` as non-null — any process with a gate step failed its prompt compile with a 500 | the first seeded gate run reproduced it | nullable read, empty rendered |
| a 300 s agent step ran under a 60 s lease with no renewal | `jobs` 7: a renewed lease is not reclaimable; a stale token can neither renew nor finish | `jobs::renew` + the worker's renewal task |
| `runs.finish` terminated a process run under its executor; cancel/fail/reap skipped only the named run's steps; the reaper's exact-key match stranded park/gate/call steps | `process_cancel` 4, `s11_process` 14 | refusal, cascade, prefix match |
| the judge document had no criteria and no output hash; `RunStepOut` had no judge; `RunOut` had no tree edge | `judge` 8, and the projections | fields added and projected |
| a SOP pinning a missing eval passed publish and parked 64 times | `process_step` 16 | `PublishFacts.evals` + `UnknownEval` |
| only `process_maps.get|list` existed, so the map validator was unreachable | `process_maps` 4 | four write actions |
| no `FLUSHALL`/replay test; gate resume had no outbox event; compose ran no worker | the replay test (below) | `Bus::flush_all`, the announcement, the `worker` service |
| `migrate --process-catalogue` installed processes but not the vocabulary they name | `catalogue_install` (one command, fresh DB) | the install runs `seed_catalogue` first |

### Measured on the final tree

- `conveyor-domain` 252+3+22+5+1.
- `conveyor-actions`: `process_catalogue` 1 (full catalogue E2E, the D29 fork, the seeded gate and
  the seeded call), `process_step` 16, `judge` 8, `call_step` 10, `process_cancel` 4,
  `process_step_agents` 1, `process_agent` 1, `process_agent_resolved` 1, `process_maps` 4,
  `process_seed` 6, `registry` 8 (292 actions = 277 internal + 15 external).
- `conveyor-store`: `s11_process` 14, `process_tree` 10, `jobs` 7, `process_seed` 6.
- `conveyor-server`: `catalogue_install` 1.
- `conveyor-api`: `project_start` 2, `computed_set` 3, `capability_readiness` 4/1 ignored.
- `served-spec.lock.json` regenerated deliberately: **319 operations** (was 315; the four map
  writes).
- **The whole workspace, measured on the final tree** (`rbuild -- test --locked --no-fail-fast`):
  **219 test binaries, 1195 passed, 5 failed** — and all five are `bus_delivery`, the Redis-absent
  limb above. `clippy --all-targets --locked -- -D warnings` is clean; `fmt --all -- --check` is
  clean. `rbuild --gate` itself aborts at `bus_delivery` (cargo stops at the first red binary), which
  is why the count above comes from the `--no-fail-fast` run.

### Still not true (stated, not hidden)

- **The bus limb has not run on the build host.** `bus_delivery` needs a real Redis; the build host
  is m1, which has no docker and no Redis and cannot reach mini3 (measured: `m1->mini3:22` and the
  LAN both fail). The code and all five tests — including the new `FLUSHALL` replay test — are
  complete; the limb is unexecuted until CI or a host with Redis.
- **No live `laya.judge` / `inference.chat` call.** Owner ruling for this unit: code completion, no
  live run. The token path is built and tested against stubs.
- **The tool-sequence `workflows/` files are not shipped** — a scope call, not a blocker (see
  `seed/processes/README.md`; the actions exist, the authoring is excluded by the owner audit).
- **The five `wf_*` card types have no processes** — they are outside the n8n matrix the 280 come
  from (documented in the seed README and asserted by the manifest test).
- The process/workflow editor (ADR 0002, audit W16) remains later product work.

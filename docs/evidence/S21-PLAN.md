# S21 step 2 — the plan projection over declared step outputs (receipt)

**Date:** 2026-11-09. **Lane:** `lane/train-2026-11-09` (integration base `e92de62`; the slice commits
on top). **Owner ruling:** add the declared `outputs`, accept the migration and the served-spec change,
then write the lock and land the train.

## What was built

- `migrations/0032_s21_plan_outputs.sql` — `procedure_step.outputs jsonb NOT NULL DEFAULT '[]'`, CHECK
  `jsonb_typeof(outputs) = 'array'`.
- `crates/domain/src/plan.rs` (new) — `project()` (the read-only plan) and `findings()` with
  `BadOutput`, `UndeclaredOutput`, `UnproducibleInput`, `MaybeAbsentInput`, `FutureInput`,
  `GateNeverOpens`, `UnreachableStep`; the JSON-Schema path walker (`properties`, `items`,
  `anyOf`/`oneOf`/`allOf`, `additionalProperties`, `$ref`→`$defs`). A reference must be BOTH declared
  by its producer and resolvable in the producer's action output schema: the declaration is the
  contract, the schema is what the run actually returns. The walker answers "does the schema DECLARE
  this path" (allOf unions; a missing additionalProperties declares nothing beyond `properties`),
  which is stated at `schema_has_path`.
- `crates/domain/src/procedure.rs` — `StepInput.outputs: Vec<String>`.
- `crates/store/src/procedure.rs` — `ProcedureStepRecord.outputs`, `STEP_COLUMNS`, `StepRow`,
  `insert_steps_tx` binding.
- `crates/actions/src/procedure/views.rs` — `StepIn.outputs`, `StepOut.outputs`.
- `crates/actions/src/procedure/{mod,procedures}.rs` — the wire mapping, gate facts from
  `meta.expires_at`, the projection call after `validate_publish`, and `422 procedure.plan_invalid`
  with one field error per finding.
- Ledgers — `crates/store/src/migrate.rs` and `crates/store/tests/s07_schema.rs` both carry 32.

## Measured gate (rbuild on m1, 2026-11-09)

| command | result |
|---|---|
| `rbuild -- test -p conveyor-domain --lib plan` | **14 passed, 0 failed** |
| `rbuild -- test -p conveyor-store --test s11_procedure` | **11 passed, 0 failed** (incl. `declared_step_outputs_round_trip`) |
| `rbuild -- test -p conveyor-store --test s07_schema` | **3 passed, 0 failed** (ledger through 32) |
| `rbuild -- test -p conveyor-actions --test procedure` | **12 passed, 0 failed** (incl. `procedure.plan_invalid`, the declared-output read-back and the non-string gate-expiry refusal) |
| `rbuild -- clippy -p conveyor-domain -p conveyor-store -p conveyor-actions --all-targets -- -D warnings` | exit 0 |
| `rustfmt --edition 2024 --check` on the eleven touched files | exit 0 |

## Reversal proofs (each fired, then reverted)

| deleted check | test that turned red |
|---|---|
| the prefix-coverage rule in `covers()` | `a_declared_reference_publishes_clean`, `the_prefix_rule_is_segment_aware` |
| the `on_error=continue` maybe-absent rule | `a_continue_producer_may_be_absent` |
| the unreadable-expiry gate arm | `an_unreadable_gate_expiry_is_refused` |
| the `$ref` arm in the schema walker | `the_schema_walker_handles_arrays_and_anyof` |
| the reference's producer-action schema check | `a_reference_deeper_than_the_action_schema_is_unproducible` |
| the `FutureInput` arm (self/forward reference) | `a_forward_or_self_reference_is_a_future_input` |
| the `BadInputReference` emission | `a_malformed_step_index_is_a_bad_input_reference` |
| the cyclic-`$ref` depth guard | `a_cyclic_ref_is_refused_without_overflowing_the_stack` (the binary aborts on the stack overflow) |

## Deep audit (fresh agent, read-only) — dispositions

Verdict on the first pass: **NO**, five findings. Dispositions:

1. **Fixed** — a present-but-non-string `meta.expires_at` was silently treated as absent. The
   publish mapping now records it for `GateNeverOpens`, and an actions test inserts a numeric-expiry
   gate row and proves the 422. ⚠️ The FIRST version of this check also refused a valid PAST
   timestamp; the workspace suite caught that as a false positive — `procedure_step`'s
   `an_expired_gate_applies_the_steps_on_error` publishes such a procedure deliberately and proves the
   runtime applies the step's `on_error`. The check now refuses only an expiry the runtime cannot
   read, and the suite test passes.
2. **Answered, documented** — `allOf` is unioned, not intersected. The walker is a declared-path
   reader, not a JSON-Schema validator; a flattened schemars object adds members in a second branch.
   The semantics are stated at `schema_has_path`.
3. **Fixed** — the projection did not reject a self/forward reference on its own. It now emits
   `FutureInput`, with a test covering both (the template error still wins at publish).
4. **Fixed** — an external `$ref` was false-refused; it now does not refuse a declaration it cannot read.
5. **Answered** — the `steps.N.output` fixtures are WORKFLOW definitions validated by
   `conveyor_workflow::definition::validate_publish`, not procedures, so they are unaffected. The
   real consequence (an existing procedure DRAFT must declare its outputs before republishing) is
   deliberate and stated in the migration comment.

**Second fresh audit: PASS.** It verified the five dispositions and returned PASS with three
non-blocking findings; two were taken as hardening: a cyclic `$ref` (root `#` or a self-referential
`$defs` entry) recursed without consuming a segment — now capped at 64 hops and refused; and a
`steps.` reference whose index is not a positive integer was silently dropped — now
`BadInputReference`. The third (a `FutureInput` also raises `UnreachableStep`) is the intended
summary shape.

## Workspace gate

- `clippy --workspace --all-targets -- -D warnings`: **exit 0**. One pre-existing red was fixed:
  `apps/server/tests/eval_support/mod.rs` is a fixture module compiled into three test binaries, so
  each binary sees the others' fields as dead; it now carries `#![allow(dead_code)]`. Peers fixed
  their own files during the run (`crates/api/tests/named_flows.rs` read its `waiting` field).
- `test --workspace --locked --no-fail-fast`: **exit 0, zero failing binaries** (196 targets; a
  peer's reproduction of the previous tip measured 195/196, the one red being item 3 below).
  Reaching green cleared five blockers, each measured rather than guessed:
  1. this slice's gate-expiry false positive — `procedure_step::an_expired_gate_applies_the_steps_on_error`
     publishes a deliberately past gate and proves the runtime applies `on_error`; the check now
     covers only an expiry the runtime cannot read;
  2. a stale tenancy test — `/api/me` with no header is the switcher bootstrap (200 + the membership
     list), not a 400; the test now proves the bootstrap and keeps the 400 on a tenant-scoped action,
     and CONTRACT §9 states the exception;
  3. `crates/api/tests/externals_declared.rs` — S15's `integrations/strata.rs` was missing from the
     exactly-compared ALLOWED list; added with the same transport rationale as `ingest.rs`;
  4. `scripts/db-roles.sql` — its two cluster-wide ALTER ROLE statements raced across parallel test
     databases (`XX000 tuple concurrently updated`; measured 27 targets failed in one run); now
     serialised by a transaction-scoped advisory lock, released at COMMIT;
  5. the environment — m1 had **7 GiB free** and the first run died `StorageFull` (the mass panics at
     `common/mod.rs:213` were that, not code); three cold conveyor slot caches were pruned, freeing
     77 GiB (84 GiB free), slot 0's warm target kept.
- `fmt --all -- --check`: red on four S19/S20 eval files only (`apps/server/src/eval_runner.rs`,
  `apps/server/tests/eval_runner.rs`, `eval_runner_properties.rs`, `eval_support/mod.rs`); none of
  this slice's files. They are the eval lane's to format.

## Served-spec lock

The declared member changes the `procedures.*` operation schemas (`StepIn` on create/update,
`StepOut` on get/list/create/retire), so `served-spec.lock.json` was regenerated over the **whole
tree** — not just this slice — with `conveyor-server --print-openapi --write-lock` on m1, copied
back with `rbuild --copy-back`. Measured 2026-11-09 19:15 CET:

- **287 → 297 operations.** Added 10: `GET/POST /api/platform/tenants`, `GET/POST
  /api/platform/tenants/{id}`, `POST /api/platform/tenants/{id}/invite-admin`,
  `POST /api/platform/tenants/{id}/retry`, `GET /api/tenants/{id}/billing`,
  `POST /api/hooks/{hook}`, and the invocation routes for `cards.list`, `runs.get` and
  `eval_runs.regrade`.
- 14 operations changed hash, including all six `procedures.*` surfaces (this slice),
  `GET /api/actions` (the manifest), `capabilities.get` (S15), `eval_climbs.compare` (S20),
  `eval_signoffs.*` (S19), `approvals.*` and `schedules.upsert`.
- Proof: `conveyor --print-openapi --check-lock` → **`lock OK: served-spec.lock.json`, exit 0**.

Earlier counts (287 on disk at session start, and the 293/294 quoted by other lanes) were snapshots
of trees that have since moved; this is the first count measured against the integrated tree.

## Not in this slice

- Memory selection (open Q3): deferred with the owner's shape — selection → processing →
  `prompt.memory`.
- Tenant-authored recipes (open Q4): deferred; needs the CRUD surface.
- No new action, scope or route; the registry count does not move.

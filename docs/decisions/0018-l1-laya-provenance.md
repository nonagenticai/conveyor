# 0018 — The L1 laya gate makes its own call

Status: Accepted · Date: 2026-11-24 · Issue: #656 · Resolves: S21 §22, SPEC_V2 §7.2, §7.3, §4.5,
rework `v1-w1c-3:T2`, `v1-w1c-4:T2`, `v1-w1c-5:T2`

## Decision

1. **The ladder owns the case; the caller supplies facts, never a verdict.** A case is
   `{model, state, questions}` in SPEC_V2 §7.2's wire shape: `model` is the tenant's
   `laya_inference_config.model_ref`, `state` is the task text derived from the caller's declared
   input, and each `questions.<qid>` is `{type, instructions, criteria}` derived from the effective
   `decision_policy` and the authored option set. `ladder.evaluate` no longer accepts `answer`,
   `confidence`, `probabilities`, `latency_ms` or `options`; a request that carries any of them is
   refused `422 caller_supplied_verdict`.

2. **The engine makes the call, outside any transaction.** The L1 request is the external action
   `ladder.precheck` (`POST /v1/systemone`, service `laya`, bearer from the sealed `laya.token`,
   `governance:write`, tenant-scoped, effect `read`). `laya.judge` stays the SOP step judge: its
   question, its threshold and its consumers are a different case with a different owner
   (CONTRACT §21). Two decisions, two actions, one laya service.

3. **A read action hands the caller the exact case.** `ladder.case` takes the same facts
   `ladder.evaluate` takes and returns `{case, case_hash, laya_call_key}`. It exists so a caller
   outside the process can obtain the exact bytes it must post to `ladder.precheck`; in-process
   callers (the executor) use the same builder function directly. The builder is deterministic:
   the same facts, policy and option-set version produce the same `case_hash`.

4. **Only a verified recorded exchange produces a decision.** `ladder.evaluate` takes
   `laya_call_key` and rebuilds the case itself. It then requires **all** of:
   `store::idempotency::lookup(tenant, "ladder.precheck", key, request_hash(case))` to return
   `Replay { status_code: 2xx, response }` — so the engine ran a call whose recorded input hash is
   this case's — and `store::actions::find_invocation_by_key` to name the invocation, whose
   `outcome` is `ok` and whose `duration_ms` is the decision's latency. The answer, confidence and
   probabilities are parsed from the stored response, never from the request. Anything else —
   `Miss`, `InFlight`, `ConflictHash`, a non-2xx status, an unreadable body — is `422
   laya_call_unverified` and writes no decision. The decision records
   `meta.laya_call_key`, `meta.laya_invocation_id`, `meta.case_hash`, the option-set version and
   the measured latency, so §7.2's "a decision is reproducible from the row" holds.

5. **Options are authored, versioned rows.** Migration `0068` adds `decision_option_set` and
   `decision_option_set_option`: one live set per `(tenant, kind, gate)`, a monotonically increasing
   `version`, and rows carrying `key`, `label`, `payload` and an explicit `payload.outcome ∈
   accepted | refused | escalated`. `decision_option_sets.set` authors a set (a version bump, never
   an in-place edit) and `decision_option_sets.get` reads it. A `choice` question is built from the
   authored labels; the answer must be an authored key (`422 laya_answer_not_in_options`); a case
   that needs a `choice` and has no authored set is refused `409 option_set_missing`, never
   answered from free text. `decision_option` remains the per-decision record of the option set it
   evaluated, written from the authored rows.

6. **`Unavailable` follows the fail policy; it is never a pass.** `domain::ladder::unavailable`
   already maps `closed → refused` and `open → the policy's action` (handover → escalated, block →
   refused, annotate → accepted with the note). Two shapes, one policy. An **outage** — the service
   unreachable, the call never completed — leaves no recorded exchange, so `ladder.evaluate` cannot
   and does not write a decision: the caller applies the policy, and the executor is that caller. A
   **recorded** call that completed without a readable answer has provenance, so the action writes
   the decision `unavailable` with `meta.gate_unavailable = true`, following the same policy. The
   executor consumes the action's outcome for a recorded call and applies the policy itself for an
   outage.

7. **`fire_when` decides whether the case is asked.** `decision_policy` gains a nullable
   `fire_when` expression in migration `0068`. When it does not hold for the resolved case, no laya
   request is made and no decision is written. Its first form is a small structured expression
   (`{"all_of": [...]}` / `{"any_of": [...]}` over `{kind, gate, field}` predicates), evaluated in
   `crates/domain`; an unparseable expression refuses the policy at authoring time.

8. **The executor's gate becomes a client of the same path.** `internal_agent` stops calling
   `laya.judge` and stops handing an answer to the ladder. It builds the case with the shared
   builder, invokes `ladder.precheck`, and calls `ladder.evaluate` with the returned key. Exactly
   one laya request per gate is a test assertion.

## Why

SPEC_V2 §7.2 makes the laya answer the evidence behind every gated choice: "Laya inference is
tenant-scoped … Every decision is recorded with the option set version, the answer, probabilities
where the type provides them, confidence, policy, latency and input hash. A decision is
reproducible from the row." Nothing in the pre-0018 ladder requests anything. `ladder.evaluate`
reads `answer`, `confidence` and `latency_ms` from the request body and stamps `rung='laya'`, so a
caller with `governance:write` can record a calibrated-looking decision it invented — rework
`v1-w1c-3`, and the same defect makes `v1-w1c-4` (options as inline text, never checked) and
`v1-w1c-5` (an outage cannot become `gate_unavailable`) possible. The executor was the one honest
caller, and even it recorded the verdict without the call that produced it.

The verification design is forced by an existing architectural rule: a handler runs inside an open
transaction, and `process/judge.rs` forbids holding one across an HTTP call ("a judge call must
never hold a database transaction"). The call therefore belongs to the engine's external-action
path, and the ladder's job is to *prove* the call happened and is about this case. The
idempotency store already carries exactly that proof: `request_hash` (an equality witness over the
canonical body), `status_code` and the stored `response`, written by the engine under
`(tenant, scope="ladder.precheck", key)`, with `lookup` refusing a key that names a different body.
`action_invocation` adds the measured `duration_ms` and the `outcome`. No new provenance table is
needed.

## Alternatives rejected

- **A laya client inside `ladder_evaluate`.** It would hold a database transaction across a 60 s
  external call, against `judge.rs`'s stated rule, and it would make the action's latency the
  caller's problem in a way a retry cannot bound.
- **A job-driven evaluation (`ladder.evaluate` enqueues a job; the worker calls laya).** It makes
  the gate asynchronous and needs a new parked run state and a resume path — a different, much
  larger unit, for no gain: the gate needs a verdict before it can decide anything.
- **Trusting a `laya_call_id` the caller supplies as opaque proof.** Without re-deriving the case
  and comparing `request_hash`, a caller could point at a call for a different question and inherit
  its answer.
- **Keeping inline options and validating them.** SPEC_V2 §7.2 says options are authored,
  versioned rows, never free text from a prompt; validating inline text would still let the caller
  choose the menu.
- **Treating an unreadable answer as a refusal.** That is how an outage becomes a blocked card;
  §7.2 makes `Unavailable` follow the fail policy, and `closed` already means refuse.
- **Reusing `laya.judge` for the L1 request.** Its scope is `processes:write` and its question is
  the SOP step's; sharing it would let a governance caller reach a process action and blur two
  different decisions.

## Consequences

- The action surface gains `ladder.case`, `ladder.precheck` and `decision_option_sets.set|get`, and
  `ladder.evaluate` loses six input fields. The served-spec lock moves by those operations; the
  registry counts and the parity sample move with them.
- Migration `0068` adds two tables and one nullable column. A tenant with a `gate.precheck` policy
  and no authored option set is refused `409 option_set_missing` until one is authored — a
  deliberate fail-closed: the alternative is answering a question whose menu the caller chose.
- Every existing test that posted `answer`/`confidence` to `ladder.evaluate` now encodes a contract
  the product no longer has. They are rewritten to go through `ladder.case` → `ladder.precheck`
  (with the harness's laya stub) → `ladder.evaluate`, never relaxed.
- The executor's `gate.precheck` path is rewritten around the same builder; its existing
  transition tests stay, and a new assertion pins one laya request per gate.
- `v1-w1c-3`, `v1-w1c-4` and `v1-w1c-5` are cleared by this unit; the register records the PR.

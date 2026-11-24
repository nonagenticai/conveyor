# S21 step 10 — the L1 laya gate: receipt

Issue #656 · branch `issue/656-s21-l1-laya` · ADR `0018-l1-laya-provenance` · CONTRACT §28 ·
S21 §22 · resolves rework `v1-w1c-3`, `v1-w1c-4`, `v1-w1c-5`

**State: skeleton.** The measured scenarios, reversal proofs, test counts and residuals are filled
at completion, from the exact head, like `docs/evidence/S21-APPLY.md` and `S21-HANDOFF.md`.

## What changes at this level

Before: `ladder.evaluate` took `answer`, `confidence`, `latency_ms` and `probabilities` from its
request body and wrote `rung = 'laya'`, so any `governance:write` caller could record a
calibrated-looking L1 decision it had invented; options were inline request text; an outage could
not become `gate_unavailable`.

After: the ladder builds the case from the effective policy and an authored option set; the engine
performs the request as the external action `ladder.precheck`; `ladder.evaluate` records a decision
only from the engine's recorded exchange (input-hash equality, 2xx, measured duration), and an
unavailable answer follows the policy's fail action.

## Scenarios (filled at completion)

| Scenario | Command / test | Result | SHA |
|---|---|---|---|
| The precheck call records the case under the ladder's key | `crates/actions/tests/ladder_laya.rs::the_precheck_call_records_the_case` | — | — |
| Evaluate derives the verdict from the recording | `::evaluate_derives_the_verdict_from_the_recording` | — | — |
| A caller-supplied confidence is refused | `::a_caller_supplied_confidence_is_refused` | — | — |
| A call for another case is refused | `::a_call_for_another_case_is_refused` | — | — |
| The gate makes one laya call | `crates/actions/tests/internal_agent_runtime.rs::the_gate_makes_one_laya_call` | — | — |
| The question comes from the authored set | `::the_question_comes_from_the_authored_set` | — | — |
| An answer outside the set is refused | `::an_answer_outside_the_set_is_refused` | — | — |
| A case with no authored set is refused | `::a_case_without_an_authored_set_is_refused` | — | — |
| An outage follows the fail policy | `::an_outage_follows_the_fail_policy` | — | — |
| A case that does not fire is not asked | `::a_case_that_does_not_fire_is_not_asked` | — | — |
| Registry, parity and the lock | `crates/actions/tests/registry.rs`, `parity_http_mcp.rs`, `--check-lock` | — | — |

## Reversal proofs (fired, red, reverted, green — filled at completion)

1. Accept a caller-supplied confidence again → a fabricated row lands.
2. Skip the case verification → a call for another case is accepted.
3. Skip the option check → an out-of-set answer is recorded.
4. Ignore `fail_policy` → an outage is treated as a pass.
5. Let the executor call laya itself → two requests per gate.

## Rework register

`v1-w1c-3`, `v1-w1c-4`, `v1-w1c-5` gain `resolved_by` at completion; the audit's `--rework-check`
must print each as cleared.

## Residuals (kept, not fixed here)

- Automatic apply of an L1-accepted draft (ADR 0011:77) — the decision now exists honestly; what
  happens to an accepted draft is its own unit.
- The SOP judge (`laya.judge`, `process/step.rs`) keeps its own question and threshold.
- A tenant-authored `fire_when` UI; the structured expression is authored through the action.

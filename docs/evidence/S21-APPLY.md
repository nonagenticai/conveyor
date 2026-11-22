# S21 step 5 — the applying gate (issue #590)

**Measured at `2126904b`, 2026-11-22.** The PR's `ci` run **45064933007** is green:
`checks-shard (1)` **1925/1925 passed**, `checks-shard (2)` **1713/1713 passed**, the `checks`
aggregate passed, and `image`, `service-image`, `design-pack`, `dco`, `preflight`, `lint`,
`no-private-refs` and `secret-scan` passed. The final commit adds the executor-gate writer check
and its store test; PR #592 attests that final run.

## What changes

- A `handover` parks the run in `waiting_approval`; the `executor` approval names the draft.
- `approvals.decide` records the L3 `gate.draft_apply` decision, settles the run `running` and
  enqueues `run.resume`; an `executor` gate refuses a machine decider in the approval writer.
- `run.resume` applies the draft through the one hardened `apply_draft_tx` and finishes the run;
  it never calls the model.
- `ladder.record` refuses a forged human rung or a pending approval; `ladder.apply_draft` requires
  the decision to name the exact draft (rework `v1-w1c-2:T2`).

## Scenarios

| Scenario | Test | Result |
|---|---|---|
| P1.1 | `decision_ladder::a_generated_draft_cannot_transition_a_card` | pass at 2126904b |
| P1.2 | `draft_apply::an_unbound_decision_cannot_apply_a_draft` | pass at 2126904b |
| P1.3 | `draft_apply::a_decision_bound_to_another_draft_is_refused` | pass at 2126904b |
| P1.4 | `draft_apply::a_human_decision_requires_a_person_and_an_approved_approval` + the rewritten `decision_ladder::a_rule_a_laya_and_a_human_decision_share_one_audit_shape` | pass at 2126904b |
| P1.5 | `draft_apply::an_approved_human_decision_applies_its_draft` | pass at 2126904b |
| P2.1/P2.2 | `internal_agent_runtime::a_handover_gate_parks_the_run_and_opens_an_approval` | pass at 2126904b |
| P3.1 | `internal_agent_runtime::an_approved_handover_applies_the_draft_and_finishes_the_run` | pass at 2126904b |
| P3.2 | the same test (draft attached, one `inference_call`, one `run.succeeded`) | pass at 2126904b |
| P3.3 | `internal_agent_runtime::a_rejected_handover_cancels_the_run_and_leaves_the_draft` + `apps/server/tests/approval_expire.rs` | pass at 2126904b |
| P3.4 | `internal_agent_runtime::a_reclaimed_resume_is_a_no_op` | pass at 2126904b |
| P3 writer | `store::executor_gate::an_executor_gate_refuses_a_machine_decision` | final PR run |

## Reversal evidence

The negative scenarios are the guard proofs; each names the guard it exercises:

- remove the draft-binding check → `an_unbound_decision_cannot_apply_a_draft` would attach;
- remove the prompt-hash/document check → `a_decision_bound_to_another_draft_is_refused` would
  apply the wrong draft;
- remove the person check → `a_human_decision_requires_a_person_and_an_approved_approval` would
  record a service actor;
- remove the approval-state check → that test's pending approval would resolve the decision;
- remove the executor-gate writer check → `an_executor_gate_refuses_a_machine_decision` would
  approve a machine decision.

The guards were not separately deleted in a local run (no local Rust compilation); the negative
tests are the standing evidence.

## Generated

- `served-spec.lock.json` — **539** operations, `--check-lock` **OK**.
- Rework `v1-w1c-2:T2` — **cleared** by the acceptance command; the findings ledger marks it
  resolved.
- `seed/programme.json` links #15, #581 and #590; coverage, issues and the docs pack regenerated;
  `check-data`, `check-architecture`, `check-docs` and `publish-standards --check` green.

## Residuals

- Automatic application of an L1-accepted draft (a policy question).
- Mid-loop token/cost budget enforcement, `memory.remember` write-back, card-driven run creation,
  a `tool_binding` provisioning action, and a live model/laya proof.
- `run.execute`'s retry after a handler DB fault is a pre-existing #581 gap; a reclaimed job
  no-ops because the run has left `queued`.

# S21 step 5 — the applying gate (issue #590)

Status: in flight, 2026-11-22. This receipt is filled at the PR head; the tables below are the
scenario map, not results.

## What changes

- A `handover` parks the run in `waiting_approval`; the `executor` approval names the draft.
- `approvals.decide` records the L3 `gate.draft_apply` decision, settles the run `running` and
  enqueues `run.resume`.
- `run.resume` applies the draft through the one helper and finishes the run; it never calls the
  model.
- `ladder.record` refuses a forged human rung; `ladder.apply_draft` requires the decision to name
  the draft (rework `v1-w1c-2:T2`).

## Scenarios

| Scenario | Test | Result |
|---|---|---|
| handover parks the run, approval names the draft | `internal_agent_runtime::a_handover_gate_parks_the_run_and_opens_an_approval` | — |
| approval records the L3 decision and enqueues the resume | `approval_standalone::an_approved_executor_gate_records_the_l3_decision_and_enqueues_the_resume` | — |
| resume applies the draft, no second model call | `executor_apply::the_resume_applies_the_draft_without_regenerating` | — |
| rejection leaves the draft unattached | `executor_apply::a_rejection_leaves_the_draft_unattached` | — |
| expiry cancels the parked run | `apps/server/tests/approval_expire.rs` | — |
| a reclaimed resume is a no-op | `executor_apply::a_reclaimed_resume_is_a_no_op` | — |
| an unbound decision cannot apply | `draft_apply::an_unbound_decision_cannot_apply_a_draft` | — |
| another draft / hash is refused | `draft_apply::a_decision_bound_to_another_draft_is_refused` | — |
| a forged human rung is refused | `draft_apply::a_human_decision_requires_a_person_and_an_approved_approval` | — |
| an approved human decision applies | `draft_apply::an_approved_human_decision_applies_its_draft` | — |
| the old apply contract, rewritten | `decision_ladder::a_generated_draft_cannot_transition_a_card` | — |

## Reversal proofs

To fill at the branch head: each guard (the person check, the approval-state check, the draft
binding, the executor-gate human-only check, the resume's no-model rule) is removed in turn and its
own test turns red.

## Rework acceptance

```
python3 scripts/audit-programme-conformance.py --snapshot docs/design/data \
  --findings docs/evidence/v1-conformance-findings.json \
  --findings docs/evidence/standards-findings.json --rework-check v1-w1c-2:T2
```

To run after `docs/evidence/v1-conformance-findings.json` marks the finding resolved.

## Residuals

- Automatic application of an L1-accepted draft (a policy question).
- Mid-loop budget enforcement, `memory.remember` write-back, card-driven run creation, a
  `tool_binding` provisioning action, and a live model/laya proof.

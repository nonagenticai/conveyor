# Architecture analysis — T3 PM job (retrospective, 2026-11-28)

## Current state

Measured at the record, the PM job is one project run end to end: `projects.charter` records the
charter and its gate, the WBS carries the typed WSJF inputs 8, 8, 5 and 3, `projects.baseline`
freezes the scope, the staffing plan names an owner per deliverable, the gate router records every
decision and refuses self-approval, a change request precedes any baseline change, and
`projects.close` records the closure against the closure review. The record shows the project built
under closed issue #74 and reopened for the #526 retrospective rework on 2026-11-26. The proof is
executable: `crates/store/tests/programme_cadence.rs`, `refinement.rs`, `demo_gate.rs`, `retro.rs`,
`tranche_review.rs` and `benefit_transition.rs`, with `crates/actions/tests/pi_plan_approval.rs`,
`demo_credit.rs` and `apps/server/tests/cadence_jobs.rs`.

## Options considered

1. A PM-specific workflow engine versus the existing action engine with a sequencer. The existing
   engine was chosen: the PM job is a sequence of governed writes, not a second engine.
2. A per-project decision table versus the shared approval gate. The shared gate was chosen so the
   distinct-approver and delegation rules are enforced in one place and cannot be weaker for the PM
   job.
3. A baseline edited in place versus an immutable baseline with a change request. Immutability was
   chosen: the change request is the audit trail, and the frozen scope stays citable.
4. A closure decided by the PM versus a closure gate decided by a distinct approver. The distinct
   approver was chosen, and the record shows the closure gate decided by an approver other than
   its requester.

## Risks

- The PM job's value depends on the record it reads; an incomplete WBS or a missing demo credit is
  refused by the close gate by name, which keeps the closure honest.
- A change request that does not name the baseline would leave the change unanchored; the change
  carries the baseline reference and the registers carry the change.
- The cadence surfaces (PI plan, demo credits, retrospective) can drift from the project's own
  record; the tranche review reads both, and the tranche-review tests are the check.
- The retrospective itself is a risk: it reconstructs the record from the snapshot and the existing
  receipt rather than replaying every construction step, and it says so rather than claiming a
  fresh end-to-end run.

## Dependency map

The record declares `meta.depends_on: [T2, P0]`: T2's store carries the project, WBS, approval and
register rows the job writes, and P0 supplies the programme definition and the tranche row that
authorises the work. The PM job is a dependency of every delivery project that uses the same
charter, baseline and closure path, and of the programme's tranche machinery, which reads projects
through the same rows. Inside the project the seven A–D artefacts run from the stakeholder map and
the driver/goal catalogue through the requirements and the solution concept to the data entity and
technology standards catalogues.

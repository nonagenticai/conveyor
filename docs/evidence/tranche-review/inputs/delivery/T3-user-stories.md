# User Stories — T3 PM job

## Personas

- **The PM job** requests and plans work: it opens the charter, plan, baseline, change and
  closure gates, and never decides one.
- **The approver job** decides every PM gate, always as an actor distinct from the requester.
- **The programme manager** owns delivery and reads the baselined plan, the change register and
  the closure.
- **The architecture Board** approves the delivery contract the PM plans against, under the
  standing delegation.

## Stories

- As the PM job I can run a request end to end — charter, plan, scope, schedule and cost
  baselines — and read every row back.
- As the approver I decide the charter, baseline, change and closure gates, and the record
  shows a distinct approver on every one.
- As the programme manager I see a baseline change travel as a change request with an impact
  block and its own gate, never as a direct write.
- As the Board I approve the delivery contract the project is closed against.
- As the programme I close a project only against its frozen record: published outputs, an
  approved closure asset and gate, and the phase-G review.

## Scenarios

- Given a project with an approved charter, when the PM job calls `pm.plan` with WBS nodes,
  then the deliverable rows exist, the scope baseline is taken and its approval is open.
- Given an open gate, when an actor who requested it tries to decide it, then the action is
  refused and no decision row is written.
- Given an approved scope baseline, when a replacement is attempted without an approved change
  request, then the plan is refused before any WBS write.
- Given a closure request, when the published evidence is missing, then `projects.close`
  refuses and the project stays open.
- Given a closed project, when the record is read, then the closure review and the contract's
  phase-G review are published and cited.

## Verify

- `acceptance/programme/T3.toml`: the manifest's checks execute against the live rows
  (`scripts/verify-project.py --project T3`).
- `crates/actions/tests/spine_gates.rs`: evidence-less gates refused, change impacts enforced,
  self-declared approvals refused.
- `crates/actions/tests/programme_baseline.rs`: the taker cannot approve their own baseline; a
  distinct reviewer approves once; all three baseline kinds read back.
- `crates/actions/tests/closure_gate.rs`: a close on missing evidence is refused.

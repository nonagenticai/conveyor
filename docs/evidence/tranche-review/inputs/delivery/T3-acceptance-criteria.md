# Acceptance Criteria — T3 PM job

## Scenarios

- **T3-1 request end to end.** Given an approved charter, when the PM runs `pm.plan` and takes
  the scope baseline, then the WBS, baseline and approval rows exist and read back.
- **T3-2 no self-approval.** Given a gate the PM requested, when the same actor decides it, then
  the decision is refused and no approval_decision row is written.
- **T3-3 change request.** Given an approved baseline, when a replacement is attempted without an
  approved change request carrying an impact block, then the plan is refused before any write.
- **T3-4 change gate.** Given a change request, when its gate is approved by a distinct approver,
  then the baseline can advance and the change reads back as approved.
- **T3-5 closure on the record.** Given a closure request, when any required row is missing
  (published output, approved asset, baselines, contract, Phase-G review), then `projects.close`
  refuses and the project stays open.

## Verify commands

- `python3 scripts/verify-project.py --project T3 --snapshot <snap>/data`
- `cargo test -p conveyor-actions --test spine_gates` (in CI)
- `cargo test -p conveyor-actions --test programme_baseline` (in CI)
- `cargo test -p conveyor-actions --test closure_gate` (in CI)

## Results

Measured at the tranche-3 review's snapshot: the manifest's checks execute against the live rows;
the receipt's per-check verdicts are the record. The closure is the live occurrence of scenario
T3-5 against the full record.

## Status

Accepted and closed: the five deliverables were accepted under change `01a2052e-5648…` and
credited at the tranche-3 demo; the project closed against its frozen baseline with the
retrospective ADM artefacts and the register rows in place.

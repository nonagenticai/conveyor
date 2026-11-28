# Definition of Done — T3 PM job

## Acceptance criteria

The `acceptance/programme/T3.toml` gate executes green: the project is closed; the scope,
schedule and cost baselines are approved by a distinct approver; the charter asset exists and
its gate is approved; the declared closure outputs are published; the delivery contract is live
and board-approved; the Phase-G review cites the standing delegation; the closure gate was
decided by an approver other than its requester; no waiver is expired; the PI cadence, plan,
objectives, WSJF inputs and demo credits exist; the retro produced a lesson; no open P0 remains;
the mandatory artefacts are evidenced; no P0/P1 rework package remains.

## Evidence at completion

The acceptance receipt (`verify-project.py --project T3`) at the deployed sha; the published
closure review; the change `01a2052e-5648…` authorising the deliverables' acceptance; the
delivery contract `01a2052f-29da-76c4-9bda-e45e0f408fe8` and its Phase-G review
`01a2052f-cc4e…`; the five accepted deliverables and five demo credits; the retrospective ADM
artefacts (Phase A–D) built from this record.

## Repo gate

`cargo fmt --all -- --check`, `cargo metadata --locked`, the repository's preflight, and the
PR's CI; the served-spec lock check.

## Merge and closure

Issue #74, one branch, one PR; the closure gate (`projects.close_request`) requested by the PM
and decided by a distinct approver; `projects.close` runs the full record; the tranche-3 review
reads the receipt at the snapshot's head sha.

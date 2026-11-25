---
document: T5/evidence
version: 1
status: published
project: T5
category: evidence
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-25
artifact_kind: evidence
phase: G
plateau: capability
---

# T5 — Kata: evidence

## Claim

T5 delivers the kata loop: a planned procedure change opens a kata whose prediction is recorded
before its measurement, the measurement runs on a held-out split, a measured regression reverts the
candidate, a flat or incomparable result adopts nothing, the kata cannot approve its own adoption,
and the open/adopt gates refuse the documented cases.

## Evidence

- `crates/actions/tests/kata_gets.rs::the_prediction_is_recorded_before_the_measurement`
- `crates/actions/tests/kata_measure.rs::a_procedure_change_is_measured_on_the_held_out_split`
- `crates/actions/tests/kata_revert.rs::a_measured_regression_reverts_the_candidate`,
  `::a_flat_or_incomparable_result_adopts_nothing`
- `crates/actions/tests/kata_adopt.rs::the_kata_cannot_approve_its_own_adoption`
- `crates/actions/tests/kata_open.rs::a_kata_without_an_instrument_is_not_opened`,
  `::a_second_kata_for_an_open_signature_is_refused`
- Migrations `0066_kata.sql` and `0067_kata_gate.sql`; merges `7e617673` (PR #636) and
  `f8c82c68` (PR #654).

## Verification

Run the suite on a checked-out main: `cargo test -p conveyor-actions --test kata_adopt --test
kata_gets --test kata_measure --test kata_open --test kata_revert`. The CI equivalent is the PR's
`preflight` and `ci-public` at the merge commits above.

## Links

- `docs/plans/inception/T5/00-project-brief.md`, `01-inception-analysis.md`,
  `02-architecture-design.md`
- Issues #77 and #653; PRs #636 and #654.

## Gap

The tranche-10 authorisation is not evidenced here and is not claimed — it is the #526 programme
chain's act. No delivery gap is claimed by the issue record.

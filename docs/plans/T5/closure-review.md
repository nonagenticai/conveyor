---
document: T5/closure-review
version: 1
status: published
project: T5
category: closure-review
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-25
artifact_kind: closure-review
phase: G
plateau: capability
---

# T5 — Kata: closure review

Whether the project delivered its acceptance, on its evidence, and the decision that follows.

## Outcome vs acceptance

| Acceptance criterion | Evidence | Result |
|---|---|---|
| A planned procedure change opens a kata whose prediction is recorded before the measurement | `crates/actions/tests/kata_gets.rs::the_prediction_is_recorded_before_the_measurement`; the `kata.*` action family in `docs/design/data/actions.json`; migration `0066_kata.sql` | pass |
| The measurement runs on the held-out split | `crates/actions/tests/kata_measure.rs::a_procedure_change_is_measured_on_the_held_out_split` | pass |
| A measured regression reverts the candidate; a flat or incomparable result adopts nothing | `crates/actions/tests/kata_revert.rs::a_measured_regression_reverts_the_candidate`, `::a_flat_or_incomparable_result_adopts_nothing` | pass |
| The kata cannot approve its own adoption | `crates/actions/tests/kata_adopt.rs::the_kata_cannot_approve_its_own_adoption`; migration `0067_kata_gate.sql` | pass |
| A kata without an instrument is not opened; a second open kata for the same signature is refused | `crates/actions/tests/kata_open.rs::a_kata_without_an_instrument_is_not_opened`, `::a_second_kata_for_an_open_signature_is_refused` | pass |
| The project's programme row and coverage agree | issue #77 closed by PR #636 (`7e617673`); #653 closed by PR #654 (`f8c82c68`); `build-coverage.py --check` and `build-programme-seed.py --check` green at `7d5ef094` | pass |

## Evidence

- Delivery merges: `7e6176733924b6bc695a7cf10a68dd5df061985c` (PR #636) and
  `f8c82c689cc4e0a848f68da1fcb520cb42d71f9e` (PR #654).
- Schema: `migrations/0066_kata.sql`, `migrations/0067_kata_gate.sql`.
- Behaviour: the `kata.*` action family (`crates/actions/src/kata.rs`,
  `crates/store/src/kata.rs`, `crates/domain/src/kata.rs`) and its integration tests under
  `crates/actions/tests/kata_*.rs`.
- The inception set: `docs/plans/inception/T5/` (brief, analysis, design).

## Open items

- The tranche-10 authorisation is **not** claimed here: the tranche's state is `planned` and its
  authorisation is the #526 programme chain. This review records delivery, not authority.
- No open delivery item is named by the issue record.

## Decision

Close. Acceptance is met on the evidence above; the tranche authorisation remains a separate
programme act (#526), and this review does not manufacture it.

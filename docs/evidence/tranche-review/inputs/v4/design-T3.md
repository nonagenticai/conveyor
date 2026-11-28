# Architecture design — T3 PM job (retrospective, 2026-11-28)

## Context and boundary

This design owns the PM job: the project's lifecycle actions and the gates between them. It must
not duplicate the approval engine, the store, the cadence surfaces or the registers; it sequences
them and refuses a step whose precondition is not met. The blueprint it fits is the one the
requirements catalogue and the solution concept name: one project run end to end, with the
approver kept distinct from the requester and a baseline change forced through a change request.

## Aggregates, tables and actions

- Aggregates: the project, the WBS node, the scope baseline, the approval and its decision, the
  register item and the cadence row.
- Tables: the project and WBS rows, the approval and decision rows, the register rows and the
  baseline rows — the same rows the close gate reads.
- Actions: `projects.charter`, `projects.baseline`, `projects.close`, `approvals.request`,
  `approvals.decide`, the cadence actions and the retrospective actions; a baseline change is
  `changes.request` and a register row, never an edit.
- Events: each gate decision, the demo credit per accepted deliverable, the PI plan approval and
  the tranche review.

## Decisions

- The approver must differ from the requester; a rule decision is accepted only under a published
  delegation naming the scope, and the decision records `decided_by_kind`.
- A baseline change requires a change request naming the baseline; the frozen row is superseded,
  never edited.
- Closure reads the declared outputs, the closure review, the live delivery contract and its
  Phase-G review, the approved scope baseline, the seven A–D artefacts and their registers, the
  WBS WSJF inputs and a demo credit per accepted deliverable.
- The charter gate and the closure gate are separate decisions; a single approval cannot stand for
  both.

## Acceptance

`acceptance/programme/T3.toml` carries the gate, and SPEC_V2 §12 states it in prose. The proof is
the cadence and gate suites — `crates/store/tests/programme_cadence.rs`, `refinement.rs`,
`demo_gate.rs`, `retro.rs`, `tranche_review.rs` and `benefit_transition.rs`, with
`crates/actions/tests/pi_plan_approval.rs`, `demo_credit.rs` and
`apps/server/tests/cadence_jobs.rs` — and the record's own closure: the project's closure review is
published and its closure gate was decided by an approver distinct from its requester.

## Architecture artefacts

The design's record is the seven Phase A–D artefacts — stakeholder map, driver/goal/objective
catalogue, requirements catalog, solution concept, business capability map, data entity catalogue
and technology standards — together with the five register kinds and the WBS deliverable. Each
artefact is a typed row with its entries and links and is published as its own page.

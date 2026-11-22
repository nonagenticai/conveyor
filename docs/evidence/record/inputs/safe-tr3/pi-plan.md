# PI Plan — conveyor buildout, tranche 3

The plan for tranche 3 (S04, T13, T3): the objectives this increment is judged against, the
deliverables materialised for it, and the constraints the order respects.

## Objectives

- **O1 (BV 10, committed).** T13 proves the governed call path: release-grant enforcement at
  `tools.call`, job storage over the broker, retirement over the zero-use report, and long-op
  resume (`v1-w4a-5`).
- **O2 (BV 8, committed).** The v1 verdict is a published table: every SPEC_V2 §12 slice gate is
  executed and recorded `implemented | partial | absent` with evidence (`scripts/verify-slices.py`).
- **O3 (BV 9, committed).** The three registers are operated: fourteen programme risks with owners
  and review dates, fifty-seven ADR rows, and B1–B7 baselined and evaluated.
- **O4 (BV 6, committed).** The record instruments tell the truth: render, strict audit, register
  checks and a promotion journal that replays every programme write.
- **O5 (BV 6, uncommitted).** S04 delivers the ActionSpec derive crate and the registry and spec
  vocabularies with manifests that execute their own gates.
- **O6 (BV 5, uncommitted).** T3 records the change path, the scope baseline, the compliance review
  and the closure with published evidence.

## Plan

- T13: the tool platform deliverable (`01a20277-8d10-702f-a975-4dd6469e7b1d`) is accepted with
  typed evidence, and the P1 batch (job storage, release grant, retirement, long-op resume) lands
  on the lane.
- S04: two open deliverables (the ActionSpec derive crate; the registry and spec vocabularies) and
  the verdict harness that executes the manifests.
- T3: the change path (consuming `v1-change-1`), the scope baseline, the compliance review and the
  closure.

## WSJF order

Ordered by business value against the tranche's risk: O1 (10), O3 (9), O2 (8), O4 (6), O5 (6),
O6 (5). The score itself is computed from the four typed inputs by the standard's `wsjf_order`
check wherever accepted WBS rows carry them; it is never stored on the plan.

## Capacity and WIP

One live-record writer; one Rust build at a time in the shared target; at most six concurrent
read/audit agents; one open card per writer. The tranche is a record boundary, not a calendar:
tranche 4 is authorised as soon as the tr3 review is recorded and the dependency graph allows.

## Evidence

- Cadence row `01a20280-a739-7bb9-823f-232f34b32aaa` (kind `pi`, period `tranche-3`).
- Plan gate approval `01a2051a-2908-7464-9b8e-5b9fa7b0b29f` — an `approved` PI-plan gate with a
  `rule` decision carrying `via.delegation_ref` `01a2021d-cca2-757c-bfc2-21bd4e4b6d85`.
- Tranche row `01a1e9e0-eb54-74d8-a2b5-44fb2b87eb52`.
- The promotion journal `docs/evidence/record/journal.jsonl`.

## Dependencies

- T3 depends on `v1-change-1` (the approval's `meta.change_id`).
- T13 depends on `v1-p1110-1`, `v1-p119-1` (retirement half; the kata half is re-anchored to T5)
  and `v1-w4a-5`.
- S04 depends on the verdict harness executing its manifests.

## Backlog refinement

The tr3 refinement cadence `01a20280-d52b-7791-a9b1-ebc2a10b5c3d` re-scored the backlog before
this plan was taken: `v1-w4a-5` moved from unscored to committed, the ADR seed to delivered, the
kata contract-test instrument to T5 scope, and the programme artefact factory to the published
programme set with per-project cards following.

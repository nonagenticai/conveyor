# PI Objectives — conveyor buildout, tranche 3

The six objectives of tranche 3, their planned business value and the measurement that decides
whether each was met.

## Objectives

- **O1.** T13 proves the governed call path: release-grant enforcement, job storage, retirement
  over the zero-use report, long-op resume.
- **O2.** The v1 verdict is a published table, one row per SPEC_V2 §12 slice gate.
- **O3.** The three registers are operated: risks, ADRs and benefits with live rows and screens.
- **O4.** The record instruments tell the truth: render, strict audit, register checks, journal.
- **O5.** S04 delivers the ActionSpec derive crate and the registry and spec vocabularies.
- **O6.** T3 records the change path, baseline, compliance review and closure.

## Business value

O1 10, O3 9, O2 8, O4 6, O5 6, O6 5. The committed set is O1–O4 (35 of 44 planned points); O5 and
O6 are stretch items for the tranche.

## Measurement

- O1: broker tests (release grant, storage round-trip, zero-use retirement, delayed-stub resume)
  plus the accepted deliverable's typed evidence.
- O2: `scripts/verify-slices.py` exits 0 and publishes the table; the table's own negative
  self-test proves a failing gate is reported `absent`.
- O3: live counts — fourteen risk rows, fifty-seven ADR rows, seven benefit baselines, profiles
  active; `check-registers.py` exits 0.
- O4: `generate-standards.py --check`, `audit-programme-conformance.py --strict`,
  `verify-findings.py` and `promote-record.py --check` all exit 0 at the tranche head.
- O5: the project manifest's gates execute their own checks; `verify-project.py --project S04`.
- O6: T3's records exist and its manifest passes.

## Evidence

- Cadence `01a20280-a739-7bb9-823f-232f34b32aaa`; approval `01a2051a-2908-7464-9b8e-5b9fa7b0b29f`.
- The v1 findings ledger at `docs/evidence/v1-conformance-findings.json` (188 findings, the
  tr3 re-anchor recorded).
- The promotion journal and the artefact pack.

## Risks

- R2 record theatre: a section resolving thinly is not evidence; the deep review samples it.
- R10 v1 quality unknown: the verdict table may find real work; that is the instrument working.
- R13 evidence theatre: every claim here is read back from the record, not asserted.

## Links

- Plan: `docs/plans/00-INDEX.md` and the tr3 slice plans.
- Tranche: `01a1e9e0-eb54-74d8-a2b5-44fb2b87eb52`.

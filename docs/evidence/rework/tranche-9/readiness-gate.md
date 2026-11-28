# Tranche-9 readiness gate — measured 2026-11-28

snapshot: `/tmp/snap-9` — 17 data files, 981 published outputs, taken from
`issue/954-tranche-9` at `87e278f319dd0f8629e80482cb61705677aebd00`. The manifest's
`generated_at` is `2026-11-28T15:09:49.665752Z`; it is **not** the run clock — the product
derives it from the newest `updated_at` among the `documents` artefacts
(`crates/pack/src/snapshot.rs::snapshot_generated_at`), so it pins the artefact set, not the
instant. The run was made on 2026-11-28 after the three packs were committed.

## Content packs

```
rework-content-check: S18 content is consistent with the facts
exit=0
rework-content-check: S20 content is consistent with the facts
exit=0
rework-content-check: S21 content is consistent with the facts
exit=0
```

Commands (all at the pinned snapshot):

- `python3 scripts/rework-content-check.py --project S18 --content docs/evidence/rework/tranche-9/rework-content-S18.json --snapshot /tmp/snap-9/data --wsjf 9,6,9,13 --closes false`
- `python3 scripts/rework-content-check.py --project S20 --content docs/evidence/rework/tranche-9/rework-content-S20.json --snapshot /tmp/snap-9/data --wsjf 7,5,6,5 --closes true`
- `python3 scripts/rework-content-check.py --project S21 --content docs/evidence/rework/tranche-9/rework-content-S21.json --snapshot /tmp/snap-9/data --wsjf 9,6,7,8 --closes false`

Each pack carries the seven A–D artefacts in the standard's order, the five registers, the
charter, the Statement of Architecture Work, the delivery contract, the Phase-G review and both
declared output bodies. S18 (level `consistent`) and S21 (level `compliant`) carry no closure key;
S20 (level `compliant`) carries `closure` and `closure_review`.

## S20's pin — the one acceptance-standard change

S20's record row is `closed` and its only linked issue (#14) is closed, while
`acceptance/programme/S20.toml` pinned `S20-project` `active`. That pin failed
`verify-project.py::check_project` as P0 and made `rework-batch.py::closes_project` return false,
so an active pack could not carry the closure review the register demands.

The pin now states `closed`, the record's measured status. Verified two ways:

- `python3 -c '<S20-project regex>'` → `S20-project pin: closed`
- `importlib`-loaded `rework-batch.py` → `manifest_status('S20') == 'closed'`;
  `closes_project({"key":"S20","meta":{"issues":[14]}}, {14:"CLOSED"}, content) == True`;
  `manifest_status('S18') == 'active'` and `manifest_status('S21') == 'active'` both return
  `closes_project(...) == False`.

A `closed` pin returns true unconditionally, so `--closes true` proves the pin, not the content.
The factory still refuses a close without `content["closure"]` and `content["closure_review"]`
(`scripts/rework-project.py:1038, 1277`), which is why the pack authors both. No script or test
reads `S20-project` besides `manifest_status`.

## Record readiness (replay record, schema 110)

- Four `programme_cadence` rows at `period='tranche-9'`: `pi` `01a222b4-5097-7fee-94ac-e7adeb1bb79c`,
  `refinement` `01a222b4-513e-761c-af98-64ef0334e89b`, `demo` `01a222b4-51df-7576-997f-ad1f6fdc66f0`,
  `retro` `01a222b4-5278-757b-aa65-184fd4d5de28`.
- The `pi` row (version 4) carries `meta.pi_plan` (scope S18/S20/S21, capacity, WIP limits,
  four objectives, five dependency edges) and `meta.documents`: `pi-plan`
  `01a222b4-5dee-7060-a889-af69cf6d2ddf`, `pi-objectives` `01a222b4-5eb7-777f-b317-c0e445e762d7`
  — both published, **P0-scoped** (`01a1dcb8-d24b-76de-b901-56fd56e001bc`) and `unit`
  `tranche-9`, read back from `outputs.js`.
- The `refinement` row (version 3) carries `meta.refinement.wsjf_inputs` — S18 9/6/9/13,
  S20 7/5/6/5, S21 9/6/7/8 — matching the three packs' WBS integers and the `--wsjf` each
  checker ran with.
- The PI-plan gate is `approved` (approval `01a222b4-f15c-7c4e-ae37-153f5bfc3e80`) as a `rule`
  decision under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the `pi` row's
  own state stays `awaiting_approval`, as tranche 8's does.
- The tranche lesson `01a222b5-1061-79dc-be2f-b1bff735a0e0` is linked to the retro cadence with
  `independent_count=2` (anchors `scripts/verify-project.py#check_project` and
  `acceptance/programme/S20.toml`).
- Ten readiness acts are journaled (`docs/evidence/record/journal.jsonl`, 508 entries,
  `promote-record.py --check` green), with their inputs under `docs/evidence/record/inputs/tr9/`.
- `outputs.js` carries both PI rows under `unit: "tranche-9"`; the period check
  (`verify-project.py::_published_for_period`) sees them.

The snapshot assertion in the issue's P1.1 passes:

```
python3 -c '… msp.json readiness assertion …'
tranche-9 readiness: ok
```

## The 37 open packages and the route that clears each

| scope | P0 | P1 | P2 | cleared by |
|---|---|---|---|---|
| S18 | 0 | 3 | 8 | `project_in_unauthorised_tranche` by authorisation only; `unpublished_declared_output` and `mandatory_artefacts` by the pack; the eight P2s by the pack's charter/baseline/registers/WBS/artefacts/contract/review |
| S20 | 1 | 4 | 8 | `project_in_unauthorised_tranche` by authorisation only; `closed_without_closure_review` and `closure_outputs_undeclared` by the published closure review and the factory's `closure_outputs` declaration; `unpublished_declared_output` and `mandatory_artefacts` by the pack; the eight P2s by the pack |
| S21 | 0 | 3 | 8 | the same three P1 routes as S18, and its eight P2s by the pack |
| `tranche-9` | 0 | 2 | 0 | `pi_cadence_missing` and `backlog_refinement_missing` by the four cadence rows and the refinement record above |

The P0 is not a pack's to discharge: only the programme owner's gate decision authorises the
tranche, and the package clears when the audit next reports on an authorised tranche.

## Batch gate — the expected authorisation precondition

```
$ python3 scripts/rework-batch.py --tranche 9 --snapshot /tmp/snap-9/data \
    --content-dir docs/evidence/rework --journal docs/evidence/rework/batch/tranche-9.jsonl --dry-run
tranche 9 is 'planned'; only authorised/closed/in_review may be reworked
exit=1
```

This is the **only** refusal: `rework-batch.py::ensure_authorised` refuses a `planned` tranche
before it reads the roster or the packs. #755 records the tranche-8 review and authorises tranche
9; on a fresh snapshot after that, the same command must exit 0 with all three members
`PREFLIGHTED`. Exit 0 is not claimed here — the refusal is construction's boundary, not a
readiness proof.

## Delivery gates

```
python3 docs/design/scripts/build-docs.py --check        -> check-docs: 545 sources, 0 failure(s)
node docs/design/scripts/check-docs.cjs                  -> check-docs: 12 checks passed
node docs/design/scripts/check-data.cjs                  -> check-data: 896 checks passed
python3 scripts/promote-record.py --check                -> 508 entries; journal is well formed
git diff --check                                         -> clean
```

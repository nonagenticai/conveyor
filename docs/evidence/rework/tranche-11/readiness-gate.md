# Tranche-11 readiness gate — measured 2026-11-28

snapshot: `/tmp/snap-11` — 17 data files, 987 published outputs, taken from
`issue/963-tranche-11` at `0f72a3deda00a74bbe1eaccda826e42ceff78a54`. The manifest's
`generated_at` is `2026-11-28T16:09:13.158006Z` and its sha256 is
`804baf81e4d582063062e69ecc6b8b23e1d6e5708f323cac0252c56d0807cce0`. `generated_at` is **not** the
run clock — the product derives it from the newest `updated_at` among the `documents` artefacts
(`crates/pack/src/snapshot.rs::snapshot_generated_at`), so it pins the artefact set, not the
instant. The run was made after the readiness rows and the three packs were committed.

## Content packs

```
rework-content-check: S14 content is consistent with the facts
exit=0
rework-content-check: T6 content is consistent with the facts
exit=0
rework-content-check: T8 content is consistent with the facts
exit=0
```

Commands (all at the pinned snapshot):

- `python3 scripts/rework-content-check.py --project S14 --content docs/evidence/rework/tranche-11/rework-content-S14.json --snapshot /tmp/snap-11/data --wsjf 8,7,9,8 --closes false`
- `python3 scripts/rework-content-check.py --project T6 --content docs/evidence/rework/tranche-11/rework-content-T6.json --snapshot /tmp/snap-11/data --wsjf 9,6,8,8 --closes true`
- `python3 scripts/rework-content-check.py --project T8 --content docs/evidence/rework/tranche-11/rework-content-T8.json --snapshot /tmp/snap-11/data --wsjf 8,5,7,5 --closes true`

Each pack carries the seven A–D artefacts in the standard's order, the five registers, the charter,
the Statement of Architecture Work, the delivery contract, the Phase-G review, both declared output
bodies, and the case's document set: S14 (active) carries the architecture-artifact index and the
four BDD delivery documents plus the analysis/design/SoAW documents; T6 and T8 (closed) carry the
same nine and add `closure` and `closure_review`.

## T6 and T8 — the acceptance-standard change

Both record rows are `closed` and their only linked issues (#80, #79) are closed, while
`acceptance/programme/{T6,T8}.toml` pinned `<KEY>-project` `active`. That pin failed
`verify-project.py::check_project` as P0 and made `rework-batch.py::closes_project` return false, so
an active pack could not carry the closure review each close requires.

Both pins now state `closed`, the record's measured status (the S20 precedent). Verified two ways:

- the regex read-back in `pins.json` → `T6-project pin: closed`, `T8-project pin: closed`;
- `importlib`-loaded `rework-batch.py` → `manifest_status('T6')='closed'`,
  `manifest_status('T8')='closed'`, `manifest_status('S14')='active'`; `closes_project` true for
  T6/T8 and false for S14.

A `closed` pin returns true unconditionally, so `--closes true` proves the pin, not the content. The
factory still refuses a close without `content["closure"]` and `content["closure_review"]`, which is
why both packs author them. S14 stays `active` because #29 is open; the factory takes the
`--keep-active` route for it.

## Record readiness (replay record, schema 110)

- Four `programme_cadence` rows at `period='tranche-11'`: `pi` `01a222e7-0eb9-7386-8e46-4b8f39fb8302`
  (version 2), `refinement` `01a222e7-1b99-7069-90d9-c34763ac508b` (version 2), `demo`
  `01a222e7-1c46-7ffe-94d3-983ede8ad40e`, `retro` `01a222e7-1cff-79d4-92b4-9f979eb4dd4b`.
- The `pi` row carries `meta.pi_plan` (scope S14/T6/T8, capacity, WIP limits, four objectives, nine
  dependency edges) and `meta.documents`: `pi-plan` `01a222e7-31e3-738a-a4a6-854970f4bff2`,
  `pi-objectives` `01a222e7-42de-739b-9381-6f5743287704` — both published, **P0-scoped**
  (`01a1dcb8-d24b-76de-b901-56fd56e001bc`) and `unit` `tranche-11`, read back from `outputs.js`.
- The `refinement` row carries `meta.refinement.wsjf_inputs` — S14 8/7/9/8, T6 9/6/8/8, T8 8/5/7/5 —
  matching the three packs' WBS integers and the `--wsjf` each checker ran with.
- The PI-plan gate is `approved` (approval `01a222e7-9198-7400-8633-d3542cb68347`) as a `rule`
  decision under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the `pi` row's own
  state stays `awaiting_approval`, as tranches 8 and 9 do.
- The tranche lesson `01a222e7-9249-743b-96cb-d2bd520092a2` is linked to the retro cadence with
  `independent_count=2` (anchors `docs/evidence/rework-register.json` and
  `seed/output-standard.json`).
- Ten readiness acts are journaled (`docs/evidence/record/journal.jsonl`, 524 entries,
  `promote-record.py --check` green), with their inputs under `docs/evidence/record/inputs/tr11/`.
- The PI objectives document was revised once after its first publication to carry the MSP benefit
  mapping; the record holds version 2, and the input files committed here are the version-2 bodies.

The snapshot assertion in the issue's P1.1 passes:

```
python3 -c '… msp.json + outputs.js readiness assertion …'
tranche-11 readiness: ok
```

## The tranche's gaps at the pinned snapshot

The live audit (`python3 scripts/audit-programme-conformance.py --snapshot /tmp/snap-11/data
--report --out /tmp/audit11`) reports 63 gaps programme-wide. Exactly three name a tranche-11
subject, and all three are the same rule:

| rule | subject | severity | cleared by |
|---|---|---|---|
| `project_in_unauthorised_tranche` | T6 (closed in a `planned` tranche) | P0 | #755's tranche-10 review, then tranche 11's authorisation |
| `project_in_unauthorised_tranche` | T8 (closed in a `planned` tranche) | P0 | the same |
| `project_in_unauthorised_tranche` | S14 (active in a `planned` tranche) | P1 | the same |

Neither `pi_cadence_missing:tranche-11` nor `backlog_refinement_missing:tranche-11` appears: the
four cadence rows cleared both, which is the readiness half of this gate. No pack can discharge the
remaining three — only the programme's own review can.

The committed register at the lane base (`docs/evidence/rework-register.json`,
`main@ea03b188a`) carries 629 open packages, of which the tranche-11 subset was exactly those two
cadence packages (both `scope: tranche-11`, `owner_project: T10`). Per member the package count is
zero, but the standard's `scope.through_tranche: 8` marks the artefact packages `scheduled`; the
register's `kinds` rows at the snapshot carry the real work the packs author:

| member | declared `now` categories | missing at the snapshot | the pack's route |
|---|---|---|---|
| S14 | 17 (+ closure) | 16 (+ closure-review, not claimed while active) | published documents, baseline, registers, WBS, contract and Phase-G review |
| T6 | 17 (+ closure) | 13 | the same, plus the retrospective closure review |
| T8 | 17 (+ closure) | 15 | the same, plus the retrospective closure review |

This gap is the lesson recorded at `lesson:tranche-11:k1`: the package count is scope-filtered, so a
tranche preparation reads each project's missing list rather than the package list.

## Batch gate — the expected authorisation precondition

```
$ python3 scripts/rework-batch.py --tranche 11 --snapshot /tmp/snap-11/data \
    --content-dir docs/evidence/rework --journal docs/evidence/rework/batch/tranche-11.jsonl --dry-run
tranche 11 is 'planned'; only authorised/closed/in_review may be reworked
exit=1
```

This is the **only** refusal: `rework-batch.py::ensure_authorised` refuses a `planned` tranche
before it reads the roster or the packs. #964 prepares tranche 10; #755 records its review and
authorises tranche 11; on a fresh snapshot after that, the same command must exit 0 with all three
members `PREFLIGHTED`. Exit 0 is not claimed here — the refusal is construction's boundary, not a
readiness proof.

## Delivery gates

```
python3 docs/design/scripts/build-docs.py                -> build-docs: 567 documents -> docs/design/docs
python3 docs/design/scripts/build-docs.py --check        -> check-docs: 567 sources, 0 failure(s)
node docs/design/scripts/check-docs.cjs                  -> check-docs: 12 checks passed
node docs/design/scripts/check-data.cjs                  -> check-data: 48 checks passed
node docs/design/scripts/check-standards.cjs             -> check-standards: 1300 artefacts, 489 evidenced, 239 partial, 572 absent (exit 0)
python3 scripts/promote-record.py --check                -> 524 entries; journal is well formed
git diff --check                                         -> clean
```

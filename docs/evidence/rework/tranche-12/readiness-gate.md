# Tranche-12 readiness gate — measured 2026-11-28

snapshot: `/tmp/snap-12` — 17 data files, 985 published outputs, taken from
`issue/961-tranche-12` at `c9e45bd814c84103a0671a68bec2c2e79e2d2209`. The manifest's
`generated_at` is `2026-11-28T16:03:37.920353Z`; it is **not** the run clock — the product derives
it from the newest `updated_at` among the `documents` artefacts
(`crates/pack/src/snapshot.rs::snapshot_generated_at`), so it pins the artefact set, not the
instant. The run was made after the pack, the readiness inputs and the manifest were committed.

## Content pack

```
rework-content-check: T15 content is consistent with the facts
exit=0
```

Command (at the pinned snapshot):

- `python3 scripts/rework-content-check.py --project T15 --content docs/evidence/rework/tranche-12/rework-content-T15.json --snapshot /tmp/snap-12/data --wsjf 8,5,8,8 --closes true`

The pack carries the seven A–D artefacts in the standard's order, the five registers, the charter,
the Statement of Architecture Work, the delivery contract, the Phase-G review, both declared output
bodies, the closure and the closure review. T15 is closed, so `--closes true`; the content checker
proves the pack's content, not the pin.

## T15's pin and gate text — the one acceptance-standard change

T15's record row is `closed` (measured `built`, `closure_outputs` `[closure-review]`) while
`acceptance/programme/T15.toml` pinned `T15-project` `active` and its gate text said "T15 is NOT
built". The pin failed `verify-project.py::check_project` as P0 and made
`rework-batch.py::closes_project` return false; the text is not machine-read, but it contradicted
the record and the published closure review.

The manifest now pins `closed`, and the gate text states the delivered acceptance, cites PR #666
(merge `a2ef9194eb2c`) and the published closure review `01a2121b-2e58-7407-84a3-3329dc2c1a8c`.
Verified two ways:

- `python3 -c '<T15-project regex>'` → `T15 manifest: closed, gate text reconciled`
- the record row `01a1d039-0d37-7f20-9959-979c31daeb5e` reads `closed`, `meta.measured_status
  built`, `meta.closure_outputs ["closure-review"]`.

The factory composes a **retrospective** closure review from the pack (the dedupe key is the word
"retrospective" in the title); the existing published closure review stays as the historical
authored record. Both are published, the newest is the pack's restatement, and nothing is deleted.

## Record readiness (replay record, schema 110)

- Four `programme_cadence` rows at `period='tranche-12'`: `pi` `01a222e1-ece8-74d3-aa15-b2dd57f9be5f`,
  `refinement` `01a222e1-edb2-797f-9d93-80964422b34f`, `demo` `01a222e1-ee63-742d-a9ad-95624a5f87ea`,
  `retro` `01a222e1-ef0d-7bf9-8480-a0f4a64133ee`.
- The `pi` row (version 3) carries `meta.pi_plan` (scope T15, capacity, WIP limits, objectives O1–O3,
  the refinement cadence link and T15's three delivered dependency edges) and `meta.documents`:
  `pi-plan` `01a222e2-4939-767c-9536-4ca371526284`, `pi-objectives`
  `01a222e2-2b3e-72a8-b886-45d213dc0768` — both published, **P0-scoped**
  (`01a1dcb8-d24b-76de-b901-56fd56e001bc`) and `unit` `tranche-12`, read back from `documents.get`
  and carried by the snapshot's `outputs.js`.
- The `refinement` row (version 3) carries `meta.refinement.wsjf_inputs` — T15 8/5/8/8 — matching
  the pack's WBS integers and the `--wsjf` the checker ran with.
- The PI-plan gate is `approved` (approval `01a222e2-a316-723c-afae-0fdde7df187a`) as a `rule`
  decision under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the `pi` row's own
  state stays `awaiting_approval`, as tranches 8 and 9 do.
- The tranche lesson `01a222e2-d4e6-76fe-a524-32355e0e920d` is linked to the retro cadence with
  `finding_key k1` and two anchors (`acceptance/programme/T15.toml` and
  `docs/plans/T15/closure-review.md`); the statement records the measured manifest trap.
- Ten readiness acts are journaled (`docs/evidence/record/journal.jsonl`, 524 entries,
  `promote-record.py --check` green), with their inputs under `docs/evidence/record/inputs/tr12/`.
- The two PI documents were composed with the title "Pi Plan"/"Pi Objectives" and corrected in place
  by `documents.update` (same ids, version 2) to the house title, then their bodies were corrected
  (version 3) to the baseline's 3-package count; the inputs, the mirrors and the journal carry the
  final title and body. The unit was unchanged.

The snapshot assertion in the issue's P1.1 passes:

```
python3 -c '… msp.json readiness assertion …'
tranche-12 readiness: ok
```

## The open packages and the route that clears each

| scope | P0 | P1 | P2 | cleared by |
|---|---|---|---|---|
| T15 | 1 | 2 | 0 | `project_in_unauthorised_tranche` by authorisation only (#755); `contract_missing` by the delivery contract; `mandatory_artefacts` by the seven A–D artefacts, the SoAW, the charter and both declared output bodies |

The register at the baseline (`generated_at` `2026-11-28T12:46:29Z`, `git_sha` `4f771d53`) carries these 3 open packages. The 2026-11-25 receipt named 10: its seven P2 standard packages (change-control, project-baseline, project-registers, project-wbs, architecture-artifact, compliance-review, phase-g) are no longer open at this baseline, and the pack still authors their subjects.

The P0 is not a pack's to discharge: only the programme owner's gate decision authorises the
tranche, and the package clears when the audit next reports on an authorised tranche. The dry-run
refusal below is construction's boundary, not a readiness failure.

## Batch gate — the expected authorisation precondition

```
$ python3 scripts/rework-batch.py --tranche 12 --snapshot /tmp/snap-12/data \
    --content-dir docs/evidence/rework --journal docs/evidence/rework/batch/tranche-12.jsonl --dry-run
tranche 12 is 'planned'; only authorised/closed/in_review may be reworked
exit=1
```

This is the **only** refusal: `rework-batch.py::ensure_authorised` refuses a `planned` tranche
before it reads the roster or the pack. #755 records the tranche-11 review and authorises tranche
12; on a fresh snapshot after that, the same command must exit 0 with T15 `PREFLIGHTED`. Exit 0 is
not claimed here.

## Delivery gates

```
python3 docs/design/scripts/build-docs.py --check        -> check-docs: 567 sources, 0 failure(s)
node docs/design/scripts/check-docs.cjs                  -> check-docs: 12 checks passed
node docs/design/scripts/check-data.cjs                  -> check-data: 48 checks passed
node docs/design/scripts/check-standards.cjs             -> check-standards: 1300 artefacts, 489 evidenced, 239 partial, 572 absent (exit 0)
python3 scripts/promote-record.py --check                -> 524 entries; journal is well formed
git diff --check                                         -> clean
```

## Post-authorisation (not claimed)

After #755 records the tranche-11 review and authorises tranche 12, the execution session takes a
fresh snapshot at the branch head, runs the same dry run to exit 0, then
`python3 scripts/rework-batch.py --tranche 12 … --publish`, `verify-project.py --project T15` and
`check-tranche-exit.py --tranche 12`, and commits the receipt, the exit and the rendered pages.
P3 and P4 of #961 are acceptance-blocked on #755, not on any content gap.

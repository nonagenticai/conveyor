# Tranche-8 readiness gate — measured 2026-11-28

snapshot: `/tmp/snap-8` — taken at `44aecddc2`, generated `2026-11-28T15:09:49Z`, manifest
sha256 `f8f428d8d1937bf9d7e4ebe048cb8bcd97b5a22e7c302d9577a9d72b6aec9523`. The manifest's
`files` count (999 at this cut) drifts as the shared replay record grows — its sha256 is the
snapshot's identity, never the count.

## Content packs

```
rework-content-check: S06d content is consistent with the facts
exit=0
rework-content-check: S13 content is consistent with the facts
exit=0
rework-content-check: S19 content is consistent with the facts
exit=0
```

Commands (all at the pinned snapshot):

- `python3 scripts/rework-content-check.py --project S06d --content docs/evidence/rework/tranche-8/rework-content-S06d.json --snapshot /tmp/snap-8/data --wsjf 7,6,7,8 --closes true`
- `python3 scripts/rework-content-check.py --project S13 --content docs/evidence/rework/tranche-8/rework-content-S13.json --snapshot /tmp/snap-8/data --wsjf 9,7,9,13 --closes true`
- `python3 scripts/rework-content-check.py --project S19 --content docs/evidence/rework/tranche-8/rework-content-S19.json --snapshot /tmp/snap-8/data --wsjf 8,6,8,8 --closes false`

## Record readiness (replay record, schema 110)

- Four `programme_cadence` rows at `period='tranche-8'`: `pi` `01a2229e-29a7-7c97-82bb-07e11fd358dc`,
  `refinement` `01a2229e-3862-7591-9819-c009d7d9615b`, `demo` `01a2229e-391f-75c4-a165-d3af214b4989`,
  `retro` `01a2229e-39c5-787f-9e98-8f148bc74ad2`.
- The `pi` row carries `meta.pi_plan` (scope, capacity, WIP, objectives, dependencies) and
  `meta.documents`: `pi-plan` `01a2229e-663b-7081-816f-da0c5557549a`, `pi-objectives`
  `01a2229e-66df-7557-b881-9f29be775395` — both published and **P0-scoped**, unit `tranche-8`.
- The `refinement` row carries `meta.refinement.wsjf_inputs` — S19 8/6/8/8, S06d 7/6/7/8,
  S13 9/7/9/13 — matching the three packs' WSJF.
- The PI-plan gate is `approved` (approval `01a2229e-bdf3-74ca-bf5f-17f28543c095`) as a `rule`
  under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`.
- The tranche lesson `01a2229e-ca74-7da8-b5db-c807b58d4036` is linked to the retro cadence with
  `independent_count=2` (card anchors `01a20ea0-7dec-79d0-b350-e193e9778941` and
  `01a20ec4-d2f2-7dbc-a0c7-417e484a32f0`).
- Ten readiness acts are journaled (`docs/evidence/record/journal.jsonl`, 508 entries,
  `promote-record.py --check` green), with their inputs under `docs/evidence/record/inputs/tr8/`.
- The PI-plan body was corrected in place after this cut: `documents.update` version 4 (same id
  `01a2229e-663b-7081-816f-da0c5557549a`, `published_at` preserved) — the Backlog-refinement
  paragraph now reads 13/14/11 and the Evidence paragraph no longer pins a file count. The
  correction is a local repair on the replay copy, not a new journaled act: the journal's
  `documents.compose` entry names the corrected input file, so the promotion replay composes the
  corrected body. Evidence: `evidence/pi-plan-body-correction.md`.

## Batch gate — the expected authorisation precondition

```
$ python3 scripts/rework-batch.py --tranche 8 --snapshot /tmp/snap-8/data \
    --content-dir docs/evidence/rework --journal docs/evidence/rework/batch/tranche-8.jsonl --dry-run
tranche 8 is 'planned'; only authorised/closed/in_review may be reworked
exit=1
```

This is the **only** refusal: `rework-batch.py:113-123` refuses a `planned` tranche before it
reads the roster or the packs. #755 records the tranche-7 review and authorises tranche 8; on a
fresh pre-write snapshot (`/tmp/snap-8-exec`) after that, the same command must exit 0 with three
`rework-batch: <KEY> DRY` lines and `{"blocked": 0, "green": 0, "red": 0}` — `PREFLIGHTED` is the
journal state of the real run, never a dry-run output. Exit 0 is not claimed here — the refusal is
construction's boundary, not a readiness proof. The real act is the factory command with
`--publish` (the close gate reads the publisher's documents); receipts and the exit then verify
against `/tmp/snap-8-post`, the snapshot taken after that run.

## Verifier preview (S06d, same snapshot)

`python3 scripts/verify-project.py --project S06d --snapshot /tmp/snap-8/data --kind snapshot
--evidence-dir /tmp/tr8-verify` (the receipt is written outside the repository):

```
PASS  S06d-pi-cadence      PI cadence 01a2229e-29a7-… and refinement 01a2229e-3862-… for tranche-8
PASS  S06d-pi-plan         the tranche-8 PI plan is published
PASS  S06d-pi-objectives   the tranche-8 PI objectives are published
PASS  S06d-lessons         lesson 01a2229e-ca74-… carries a statement and evidence from the tranche-8 retro
S06d: red — 18 checks, 12 failures, 0 warnings
```

The red is the expected pre-factory state: baseline, charter, contract, review, closure gate,
WBS, demo credit, product and the artefact/rework standard are the factory's acts. The four PASS
lines are the readiness subset — the receipt the factory writes after applying the pack, not this
preview, is the acceptance.

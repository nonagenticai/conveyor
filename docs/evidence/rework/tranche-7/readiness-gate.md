# Tranche-7 readiness gate — measured 2026-11-28

## Pinned snapshot

- Path: `/tmp/snap-7/data` (rendered outputs `/tmp/snap-7/docs`).
- Manifest `git_sha`: `7e93a325fe0176ba6e98ce47cae7e1ea2115543f` (the lane's claim commit).
- `generated_at`: `2026-11-28T14:37:10.839603+00:00`; manifest sha256 `d0e81f9f5f2624b6674d53dc987da4c88505f64ea6a27d01d4222255dcbdec07`.
- 931 data files; 913 rendered outputs.

## Content checks (offline, against the pinned snapshot)

```
python3 scripts/rework-content-check.py --project S06c --content docs/evidence/rework/tranche-7/rework-content-S06c.json --snapshot /tmp/snap-7/data --wsjf 8,6,6,8 --closes true
python3 scripts/rework-content-check.py --project S15 --content docs/evidence/rework/tranche-7/rework-content-S15.json --snapshot /tmp/snap-7/data --wsjf 10,8,7,13 --closes false
python3 scripts/rework-content-check.py --project S16 --content docs/evidence/rework/tranche-7/rework-content-S16.json --snapshot /tmp/snap-7/data --wsjf 9,7,9,8 --closes true
```

```
rework-content-check: S06c content is consistent with the facts   exit=0
rework-content-check: S15 content is consistent with the facts   exit=0
rework-content-check: S16 content is consistent with the facts   exit=0
```

## Batch dry-run (readiness gate)

```
python3 scripts/rework-batch.py --tranche 7 --snapshot /tmp/snap-7/data --content-dir docs/evidence/rework --journal docs/evidence/rework/batch/tranche-7.jsonl --dry-run
exit=0
```

```
rework-content-check: S06c content is consistent with the facts
rework-content-check: S15 content is consistent with the facts
rework-content-check: S16 content is consistent with the facts
rework-batch: S06c DRY
rework-batch: S15 DRY
rework-batch: S16 DRY
{"blocked": 0, "green": 0, "red": 0}
```

The three `DRY` lines are the preflight verdicts. A dry run writes no journal, and the summary
counts only real outcomes (`RECEIPT_GREEN`/`RECEIPT_RED`/`BLOCKED`); the zeros mean no member was
blocked before the first act, not that the tranche has zero members. A real run journals
`PREFLIGHTED` for each member before it acts.

## Record readiness (replay copy `conveyor-restore`, `127.0.0.1:55458`, schema 110)

| Artefact | Id | Verified |
|---|---|---|
| tranche 7 `authorised` | `01a1e9e1-0fab-7068-99f2-a233045b1276` | `programme_tranche` |
| cadences pi / refinement / demo / retro | `01a22294-e13c…` / `…-e1da…` / `…-e26f…` / `…-e301…` | `programme_cadence`, period `tranche-7` |
| refinement WSJF inputs | in the refinement row's `meta.refinement` | matches the packs' `wbs_deliverable` |
| PI plan (published, P0) | `01a22295-ca02-71ab-bd1b-93d4b1ded72a`, `unit=tranche-7` | `documents.get`; rendered `outputs.js` |
| PI objectives (published, P0) | `01a22295-cad4-741d-bf7d-7c6640125518`, `unit=tranche-7` | `documents.get`; rendered `outputs.js` |
| PI plan approval | `01a22296-3270-76c3-be7e-77da477806ed`, `approved`, rule decision under delegation `01a1ff1f-e4ba…` | `approval` + `approval_decision` |
| tranche-7 retro lesson | `01a222a4-22a7-7610-93a8-0ea3ae426be6`, `k1`, cadence `01a22294-e301…` | `programme_lessons.list`; snapshot `msp.json` |

### Fixes made at evaluation (2026-11-28)

Two defects would have made every member's receipt red; both were measured and fixed before the
factory run:

1. **PI documents had no `unit`.** `verify-project.py::_published_for_period` matches the literal
   token `tranche-7` in title+unit; the two rows rendered `unit: null`. Both now carry
   `unit: "tranche-7"` (versions 3 and 2, still published). Evidence:
   `docs/evidence/rework/tranche-7/pi-documents-unit.json`.
2. **No lesson on the tranche-7 retro cadence.** `check_lessons` requires one; `k1` was proposed
   against `01a22294-e301…` with a statement and evidence, and journaled as
   `lesson:tranche-7:k1`. Evidence: `docs/evidence/rework/tranche-7/lesson.json`,
   `docs/evidence/record/inputs/tr7/lesson_tranche-7_k1.json`.

`python3 scripts/promote-record.py --check` is green with 499 entries.

## Result

The tranche roster resolves (authorised), every content pack is consistent with the pinned
snapshot, the PI readiness set and the lesson are on the record, and the dry run exits 0. The
factory run, the three receipts and `check-tranche-exit.py --tranche 7` are the next acts on this
lane (`issue/951-tranche-7`).

# Tranche-9 readiness gate — measured 2026-11-28 (correction slice)

snapshot: `/tmp/snap-9b` — 17 data files and 1,003 indexed files, taken from the corrected replay
record; its manifest `generated_at` is `2026-11-28T16:03:37Z` (derived from the newest document
`updated_at`, not the run clock). The branch base is `main@9ff85293d`
(#948's BDD contract included); the pre-correction reads used `/tmp/snap-9` and are superseded.

This slice evaluated the preparation reported at `bff91455c` independently and corrected four
hard defects and three quality defects. Every claim below was re-measured after the corrections.

## Content packs

```
rework-content-check: S18 content is consistent with the facts   exit=0
rework-content-check: S20 content is consistent with the facts   exit=0
rework-content-check: S21 content is consistent with the facts   exit=0
```

Commands (all at `/tmp/snap-9b/data`):

- `python3 scripts/rework-content-check.py --project S18 --content docs/evidence/rework/tranche-9/rework-content-S18.json --snapshot /tmp/snap-9b/data --wsjf 9,6,9,13 --closes false`
- `python3 scripts/rework-content-check.py --project S20 --content docs/evidence/rework/tranche-9/rework-content-S20.json --snapshot /tmp/snap-9b/data --wsjf 7,5,6,5 --closes true`
- `python3 scripts/rework-content-check.py --project S21 --content docs/evidence/rework/tranche-9/rework-content-S21.json --snapshot /tmp/snap-9b/data --wsjf 9,6,7,8 --closes false`

Each pack now authors the nine documents the post-v4 class binds (the tranche-7 S16 pack is the
exemplar): `architecture-artifact`, `inception-analysis`, `architecture-analysis`,
`architecture-design`, `statement-of-architecture-work`, `user-stories`, `acceptance-criteria`,
`definition-of-ready`, `definition-of-done`. S18 (level `consistent`) and S21 (`compliant`) carry
no closure key; S20 (`compliant`) carries `closure` and `closure_review`.

### Why the nine: S20's close gate reads published documents per class category

`crates/actions/src/programme.rs::assert_mandatory_artefacts` (the `projects.close` gate) reads the
standard class's `now` + `closure` categories. For `v1-built` that is 14 published documents for
S20. `scripts/publish-artefact-bodies.py` composes only categories applicable without the class
(TOGAF-governed or declared), so seven categories can come only from the pack: `inception-analysis`,
`architecture-analysis`, `architecture-design`, `user-stories`, `acceptance-criteria`,
`definition-of-ready`, `definition-of-done`. Without them the close is refused
`409 closure_mandatory_artefacts_missing`, and the batch's publish-and-resume cannot clear it.

Coverage measured on the corrected pack (14/14):

| category | source |
|---|---|
| project-brief, evidence | `meta.expected_outputs`; the factory publishes both |
| inception-analysis, architecture-analysis, architecture-design | pack document |
| architecture-artifact, statement-of-architecture-work | pack document |
| user-stories, acceptance-criteria, definition-of-ready, definition-of-done | pack document |
| project-wbs, project-registers | publisher composes from the record rows |
| closure-review | factory close step (`meta.closure_outputs`) |

## S20's manifest — two changes, both recorded

1. **The pin is aligned, not weakened.** `S20-project` expected `active` while the record row was
   `closed`; it now states `closed`, which is what the factory's close step leaves behind and what
   `check_project` asserts. A failed close reddens the check. `manifest_status` is the only script
   reader of the id.
2. **Two closure checks are added**, mirroring the closed `v1-built` exemplar S13:
   `S20-closure-outputs` (S20's `meta.closure_outputs` is non-empty and each declared category is
   published in the snapshot) and `S20-closure-gate` (the latest approved closure gate carries an
   approve decision by an approver other than the requester, targeting this project). The manifest
   now has 17 checks, all named in `gate_checks`.

## Record readiness (replay record, schema 110)

- Four `programme_cadence` rows at `period='tranche-9'`: `pi` `01a222b4-5097-7fee-94ac-e7adeb1bb79c`,
  `refinement` `01a222b4-513e-761c-af98-64ef0334e89b` (version 4), `demo`
  `01a222b4-51df-7576-997f-ad1f6fdc66f0`, `retro` `01a222b4-5278-757b-aa65-184fd4d5de28`.
- The `pi` row carries `meta.pi_plan` and `meta.documents`: `pi-plan`
  `01a222b4-5dee-7060-a889-af69cf6d2ddf`, `pi-objectives`
  `01a222b4-5eb7-777f-b317-c0e445e762d7` — both published, P0-scoped and `unit: tranche-9`.
- The `refinement` row carries `meta.refinement.wsjf_inputs` — S18 9/6/9/13, S20 7/5/6/5,
  S21 9/6/7/8 — matching the packs' WBS integers and the `--wsjf` each checker ran with.
- The PI-plan gate is `approved` as `kind=plan` (approval `01a222b4-f15c-7c4e-ae37-153f5bfc3e80`)
  under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the row's state stays
  `awaiting_approval`, as tranche 7's and tranche 8's do.
- The lesson `01a222b5-1061-79dc-be2f-b1bff735a0e0` is linked to the retro cadence with
  `independent_count=2`.

**Record correction (live-only).** The earlier PI plan, PI objectives and refinement meta carried a
false count — "37 open packages", S18/S21 11 (3 P1, 8 P2), S20 13 (1 P0, 4 P1, 8 P2). The derived
register carries none of the eight per-member P2s. The three texts were corrected on the replay
record on 2026-11-28 with `documents.update` + `documents.publish` (both documents at version 2,
`published_at` set) and `cadences.update` (refinement at version 4), read back with the corrected
counts; the evidence notes are `evidence/pi-plan-correction.md`, `evidence/pi-objectives-correction.md`
and `evidence/refinement-correction.md`. These acts are **not journaled**: `promote-record.py`
refuses `documents.update` as unreplayable, so the promotion ledger (508 entries, still green)
does not carry them — #852 must replay the correction when it promotes.

Reading the replay record requires `SET app.current_tenant='01a1cf6d-23af-7e0e-8b07-89c8f06bafb4'`
(RLS is forced); without it every table reads empty.

## The measured register: 13 open packages, not 37

Source: `scripts/rework_register.py::build` on `/tmp/snap-9b` (the same derivation the receipt's
`rework` check uses). The committed `docs/evidence/rework-register.json` agrees on the member sets
(4/5/4) but still lists the two tranche-scope packages (`pi_cadence_missing:tranche-9`,
`backlog_refinement_missing:tranche-9`); the readiness rows clear them in the derived view, which
is the one the receipt reads.

| scope | total | P0 | P1 | P2 | cleared by |
|---|---|---|---|---|---|
| S18 | 4 | 0 | 3 | 1 | `project_in_unauthorised_tranche` by authorisation only; `unpublished_declared_output` and `mandatory_artefacts` by the pack's outputs and seven A–D artefacts; `wbs_absent` by the WBS deliverable |
| S20 | 5 | 1 | 4 | 0 | `project_in_unauthorised_tranche` (P0) by authorisation only; `closed_without_closure_review` and `closure_outputs_undeclared` by the published closure review and the factory's declaration; `unpublished_declared_output` and `mandatory_artefacts` by the pack |
| S21 | 4 | 0 | 3 | 1 | the same routes as S18 |
| `tranche-9` | 0 | — | — | — | the two tranche-scope packages were cleared by the four readiness rows |

`project_in_unauthorised_tranche:S20` is P0 and the only package no pack can clear.

## Batch gate — the expected authorisation precondition

```
$ python3 scripts/rework-batch.py --tranche 9 --snapshot /tmp/snap-9b/data \
    --content-dir docs/evidence/rework --journal docs/evidence/rework/batch/tranche-9.jsonl --dry-run
tranche 9 is 'planned'; only authorised/closed/in_review may be reworked
exit=1
```

This is the only refusal: `ensure_authorised` refuses a `planned` tranche before reading the roster
or the packs. #755 records the tranche-8 review and authorises tranche 9; on a fresh snapshot after
that, the same command must exit 0 with all three members reported `DRY` (a dry run writes no
journal; `PREFLIGHTED` is the real run's journal state). Exit 0 is not claimed here.

## Delivery gates (measured at this slice)

```
python3 docs/design/scripts/build-docs.py --check   -> exit 0
node docs/design/scripts/check-docs.cjs             -> 0 failure(s)
node docs/design/scripts/check-data.cjs             -> 0 failures
python3 scripts/promote-record.py --check           -> 508 entries; journal is well formed
python3 scripts/ci/board.py --contract-body …       -> the issue content contract is complete
python3 scripts/ci/board.py --dor 954               -> ready
git diff --check                                    -> clean
```

## Corrections from the independent evaluation

- **D1 — issue contract.** #948 requires `## User story` and one DoD row per `@verify` id for an
  issue created on/after 2026-11-28. The body now carries both; `board.py --contract-body` and
  `--dor 954` exit 0 on `main@9ff85293d`.
- **D2 — base.** The branch is rebased onto `main@9ff85293d`; the generated design index was
  regenerated, never hand-merged.
- **D3 — the nine documents.** Added to all three packs; the S20 coverage proof above closes the
  close-gate hole.
- **D4 — S20's closure checks.** Added, mirroring S13.
- **Q1 — S20's closure text** no longer claims the review preceded the close; it states the record
  and conditions the re-close on authorisation.
- **Q2 — S18's findings** name the measured 4 packages and no `(P0)` P2; the artefact checks are
  exact commands and the states name the suite that proves each or say it has none.
- **Q3 — S21's suite inventory** is reconciled against the tree (14 names, each a test target;
  `ladder_laya` is stale and dropped), and the `(prompt_ref, prompt_sha256)` pair is recorded as a
  design claim with no suite asserting it at this date.

## Open items (recorded, not this pack's to clear)

- `project_in_unauthorised_tranche:S20` is P0 and clears only on the sponsoring group's
  authorisation of tranche 9 (#755).
- S20's gate text still names `rbuild --gate`, retired by the owner on 2026-11-20; the replacement
  is the repository's `preflight.yml`. The text is the owner's acceptance statement and is not
  edited here.
- S18's live katas proof is issue #12's; the project stays active and its manifest pins it active.
- The record correction is live-only; #852 must replay it.

# Tranche-10 readiness gate — measured 2026-11-28

snapshot: `/tmp/snap-10` — 15 data files, 987 published output pages, taken from
`issue/964-tranche-10` at `5389ef2b6ae1a8e38767ce7bed16d629f5fb8eb8` (the commit that lands the readiness
set, both packs and T5's pin). The manifest's `generated_at` is `2026-11-28T16:09:13.158006Z`; it is
**not** the run clock — the product derives it from the newest `updated_at` among the `documents`
artefacts (`crates/pack/src/snapshot.rs::snapshot_generated_at`), so it pins the artefact set, not
the instant. The snapshot was taken after the commit, and every gate below was re-run against it.

## Content packs

```
rework-content-check: S12 content is consistent with the facts
exit=0
rework-content-check: T5 content is consistent with the facts
exit=0
```

Commands (all at the pinned snapshot):

- `python3 scripts/rework-content-check.py --project S12 --content docs/evidence/rework/tranche-10/rework-content-S12.json --snapshot /tmp/snap-10/data --wsjf 8,7,6,8 --closes false`
- `python3 scripts/rework-content-check.py --project T5 --content docs/evidence/rework/tranche-10/rework-content-T5.json --snapshot /tmp/snap-10/data --wsjf 9,6,9,13 --closes true`

Each pack carries the seven A–D artefacts in the standard's order, the five registers, the charter,
the Statement of Architecture Work, the delivery contract, the Phase-G review, both declared output
bodies **and the four delivery documents plus the four close-gate documents** — nine documents each
(the v4 set; tranche 9's packs carried only the architecture-artefact index, which the checker does
not refuse and the receipt does). T5 carries `closure` and `closure_review`; S12 deliberately does
not (see below).

## S12's gate — the measured state half and the deferred closure

S12's project gate is: every spec asserts empty state, error state and happy path — a spec that
only asserts the happy path fails review — and the coverage table has no orphan. Measured on
2026-11-28 at `main@ea03b188a`:

- **Coverage half met.** `python3 scripts/check-parity.py --require-native` → `parity: OK: 2d7b93c56a07
  signed, 1300 rows (used 218, rare 295, unused 543, dropped 244); native map present`.
- **State half partial.** 21 distinct screen specs exist for the 22 rows. Ten assert empty, error
  and happy; three are happy-path-only (`board`, `prompt-audit`, `eval-audit`); six miss an empty
  state (`shell-login`, `cockpit`, `benchmarks`, `improvement`, `eval-interop`, `process-map`); two
  render no refusal (`copilot`, `inference-usage`); screen 16 (eval run) has no dedicated spec.

The full per-spec table is in #965, the test-only unit that closes the gap. Because the gate is not
met at this snapshot, **S12's pack is stay-active**: it carries no `closure`/`closure_review` keys,
the checker runs with `--closes false`, and the closure review and the declared closure outputs are
authored after #965 lands. The preview receipt on this snapshot shows exactly that: the factory will
clear the document and artefact packages, while `S12-closure-outputs`, `S12-closure-gate`,
`S12-review` and the two closure P1s stay red until the closure is authored.

## T5's pin — the one acceptance-standard change

T5's record row is `closed` while `acceptance/programme/T5.toml` pinned `T5-project` `active`. That
pin failed `verify-project.py::check_project` as P0 and made `rework-batch.py::closes_project` return
false, so a closing pack could not preflight.

The pin now states `closed`, the record's measured status. Verified two ways:

- `python3 -c '<manifest_status regex>'` → `T5-project pin: closed`
- `importlib`-loaded `rework-batch.py` → `manifest_status('T5') == 'closed'`;
  `closes_project({"key":"T5","meta":{"issues":[77,653]}}, {77:"CLOSED",653:"CLOSED"}, content) == True`;
  `manifest_status('S12') == None` and `closes_project` for S12 also true on all-closed issues and
  closure content — S12 carries no pin, so its close is decided by the closure content it withholds.

T5's three CODE packages (`v1-p119-1`, `v1-p93-1`, `v1-w1b-3`) are carried by #962; the receipt is
green only when they land, and the pack does not pretend to clear them.

## Record readiness (replay record, schema 110)

- Four `programme_cadence` rows at `period='tranche-10'`: `pi` `01a222e3-61b5-75f3-b122-75c8af9d859d`
  (version 3), `refinement` `01a222e3-86d4-7812-9ab1-1f550a3c603c` (version 3), `demo`
  `01a222e3-8768-7b1b-8100-6ce2601faae2`, `retro` `01a222e3-87fb-72ce-b917-493ac7487a89`.
- The `pi` row carries `meta.pi_plan` (scope S12/T5, capacity, WIP limits, four objectives, four
  dependency edges) and `meta.documents`: `pi-plan` `01a222e3-9670-70fc-8280-50493a8dfbab`,
  `pi-objectives` `01a222e3-c641-7359-95fc-2faea4f9198d` — both published, **P0-scoped**
  (`01a1dcb8-d24b-76de-b901-56fd56e001bc`) and `unit` `tranche-10`, read back from `outputs.js`.
  Both were corrected after the spec audit moved S12's closure to #965; the corrected bodies name the
  gap and the unit.
- The `refinement` row carries `meta.refinement.wsjf_inputs` — S12 8/7/6/8, T5 9/6/9/13 — matching
  the two packs' WBS integers and the `--wsjf` each checker ran with.
- The PI-plan gate is `approved` (approval `01a222e4-408d-7d71-9931-913ecdfc054c`) as a `rule`
  decision under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the `pi` row's own
  state stays `awaiting_approval`, as tranche 9's does.
- The tranche lesson `01a222e4-bd0d-7bfb-b7de-976b5ee29493` is linked to the retro cadence with
  `independent_count=2` (anchors `scripts/rework-content-check.py` and `seed/output-standard.json`).
- Ten readiness acts are journaled (`docs/evidence/record/journal.jsonl`, 524 entries,
  `promote-record.py --check` green), with their inputs under `docs/evidence/record/inputs/tr10/`.
  The two PI-document corrections and the refinement re-write are updates to rows the journal
  already creates; they add no duplicate natural key.

The snapshot assertion in the issue's P1.1 passes:

```
python3 -c '… msp.json readiness assertion …'
tranche-10 readiness: ok
```

## The open packages and the route that clears each

| scope | P0 | P1 | cleared by |
|---|---|---|---|
| S12 | 1 | 4 | `project_in_unauthorised_tranche` by authorisation only; `unpublished_declared_output` and `mandatory_artefacts` by the pack's documents, outputs and close-gate set; `closed_without_closure_review` and `closure_outputs_undeclared` only after #965 lands and the closure review is authored |
| T5 | 1 | 5 | `project_in_unauthorised_tranche` by authorisation only; `contract_missing` and `mandatory_artefacts` by the pack; `v1-p119-1`, `v1-p93-1`, `v1-w1b-3` by #962 only |
| `tranche-10` | 0 | 2 | `pi_cadence_missing` and `backlog_refinement_missing` by the four cadence rows and the refinement record above |

The P0 is not a pack's to discharge: only the programme owner's gate decision authorises the
tranche, and the package clears when the audit next reports on an authorised tranche.

## Batch gate — the expected authorisation precondition

```
$ python3 scripts/rework-batch.py --tranche 10 --snapshot /tmp/snap-10/data \
    --content-dir docs/evidence/rework --journal docs/evidence/rework/batch/tranche-10.jsonl --dry-run
tranche 10 is 'planned'; only authorised/closed/in_review may be reworked
exit=1
```

This is the **only** refusal: `rework-batch.py::ensure_authorised` refuses a `planned` tranche
before it reads the roster or the packs. #755 records the tranche-9 review and authorises tranche 10;
on a fresh snapshot after that, the same command must exit 0 with S12 and T5 `PREFLIGHTED`.

## Receipt previews at the pin

Both previews were written with `--evidence-dir /tmp/tr10/verify` (never into the repo's evidence
directory) and are the factory's to replace:

- **S12** — red on `S12-baseline`, `S12-charter`, `S12-closure-outputs`, `S12-contract`, `S12-review`,
  `S12-closure-gate`, `S12-wsjf`, `S12-demo-credits`, `S12-artefact-standard`, `S12-rework` and the
  two gate rows. The pack clears the document, artefact, contract, review and WBS checks; the
  closure rows stay red until #965 and the closure review.
- **T5** — red on `T5-baseline`, `T5-charter`, `T5-contract`, `T5-review`, `T5-wsjf`,
  `T5-demo-credits`, `T5-artefact-standard`, `T5-rework` and the two gate rows. `T5-project` now
  passes on the corrected pin. The pack clears everything except the three CODE packages until #962
  lands.

## Delivery gates

```
python3 docs/design/scripts/build-docs.py --check        -> check-docs: 566 sources, 0 failure(s)
node docs/design/scripts/check-docs.cjs                  -> check-docs: 12 checks passed
node docs/design/scripts/check-data.cjs                  -> check-data: 48 checks passed
python3 scripts/promote-record.py --check                -> 524 entries; journal is well formed
git diff --check                                         -> clean
```

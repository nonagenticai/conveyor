# Tranche-3 SAFe cadences — evidence

**Created 2026-11-22** for #526 (S04's record). This file exists **before** the four write actions
run: the recorder refuses a write whose evidence path does not already exist.

## Source

- Tranche 3 is `authorised`: row `01a1e9e0-eb54-74d8-a2b5-44fb2b87eb52`, programme
  `01a1d039-0d24-7d16-ae52-4841e96ccfac`. It was authorised by the tranche-2 review under the
  published delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`.
- The measured baseline for the tranche is the fresh full-fidelity snapshot at lane `582e2dc6`
  (2026-11-22): **12 green / 35 red** manifests, **919 standard artefacts / 456 findings**, strict
  audit 200 gaps (23 P0 / 157 P1 / 20 P2), viability NOT VIABLE 6/6. The work table is
  [comment 7076904694](https://github.com/nonagenticai/conveyor/issues/526#issuecomment-7076904694).

## What is created

Four `programme_cadence` rows for tranche 3 — `pi`, `refinement`, `demo`, `retro` — each with
`period = tranche-3` and `meta = {tranche_id, tranche_number: 3, delegation_ref}`. That meta link is
what `verify-project.py`'s `pi_cadence` check and the conformance audit read.

`cadences.create` is a write with `confirm = "none"`; no approval gate applies. The demo cadence's id
is sent to T13's owner so its demo credit lands on the tranche-3 cadence rather than the tranche-0
default.

## Read-back

Each action is followed by the recorder's read-back; the ids are recorded in the session and in the
issue work table. The snapshot after the writes must show the four rows under tranche 3.

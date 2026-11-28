# Refinement — tranche 11

Action: `cadences.update` on `01a222e7-1b99-7069-90d9-c34763ac508b` with
`docs/evidence/record/inputs/tr11/cadence_tranche-11_refinement_meta.json`, `--if-match 1`,
read back on the same list.

`meta.refinement.wsjf_inputs`: S14 8/7/9/8 (WSJF 3.00), T6 9/6/8/8 (2.875), T8 8/5/7/5 (4.00).
The same four integers are in each pack's `wbs_deliverable` and in the `--wsjf` the checker runs
with, so the three consumers cannot disagree.

`meta.refinement.reordered` records the move from the seed's membership order (S14, T6, T8) to
T8, S14, T6 with the reason; `reestimate` records the measured v4 gap (S14 16, T6 13, T8 15 of the
17 `now` categories) rather than the package count, because the standard's
`scope.through_tranche: 8` marks tranche-11 artefact packages `scheduled`.

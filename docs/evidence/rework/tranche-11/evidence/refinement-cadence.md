# Refinement cadence — tranche 11

Action: `cadences.create` with `docs/evidence/record/inputs/tr11/cadence_tranche-11_refinement.json`.
Read back on the replay record (`cadences.list`, `kind=refinement`, `period=tranche-11`).

- id `01a222e7-1b99-7069-90d9-c34763ac508b`, kind `refinement`, period `tranche-11`; version 2 after the refinement
  metadata update.
- meta at creation: `tranche_id`, `tranche_number` 11, `delegation_ref`.

The row is what clears `backlog_refinement_missing:tranche-11`; the audit at the pinned snapshot no
longer reports the rule for this tranche.

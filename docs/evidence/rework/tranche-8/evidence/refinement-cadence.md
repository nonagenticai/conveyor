# Evidence — tranche-8 backlog-refinement cadence

Creates the `refinement` cadence row for `period='tranche-8'`. Its `meta.refinement` is written after the create and carries the WSJF inputs (S19 8/6/8/8, S06d 7/6/7/8, S13 9/7/9/13), the re-estimates against the 26 open packages, and the reorder from the seed's membership order to the WSJF order. `rework-batch.py` reads `meta.refinement.wsjf_inputs`; `rework-content-check.py --wsjf` must match the same triple.

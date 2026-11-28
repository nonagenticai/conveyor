# Evidence — tranche-7 refinement and WSJF

Updates the `refinement` cadence's `meta.refinement` with the reordered membership (S16, S06c, S15), the re-estimate and the four WSJF inputs per member. `rework-batch.py:134` reads `meta.refinement.wsjf_inputs` and passes it to `rework-content-check.py --wsjf`, so this row and the three content packs must carry the same numbers. Values: S16 9/7/9/8, S06c 8/6/6/8, S15 10/8/7/13.

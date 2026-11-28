# Evidence — tranche-8 refinement WSJF

Updates the `refinement` cadence row's `meta.refinement`: the reorder (seed order S06d, S13, S19 → WSJF order S19, S06d, S13), the re-estimates against the 26 open packages, and the four WSJF inputs per member. `rework-batch.py` reads `meta.refinement.wsjf_inputs`; the three content packs are preflighted with the same triples. Made with `--if-match` against the row's created version.

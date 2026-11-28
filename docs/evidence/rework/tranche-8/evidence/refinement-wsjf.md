# Evidence — tranche-8 refinement WSJF

Updates the `refinement` cadence row's `meta.refinement`: the reorder (seed order S06d, S13, S19 → WSJF order S19, S06d, S13), the re-estimates against the baseline register's 38 open packages, and the four WSJF inputs per member. `rework-batch.py` reads `meta.refinement.wsjf_inputs`; the three content packs are preflighted with the same triples. Made with `--if-match` against the row's created version.


**Correction 2026-11-28:** the re-estimate was first written from the 2026-11-27 cut of 26 packages; it now records the baseline register's 38 (S06d 13, S13 14, S19 11). The WSJF inputs are unchanged.

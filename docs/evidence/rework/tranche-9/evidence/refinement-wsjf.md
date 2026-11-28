# Evidence — tranche-9 refinement record

Writes the refinement's re-estimate onto the `refinement` cadence row: the four integers for each member (S20 7/5/6/5, S21 9/6/7/8, S18 9/6/9/13), the reorder from the seed's membership order to S20, S21, S18, and each member's open-package count. This is the record `rework-batch.py::wsjf_for` returns and the value every `rework-content-check.py --wsjf` must match, so it is written after the packs' integers are settled and read back before the preflight.

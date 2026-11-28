# Evidence — tranche-10 refinement record

Writes the refinement's re-estimate onto the `refinement` cadence row: the four integers for each member (S12 8/7/6/8, T5 9/6/9/13), the order (S12 then T5), and each member's open-package count. This is the record `rework-batch.py::wsjf_for` returns and the value every `rework-content-check.py --wsjf` must match, so it is written after the packs' integers are settled and read back before the preflight.

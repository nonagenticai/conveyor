# Evidence — tranche-9 refinement cadence

Creates the `refinement` cadence row at `period='tranche-9'`. The row is what `rework-batch.py::wsjf_for` reads before the first pack is preflighted, so its `meta.refinement.wsjf_inputs` must equal the four integers in each member's content pack. The row starts with only its tranche identity; the refinement record is written by a separate update once the row has an id and a version.

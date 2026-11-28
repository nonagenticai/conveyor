# Evidence — tranche-9 demo cadence

Creates the `demo` cadence row at `period='tranche-9'`. The factory refuses a tranche with no demo cadence (`rework-batch.py::demo_cadence`), and each member's WBS deliverable is credited to this row at the factory's accept step, which is what makes `check_demo_credits` green for every accepted node.

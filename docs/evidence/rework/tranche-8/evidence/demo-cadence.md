# Evidence — tranche-8 demo cadence

Creates the `demo` cadence row for `period='tranche-8'`. `rework-batch.py::demo_cadence` refuses the whole tranche without it — the refusal names the missing readiness step — and the factory credits each accepted WBS deliverable against this row. The row carries `meta.tranche_number` 8, which is the key the driver matches.

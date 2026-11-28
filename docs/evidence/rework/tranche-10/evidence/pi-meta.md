# Evidence — tranche-10 PI row plan

Updates the `pi` cadence row's `meta.pi_plan` with the plan's scope (S12, T5), its capacity and WIP limits, its objectives and dependencies, and `meta.documents` with the two published PI output ids. The update is made with `--if-match` against the version returned by the create and with an explicit `--read-input` carrying `programme_id`, because `cadences.list` requires it. The tranche-9 row carries this shape, and the review reads the plan from the row rather than from a file.

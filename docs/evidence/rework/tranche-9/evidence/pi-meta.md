# Evidence — tranche-9 PI row plan

Updates the `pi` cadence row's `meta.pi_plan` with the plan's scope (S18, S20, S21), its capacity and WIP limits, its objectives and dependencies, and `meta.documents` with the two published PI output ids. The update is made with `--if-match` against the version returned by the create. The tranche-6 and tranche-8 rows carry this shape, and the review reads the plan from the row rather than from a file.

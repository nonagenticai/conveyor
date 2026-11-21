# Project acceptance manifests

One TOML manifest per programme project. `scripts/verify-project.py` runs it against a
snapshot (and, for `kind = "command"` clauses, the live engine) and writes
`docs/evidence/rework/<KEY>.json`.

A manifest declares the project's own gate as `[[check]]` rows. A `check` row names either a
shared snapshot check (`project`, `project_baseline`, `closure_outputs`, `contract`, `review`,
`closure_gate`, `waivers`, `benefit_baseline`) or a shell `command`; every row carries a
severity. A `[[allow]]` row records a documented residual with an owner, an issue and an
expiry — never a silent pass. A project closes only on a green manifest at a named snapshot.

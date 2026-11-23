# Project acceptance manifests

One TOML manifest per programme project. `scripts/verify-project.py` runs it against a
snapshot (and, for `kind = "command"` clauses, the live engine) and writes
`docs/evidence/rework/<KEY>.json`.

A manifest declares the project's own gate as `[[check]]` rows. A `check` row names either a
shared snapshot check (`project`, `project_baseline`, `closure_outputs`, `contract`, `review`,
`closure_gate`, `waivers`, `benefit_baseline`, `pi_cadence`, `pi_plan`, `pi_objectives`,
`wsjf`, `demo_credits`, `lessons`) or a shell `command`; every row carries a severity. The
`gate` is prose, and `gate_checks = [ids...]` names the checks that discharge it: each id
must resolve to a row in the same manifest that runs and passes. An active or closed project
with no `gate_checks` fails with "the gate names no executable checks". A `[[allow]]` row is
refused as a finding (ADR 0012): no deferral or residual is a disposition. A project closes
only on a green manifest at a named snapshot.

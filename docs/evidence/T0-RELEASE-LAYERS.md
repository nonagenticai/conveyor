# T0 — the release compiles the knowledge layers: receipt

Issue #697 · branch `issue/697-s21-execution-loop` · ADR `0020-release-knowledge-layers` ·
CONTRACT §30 · S21 §23 P4 · migration `0072` · resolves rework `v1-w1a-2`

**State: planned.** This file is the skeleton the issue's Definition of Done fills at the exact PR
head. Nothing here is claimed until the measured tables carry the head's rows.

## What changes at this level

Before: `compile_release_tx` built the release snapshot from `job_catalogue.definition`, the
industry and the placement rows. No code read a knowledge layer, so an unresolved required kind
could not stop a publish and two sibling departments could disagree silently; neither
`release_not_ready` nor `department_conflict` existed anywhere under `crates/`.

After: migration `0072` adds `organisation_layer` (base, industry, department archetype, each
department in declared `ordinal`, job). The publish resolves each kind in that order, tightens
`required` downward only, refuses `422 department_conflict` for disagreeing siblings unless a
job-layer row resolves it, and refuses `409 release_not_ready` naming the kind and department when a
required kind has no value. The snapshot carries every resolved value with its level and source key,
sorted, so two identical publishes hash identically. `organisation_layers.set|list` author and read
the rows; no new scope.

## Measured suites (exact head `PENDING`)

| Suite | Result |
|---|---|
| `crates/actions/tests/release_layers.rs` (new) | PENDING |
| `crates/store/tests/organisation.rs` | PENDING |
| `crates/actions/tests/registry.rs` | PENDING |
| `crates/actions/tests/parity_http_mcp.rs` | PENDING |
| `crates/store/tests/s07_schema.rs` (ledger through `0072`) | PENDING |
| `clippy -D warnings` / `fmt --check` | PENDING |
| `served-spec.lock.json` `--check-lock` | PENDING |
| rework audit `--rework-check v1-w1a-2` | PENDING |

## Scenarios

| Scenario | Test | Result |
|---|---|---|
| P4.1 the declared layer order resolves, sources named | `release_layers::the_release_resolves_layers_in_declared_order` | PENDING |
| P4.2 an unresolved required kind refuses publish | `release_layers::an_unresolved_required_kind_refuses_publish` | PENDING |
| P4.3 sibling departments conflict unless the job resolves | `release_layers::sibling_departments_conflict_unless_the_job_resolves` | PENDING |
| P4.4 required only tightens downward | `release_layers::required_only_tightens_downward` | PENDING |
| P4.5 two identical publishes hash identically | `organisation::two_identical_publishes_hash_identically` | PENDING |
| P4.6 the two actions are parity-sampled | `parity_http_mcp` + `--check-lock` | PENDING |

## Reversal proofs

| # | What is reverted | Test that must go red | Result |
|---|---|---|---|
| 1 | the layer read in `compile_release_tx` | P4.1 | PENDING |
| 2 | the `release_not_ready` check | P4.2 | PENDING |
| 3 | the sibling-conflict check | P4.3 | PENDING |
| 4 | the downward-only `required` merge | P4.4 | PENDING |
| 5 | the sorted snapshot section | P4.5 | PENDING |

## Residuals

- Region sub-layers are folded into the department level's source key; no region rows are seeded
  and no separate region level exists until the spec gives it content.
- Existing release rows keep their bytes and hashes; only publishes after this unit carry the
  `layers` member.

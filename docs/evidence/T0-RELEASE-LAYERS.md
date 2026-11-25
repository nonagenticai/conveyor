# T0 — the release compiles the knowledge layers: receipt

Issue #697 · branch `issue/697-s21-execution-loop` · ADR `0020-release-knowledge-layers` ·
CONTRACT §30 · S21 §23 P4 · migration `0074` · resolves rework `v1-w1a-2`

**State: merged** (PR #713, `e7aa67b7`, 2026-11-25). Measured at `8eef79ba` on the branch that
merged with `main@9ace4e81`; the fixture-contract rewrite is `a21c1965` and the migration-ledger pin
`20efce1c`. The required CI contexts were green at the head `28401de2`.

## What changes at this level

Before: `compile_release_tx` built the release snapshot from `job_catalogue.definition`, the
industry and the placement rows. No code read a knowledge layer, so an unresolved required kind
could not stop a publish and two sibling departments could disagree silently; neither
`release_not_ready` nor `department_conflict` existed anywhere under `crates/`.

After: migration `0074` adds `organisation_layer` (base, industry, department archetype, each
department in declared `ordinal`, job). The publish resolves each kind in that order, tightens
`required` downward only, refuses `422 department_conflict` for disagreeing siblings unless a
job-layer row resolves it, and refuses `409 release_not_ready` naming the kind and department when a
required kind has no value. The snapshot carries every resolved value with its level and source key,
sorted, so two identical publishes hash identically. `organisation_layers.set|list` author and read
the rows; no new scope.

## Measured suites (exact head `8eef79ba`)

| Suite | Result |
|---|---|
| `crates/store/tests/organisation.rs` (five new layer tests) | **14 passed** |
| `crates/actions/tests/parity_http_mcp.rs` | **7 passed** — `organisation_layers.set\|list` sampled over HTTP and MCP |
| `crates/actions/tests/registry.rs` | **12 passed** — 589 actions (565 internal, 24 external) |
| `clippy -D warnings` (store, actions; all targets) / `fmt --check` | **exit 0** |
| `crates/store/src/migrate.rs` — the embedded set | **asserts 74**; the `organisation` fixture applied `0074` |
| `served-spec.lock.json` | **620 operations**, `--check-lock` exit 0 |
| rework audit `--rework-check v1-w1a-2` | **prints "is cleared"** |

## Scenarios

| Scenario | Test | Result |
|---|---|---|
| P4.1 the declared layer order resolves, sources named | `organisation::the_release_resolves_layers_in_declared_order` | **pass** |
| P4.2 an unresolved required kind refuses publish | `organisation::an_unresolved_required_kind_refuses_publish` | **pass** |
| P4.3 sibling departments conflict unless the job resolves | `organisation::sibling_departments_conflict_unless_the_job_resolves` | **pass** |
| P4.4 required only tightens downward | `organisation::required_only_tightens_downward` | **pass** |
| P4.5 two identical publishes hash identically | `organisation::two_identical_publishes_hash_identically` | **pass** |
| P4.6 the two actions are parity-sampled | `parity_http_mcp` + `--check-lock` | **pass** |

## Reversal proofs

| # | What is reverted | Test that must go red | Result |
|---|---|---|---|
| 1 | the layer read in `compile_release_tx` | P4.1 | **red** — the snapshot carries no `layers` |
| 2 | the `release_not_ready` check | P4.2 | **red** — the release is published with the kind unresolved |
| 3 | the sibling-conflict check | P4.3 | **red** — the later department's value wins silently |
| 4 | the downward-only `required` merge | P4.4 | **red** — a lower `required=false` loosens the ancestor's |
| 5 | a timestamp in the snapshot (non-determinism) | P4.5 | **red** — two identical publishes hash differently |

## Residuals

- Region sub-layers are folded into the department level's source key; no region rows are seeded
  and no separate region level exists until the spec gives it content.
- Existing release rows keep their bytes and hashes; only publishes after this unit carry the
  `layers` member.

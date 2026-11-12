# S15 step 12 — capability readiness — evidence

Measured 2026-11-09 on mini3; every Rust command through `rbuild` on m1 (CONTRACT §16).

## Built

- `crates/actions/src/capability/readiness.rs` — one computation, `capability_readiness_tx`
  (`domain::capability::Readiness` blockers): capability exists and is `active`; exactly one active
  `capability_agent`; its `prompt_ref` is an **active** catalogue pin (absent or retired is a
  blocker); every `allowed_actions` entry is registered; every **required** knowledge pack resolves
  to at least one leaf. `readiness_fields` maps the blockers to `FieldError` **once**.
- Both surfaces consume that one mapping: `capabilities.get` returns `{ready, blocking[]}` and
  `procedure_runs.start`'s `ensure_start_ready` refuses `409 capability_not_ready` with the same
  array in `errors[]`.
- The guard's own vocabulary is deleted (it had `no_active_agent`, `unknown_action`,
  `bad_prompt_ref`, `knowledge_pack_unavailable`, …). The stable codes are now the domain's:
  `capability_missing`, `capability_not_active`, `agent_missing`, `agent_ambiguous`,
  `action_not_registered`, `pack_not_published`, `pack_unavailable`, `knowledge_not_configured`,
  `prompt_unresolved`.
- `prompt_ref` resolvability is a readiness fact (row k amended), read from the tenant's catalogue
  (`store::prompt::get_prompt_tx`, S13's `0030`); `prompt_unresolved` is the executor's own code.
- The empty-leaves defect: `pack_leaves` returns `Ok(vec![])` for a published-but-empty pack; the
  old guard passed it. `pack_state` now treats emptiness as `pack_not_published` (CONTRACT §17's
  "no resolvable leaves").
- No new action, scope, event or migration. Live registry **266 actions** · `AGENT_TOOLS` **40**.

## Evidence

| command | result |
|---|---|
| `rbuild -- test -p conveyor-domain readiness` | **6 passed / 0 failed** (2 are this lane's) |
| `rbuild -- test -p conveyor-actions --test capability` | **9 passed / 0 failed** |
| `rbuild -- test -p conveyor-actions --test run_capability` | **3 passed / 0 failed** |
| `rbuild -- test -p conveyor-api --test capability_readiness` | **3 passed / 0 failed** |
| `rbuild -- test -p conveyor-api --test capability_readiness -- --ignored` | **1 passed / 0 failed** — the live missing-pack case against the real `strata_rag` (`mini3.local:8092`, compose `strata` profile up; the tenant token sealed under the test KEK) |
| `rbuild -- test -p conveyor-actions --test registry` | **8 passed / 0 failed** at `EXPECTED: [&str; 266]` |
| `rbuild -- test -p conveyor-actions --test parity_http_mcp` | **4 passed / 0 failed** when run before S20 registered `eval_runs.regrade`; the same binary is now red only on that action's missing sample (`missing ["eval_runs.regrade"]`, S20's lane — `capabilities.get`'s sample is present and passed) |
| `rbuild -- test -p conveyor-actions --lib readiness` | **1 passed / 0 failed** (the pack decision table) |
| scoped `clippy -- -D warnings` (domain lib+tests; actions lib + the two targets; api target) | exit 0 |
| `rustfmt --edition 2024 --check` on every file this lane touched | clean |

The live case proves exactly row j's first clause: `capabilities.bind_knowledge` pins
`capability-design@999` as `required` → `capabilities.get` reports `ready:false` with
`pack_not_published` → `procedure_runs.start` is `409 capability_not_ready` with the same code and
**zero** `run`/`run_step` rows.

## Reversal proofs

1. **Empty leaves.** Deleting the `Ok(leaves) if leaves.is_empty()` arm from `pack_state` fails the
   unit test (`left: None`, `right: Some(NotPublished)`); restored.
2. **Unregistered action.** Deleting the `ActionNotRegistered` push fails
   `readiness_reports_each_blocking_fact` (GET), and on the start path `run_capability` returns
   **201** — the run started — instead of `409`; restored.

## Carried proof — the `0020` catalogue→capability move on a pre-existing row

A dedicated ephemeral `postgres:18` container (no volume, removed afterwards; the compose dev DB was
not touched): `scripts/db-roles.sql`, the `_sqlx_migrations` table, then all migrations `0001`–`0030`
applied in filename order. A `catalogue_entry(kind='capability', code='legacy_move_probe',
label='Legacy Probe', meta='{"description":"from the old vocabulary"}')` row was inserted, then the
exact move block from `migrations/0020_capability.sql:134-145` was executed:

```
--- moved capability row
       code        |    label     |       description       | status | meta
-------------------+--------------+-------------------------+--------+------
 legacy_move_probe | Legacy Probe | from the old vocabulary | active | {}
(1 row)
--- catalogue rows left with kind=capability
 remaining
-----------
         0
(1 row)
```

The description was lifted out of `meta`, the status mapped from `is_active`, and the catalogue row
was deleted. (The sqlx ordering itself is already proven by the fresh-DB ledger tests.)

## External reds, attributed by name (not this lane)

- Workspace `clippy --workspace --all-targets -- -D warnings` stops on
  `crates/actions/tests/approval_expired.rs:87` (`explicit_auto_deref`) — another lane's test.
- Crate-wide `cargo fmt --check` stops on the tenancy lane's invite test; every file this lane
  touched is rustfmt-clean by direct check.
- `--check-lock` exits 1: **changed `POST /api/actions/capabilities.get/invocations`** (this lane's
  output schema + timeout) and **added six `/api/platform|tenants` routes** (the tenancy lane, in
  flight). No other action drifted. The lock on disk is 287 ops, sha256
  `47550345fe643d35361f32cc07d7e7c9208a068f3f238b407a8c6da333846fd9`, with `capabilities.get` still
  at `070d0c0a…`; the last settling lane writes it, which will make it **293**.
- `parity_http_mcp` is now red on **`eval_runs.regrade` with no sample** — S20's action, registered
  mid-gate; the set-equality assertion fails before any sample runs. This lane's `capabilities.get`
  sample is present and passed in the same binary.
- The api crate was transiently unbuildable during this gate (`platform.rs` E0308) and the actions
  crate once after (`eval::runs::{RegradeIn,RegradeOut}`); both were peers' in-flight edits, both
  resolved by their owners, and neither is this lane's change.

## Known limits

- Readiness is a LIVE external check for required packs: `capabilities.get` holds its transaction
  across up to one `pack_leaves` call per required binding (the start guard already did). The
  action budget is 30 s (raised from 15 s to match `procedure_runs.start`); a tenant with many
  required packs would need a bounded loop, not a larger timeout.
- `blocking[]` is the GET's field name; the 409 carries the identical `Vec<FieldError>` in the
  API-wide `errors[]` envelope (CCR 1, S15 §13). There is one producing function.

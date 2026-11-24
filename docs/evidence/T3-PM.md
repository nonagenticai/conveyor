# T3 — PM job: gate evidence

Slice T3 of `docs/SPEC_V2.md` §12 (row at `docs/SPEC_V2.md:1724`). The gate text is:

> PM runs a request end to end; cannot approve its own gate; a baseline change requires a change
> request.

Each clause is proved by an integration test in the `conveyor-api` package. Every test calls the
served HTTP API with a real `job_binding` principal; the harness (`crates/actions/tests/support/mod.rs`,
`Running`) boots the real Axum app against an ephemeral Postgres 18 with the real migrations and
tenant RLS. These are not unit tests with mocked stores.

## Clause → test

| §12 gate clause | Test | Command |
|---|---|---|
| PM runs a request end to end | `the_pm_charters_plans_and_baselines_a_request` | `cargo test -p conveyor-api --test pm_lifecycle the_pm_charters_plans_and_baselines_a_request` |
| the PM cannot approve its own gate | `the_pm_cannot_decide_its_own_gate` | `cargo test -p conveyor-api --test pm_self_approval the_pm_cannot_decide_its_own_gate` |
| a baseline change requires a change request | `a_new_baseline_without_a_change_request_is_refused` | `cargo test -p conveyor-api --test pm_baseline_change a_new_baseline_without_a_change_request_is_refused` |
| a baseline change requires a change request (the `pm.plan` path) | `plan_without_a_change_request_is_refused` | `cargo test -p conveyor-api --test pm_baseline_change plan_without_a_change_request_is_refused` |

### 1. PM runs a request end to end — `pm_lifecycle.rs`

`the_pm_charters_plans_and_baselines_a_request` drives the whole spine through the API:
`pm.charter` creates the project, the charter asset and the charter gate in one transaction; a
distinct principal approves the gate; `pm.plan` writes the WBS (parents before children) and the
scope baseline plus its baseline gate; that gate is approved; `pm.get` then returns the project,
the charter asset, the three WBS nodes in order, one baseline whose `approved_by` is the approving
principal, and both gates `approved`. It also reads `governed_asset` directly to confirm the charter
gate's effect approved the asset.

Supporting: `only_pm_creates_and_closes` (same file) shows a non-PM `job_binding` in the same tenant
is refused when it tries to charter or close, and writes nothing.

### 2. The PM cannot approve its own gate — `pm_self_approval.rs`

`the_pm_cannot_decide_its_own_gate` first proves the gate's `requested_by` is the PM principal, then
has that same principal decide it: `409 conflict` with detail `the requester cannot decide their own
gate`; zero `approval_decision` rows written; the gate stays `pending`; the charter asset stays
`draft`. `a_distinct_approver_is_recorded` is the positive control: a distinct principal's approval
is recorded exactly once against that principal and the gate and asset resolve.

### 3. A baseline change requires a change request — `pm_baseline_change.rs`

- `a_new_baseline_without_a_change_request_is_refused`: `baselines.take` on a project with an
  approved baseline, no `change_id` → `409 change_request_required`, detail naming the protected
  baseline; the approved row is byte-identical afterwards and no second baseline is written.
- `approved_change_request_advances_the_baseline`: `changes.request` writes an open change whose
  `baseline_ref` names the baseline it would replace and whose impact block round-trips through
  `meta`; a second principal approves its gate; the new baseline cites the change.
- `plan_without_a_change_request_is_refused`: the same rule on `pm.plan` — no `change_id` → `409
  change_request_required`, refused before any write (WBS count unchanged, still one baseline).
- `plan_with_an_approved_change_advances`: a named, approved `change_id` lets `pm.plan` advance;
  the new baseline row cites the approved change.

## Additional T3 action coverage — `pm_status.rs` (§6.1)

The §12 gate text names three clauses; the T3 action set also includes `pm.status`, proved here and
cited in `scripts/v2-slice-artefacts.json` with the files above:

- `status_report_cites_live_baseline_and_gates` — `cargo test -p conveyor-api --test pm_status status_report_cites_live_baseline_and_gates`:
  `pm.status` writes exactly one `draft` governed asset of kind `status_report` whose body cites the
  live project, baseline, charter and baseline gates, pending change and its gate by id, the WBS,
  and the caller's narrative; the action never publishes it.
- `empty_narrative_refused` — `cargo test -p conveyor-api --test pm_status empty_narrative_refused`:
  empty and whitespace-only narratives are refused `422` with field `narrative` and code
  `status_narrative_required`, and no report is written.

## Verification status — what is and is not claimed

- **Verified while writing this page (commit `93c91ef8`):** all four test files exist and every test
  name above is present in its file as a `#[tokio::test]` integration test.
- **Test-verified:** the behaviour above is what these tests assert end to end against a real
  Postgres 18 and a real HTTP server.
- **Not live-verified:** no run against a deployed environment is cited here. This page claims no
  live deployment record for T3.
- **Not re-executed here:** Rust compilation and test runs are off-box on this estate; the pass
  record is the repository's CI/preflight for the branch, not a local run made while writing this
  page.

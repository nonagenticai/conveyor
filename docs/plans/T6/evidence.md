---
document: T6/evidence
version: 1
status: published
project: T6
category: evidence
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-25
artifact_kind: evidence
phase: G
plateau: capability
---

# T6 — Development project: evidence

## Claim

T6 delivers the development project's governed chain: every step from lane to live verify is
recorded, a red CI run refuses the merge gate, the implementer cannot decide the gates that judge
it, a deploy without a tested rollback is refused and an exercised rollback records both digests
and the schema generation.

## Evidence

- `crates/actions/tests/development_project.rs::a_development_project_records_every_step_from_lane_to_live_verify`
- `::a_red_ci_run_refuses_the_merge_gate`
- `::the_implementer_cannot_decide_its_own_merge_gate`,
  `::a_development_project_cannot_change_the_gates_that_judge_it`
- `::a_deploy_without_a_tested_rollback_is_refused`,
  `::an_exercised_rollback_records_both_digests_and_the_schema_generation`,
  `::an_application_rollback_across_a_non_additive_migration_is_refused`
- `::auto_merge_defaults_off_and_reads_the_tenant_policy_row`
- Migration `0068_development_project.sql`; merges `00094194` (PR #658) and `a8c22f8e` (PR #672).
- **Live rollback (2026-11-27, #669):** a binary embedding migrations through 0080 ran against a
  database carrying 0081 — both digests and `/health` 200 at each step, with a database-backed
  action served on the rolled-back digest; restored by a plain revert. Record:
  `docs/evidence/2026-11-27-rollback-migration-boundary.md`.

## Verification

Run `cargo test -p conveyor-actions --test development_project` on a checked-out main; the CI
equivalent is the PRs' `preflight` and `ci-public` at the merge commits above.

## Links

- `docs/plans/inception/T6/00-project-brief.md`, `01-inception-analysis.md`,
  `02-architecture-design.md`
- Issue #80; PRs #658 and #672; issue #669 (the evidence correction).

## Gap

The rollback evidence was not final at #658 and was corrected by #672 — this document cites the
corrected record. The live rollback (#669, 2026-11-27) was exercised through the deployment
repository and the GitOps reconcile path, not through conveyor's own deploy applier; the applier
gap stands, and the tests prove the refusals and the step records. The tranche-11 authorisation is
not claimed.

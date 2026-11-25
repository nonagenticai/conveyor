---
document: T6/closure-review
version: 1
status: published
project: T6
category: closure-review
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-25
artifact_kind: closure-review
phase: G
plateau: capability
---

# T6 — Development project: closure review

Whether the project delivered its acceptance, on its evidence, and the decision that follows.

## Outcome vs acceptance

| Acceptance criterion | Evidence | Result |
|---|---|---|
| A development project records every step from lane to live verify | `crates/actions/tests/development_project.rs::a_development_project_records_every_step_from_lane_to_live_verify`; migration `0068_development_project.sql`; issue #80 closed by PR #658 (`00094194`) | pass |
| A red CI run refuses the merge gate | `development_project.rs::a_red_ci_run_refuses_the_merge_gate` | pass |
| The implementer cannot decide the gates that judge it | `development_project.rs::the_implementer_cannot_decide_its_own_merge_gate`, `::a_development_project_cannot_change_the_gates_that_judge_it` | pass |
| A deploy without a tested rollback is refused; an exercised rollback records both digests and the schema generation | `development_project.rs::a_deploy_without_a_tested_rollback_is_refused`, `::an_exercised_rollback_records_both_digests_and_the_schema_generation`, `::an_application_rollback_across_a_non_additive_migration_is_refused` | pass |
| Auto-merge defaults off and reads the tenant policy row | `development_project.rs::auto_merge_defaults_off_and_reads_the_tenant_policy_row` | pass |
| The project row, coverage and programme agree | #80 closed; PR #672 (`a8c22f8e`) carries the schema-aware evidence correction (#669); `build-coverage.py --check` green at `7d5ef094` | pass |

## Evidence

- Delivery merges: `00094194130b2e652a2c62fe1f0c11ccfa3e3fdb` (PR #658) and
  `a8c22f8e3401d66f2e2f00d579ee00d73b7775b3` (PR #672, the post-closure evidence correction).
- Schema: `migrations/0068_development_project.sql`.
- Behaviour: `crates/actions/tests/development_project.rs` (the scenarios above).
- Deployment record: deployed digest `sha256:3ce4f31f…` (tag `sha-a2ef919` = main `a2ef9194`)
  containing the merge.

## Open items

- The tranche-11 authorisation is **not** claimed here: the tranche is `planned` and its
  authorisation is the #526 programme chain.
- The rollback evidence was not final at #658 and was corrected by #672; this review cites the
  corrected record.
- No production deploy applier is claimed; the record proves the refusals and the recorded steps.

## Decision

Close. Acceptance is met on the evidence above; the tranche authorisation remains a separate
programme act (#526), and this review does not manufacture it.

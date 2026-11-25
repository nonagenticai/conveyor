---
document: T19F/evidence
version: 2
status: published
project: T19F
category: evidence
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-12-01
artifact_kind: evidence
phase: G
plateau: capability
---

# T19F — Forum bindings: evidence

## Claim

Issue #683 implements the two T19 follow-ups on `main` (merge `eb0deb61`): a `meeting_type`
version is written only by the reviewed authoring route, a governance body decides a subject in a
sitting whose seats write `via='meeting'` decision rows, and a cadence row materialises its own
meeting once. Issue #684 runs the live chain and gate on the deployed engine and records the
programme publication.

The construction evidence below is at `eb0deb61`; the live rows are recorded by
`scripts/t19f-live-gate-check.py --apply` and re-read by `--check`.

## Construction evidence (issue #683)

| Part | Scenario | Evidence | Result |
|---|---|---|---|
| P1.1 | policy v2 publishes; the in-flight v1 meeting still resolves v1 | `crates/api/tests/meetings_authoring.rs::policy_publish_versions_meeting_type` | pass |
| P1.2 | an unreviewed policy asset is refused | `meetings_authoring.rs::policy_publish_requires_review` | pass |
| P1.3 | a reused authoring asset key is refused | `meetings_authoring.rs::a_reused_asset_key_is_refused` | pass |
| P1.4 | the type reads return the versions in order | `meetings_authoring.rs::type_list_and_get_read_versions` | pass |
| P2.1 | the sitting closes through `close_item_tx` with `via='meeting'` rows | `crates/api/tests/meetings_binding.rs::board_sitting_uses_close_item` | pass |
| P2.2 | a body with no bound type refuses a sitting `422` | `meetings_binding.rs::a_body_without_a_type_refuses_a_sitting` | pass |
| P2.3 | a quorum miss defers `inquorate` | `meetings_binding.rs::a_quorum_miss_defers_inquorate` | pass |
| P2.4 | the direct `board_decide` path is unchanged | `meetings_binding.rs::the_direct_board_decide_path_still_works` | pass |
| P3.1 | materialise is idempotent | `meetings_binding.rs::cadence_materialise_is_idempotent` | pass |
| P3.2 | a cadence with no type is refused `422 cadence_type_missing` | `meetings_binding.rs::a_cadence_without_a_type_is_refused` | pass |
| P3.3 | the cadence agenda item resolves the slot | `meetings_binding.rs::the_cadence_item_resolves` | pass |
| P3.4 | T19's slot rule is unchanged (a second scheduled meeting is legal) | `crates/api/tests/meetings_policy.rs::a_second_live_meeting_in_a_slot_is_refused` | pass |
| P4.1 | the mock UI carries the policy, sitting and cadence panels | `node docs/design/scripts/check-v2.cjs` | pass (281 checks) |
| P4.2 | the real app renders the pinned type/version and the policy list | `apps/web/e2e/meetings.spec.ts` in `web.yml` | pass at `eb0deb61` |
| P5.1 | every new action has one declaration and HTTP/MCP agree | `cargo test -p conveyor-actions --test registry --test parity_http_mcp` | pass (12/12, 7/7) |
| P5.2 | the migration ledger reaches 73 | `cargo test -p conveyor-store --test s07_schema` | pass (3/3) |
| P5.3 | T19F is `built` in the coverage and seed data | `python3 scripts/build-coverage.py --check && node docs/design/scripts/check-coverage.cjs` | pass |
| P5.4 | the design pack agrees | `check-data`, `check-docs`, `check-v2`, `check-console`, `build-docs --check` | pass |

## The live gate (issue #684)

`scripts/t19f-live-gate-check.py --apply` performs, through the deployed engine's action surface:
a meeting scheduled against forum policy v1; a v2 published through `authoring.intake` → `draft` →
`approvals.request`/`decide` → `authoring.review` → `authoring.publish`; a board sitting opened on
a real contract with `architecture.board_sit_open`, one chair invitation/response/position and
`architecture.board_sit_decide`; and a cadence row materialised twice. Every write names an
evidence file under `docs/evidence/t19f/gate/`; the ids are `docs/evidence/t19f/live-gate.json`.
`--check` re-reads the rows and asserts:

| Invariant | Evidence | Result |
|---|---|---|
| the v1-pinned meeting still resolves version 1 after the v2 route is exercised | meeting `01a210c2-a327-7dc7-bab4-5a402a8497a8`, `live-gate.json.policy` | pass |
| the forum type carries an active v2 | `live-gate.json.policy` | **blocked** — the review needs a human decision (see the gap below) |
| the sitting's approval is `approved` with exactly one `via.surface='meeting'` decision row naming the chair | approval `01a210c8-9273-7964-9b97-0f3dd810bcf3`, `live-gate.json.sitting` | pass |
| the sitting's agenda item closed as `decision` | item `01a210c8-97c5-7850-8270-16b37a07d8ce` | pass |
| the cadence meeting carries the bound type and the slot, non-cancelled | meeting `01a210c9-2397-73f1-8d07-7e8e807e524f` (materialised twice from one cadence) | pass |

The gate run is `docs/evidence/t19f/live-gate-check.txt`; its state is `live-gate.json`. Eleven of
the twelve invariants pass; the twelfth is blocked on the human review the authoring gate requires
(a service decision is refused verbatim), recorded in the same file.

### The publication

`docs/design/scripts/publish-data.sh` regenerated the pack from the live database through the
deployed binary (`docs/evidence/t19f/publish-final.log`). The strict gate is red on programme-wide
debt that predates this unit (T5/T6/T8/T15/T17 closed since the accepted baseline; S18/S21/SPEC-V2
ledger rows) plus T19F's declared `project_in_unauthorised_tranche` P1 (active in tranche 6, whose
authorisation is #526's unit, and the plan requires T19F to stay active until the tranche-6
review). The pack was therefore published report-only, with the exact findings in the log and in
`conformance.json`.

## The governance chain (issue #684)

`scripts/governance-chain.py --project T19F --check` asserts the T19F record: seven approved
artefacts, both live contracts with a requester distinct from the approver, B7 read back and not
overwritten, one Phase-G compliance review per contract (read from the published
`architecture.json`), and the published `architecture-artifact` document with its four headings.
Evidence under `docs/evidence/t19f/gov/`.

| Invariant | Evidence | Result |
|---|---|---|
| seven mandatory artefacts are approved on T19F | `gov/artifact-*.md` | pass |
| the SoAW and the delivery contract are live, requester ≠ approver | `gov/contract-*.md` | pass |
| B7's baseline and owner are read back, not overwritten | `gov/benefit-b7-baseline.md` | pass |
| both contracts carry a Phase-G compliance review (level `conformant`) | published `architecture.json` | pass |
| the `architecture-artifact` document is published with Metadata/Entries/Links/Document | `gov/artifact-document-publish.md` | pass |

`scripts/governance-chain.py --project T19F --check` passes 24/24 on the published snapshot, and
the T19 invocation it generalised still passes 18/18 (re-run live after the refactor).

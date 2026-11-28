# T4 — Remediation evidence (2026-11-24)

The 2026-11-22 Phase-G review recorded T4 non_conformant and named v1-p6-2, v1-p6-4, v1-p6-5 and
v1-p6-6. Each is resolved in `docs/evidence/v1-conformance-findings.json` with a commit and a test:

- v1-p6-2 — `crates/actions/tests/coach.rs::a_systemic_class_opens_one_measured_kata` (2fe0d30c)
- v1-p6-4 — `crates/api/tests/computed_set.rs::a_distinct_signature_is_never_suppressed_by_a_card_level_count` (8842195f)
- v1-p6-5 — `apps/server/tests/approval_expire.rs::a_card_scoped_gate_timeout_opens_one_interruption` (fc020437)
- v1-p6-6 — `crates/actions/tests/coach.rs::a_coach_card_cannot_be_created_through_the_public_path` (8842195f)

The delivery contract v2 cites these resolutions; the new Phase-G review attests the remediated
state. The 2026-11-22 review is retained as history, not rewritten.

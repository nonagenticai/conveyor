---
document: T12/evidence
version: 1
status: published
project: T12
category: evidence
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-25
artifact_kind: evidence
phase: G
plateau: capability
---

# T12 — One organisation loop: evidence

## Claim

T12's loop is constructed and proven in the server harness: request → PM → delivery jobs → coach
interruption → kata → development project → deploy/live verify → operations incident → lesson, with
every transition audited and the loop within budget, and one benefit baselined before its
transition and read by the acceptance harness. The project's **acceptance is open**: the live half
of P7 was not exercised (owner ruling 2026-11-24).

## Evidence

- `apps/server/tests/organisation_loop.rs` (12 scenarios), including `request_to_pm`,
  `development_project_deploy_and_live_verify`, `deploy_failure_to_incident`,
  `incident_to_promoted_lesson`, `every_loop_transition_audited`, `loop_within_budget` and
  `architecture_and_benefits_acceptance`.
- PR #670, merge `64f3d6ec370da089cefa46b4d8b6b80092ac98ad` (head `e65552f0`).
- The deployed P6 service answered `GET /health` 200 at digest `sha256:84c75723…`, tag
  `sha-64f3d6e`, 2026-11-24T21:00:07Z.

## Verification

`cargo test -p conveyor-server --test organisation_loop` on a checked-out main; the deployment
probe is `curl -fsS https://<deployed-host>/health` at the tag above (recorded in the unit's
evidence, not repeated here).

## Links

- `docs/plans/inception/T12/00-project-brief.md`, `01-inception-analysis.md`,
  `02-architecture-design.md`
- Issue #88 (open); PR #670; `docs/evidence/v2-coverage.md`.

## Gap

The acceptance is open and this document does not claim it:

- P7's live half is **not exercised** (owner ruling 2026-11-24).
- The rollback exercise was not run.
- The budget binding is an open question (tenant, programme or job binding; default tenant with
  per-binding defaults); the observed refusal is the inference router's `429 budget_exceeded`, not
  a conveyor-side spend gate.
- No live benefit delta is claimed; `architecture_and_benefits_acceptance` is a harness test.

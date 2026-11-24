---
document: T19/closure-review
version: 2
status: published
project: T19
category: closure-review
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-24
artifact_kind: closure-review
phase: G
plateau: capability
---

# T19 — Decision forums: closure review

Whether the project delivered its acceptance, on its evidence, and the decision that follows.

## Outcome vs acceptance

| Acceptance criterion | Evidence | Result |
|---|---|---|
| Every D-scenario in `docs/plans/T19-decision-forums.md` §5 passes | 26 scenarios across the eight `meetings_*` test files; table in `docs/plans/T19/evidence.md` | pass |
| The repo gate is green at the exact head | PR #640's `preflight` and `ci-public`; merge commit `3880d28a` | pass |
| The five tables and two alterations are in the migration ledger | `(65, "meetings")`; live `_sqlx_migrations` max is 65 | pass |
| The registry, parity and OpenAPI lock agree | 530 actions / 512 internal; `registry`, `parity_http_mcp`, `--check-lock` | pass |
| The console and design/docs/coverage gates pass | `check-v2`, `check-console`, `check-coverage`, `check-docs`, `check-data` | pass |
| The real app's Meetings destination is proven | `apps/web/e2e/meetings.spec.ts` in `web.yml` | pass |
| One PR `Closes #606` merged | PR #640 merged 2026-11-24T02:29:06Z | pass |
| **The live gate**: the owner decides one item in the web room and one over MCP, both `via='meeting'` | `scripts/t19-live-gate-check.py --state docs/evidence/t19/live-gate.json --seed` — 15/15 rows pass on deployed `a8f67c3e`, and again on the current `7e617673` | pass |
| The meeting's own DoD: every item closed with an outcome and the minutes published | minutes asset `01a20ca3-7d7a-72f0-b63d-96a09064c6df`; meeting `closed` 2026-11-24T08:19:39Z | pass |
| T19 is a live programme row with its benefit | project `T19` on its board; benefit B7; `migrate --seed --tenant nonagentic` read-back | pass |
| T19's seven mandatory TOGAF artefacts are approved rows | `scripts/t19-governance-chain.py --check` — 7/7 approved against T19 | pass |
| T19 has a live architecture contract, requester ≠ approver | Statement of Architecture Work `01a20ce0-5183-776c-84bd-327f2301b19b`, board decision `01a20ce0-644d-7340-a696-1af10fa8c61e` | pass |
| T19 has a live phase-F delivery contract | `01a20cea-91bb-77f4-a880-044c90bb5f0e`, board decision `01a20cea-a68f-7e08-8d0c-399759ef16e0` | pass |
| Each live contract has a Phase-G compliance review | `architecture.reviews_record` × 2, level `conformant`, follow-ups named; the audit's `compliance_review_missing` clears | pass |
| B7 has a measurement baseline and an owner | baseline `{value: 0, …}`; benefit owner = programme-manager job | pass |
| The TOGAF `architecture-artifact` standard is evidenced, not partial | published output `01a20ce4-73e4-7009-9ec7-eb2265e9e84e` with Metadata / Entries / Links / Document | pass |

## Evidence

- Implementation: PR #640 (`3880d28a`), deployed revision `a8f67c3e`; the twelve `meetings.*`
  actions and migration `0065_meetings` are present in the running engine.
- Live gate: `docs/plans/T19/evidence.md` § "The live gate"; raw read-backs in
  `docs/evidence/t19/` (`live-gate.json`, `live-gate-check.txt`, `meeting-schedule.json`,
  `minutes-publish.json`, `meeting-close.json`, `refresh-readback.json`, `seed-readback.txt`).
  The human seat was exercised by the tenant admin identity `FR Test`, supplied by the owner; the
  record names that identity, not the owner's personal account.
- Deployment fixes the gate required, both merged in `gitops-production`: #5429 (the public MCP
  host was not in `CONVEYOR_MCP_ALLOWED_HOSTS`, so every handshake was `403 Host header is not
  allowed`) and #5432 (the MCP session lives in one pod; a DestinationRule now pins it on
  `mcp-session-id`, and the probe went from 6/8 failures to 8/8 served).
- Programme: `migrate --seed --tenant nonagentic` created the catalogue's `meeting` card type and
  T19 with its board; `programmes.refresh` then reported created=0, updated=0, skipped=51.
  The published snapshot is regenerated in the same unit.
- Conformance chain (deployed `7e617673`): the seven mandatory TOGAF artefacts and their links, the
  live Statement of Architecture Work and the published `architecture-artifact` output are recorded
  in `docs/plans/T19/evidence.md` § "The T19 conformance chain" with per-act read-backs under
  `docs/evidence/t19/gov/`. The approvals were performed by the tenant's `conveyor-approver` seat
  under the published standing delegation `01a1fb9f-ac2d-732d-b274-1079be3dd2b3`; they are not
  claimed as the owner's personal decisions.

## Open items

| Item | Owner | Next step |
|---|---|---|
| The `meeting_type` authoring route (a policy change is a new version through a governance route) | programme-architect | a versioned follow-up unit; not part of D1–D8 |
| The T9 board / T10 cadence binding to meeting types | programme-architect | the plan's declared follow-up integration unit |
| Tranche 4 authorisation: T19 is closed while tranche 4 is `planned` (audit P0 `project_in_unauthorised_tranche`) | the #526 programme-rework lane | the retrospective tranche 2→4 review chain; this unit does not backfill reviews whose exit criteria other projects have not met |
| The homepage mirror of the design pack | programme-manager | a separate gated `make verify` + PR in the homepage repository |
| The superseded approval `t19_live_gate_web` (the first request, made by the seat itself) | programme-manager | rejected with a note so the queue shows it was replaced by `t19_live_gate_web_v2` |

## Decision

Close. T19's D1–D8 acceptance, its live gate on the deployment now running, and its own TOGAF chain
(seven approved artefacts, links, live contract, B7 measurement, published architecture-artifact)
are met on the evidence above; the meeting closed on its own DoD and the project is a live programme
row with benefit B7. The remaining open items are named follow-ups with owners; the tranche-4
authorisation is the #526 lane's retrospective chain, not a condition of this closure.

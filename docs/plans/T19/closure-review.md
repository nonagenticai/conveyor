---
document: T19/closure-review
version: 1
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
| **The live gate**: the owner decides one item in the web room and one over MCP, both `via='meeting'` | `scripts/t19-live-gate-check.py --state docs/evidence/t19/live-gate.json --seed` — 15/15 rows pass on deployed `a8f67c3e` | pass |
| The meeting's own DoD: every item closed with an outcome and the minutes published | minutes asset `01a20ca3-7d7a-72f0-b63d-96a09064c6df`; meeting `closed` 2026-11-24T08:19:39Z | pass |
| T19 is a live programme row with its benefit | project `T19` on its board; benefit B7; `migrate --seed --tenant nonagentic` read-back | pass |

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

## Open items

| Item | Owner | Next step |
|---|---|---|
| The `meeting_type` authoring route (a policy change is a new version through a governance route) | programme-architect | a versioned follow-up unit; not part of D1–D8 |
| The T9 board / T10 cadence binding to meeting types | programme-architect | the plan's declared follow-up integration unit |
| T19's wider MSP chain (charter, approved baseline, WBS/WSJF, SoAW + delivery contract, Phase-G review, demo credit) | the #526 programme-rework lane | recorded there under its own verdict; this closure does not claim it |
| The homepage mirror of the design pack | programme-manager | a separate gated `make verify` + PR in the homepage repository |
| The superseded approval `t19_live_gate_web` (the first request, made by the seat itself) | programme-manager | rejected with a note so the queue shows it was replaced by `t19_live_gate_web_v2` |

## Decision

Close. T19's D1–D8 acceptance and its live gate are met on the evidence above, the meeting closed
on its own DoD, and the project is a live programme row with benefit B7. The open items are named
follow-ups with owners; the wider MSP chain is the #526 lane's record, not a condition of this
closure.

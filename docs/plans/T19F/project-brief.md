---
document: T19F/project-brief
version: 1
status: draft
project: T19F
category: project-brief
owner: programme-manager
reviewer: SRO
review_date: 2026-12-01
artifact_kind: project-brief
phase: Preliminary
plateau: definition
---

# T19F — Decision-forum governance: project brief

## Purpose

Close the two seams T19 left open: a forum policy can be created but not changed through a
governed route, and the T9 board and T10 cadence rows do not name the meeting types they use. The
project makes the policy a versioned release through the tenant's authoring route, and makes the
board sitting and the cadence materialisation first-class consumers of T19's meeting arithmetic.

## Outcome

After T19F:

- a policy change is a reviewed authoring asset that publishes a new immutable `meeting_type`
  version; an in-flight meeting keeps the version it pinned;
- a board subject decision can be taken in a sitting whose per-seat decisions are recorded
  `via='meeting'`, with the same quorum, department, evidence and conflict rules as T19;
- a cadence row names its meeting type and materialises exactly one meeting, idempotently;
- the direct board path and every T19 primitive are unchanged;
- T19F is a live programme row with its MSP/TOGAF/SAFe artefacts, evidence and a closure review
  that defers closure to the tranche-6 review.

## Scope boundary

In: the authoring hook, the two read actions, the binding columns, the board-sitting actions, the
cadence materialisation, the mock-UI and real-app panels, the programme record and the live proof.

Out: a second decision store, a chat/AV/transcript system, a rebuild of T9/T10 or the approval
primitive, a change to the direct `board_decide` path, the T18 shell/nav, the homepage mirror, the
tranche 2→4 retrospective chain (#526).

## Stakeholders

| Role | Interest |
|---|---|
| Programme sponsor / SRO | a governed forum policy and a board record that shows its deliberation |
| Programme manager | T19F's tranche-6 evidence and the B7 realisation increment |
| Programme architect / Architecture Board | the sitting path, the contract, the artefact set |
| BCM | B7's measurement across board sittings and materialised cadences |
| Department heads and job owners | seats that are invited and counted by department |
| Delivery teams | the unchanged direct paths and the existing meetings |

## Constraints

- The authoring route is T17's; no parallel policy store.
- `meeting_type` versions are immutable; a meeting pins its version.
- The board's decision arithmetic stays T19's `close_item_tx`; no second calculation.
- The cadence instance rule is a database constraint, not an action convention.
- No local Rust compile; the compile/test verdict is `preflight.yml` and the PR's CI.
- T19F stays `active` while its tranche is planned; closure is a programme decision.

## Acceptance

The follow-on plan's P1–P5 scenarios (`docs/plans/T19F-meeting-bindings.md` §5) pass at the PR
head or on the deployed engine; the migration ledger reaches 68; the programme record carries T19F
and B7; the strict conformance publish is green with no new T19F finding.

## Risks

| Risk | Response |
|---|---|
| A policy change is smuggled through `meetings.schedule` | the schedule refuses a policy when the key exists; the authoring route is the only version writer |
| A board sitting lowers dual control | `close_item_tx` applies the linked approval's stricter policy; the sitting adds no second path |
| A cadence slot gets two meetings | the partial unique index on the slot; the action returns the existing instance |
| T17's authoring route is mid-flight (#655) | the hook is additive and signature-stable; rebase or coordinate with the T17 lane |
| T19F closes in a planned tranche | it stays active; the closure review defers closure to the tranche-6 review |

## Approval

The brief is reviewed with the inception, architecture, MSP and SAFe artefacts on 2026-12-01; the
Approval of the plan is the programme manager's, the architecture review is the Architecture
Board's.

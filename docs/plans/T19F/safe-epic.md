---
document: T19F/safe-epic
version: 1
status: draft
project: T19F
category: epic
owner: programme-manager
reviewer: SRO
review_date: 2026-12-01
artifact_kind: epic
phase: F
plateau: HLD
---

# T19F — SAFe epic

## Epic hypothesis

If a forum policy is a reviewed authoring release and the board/cadence rows name their meeting
types, then every change to how the organisation decides is governed, every board decision carries
its deliberation, and every cadence sitting has its meeting — measured by the B7 increment.

**Leading indicator:** a policy v2 published through the authoring route with v1 still pinned by an
in-flight meeting; one board sitting whose decisions are `via='meeting'`; one cadence slot
materialised once. **Lagging indicator:** B7's board-sitting and cadence counts at the tranche-6
review.

## Non-goals

A second decision store; a chat/AV/transcript system; a rebuild of T9/T10; a change to the direct
board path; a new UI destination.

## Features

| Id | Feature | Traces |
|---|---|---|
| F1 | A reviewed policy asset publishes the next immutable `meeting_type` version | R1, R2, R3 |
| F2 | A board subject decision is taken in a sitting with per-seat `via='meeting'` decisions | R4, R5, R7 |
| F3 | A cadence row names its type and materialises exactly one meeting | R6 |
| F4 | The mock UI and the real app show the policy versions, the sitting and the cadence state | §10.5; T18's screen |

## Enablers

| Id | Enabler | Serves |
|---|---|---|
| E1 | Migration 0068: the authoring link, the two binding keys and the cadence-slot index | F1, F2, F3 |
| E2 | The extracted board approval insert shared by `board_decide` and `board_sit_open` | F2 |
| E3 | The registry/OpenAPI/coverage regeneration and the T19F programme row | F1–F4 |

## WSJF inputs

| Feature | Business value | Time criticality | Risk reduction | Job size | WSJF |
|---|---:|---:|---:|---:|---:|
| F1 policy route | 8 | 5 | 5 | 3 | 6.0 |
| F2 board sitting | 8 | 5 | 8 | 5 | 4.2 |
| F3 cadence | 5 | 3 | 5 | 3 | 4.3 |
| F4 surfaces | 5 | 3 | 3 | 3 | 3.7 |

## PI objectives

| Objective | Outcome | Confidence |
|---|---|---|
| O1 | The policy route is proven by a unit and a live gate | high |
| O2 | The board sitting is proven with a real subject decision | high |
| O3 | The cadence slot materialises once, idempotently | high |
| O4 | T19F is in the programme record with its artefacts and a strict publish | medium (deployment-gated) |

## Inspect and adapt

The PI review reads the live gate and the B7 increment; a policy change that cannot be published, a
sitting that cannot reach quorum, or a cadence that materialises twice is a defect and a kata
candidate, not a waived item.

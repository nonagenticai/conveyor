# Tranche 2 / PI 2 — the rework record (2026-11-21)

Delegation: `01a1ff1f-e4ba-72dc-9c1a-13049933cf14` (the U5 extension of the standing delegation).
Tranche: `01a1e9e0-e2ac-786b-ac48-3b8bcdf12188` (state `authorised` at 2026-11-21T19:51:16Z by the
tranche-1 `continue` review `01a1ffa9-c06f-7921-9d6b-611d871ce6cc`).
PI period: `tranche-2`. Programme: `01a1d039-0d24-7d16-ae52-4841e96ccfac`.

## Why this tranche is retrospective

Both of its projects are already recorded as delivered: S03 closed its V1 slice and T2 closed the
decision ladder. What was never recorded is the programme's own evidence for them — charter, scope
baseline, delivery contract, the A–D artefacts, a Phase-G review and a closure review. This batch
writes that record from each project's plan and its code at HEAD, and re-closes each project through
the one gate.

T2 is additionally one of the four live-register `closed_without_closure_review` projects: its
closure has no review at all, so it is reopened before it is re-closed.

## The acts

| act | action | row |
|---|---|---|
| PI cadence | `cadences.create` | `01a2004a-51f4-7482-b5f8-455a4bd371b5` |
| backlog refinement | `cadences.create` | `01a2004a-739d-717e-81dd-15ddf37f4c2f` |
| system demo | `cadences.create` | `01a2004a-87d8-77e9-b06a-171361548e38` |
| inspect and adapt | `cadences.create` | `01a2004a-9c9a-7999-bbbd-9976427009b6` |

Each write was read back through `cadences.list` and its id matched in the returned rows; the
live list shows 12 cadence rows, four of them for `tranche-2`.

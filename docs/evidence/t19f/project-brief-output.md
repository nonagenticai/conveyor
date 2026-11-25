# T19F — Project brief

One page: what this project changes, what it reuses, and how it is accepted.

## Outcome

After T19F, a forum policy changes only through the reviewed authoring route (publishing a new
immutable `meeting_type` version while an in-flight meeting keeps its pinned version), a board
subject decision is taken in a sitting whose per-seat decisions are recorded `via='meeting'`, and
a cadence row materialises exactly one meeting — with T19's primitives and the direct board path
unchanged.

## Boundary

In: the authoring publish hook and the two policy reads, the `governance_body` and
`programme_cadence` bindings, the board-sitting actions, the cadence materialisation, the mock-UI
and real-app panels, the programme record and the live proof.

Out: a second decision store, a chat/AV/transcript system, a rebuild of T9/T10 or the approval
primitive, a change to the direct `board_decide` path, the T18 shell/nav, the homepage mirror, the
tranche 2→4 retrospective chain (#526), and programme-wide conformance debt.

## Reuse sources

- T17's authoring route (`authoring.intake/draft/review/publish`) — the only policy writer.
- T19's meeting aggregate and `close_item_tx` — the only decision arithmetic.
- T9's `governance_body` — the sitting's convener and body.
- T10's `programme_cadence` — the materialised forum.
- `docs/SPEC_V2.md` §6.8 and §12 T19F — the gate.

## Inception units

- `docs/plans/T19F/inception-analysis.md` — stakeholders, risks and the R1–R8 requirements.
- `docs/plans/T19/closure-review.md` — the two open items this project closes.
- `docs/plans/T19F/architecture-design.md` — PSA + HLD.

## Dependencies

- blocked-by #683 — the implementation (merged in #700, `eb0deb61`).
- acceptance-blocked-by — the deployed image carrying migration 0073 (rolled out and verified).
- related #526 — tranche authorisation; T19F stays `active` until the tranche-6 review.
- related #655 — the authoring route's own unit.
- related #650 — the T19 close-out this project follows.

## Acceptance

- Given a live meeting pinned to v1, when a reviewed policy asset publishes v2, then the meeting
  still resolves v1 and the type list carries both versions.
- Given a board subject and a bound forum type, when the sitting's chair records a position and
  the item is decided, then the approval carries one `via='meeting'` decision row per seat.
- Given a cadence row with a type, when materialise runs twice, then exactly one non-cancelled
  meeting exists with the bound type and the slot.
- Given T19F's record, when the governance chain checks, then seven artefacts, both contracts,
  both Phase-G reviews, B7 read-not-overwritten and the published architecture-artifact exist.

## Capabilities

- B1 Governed tenant assets — T17's route extended with the policy payload.
- B2 Deliberated decisions — T19's meeting reused by the board sitting.
- B3 Cadence-driven forums — T10's cadence row bound to a meeting type.

## Plan

`docs/plans/T19F-meeting-bindings.md`, delivered as #682 (plan and artefacts), #683
(implementation, merged `eb0deb61`) and #684 (live proof and publication).

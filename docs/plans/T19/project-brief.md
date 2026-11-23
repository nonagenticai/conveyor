---
document: T19/project-brief
version: 1
status: draft
project: T19
category: project-brief
owner: programme-manager
reviewer: SRO
review_date: 2026-11-30
---

# T19 — Project brief

## Outcome

A tenant convenes a cross-department meeting, resolves existing approvals through typed agenda
items and positions, records each outcome by the forum's rule and the linked approval's stricter
policy, and keeps the commitments, actions, dissent and minutes as governed rows — with any human,
including the owner, deciding live in the web room or asynchronously over MCP.

## Boundary

**In:** the five meeting tables and the two additive alterations; the `meetings.*` action family;
the meeting card and DoD; the Meetings console destination; the `governance_body` and
`programme_cadence` consumer seams; the seven decision instruments and the never-decided flag.

**Out:** chat, transcripts, AV integration, a calendar dependency, an LLM that selects or decides,
a second decision store, a rebuild of T9's board or T10's cadence, and the T18 UI lane
(#562–#566, PR #603). The approval primitive is extended for job-binding participants; no other
V1 table changes.

## Reuse sources

| Source | Path:symbol | Question the analysis answers |
|---|---|---|
| Approval primitive | `migrations/0002_domain.sql:188` `approval`; `:231` `approval_decision` | can a job-binding seat write a decision row? (No — extension required) |
| Surface record | `migrations/0002_domain.sql:241` `approval_decision.via jsonb`; `crates/api/tests/approval_dual.rs:147` | how is a surface recorded today? |
| Human participants | `migrations/0002_domain.sql` `app_user` | can every human hold a seat? (Yes) |
| Card and DoD | `card`, `card_type`; `seed/catalogue.json` | is `meeting` a card type, or a new unit? (A card) |
| Governed assets | `migrations/0044_plan_spine.sql:411-419` | is `minutes` a kind? (No — widen) |
| Dependencies | `migrations/0044_plan_spine.sql` `project_dependency` | commitment → dependency edge |
| Board context | `migrations/0054_architecture.sql` `governance_body` | convener seam, built by T9 |
| Cadence context | `migrations/0056_cadence_tranche.sql` `programme_cadence` | cadence-slot agenda source, built by T10 |
| Action registry | `crates/actions/src/control/approvals.rs`; `crates/actions/tests/` | the declaration → HTTP/MCP/OpenAPI pattern |
| Console | `apps/web/src/app.rs`; `docs/design/v2.js` §10.5 | route and screen contract, owned by T18/T19 |

## Inception units

The analysis and architecture outputs are published with this brief:
`docs/plans/T19/inception-analysis.md` and `docs/plans/T19/architecture-design.md`. Both carry the
standard's metadata; the architecture set is catalogued in `artifact-register.md`.

## Dependencies

| Relation | Issue | Why |
|---|---|---|
| blocked-by | #72 (T1) | plan spine, registers, `project_dependency` |
| blocked-by | #73 (T2) | L3 ladder and policy evaluation |
| blocked-by | #74 (T3) | gate routing and the card/closure path |
| blocked-by | #120 (P0) | the programme gate and this brief's standard |
| related | #81 (T9) | `governance_body` consumer; follow-up binding, not a rebuild |
| related | #82 (T10) | `programme_cadence` consumer; follow-up binding, not a rebuild |
| related | #562–#566 | T18 UI lane, separate files |

## Acceptance

The SPEC §12 T19 gate, in six clauses: a three-department majority with recorded dissent; an
`inquorate` deferral that cannot decide; no lowering of dual control or waiving of evidence; the
owner decides one item live and one over MCP, both `via->>'surface'='meeting'`; minutes behind the
chair's gate and every action a card; a decision-free forum flagged. The plan's D1–D8 carry the
per-part BDD; the implementation issue is #606.

## Capabilities

Programme manager, programme architect, business analyst, project manager, the SRO/BCM and the
department positions that must be represented. The organization's high-level work is the
capability this brief serves; the meeting is its decision forum, not a new job.

## Plan

`docs/plans/T19-decision-forums.md` (this unit). Implementation: #606.

## Risks

| Risk | Posture |
|---|---|
| R20 meeting theatre | typed items, evidence minimum, closed outcomes, owner+date commitments, minutes matching rows, metered and retired |
| Lowering dual control or waiving evidence | stricter-wins arithmetic; the approval's policy is evaluated after the meeting rule |
| Job-seat misattribution | the participant extension is explicit and tested; no human approver is borrowed |
| Calendar/AV dependency | attendance is the record; ICS is a read; no calendar or AV service is required |
| T18 lane collision | different files and routes; T19 owns `/meetings`, T18's shell links it |

## Gaps

The meeting code, migrations, actions and screen are absent; this unit delivers the plan and the
programme artefacts only. The `approval_decision` participant extension and the `minutes` kind are
the two measured schema gaps.

## Links

- Plan: `docs/plans/T19-decision-forums.md`
- Analysis: `docs/plans/T19/inception-analysis.md`
- Architecture: `docs/plans/T19/architecture-design.md`
- MSP: `docs/plans/T19/msp-profile.md`
- SAFe: `docs/plans/T19/safe-epic.md`
- Register: `docs/plans/T19/artifact-register.md`
- Spec: `docs/SPEC_V2.md` §6.8, §8.1–§8.3, §10.3–§10.5, §12 T19, D66

---
document: T19/inception-analysis
version: 1
status: draft
project: T19
category: inception-analysis
owner: business-analyst
reviewer: programme-architect
review_date: 2026-11-30
---

# T19 — Inception analysis

## Current state

Measured at `main@b9a6e293` on 2026-11-23.

| Area | State | Evidence |
|---|---|---|
| Meeting aggregate | ABSENT | `git grep -n 'meeting_type\|meeting_position\|meetings\.' -- migrations crates apps` → 0 |
| `approval_decision` participants | HUMAN-ONLY | `migrations/0002_domain.sql:231-248`: `approver_id uuid NOT NULL REFERENCES app_user(id)`; `via jsonb` |
| Decision values | `approve \| reject` | `migrations/0002_domain.sql:236`; conditions must live in the position/approval meta |
| `minutes` as a governed asset | ABSENT | `migrations/0044_plan_spine.sql:411-419` kind CHECK |
| `meeting` card type | ABSENT | `seed/catalogue.json` has no `meeting` |
| Meetings console route | ABSENT | `apps/web/src/app.rs` has no `/meetings`; `docs/design/v2.js` has no Meetings screen |
| Approvals, cards, runs | DONE | `migrations/0002_domain.sql`; `crates/actions/src/control/approvals.rs` |
| `app_user` humans | DONE | `migrations/0002_domain.sql`; referenced by `approval_decision` |
| `project_dependency` | DONE (T1) | `migrations/0044_plan_spine.sql` |
| `governance_body` | DONE (T9, #81 closed) | `migrations/0054_architecture.sql` |
| `programme_cadence` | DONE (T10, #82 closed) | `migrations/0056_cadence_tranche.sql` |
| T18 UI lane | IN FLIGHT | #562–#566; PR #603; separate files |
| T19 programme entries | ABSENT | `seed/programme.json` has no T19; `scripts/v2-slice-artefacts.json` has T0–T18 |

## Reuse sources

| Source | Path:symbol | Reuse decision |
|---|---|---|
| Approval and decision | `migrations/0002_domain.sql:188` `approval`, `:231` `approval_decision` | Reuse lifecycle and uniqueness; **extend** the participant columns |
| Decision attribution | `crates/actions/src/control/approvals.rs:87` `decided_by_kind` | Reuse; job seats use `agent`, humans `human` |
| Decision read model | `crates/actions/src/control/approvals.rs:930` `decisions_for`; `crates/actions/src/control/views.rs:447` `ApprovalDecisionOut` | Reuse; add participant kind to the view |
| Store record | `crates/store/src/approval.rs:210` `ApprovalDecisionRecord` | Reuse; add `job_binding_id`/`participant_kind` |
| Surface record | `migrations/0002_domain.sql:241` `via jsonb`; `crates/api/tests/approval_dual.rs:147` | Reuse; `via->>'surface'='meeting'` |
| Human seat | `app_user` (`migrations/0002_domain.sql`) | Reuse |
| Card and DoD | `card`, `card_type`, `seed/catalogue.json` | Reuse; add the `meeting` type |
| Minutes | `governed_asset` (`migrations/0002_domain.sql:287`; `0044_plan_spine.sql:411`) | Reuse; widen the kind CHECK with `minutes` |
| Commitments | `project_dependency` (`0044_plan_spine.sql`) | Reuse |
| Board convener | `governance_body` (`0054_architecture.sql`) | Reuse; T19 consumes, T9's macros follow |
| Cadence slot | `programme_cadence` (`0056_cadence_tranche.sql`) | Reuse; T19 consumes, T10's binding follows |
| Action declaration | `crates/actions/src/control/approvals.rs`; `crates/actions/tests/` | Reuse the `ActionSpec` pattern for `meetings.*` |
| Console contract | `apps/web/src/app.rs`; `docs/design/v2.js` §10.5 | Extend the T18 route registry with `/meetings` and its spec |

## Build vs reuse

**Build:** the five meeting tables; the twelve actions; the console screen; the participant
extension; the `minutes` kind; the card type and seed; the instruments and flag.

**Reuse, never rebuild:** the approval lifecycle, the card/run path, the governed-asset lifecycle,
the dependency edge, the board and cadence rows. The meeting is a composition over these
primitives; a second decision table, a separate meeting-decision table, or a chat store is
rejected by §6.8's first principle.

**The one core-table decision:** a job-binding seat must write an `approval_decision` row. The
existing `approver_id` FK is human-only, so T19 adds nullable `approver_id`, `job_binding_id`,
`participant_kind`, an exactly-one CHECK, and a partial unique index. The alternative — writing
L1 `decision` rows for job seats — would split the decision record and is rejected.

## Constraints and licences

All work is in this repository; the tables, actions, console and tests use the existing Rust
workspace and Leptos app. No third-party licence is introduced. No calendar or AV service is
integrated; an ICS feed is a read of attendance. The MCP surface is the existing stateless
infrastructure contract; no new port or external runtime is used.

## Risks

| Risk | Evidence it is real | Mitigation in this slice |
|---|---|---|
| Meeting theatre (R20) | approvals today have no deliberation record | typed items, evidence minimum, closed outcomes, owner+date commitments, minutes matching rows, metering |
| Lowering dual control | the linked approval's policy can be stricter | evaluate the approval after the meeting rule; stricter wins; a test with dual control |
| Job-seat misattribution | `approver_id → app_user` cannot express a job | explicit participant extension, CHECK and test |
| Silent LLM selection | an agenda could be generated | L0 query + `agenda_add`; the LLM has no action to select or position |
| Calendar dependency | an ICS feed can imply a calendar system | attendance is the record; the feed is a read |
| T18 lane collision | `/meetings` is a new route in a lane that is in flight | T19 owns the route/screen; T18's shell lists it without building it |

## Open questions

| # | Question | Default |
|---|---|---|
| 1 | Should a `meeting_type` allow a job to convene without a human chair? | yes; the type's authority row decides, and the chair seat remains optional where the type allows |
| 2 | Does an `approve_with_conditions` decision row store `approve` and leave conditions in the position? | yes; `approval_decision` admits only approve/reject and the derived outcome is computed from positions |
| 3 | Is a minute line that cites a row enough, or must it embed the row id? | embed the row id; a line without a matching row is a defect |

## Links

- Brief: `docs/plans/T19/project-brief.md`
- Architecture: `docs/plans/T19/architecture-design.md`
- Plan: `docs/plans/T19-decision-forums.md`
- Spec: `docs/SPEC_V2.md` §6.8, §10.3–§10.4, §12 T19, D66

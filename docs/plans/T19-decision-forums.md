---
slice: T19
title: Decision forums — meetings, positions and the outcome arithmetic
status: built
depends_on: [T1, T2, T3, P0]
spec_sections: ["§6.8", "§8.1", "§8.2", "§8.3", "§10.3", "§10.4", "§10.5", "§12 T19", "D66"]
adds_tables: [meeting_type, meeting, meeting_agenda_item, meeting_attendance, meeting_position]
adds_actions: [meetings.schedule, meetings.open, meetings.invite, meetings.respond, meetings.agenda_add, meetings.position, meetings.defer, meetings.close_item, meetings.close, meetings.minutes_publish, meetings.list, meetings.get]
adds_events: [meeting.opened, meeting_item.closed, meeting_minutes.published, meeting.closed]
adds_modules: [meetings]
parity_ids: []
---

# T19 — Decision forums (meetings)

Programme project: T19 · issue #606 · close-out #650

**Gate (SPEC_V2 §12 T19):** a three-department board meeting decides its agenda items by recorded
majority with a recorded dissent; an item missing a required department is deferred `inquorate` and
cannot be decided; a meeting cannot lower a linked approval's dual control or waive its evidence;
the owner decides one item in the web meeting room and another asynchronously from the agent front
end over MCP, both recorded `via='meeting'`; minutes publish behind the chair's gate and every
action becomes a card; a forum with no decision over a cycle is flagged.

## 1. Objective

The conveyor records the decision *and* the deliberation. High-level work — programme management,
architecture, analysis, project management — happens in meetings between positions from different
departments; today only the resulting `approval` row survives. T19 makes the meeting a first-class
**card** whose agenda items resolve existing approvals, whose participants are job bindings or
humans holding typed positions, and whose outcomes are derived by arithmetic over those positions
and the linked approval's stricter policy. The record is rows, not a transcript: the meeting adds
the cross-department positions, conditions, commitments and dissent, and never creates a second
decision store.

## 2. Starting state (measured at `main@b9a6e293` on 2026-11-23)

| Part | State | Evidence |
|---|---|---|
| Meeting tables and actions | ABSENT | `git grep -n 'meeting_type\|meeting_position\|meetings\.' -- migrations crates apps` → 0 |
| `approval_decision` participant model | PARTIAL | `migrations/0002_domain.sql:231-248`: `approver_id → app_user` is human-only; `via jsonb` exists; `decision ∈ approve\|reject` |
| `governed_asset(kind='minutes')` | ABSENT | `migrations/0044_plan_spine.sql:411-419` kind CHECK has no `minutes` |
| Meeting card type / console route | ABSENT | `seed/catalogue.json` has no `meeting`; `apps/web/src` has no `/meetings` |
| Approvals, cards, runs, `app_user` | DONE, reused | `migrations/0002_domain.sql`; `approval_decision.via` is the surface record |
| `project_dependency` | DONE (T1) | `migrations/0044_plan_spine.sql` |
| `governance_body` | DONE (T9) | `migrations/0054_architecture.sql`; #81 closed |
| `programme_cadence` | DONE (T10) | `migrations/0056_cadence_tranche.sql`; #82 closed |
| T18 UI lane | IN FLIGHT, separate | #562–#566, PR #603; T19 does not touch it |
| T19 plan/programme artefacts | ABSENT | this unit (#605) creates them |

The two measured gaps that change the shape of the build: `approval_decision` has no job-binding
participant (§4.1), and `minutes` is not a governed-asset kind (§6).

## 3. Scope

**IN** — `meeting_type` policy and versioning; `meeting` as a card with its DoD; agenda items over
`approval | register_item | asset | cadence`; attendance for `job_binding | app_user`; one live
position per participant per item; quorum, required departments and the three decision rules;
`close_item`'s per-seat `approval_decision` rows and stricter-wins arithmetic; the closed outcome
set; commitments as `project_dependency` or cards; actions as cards; minutes as a chaired
`governed_asset`; the human seat (including the owner) live in the web room and asynchronously over
web/Telegram/MCP; participant-hours metering; the never-decided forum flag; the `/meetings` console
destination; the `meeting` card type and seed; the T9 `governance_body` and T10 `programme_cadence`
consumer seams.

**OUT** — a chat system, a transcript store or an AV product; a calendar dependency (ICS is a read
of attendance); an LLM that selects items, decides, infers a position or marks attendance; a
second decision store; a rebuild of T9's board, T10's cadence or the approval primitive; the T18 UI
lane; ArchiMate-style rendering of the minutes.

**UI surface declared for the console:** the **Meetings** destination (§10.5) with the invitation
list, the room (agenda, evidence, participants by department/seat, live positions and the running
outcome), the asynchronous decision queue, minutes/actions, and history/latency/load. The route
`/meetings` and `/meetings/:id` are declared in the spec; T18's shell lists the destination, T19
owns the screen and its `meetings.spec.ts`.

## 4. The model

### 4.1 The meeting, the card and the participant extension

A meeting is a `card` (its chair owns it, WIP limits and DoD apply) plus a `meeting` row. Its DoD is
every agenda item closed with an outcome and the minutes published. The convener is a
`governance_body`, a `job_binding` or an `app_user`; one live meeting exists per cadence slot
(`uq_meeting_live`).

The approval primitive's decision row is human-only today: `approval_decision.approver_id uuid NOT
NULL REFERENCES app_user(id)`. §6.8 requires one decision row per qualifying **seat**, and seats
are both humans and job bindings. T19 therefore extends the primitive rather than inventing a
second decision record:

- `approver_id` becomes nullable; add `job_binding_id uuid REFERENCES job_binding(id)`;
- add `participant_kind text NOT NULL DEFAULT 'human' CHECK (participant_kind IN ('human','job'))`;
- a CHECK requires exactly one of `approver_id`, `job_binding_id`;
- the existing unique `(approval_id, approver_id)` stays; add a partial unique
  `(approval_id, job_binding_id) WHERE job_binding_id IS NOT NULL`;
- existing rows backfill `participant_kind='human'`; `decided_by_kind='agent'` for job seats.

The alteration is additive and tenant-scoped like the existing table. It is the one core-table
change in this slice; the alternative — writing a meeting decision into the L1 `decision` table —
would create the second decision store §6.8 forbids.

### 4.2 Policy and convening

`meeting_type` is the forum policy: purpose, `agenda_sources[]`, required participation
(departments and positions with minimum counts), quorum, decision rule (`unanimous | majority |
chair`), evidence minimum, decision window, timebox and allowed outcomes. It is tenant-scoped and
versioned (P11): a meeting pins the type version it was opened under, so a policy change never
rewrites an in-flight forum. Who may convene is an authority row, not code.

### 4.3 The agenda is an L0 query

The item set is L0 over `agenda_sources[]` — open approvals for the body, register items of named
kinds, dependency slips, cadence slots — plus explicit `meetings.agenda_add`. A deciding item names
exactly one pre-opened `approval` with its evidence and policy; there is no implicit approval. An
item without the type's evidence minimum is refused `422 item_evidence_missing`. The LLM drafts
wording and the pre-read pack; the chair adopts the agenda. It never selects.

### 4.4 Attendance and positions

`meeting_attendance` holds the participant (`job_binding` or `app_user`), seat
(`chair | member | presenter | observer | delegate`), department and invitation/response state;
delegation is allowed only where the type permits it. `meeting_position` is one live row per
participant per item: stance (`approve | approve_with_conditions | reject | abstain |
request_evidence`), rationale, conditions, optional commitment, surface and time. The position is
immutable once the item closes. An observer's position is recorded and never counts.

`approval_decision.decision` admits only `approve | reject`; a position of
`approve_with_conditions` writes an `approve` decision row and the conditions stay in the position,
the approval's `meta` and the minutes. The derived item outcome can still be "approved with
conditions" because it is computed from the positions, not copied from the decision column.

### 4.5 The outcome arithmetic

`close_item` derives the item outcome in this order:

1. the attendance quorum over the type's required participation; a missing required department
   defers the item `inquorate`;
2. the meeting's rule over the cast positions: a majority counts cast positions, abstentions count
   toward quorum only, a tie defers, `chair` carries once quorum exists and every required
   department has stated a position;
3. the linked approval's policy: required decisions, distinct approvers, evidence minimum, P4
   conflict exclusion and dual control;
4. the stricter result wins. The meeting never lowers dual control, never waives evidence and never
   bypasses P4. An observer never counts. A participant whose own work the item judges is excluded
   by the same conflict rule.

On a deciding close, one `approval_decision` row per qualifying seat is written with
`via = {"surface":"meeting","channel":"web|telegram|mcp","meeting_id":...}` (the JSON `via` column
already exists). `meeting_position` is never a decision row.

### 4.6 Outcomes, commitments, actions and minutes

The closed set is `decision | commitment | action | information | deferred`. A decision is the
linked approval's recorded result, derived from the decision rows. A commitment names the
committing position, beneficiary, what, by when and its condition, and becomes a
`project_dependency` edge or a card; no owner or date = refused. An action becomes a card (§6.6).
An information item is typed and counted. No outcome = cannot close. Minutes are a
`governed_asset(kind='minutes')` drafted from the rows and published behind the chair's approval;
the rows remain the record, and a minute line that does not match a row is a defect.

### 4.7 The human seat, live and asynchronous

Any human, including the owner, holds an ordinary `app_user` attendance row in any seat. Every
deciding item is also an approval with a decision window, answerable from the web room, the web
UI, Telegram or the agent front end over MCP. `via` records the surface; the first decision across
surfaces counts and later ones are refused by the existing `(approval_id, approver_id)` unique
rule. No calendar system is depended on: `meeting_attendance` is the record, and an ICS feed is a
read of it. Absence is recorded as absence, never invented as an approval. A conversational room
over MCP is the agent front end's surface; the conveyor supplies the items and records the
positions, so the record does not depend on the room.

### 4.8 Metering and retirement

Participant-hours derive from attendance × duration and enter D52's roll-up as a third source
beside `inference_call` and `tool_call`, at tenant/department/programme/project/job grain. The
instruments are decision latency, agenda conversion, quorum-miss rate, commitment slippage,
action closure, person-hours per decision and decisions per meeting. A `meeting_type` whose
instances produce no decision over a cadence cycle is flagged and retired, exactly as a
never-refused gate is flagged (P10).

## 5. Deliverables

### D1 — Policy, convening and the participant extension

`meeting_type` (versioned, tenant-scoped), `meeting`, the `approval_decision` participant
extension, `meetings.schedule/open/invite/respond`, one live meeting per cadence slot, the meeting
card and its DoD.

- Given a `meeting_type`, when a second live meeting is opened in the same cadence slot, then it is
  refused `409` and the first remains.
  `Verify:` `cargo test -p conveyor-api --test meetings_policy a_second_live_meeting_in_a_slot_is_refused`
- Given a meeting, when it is opened, then it is a card with the meeting DoD and a
  `meeting.opened` event.
  `Verify:` `cargo test -p conveyor-api --test meetings_policy a_meeting_is_a_card_with_its_dod`
- Given a job-binding seat, when it decides an item, then the approval_decision row carries
  `job_binding_id` and `participant_kind='job'`; a human seat carries `approver_id` and
  `participant_kind='human'`.
  `Verify:` `cargo test -p conveyor-api --test meetings_policy a_job_seat_writes_a_participant_decision`

### D2 — The agenda is a query

L0 over `agenda_sources[]`, `meetings.agenda_add`, the four resolved object kinds, the evidence
minimum, `422 item_evidence_missing`, chair adoption.

- Given open approvals and named register kinds, when the agenda is materialised, then every item
  is a row from the L0 query and no item is selected by a model.
  `Verify:` `cargo test -p conveyor-api --test meetings_agenda the_item_set_is_l0_over_the_sources`
- Given a deciding item with no evidence ref, when it is added, then `422 item_evidence_missing`
  and no item row.
  `Verify:` `cargo test -p conveyor-api --test meetings_agenda an_item_without_evidence_is_refused`
- Given an item naming two approvals, when it is added, then it is refused: exactly one approval.
  `Verify:` `cargo test -p conveyor-api --test meetings_agenda an_item_names_exactly_one_approval`

### D3 — Attendance, positions and quorum

`meeting_attendance`, delegation policy, `meetings.position`, one live position per participant
per item, immutable after close, required departments and `inquorate`.

- Given two positions from one participant on one item, when the second is recorded, then the first
  is superseded or refused; one live row remains.
  `Verify:` `cargo test -p conveyor-api --test meetings_attendance one_live_position_per_participant`
- Given a required department with no attendance, when the item closes, then it is deferred
  `inquorate` and no decision row is written.
  `Verify:` `cargo test -p conveyor-api --test meetings_attendance a_missing_department_defers_inquorate`
- Given a closed item, when a position is edited, then the edit is refused.
  `Verify:` `cargo test -p conveyor-api --test meetings_attendance a_closed_item_position_is_immutable`

### D4 — The outcome arithmetic

`meetings.close_item`, per-seat decision rows, majority/unanimous/chair, abstentions, ties, observer
exclusion, stricter-wins against dual control/evidence/P4.

- Given three departments and a majority rule, when the item closes, then each qualifying seat is
  one decision row `via->>'surface'='meeting'` and the dissent is kept.
  `Verify:` `cargo test -p conveyor-api --test meetings_decision majority_records_each_seat_and_keeps_dissent`
- Given a linked approval with dual control, when the meeting rule would need fewer decisions, then
  the approval's stricter requirement wins and the item is refused.
  `Verify:` `cargo test -p conveyor-api --test meetings_decision a_meeting_never_lowers_dual_control`
- Given an even split, when the item closes, then it defers; an observer's position never counts.
  `Verify:` `cargo test -p conveyor-api --test meetings_decision a_tie_defers_and_observers_do_not_count`
- Given a participant whose own work the item judges, when positions are counted, then that seat is
  excluded by P4.
  `Verify:` `cargo test -p conveyor-api --test meetings_decision the_conflict_rule_excludes_the_judged_seat`

### D5 — Outcomes, commitments, actions and minutes

The closed outcome set, `meetings.defer/close`, owner+date commitments, dependency/card creation,
action cards, the `minutes` kind, `meetings.minutes_publish` behind the chair's approval, the
minute-line check.

- Given a commitment without owner or date, when it is recorded, then it is refused.
  `Verify:` `cargo test -p conveyor-api --test meetings_outcomes a_commitment_needs_owner_and_date`
- Given an action outcome, when the item closes, then a card exists and is linked from the item.
  `Verify:` `cargo test -p conveyor-api --test meetings_outcomes an_action_becomes_a_card`
- Given minutes, when publication is requested without the chair's approval, then it is refused;
  with it, the governed asset is published and every line matches a row.
  `Verify:` `cargo test -p conveyor-api --test meetings_outcomes minutes_need_the_chair_and_match_the_rows`

### D6 — The human seat across surfaces

`app_user` seats in every role, the live web room, asynchronous web/Telegram/MCP, `via` surface,
first-decision-wins, ICS read, draft-only LLM.

- Given one deciding item, when the owner decides in the web room and over MCP, then exactly one
  decision row exists and it records the first surface.
  `Verify:` `cargo test -p conveyor-api --test meetings_human_seat the_first_decision_across_surfaces_counts`
- Given the web room, when the owner opens it, then the agenda, evidence, participants, positions
  and running outcome render, and the async queue offers the same deciding items.
  `Verify:` `npx playwright test apps/web/e2e/meetings.spec.ts --grep 'owner seat'`
- Given an ICS request, when it is read, then it is derived from attendance and writes nothing.
  `Verify:` `cargo test -p conveyor-api --test meetings_human_seat an_ics_feed_is_a_read`

### D7 — Metering and retirement

Participant-hours as the third D52 source; the seven instruments; the never-decided retirement flag.

- Given attendance and duration, when the roll-up is read, then participant-hours recompute from
  rows and attribute to the programme/project/job.
  `Verify:` `cargo test -p conveyor-api --test meetings_metering participant_hours_enter_the_rollup`
- Given a forum with no decision over a cadence cycle, when metrics run, then it is flagged and
  eligible for retirement.
  `Verify:` `cargo test -p conveyor-api --test meetings_metering a_forum_with_no_decision_is_flagged`

### D8 — The console destination and the seams

`/meetings` and `/meetings/:id`, the five screens, the `meeting` card type and seed, the
`governance_body` convener, the cadence-slot agenda source.

- Given a `governance_body`, when its standing meeting opens, then the body is the convener and the
  meeting type is the standing one, with no second decision path.
  `Verify:` `cargo test -p conveyor-api --test meetings_console a_governance_body_convenes_its_standing_meeting`
- Given a `programme_cadence` slot, when it materialises, then its items resolve to a meeting
  instance.
  `Verify:` `cargo test -p conveyor-api --test meetings_console a_cadence_slot_resolves_to_a_meeting`
- Given the console, when the Meetings destination opens, then every screen renders empty, error,
  happy and one refusal state.
  `Verify:` `npx playwright test apps/web/e2e/meetings.spec.ts`

## 6. Data model

Five new tables, all tenant-scoped, `ENABLE`+`FORCE RLS`, with `created_at`/`updated_at`/`version`:

| Table | Columns and constraints |
|---|---|
| `meeting_type` | `key`, `name`, `purpose`, `agenda_sources jsonb`, `required_participation jsonb`, `quorum int`, `decision_rule text CHECK (unanimous\|majority\|chair)`, `evidence_minimum int`, `decision_window interval`, `timebox interval`, `allowed_outcomes text[]`, `status`, `version`; unique `(tenant_id, key, version)`; a version is immutable once a meeting pins it |
| `meeting` | `card_id`, `convener_kind CHECK (governance_body\|job_binding\|app_user)`, `convener_id`, `meeting_type_id`, `meeting_type_version`, `opens_at`, `decision_window_end`, `cadence_slot`; partial unique `(tenant_id, meeting_type_id, cadence_slot) WHERE status='live'` |
| `meeting_agenda_item` | `meeting_id`, `sequence`, `resolved_kind CHECK (approval\|register_item\|asset\|cadence)`, `resolved_id`, `participating_departments uuid[]`, `evidence_refs jsonb`, `outcome text CHECK (decision\|commitment\|action\|information\|deferred)`, `defer_reason`; unique `(meeting_id, sequence)` |
| `meeting_attendance` | `meeting_id`, `participant_kind CHECK (job_binding\|app_user)`, `participant_id`, `seat CHECK (chair\|member\|presenter\|observer\|delegate)`, `department_id`, `invitation_state`, `response_state`, `delegated_from`; unique `(meeting_id, participant_kind, participant_id)` |
| `meeting_position` | `meeting_id`, `agenda_item_id`, `participant_kind`, `participant_id`, `stance CHECK (approve\|approve_with_conditions\|reject\|abstain\|request_evidence)`, `rationale`, `conditions jsonb`, `commitment jsonb`, `surface CHECK (web\|telegram\|mcp)`, `recorded_at`; partial unique `(agenda_item_id, participant_kind, participant_id) WHERE superseded_at IS NULL`; immutable once the item closes |

Two alterations, both additive:

1. `approval_decision`: nullable `approver_id`; add `job_binding_id uuid REFERENCES job_binding(id)`
   and `participant_kind text NOT NULL DEFAULT 'human' CHECK (participant_kind IN ('human','job'))`;
   CHECK exactly one participant; partial unique `(approval_id, job_binding_id) WHERE
   job_binding_id IS NOT NULL`; existing rows keep `participant_kind='human'`.
2. `governed_asset_kind_check`: widen with `minutes`.

`approval_decision.via` is already `jsonb`; a meeting decision writes
`{"surface":"meeting","channel":"web|telegram|mcp","meeting_id":"<uuid>"}`. No other V1 table is
altered. `card.job_binding_id` exists (T0); `project_dependency` (T1), `governance_body` (T9) and
`programme_cadence` (T10) are reused.

## 7. Actions and events

Every action is an `ActionSpec` with one declaration → HTTP + MCP + OpenAPI + generated client.
The family is `meetings.<noun>_<verb>` because the registry requires exactly one dot:

| Action | Effect | Notes |
|---|---|---|
| `meetings.schedule` | write | creates the meeting card + `meeting` in `scheduled`; policy-version pinned |
| `meetings.open` | write | `scheduled → live`; emits `meeting.opened` in one transaction |
| `meetings.invite` | write | creates/updates attendance; authority-checked |
| `meetings.respond` | write | invitation response; never invents a position |
| `meetings.agenda_add` | write | explicit item; evidence minimum; exactly one approval |
| `meetings.position` | write | one live position per participant/item; draft-only LLM cannot call it |
| `meetings.defer` | write | deferred outcome with a reason (`inquorate` included) |
| `meetings.close_item` | write | writes per-seat `approval_decision` rows; derives the outcome; emits `meeting_item.closed` |
| `meetings.close` | write | closes the meeting; refuses while an item has no outcome |
| `meetings.minutes_publish` | write | publishes the governed asset behind the chair's approval; emits `meeting_minutes.published` |
| `meetings.list` / `meetings.get` | read | invitations, room, history, instruments |

Events are one subject and one verb; the writing action and its event commit in one transaction.
`architecture.board_open|get|decide` remain T9's macros over these actions and add no decision path.

## 8. The programme

T19 is a programme project at tranche 4. The MSP record is `docs/plans/T19/msp-profile.md`: benefit
**B7 — Deliberated cross-department decisions**, the benefits-map edge, risk **R20**, the
stakeholder profiles and the tranche-4 evidence. The TOGAF record is
`docs/plans/T19/architecture-design.md`: the PSA set (principles, requirements, stakeholders,
drivers/goals, solution concept) and the HLD set (business, data, application and technology views,
migration), each artefact with the standard's common metadata and links; the Statement of
Architecture Work and the compliance approach close it. The SAFe record is
`docs/plans/T19/safe-epic.md`: the epic hypothesis, features F1–F8, enablers E1–E6, WSJF inputs and
the PI objectives for tranche 4. `docs/plans/T19/artifact-register.md` is the typed catalogue of
those artefacts. The programme doc's "decision forums" subsection names each framework's
procedures as meeting types (programme board, tranche review, Architecture/Design Board, PI
planning, system demo, inspect & adapt, incident review).

## 9. Dependencies

| Relation | Issue | Source |
|---|---|---|
| blocked-by | #72 (T1) | `seed/programme.json` T19 `meta.depends_on`; plan spine, registers, dependencies |
| blocked-by | #73 (T2) | the L3 ladder and policy evaluation the approval arithmetic sits on |
| blocked-by | #74 (T3) | the PM's gate routing and the card/closure path |
| blocked-by | #120 (P0) | every delivery slice is P0-gated; the brief and inception standard |
| related | #81 (T9) | built; `governance_body` is the convener seam, not a rebuild |
| related | #82 (T10) | built; `programme_cadence` is the cadence-slot seam, not a rebuild |
| related | #562–#566 | the T18 UI lane; no ordering, different files |
| blocks | #606 | the T19 implementation issue carries D1–D8 |

T9 and T10 are measured `built`. Their board/cadence **binding** to T19's meeting types is a
follow-up integration unit; it is not a `blocked-by` on a built slice and does not freeze T19.

## 10. Acceptance and definition of done

Every D-scenario passes at the PR head with a row in `Evidence at completion`; the repo gate is
green at the exact head (`rbuild --gate`); migrations and the ledger pass `--check-lock`; the
console check and the design/docs/coverage gates pass; one PR `Closes #606` merged. The five new
tables and two alterations are in the migration ledger; `check-coverage` reports T19 `built` only
when every declared table, action and module exists. The live gate is the owner deciding one item
in the web room and one over MCP, both `via->>'surface'='meeting'`.

## 11. Not claimed

No chat or transcript store, no AV integration, no calendar dependency, no LLM that selects,
decides, infers a position or marks attendance, no second decision store, no rebuild of the
approval primitive beyond the participant extension, no ArchiMate rendering of minutes, no
full-suite replacement of T9's board or T10's cadence.

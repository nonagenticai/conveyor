---
slice: T19F
title: Decision-forum governance — policy authoring and the T9/T10 binding
status: built
depends_on: [T19, T17, T9, T10, P0]
spec_sections: ["§6.8", "§8.1", "§8.2", "§8.3", "§12 T19", "§12 T19F", "D66"]
adds_tables: []
adds_actions: [meetings.type_list, meetings.type_get, architecture.board_sit_open, architecture.board_sit_decide, cadences.materialise]
adds_events: []
adds_modules: []
parity_ids: []
---

# T19F — Decision-forum governance (follow-on)

Programme project: T19F · plan issue #682 · implementation #683 · proof #684

**Gate:** a policy change is authored through the tenant's authoring route and published as a new
immutable `meeting_type` version while an in-flight meeting keeps its pinned version; a board
subject decision is taken in a board sitting, with one `approval_decision` row per qualifying seat
carrying `via='meeting'`; a cadence row names its meeting type and materialises exactly one
meeting; the direct `architecture.board_decide` path is unchanged; the T19F project is a live
programme row with its MSP/TOGAF/SAFe artefact set and a closure review that defers closure to the
tranche-6 review.

## 1. Objective

T19 made the meeting the decision forum, but left two seams open. A forum's policy could only be
created implicitly by a schedule and never changed: the engine answers a policy change with "a
policy change is a new version through the governance route", and that route did not exist. The
T9 `governance_body` and T10 `programme_cadence` only resolved a meeting by convention — a body's
key doubled as its standing type, and a cadence slot was passed by hand — so the board's decisions
and the cadence's sittings were not bound to the forum records they belong to.

T19F closes both seams without rebuilding either consumer. It adds the versioned authoring route
through T17, an explicit meeting-type binding on the body and the cadence row, a board-sitting
entry that uses T19's own outcome arithmetic, and a cadence materialisation that is idempotent.

## 2. Starting state (measured at `main@2debb226`, 2026-11-24)

| Part | State | Evidence |
|---|---|---|
| Policy versioning | PARTIAL | `migrations/0065_meetings.sql`: `meeting_type` is versioned and immutable; `crates/store/src/meetings.rs:schedule_tx` creates v1 and refuses a change ("a policy change is a new version through the governance route") |
| Policy authoring route | ABSENT | `git grep -n 'name = "meetings.type' origin/main -- crates` → 0; `authoring.publish` writes no `meeting_type` row |
| Board binding | ABSENT | `migrations/0054_architecture.sql` `governance_body` has no meeting-type column; `crates/store/src/architecture.rs:decide_board_tx` writes one approval row directly |
| Cadence binding | ABSENT | `migrations/0044_plan_spine.sql` `programme_cadence` has only `meta`; no action materialises a meeting from a cadence row |
| Meeting arithmetic | DONE, reused | `crates/store/src/meetings.rs:close_item_tx`; D4's 26-scenario evidence in `docs/plans/T19/evidence.md` |
| Authoring route (T17) | DONE, reused | `authoring.intake/draft/review/publish/assets`; `kind='policy'`; one published row per `(tenant, kind, key)` |
| Mock UI / real app | PARTIAL | `docs/design/v2.js` `meetingsPage`; `apps/web/src/screens/meetings.rs`; no policy/sitting/cadence panel |
| Programme row | ABSENT | no `T19F` in `seed/programme.json`, coverage or the programme doc |

## 3. Scope

**IN** — the versioned `meeting_type` authoring route through T17; `meetings.type_list/get`; the
explicit meeting-type binding on `governance_body` and `programme_cadence`; the board-sitting open
and decide actions; the cadence materialisation; the mock-UI and real-app panels; the T19F plan,
MSP/TOGAF/SAFe artefact set, programme registration and live proof.

**OUT** — a second decision store; a chat/AV/transcript store; a rebuild of T9's board, T10's
cadence or the approval primitive; a change to the direct `board_decide` path; the T18 shell/nav;
the homepage mirror; the tranche 2→4 authorisation chain and #526's ledger; programmatic LLM
selection of agenda items, positions or attendance.

## 4. The model

### 4.1 Policy authoring is a versioned release, not an edit

`meeting_type` rows stay immutable. A change is a new authoring asset of `kind='policy'` whose
payload carries the policy, reviewed by a human through `authoring.review` (an approved
`approval_id`) and published by `authoring.publish`. Publication writes the next `max(version)+1`
row for the policy key in the same transaction and records the output link on the asset. The
authoring asset key and the meeting-type key are different: a new version needs a new asset key
carrying the same `meeting_type_key`, because T17 keeps one published row per `(tenant, kind, key)`.
A meeting pins `meeting_type_id` and `meeting_type_version` at schedule time; it never follows a
later version.

### 4.2 The board sits through the meeting arithmetic

`governance_body.meeting_type_key` names the body's standing type explicitly (defaulting to the
body key for existing rows). `architecture.board_sit_open` validates the body and the subject,
opens the board approval and creates or reuses the live sitting with the body's type, then adds the
approval as the deciding agenda item. Seats record positions; `architecture.board_sit_decide` calls
T19's `close_item_tx`, so the quorum, the required departments, the meeting rule, the linked
approval's stricter policy, P4 exclusion and the `via='meeting'` decision rows all come from the
one arithmetic T19 already proves. A quorum miss defers `inquorate`; it is not an error. The direct
`architecture.board_decide` writes its approval row as before; the sitting is the bound path.

### 4.3 A cadence row materialises its own meeting

`programme_cadence.meeting_type_key` names the forum type for PI planning, the demo, the retro or a
refinement. `cadences.materialise` resolves the latest non-retired version, schedules the meeting
with the slot, and returns the existing non-cancelled instance on a repeat. A partial unique index
on `(tenant_id, cadence_slot)` for non-cancelled meetings makes "one cadence row, one meeting" the
database's promise; a cancelled instance may be replaced.

### 4.4 What does not change

The direct board path, the approval primitive, T19's outcome arithmetic, the meeting DoD, the
minutes gate, the metering roll-up and the never-decided retirement flag are untouched. The tenancy,
RLS and audit rules are the existing rows'.

## 5. Deliverables

### P1 — The policy authoring route

Migration `0073_meeting_bindings.sql`; the `authoring.publish` hook; `meetings.type_list/get`.

- Given a live meeting pinned to a policy's v1, when a reviewed `kind=policy` asset for the same key
  is published, then a v2 row exists and the meeting still resolves v1.
  `Verify:` `cargo test -p conveyor-api --test meetings_authoring policy_publish_versions_meeting_type`
- Given an unreviewed policy asset, when it is published, then the publish is refused.
  `Verify:` `cargo test -p conveyor-api --test meetings_authoring policy_publish_requires_review`
- Given an already-published asset key, when another version is drafted under it, then the route
  refuses and a new asset key is required.
  `Verify:` `cargo test -p conveyor-api --test meetings_authoring a_reused_asset_key_is_refused`
- Given the reads, when list/get run by key, version or status, then the versions return in order.
  `Verify:` `cargo test -p conveyor-api --test meetings_authoring type_list_and_get_read_versions`

### P2 — The board-sitting binding

`governance_body.meeting_type_key`; the `board_open` input; `architecture.board_sit_open/decide`.

- Given a body with a bound type and a subject, when the sitting opens, the seats record positions
  and the chair decides, then per-seat `approval_decision` rows exist with `via='meeting'` and the
  approval status matches the derived rule.
  `Verify:` `cargo test -p conveyor-api --test meetings_binding board_sitting_uses_close_item`
- Given a body with no bound type, when a sitting is opened, then it is refused `422`.
  `Verify:` `cargo test -p conveyor-api --test meetings_binding a_body_without_a_type_refuses_a_sitting`
- Given a missing required department, when the chair decides, then the item defers `inquorate`.
  `Verify:` `cargo test -p conveyor-api --test meetings_binding a_quorum_miss_defers_inquorate`
- Given a body without a sitting, when `board_decide` is called, then the direct path still records.
  `Verify:` `cargo test -p conveyor-api --test meetings_binding the_direct_board_decide_path_still_works`

### P3 — The cadence materialisation

`programme_cadence.meeting_type_key`; the one-instance index; `cadences.materialise`.

- Given a cadence row with a type, when materialise is called twice, then one non-cancelled meeting
  exists with the bound type and the slot.
  `Verify:` `cargo test -p conveyor-api --test meetings_binding cadence_materialise_is_idempotent`
- Given a cadence without a type, when materialise runs, then it is refused `422 cadence_type_missing`.
  `Verify:` `cargo test -p conveyor-api --test meetings_binding a_cadence_without_a_type_is_refused`
- Given a materialised meeting, when the agenda is read, then the `cadence` item resolves the slot.
  `Verify:` `cargo test -p conveyor-api --test meetings_binding the_cadence_item_resolves`

### P4 — The surfaces

The mock UI's Meetings page carries the policy, sitting and cadence panels; the real app's Meetings
screen shows the pinned type/version and the policy list; the Playwright spec covers the states.

- Given the mock pack, when `check-v2.cjs` runs, then the Meetings page carries the panels.
  `Verify:` `node docs/design/scripts/check-v2.cjs`
- Given the real app, when the Meetings destination renders, then the new surfaces show empty,
  happy and one refusal state.
  `Verify:` `npx playwright test apps/web/e2e/meetings.spec.ts`

### P5 — The programme record and the live proof

The T19F project row, the MSP/TOGAF/SAFe artefact set, the design-pack registration, the live
governance chain, the live gate and the strict programme publish.

- Given the seeded tenant, when `migrate --seed` and `programmes.refresh` run, then T19F, its board
  and the B7 link are live rows.
  `Verify:` the read-back under `docs/evidence/t19f/`
- Given the deployed engine, when the chain and gate checks run, then the seven artefacts are
  approved, the contracts live, the policy/sitting/cadence proofs pass.
  `Verify:` `python3 scripts/governance-chain.py --project T19F --check` and
  `python3 scripts/t19f-live-gate-check.py --check`
- Given the live snapshot, when the strict publish runs, then no new T19F P0/P1 exists and the
  pack carries T19F.
  `Verify:` `CONFORMANCE_STRICT=1 docs/design/scripts/publish-data.sh`

## 6. Data model

`migrations/0073_meeting_bindings.sql`, additive only:

| Object | Change |
|---|---|
| `authoring_asset` | `published_meeting_type_id uuid NULL REFERENCES meeting_type(id)`, `CHECK (published_meeting_type_id IS NULL OR kind = 'policy')`, partial unique index on the column |
| `governance_body` | `meeting_type_key text NULL`, `CHECK (IS NULL OR btrim(...) <> '')` |
| `programme_cadence` | `meeting_type_key text NULL`, same blank check |
| `meeting` | `CREATE UNIQUE INDEX uq_meeting_cadence_instance ON meeting (tenant_id, cadence_slot) WHERE cadence_slot IS NOT NULL AND status <> 'cancelled'` |

No new table. No FK on a versioned key: the actions resolve the latest non-retired version.

## 7. Actions and events

| Action | Effect | Notes |
|---|---|---|
| `meetings.type_list` | read | key/status filter, version desc |
| `meetings.type_get` | read | id, or key + optional version |
| `architecture.board_sit_open` | write | approval + sitting + agenda item, idempotent |
| `architecture.board_sit_decide` | write | calls `close_item_tx`; returns the item outcome and approval id |
| `cadences.materialise` | write | idempotent per cadence slot |
| `authoring.publish` | write (changed) | a `kind=policy` payload writes the next `meeting_type` version |

No new event: the authoring publish keeps T17's event surface, and the meeting events stay
T19's.

## 8. The programme

- **MSP** — `docs/plans/T19F/msp-profile.md`: the B7 realisation increment ("Deliberated
  cross-department decisions"), the benefit map link, the risk, the stakeholders and the tranche-6
  evidence. No new benefit profile; B7's measured baseline is read, never overwritten.
- **TOGAF** — `docs/plans/T19F/architecture-design.md`: the PSA set (principles, requirements
  R1–R8, stakeholders, drivers/goals, solution concept) and the HLD set (business, data,
  application and technology views, migration), the seven mandatory artefact types and their
  links, closing with the Statement of Architecture Work and the compliance approach.
- **SAFe** — `docs/plans/T19F/safe-epic.md`: the epic hypothesis, features F1–F4, enablers E1–E3,
  WSJF inputs and the PI objectives.
- `docs/plans/T19F/artifact-register.md` is the typed catalogue. The programme doc's
  "decision forums" subsection names this slice's follow-on and the tranche-6 placement.

## 9. Dependencies

| Relation | Issue | Source |
|---|---|---|
| blocked-by | #682 | this plan and its artefacts |
| related | #606, #650 | T19 implementation and close-out; the feature and its declared follow-ups |
| related | #655 | `authoring.intakes`; file-adjacent, no ordering |
| related | #565, #566 | the T18 real UI; the Meetings screen is screen-local |
| related | #526 | the tranche 2→4 retrospective chain; T19F stays active |

T19 and T17 are built; their dependency is provenance, not a frozen construction order. T19F is a
follow-on at tranche 6 by the coverage graph.

## 10. Acceptance and definition of done

Every P-scenario passes at the PR head with a row in `docs/plans/T19F/evidence.md`; the repo gate is
green at the exact head (`preflight.yml` for the compile verdict, the PR's `ci-public`); the
migration ledger reaches 68; the action registry, parity and OpenAPI lock agree; the coverage and
design-pack checks pass; one PR per issue; the live gate runs on the deployed engine; the strict
publish is green with no new T19F finding. T19F remains `active`; the closure review records
acceptance met and defers closure to the tranche-6 review.

## 11. Not claimed

No chat or transcript store, no AV integration, no calendar dependency, no second decision store,
no LLM that selects, decides or marks attendance, no rebuild of T9's board or T10's cadence, no
change to the direct board path, no T18 shell work.

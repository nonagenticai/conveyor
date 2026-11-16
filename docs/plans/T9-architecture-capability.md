---
slice: T9
title: TOGAF ADM capability — artefacts, phases, contracts and governance
status: planned
depends_on: [T1, T3, P0]
spec_sections: ["§8.1", "§8.5", "§10.3", "§10.4", "§10.5", "§12 T9", "D63"]
adds_tables: [artifact_type, architecture_artifact, architecture_artifact_entry, architecture_artifact_link, adm_profile, architecture_contract, architecture_compliance_review, architecture_waiver, governance_body, architecture_maturity, architecture_skill_assessment]
adds_actions: [architecture.artifacts.list, architecture.artifacts.get, architecture.artifacts.create, architecture.artifacts.update, architecture.artifacts.approve, architecture.artifacts.supersede, architecture.artifacts.link, architecture.repository.search, architecture.contracts.request, architecture.contracts.approve, architecture.contracts.supersede, architecture.phases.advance, architecture.reviews.record, architecture.reviews.waive, architecture.board.decide, architecture.maturity.assess, architecture.maturity.plan, architecture.skills.assess, architecture.profile.get, architecture.profile.tailor]
adds_events: [architecture_artifact.changed, architecture_phase.closed, compliance_review.recorded, architecture_waiver.granted]
adds_modules: [architecture]
parity_ids: []
---

# T9 — TOGAF ADM capability

Programme project: T9 · issue #81

**Gate (SPEC_V2 §12 T9):** an architecture project passes Preliminary and A–F with each phase's
mandatory artefacts approved and its contract live; a phase missing a mandatory artefact is
refused; a delivery project without a live contract is refused at design; a compliance review
records one level and a waiver names its owner and review date; an H review produces exactly one
change or kata.

## 1. Objective

The conveyor runs an architecture capability, not a document library. The Preliminary phase
establishes the capability — principles, the tailored ADM profile, the artefact standard and
repository, the Architecture Board, governance and contracts, maturity and skills. An
`architecture` project then walks phases A–H as WBS nodes, each phase carrying the mandatory
artefacts its profile names and closing on an approved `architecture_contract`. Every artefact is
a typed row — catalog, matrix or diagram — with the standard's common metadata, queryable entries
and trace links, so the repository answers what it derives from, realises and reuses. The conveyor
buildout itself runs under this capability: P0 is the Preliminary-phase and programme-definition
project; T1–T17 are the delivery projects the phase-G reviews govern.

## 2. Starting state (measured at `main@48fc44c` on 2026-11-16; re-measured at claim)

| Part | State | Evidence |
|---|---|---|
| Artefact model | ABSENT | `git grep -n 'architecture_artifact\|artifact_type' -- crates migrations` → 0 hits |
| ADM phases and contracts | ABSENT | `git grep -n 'architecture_contract\|adm_profile' -- crates migrations` → 0 hits; `governed_asset.kind` is `requirement\|principle\|adr` (`migrations/0002_domain.sql:256`) |
| Delivery design gate | ABSENT | `git grep -n 'design_gate' -- crates migrations` → 0 hits |
| Compliance and waivers | ABSENT | no review or waiver table exists; no compliance level is stored |
| Capability assessment | ABSENT | no maturity or skills row exists |
| Phase H outcome | ABSENT | `project_register_item` allows `resource\|risk\|benefit` only (`migrations/0008_s08_portfolio.sql:112`); no kata table |

Reused, not rebuilt: `governed_asset` (requirements, principles, ADRs and their supersession),
`trace_link` (both directions, unique edge), `catalogue_entry` (the project-type × card-type matrix
and the seeded phase catalogue), `process_step.kind='gate'` (parks a step until an approval
exists), the approval primitive (`approval`, `approval_decision`, dual control), `document` +
MinIO for artefact documents, `wbs_node` for phase nodes (T1), and the PM's gate routing (T3).

## 3. Scope

**IN** — the eleven tables of SPEC_V2 §10.3; the `architecture.*` action family; the four contract
kinds; the six compliance levels; the waiver register; the maturity and skills assessments; the
Architecture Board as a governance body; the `architecture` project type with Preliminary and
A–H as WBS nodes; the delivery design gate and the phase-G review; the Architecture destination in
the console; the seed's T9 gate and the T9/P0 expected-output categories.

**OUT** — an ArchiMate editor or live metamodel (a diagram is a document plus typed metadata);
rendering catalogues/matrices as live models; full TOGAF conformance; a portfolio optimiser; the
T1 plan rows and T3 PM job (consumed, never rebuilt); the T10 tranche reviews and benefits
realization (they consume the architecture artefacts, not the reverse).

**UI surface declared for the console**: the **Architecture** destination with four tabs —
Capability (Preliminary), ADM phases, Artefact repository, Board & compliance — plus the artefact
inspector (metadata, entries, links, document) and the phase-advance refusal.

## 4. The model

### 4.1 The artefact standard

`architecture_artifact` is the parent. Its `type` is `catalog`, `matrix` or `diagram`; all three
inherit the common metadata: title, status, version, author and date, reviewer and review date, the
related change initiative or project, purpose/stakeholders/scope, and the published document or
contract. Type-specific metadata: a catalog names its unique identifier, item name and description
of use or structure; a matrix names its axes (usually two); a diagram names its building
block/metamodel and viewpoint.

Entries are rows (`architecture_artifact_entry`: `axis_a`, `axis_b`, `value`). A catalog uses one
axis; a matrix uses two. A diagram's content is a `document`; the row carries the metadata and the
viewpoint, not the pixels.

Each artefact names its `phase_code` (Preliminary, A–H) and its `plateau` — `psa` (Project Start
Architecture), `hld` (High-Level Design) or `sd` (Solution Design). The profile decides which
plateau a phase requires. `architecture_artifact_link` carries `derivation`, `realization`, `reuse`
and `dependency` edges between artefacts; `architecture.repository.search` answers reuse across
projects, phases and plateaus.

### 4.2 The capability (Preliminary phase)

`adm_profile` holds the tenant's tailoring: the phase set, the mandatory artefact types per phase
and plateau, and the contract kind that closes each phase. `artifact_type` is the seeded standard
(§4.1's catalogue with phase, plateau, type and mandatory flag); the profile filters it, so a young
capability carries fewer artefacts and the set grows with the assessed maturity.

`governance_body(kind='architecture_board')` holds the board's membership, charter, commissioning
authority and decision limits; `architecture.board.decide` records a board decision with its
evidence. The principles catalog is an ordinary artefact (`type='principles_catalog'`) whose entries
carry statement, rationale and implications. `architecture_maturity` scores nine dimensions on 0–5
per stage with a baseline and milestones; `architecture_skill_assessment` scores the seven skills
categories per role.

### 4.3 The phases and the contracts

An `architecture` project materialises Preliminary and A–H as `wbs_node(kind='phase')` rows.
`architecture.phases.advance` refuses unless every artefact the profile marks mandatory for that
phase and plateau is `approved`, every artefact link the standard requires resolves, and the phase's
contract is `live`; the refusal names the missing artefact or the contract's state, and writes no
contract row.

`architecture_contract` carries four kinds across the ADM: `statement_of_architecture_work`
(Phase A — assignment, scope, responsibilities, acceptance criteria; distinct from the designs
produced under it), `delivery_contract` (after F — delivery commitments and the method for assessing
compliance), `statement_of_intent` (at completion — intended use, ownership, measures of
effectiveness) and `request_for_architecture` (Phase H — further architecture work). A contract
change is a new version and re-opens its gate; the approved body is immutable.

### 4.4 Delivery compliance and change management

`architecture_compliance_review` records exactly one level per delivery project —
`irrelevant`, `consistent`, `compliant`, `conformant`, `fully_conformant` or `non_conformant` —
with findings and evidence refs. A deviation is rejected, mitigated, or accepted through
`architecture_waiver` for a defined period; the waiver names the accepting asset owner, the reason,
the additional cost and risk, and the reassessment date, and is refused without an owner and a date.

A Phase-H review records exactly one typed outcome: a `project_register_item(kind='change')` with
its impact block and the change gate, or a kata candidate routed to the kata lane. A review with no
recorded verdict cannot close the phase. A `request_for_architecture` is the third artifact of a
change review — it opens further architecture work when the current architecture no longer meets its
purpose.

The delivery design gate resolves only against a live approved contract trace-linked to the delivery
project; absent, draft or superseded all refuse, and no approval row is written.

## 5. Deliverables

### D1 — The artefact standard and repository

`artifact_type` seeded; `architecture_artifact`, `architecture_artifact_entry`,
`architecture_artifact_link`; `architecture.artifacts.*` and `architecture.repository.search`;
approve/supersede immutability; catalog and matrix entries queryable; diagram metadata with
viewpoint.

- Given a catalog with ten entries, when it is queried, then the entries are rows and the
  artefact's metadata names its owner, reviewer, version and document.
  `Verify:` `cargo test -p conveyor-api --test architecture_artifacts a_catalog_is_queryable_rows`
- Given an approved artefact, when it is edited, then the edit is refused and a new version
  supersedes it; the approved body is unchanged.
  `Verify:` `crates/api/tests/architecture_artifacts.rs::an_approved_artefact_is_immutable`
- Given two artefacts, when a `reuse` link is recorded, then the repository search returns the
  reused artefact for the solution artefact across plateaus.
  `Verify:` `crates/api/tests/architecture_artifacts.rs::reuse_is_searchable_across_plateaus`

### D2 — The Preliminary capability

Principles catalog; `adm_profile`; `artifact_type` mandatory sets; `governance_body` (the
Architecture Board); maturity and skills assessments; the profile's tailoring.

- Given a profile that marks the IS contract's mandatory artefacts, when the profile is read, then
  the phase's mandatory set is returned filtered by plateau and the board charter is a governance
  body row.
  `Verify:` `cargo test -p conveyor-api --test architecture_capability the_profile_names_the_mandatory_set`
- Given a maturity assessment over nine dimensions, when it is read, then every dimension has a
  0–5 score, a stage and a date; a missing dimension is refused.
  `Verify:` `crates/api/tests/architecture_capability.rs::a_maturity_assessment_needs_every_dimension`
- Given a skills assessment, when a role's proficiency is recorded, then it names the seven
  categories and the job key.
  `Verify:` `crates/api/tests/architecture_capability.rs::a_skills_assessment_names_its_role`

### D3 — The architecture project and its phase gates

The `architecture` project type; Preliminary and A–H as WBS nodes; the four contract kinds; phase
advance refuses on a missing mandatory artefact or a non-live contract.

- Given an approved artefact set and a live SAW, when phase A is advanced, then the phase closes
  and B is open.
  `Verify:` `cargo test -p conveyor-api --test togaf_architecture_phases phases_a_to_f_pass_in_order`
- Given one mandatory artefact still draft, when the phase is advanced, then it is refused naming
  that artefact and no contract row is written.
  `Verify:` `crates/api/tests/togaf_architecture_phases.rs::a_draft_artefact_refuses_the_contract`
- Given an approved contract, when its content changes, then a new version supersedes it and the
  phase gate re-opens.
  `Verify:` `crates/api/tests/togaf_architecture_phases.rs::a_contract_change_reopens_the_gate`

### D4 — Delivery compliance

The design gate against a live contract; the phase-G review; the six levels; the waiver register.

- Given a delivery project with no governing contract, when its design gate is requested, then it
  is refused and no approval row is written.
  `Verify:` `cargo test -p conveyor-api --test togaf_delivery_design_gate`
- Given a superseded contract, when design is requested, then it is refused naming its status.
  `Verify:` `crates/api/tests/togaf_delivery_design_gate.rs::a_superseded_contract_refuses_design`
- Given a review, when one compliance level is recorded, then a second level on the same review is
  refused; a waiver without an owner or a review date is refused.
  `Verify:` `crates/api/tests/architecture_compliance.rs::one_level_and_a_dated_waiver`

### D5 — Change management

Phase H's exactly-one outcome; the change gate; the kata candidate; the Request for Architecture
Work.

- Given a review finding a scope change, when the verdict is recorded, then a
  `project_register_item(kind='change')` with impact exists and is routed to the change gate.
  `Verify:` `cargo test -p conveyor-api --test togaf_h_change`
- Given a systemic finding, when it is recorded, then the verdict is a kata candidate and no change
  register row is written.
  `Verify:` `crates/api/tests/togaf_h_change.rs::a_systemic_finding_produces_a_kata_candidate`
- Given a review with no verdict, when the phase-H gate is requested, then it is refused.
  `Verify:` `crates/api/tests/togaf_h_change.rs::an_unrecorded_review_refuses_the_gate`

### D6 — The console surface and the seed

The Architecture destination (four tabs); the artefact inspector; the phase-advance refusal; the
seed's T9 gate and the T9/P0 expected-output categories.

- Given the console, when the Architecture destination opens, then the four tabs render and each
  interaction changes its recorded state.
  `Verify:` `node docs/design/scripts/check-v2.cjs`
- Given the seed, when it is regenerated, then T9's gate text is the §12 row's.
  `Verify:` `python3 scripts/build-programme-seed.py --check`
  (The architecture expected-output categories are #221's; this unit consumes them once they land.)

## 6. Data model

The eleven tables are in SPEC_V2 §10.3. New columns only where unavoidable:
`project.project_type_code` already exists; `governed_asset.kind` is extended where a definition
artefact is generic (charter, closure, mandate, brief, blueprint, business case, refined vision,
benefits strategy, realization plan, projects dossier), never for an architecture artefact or
contract, which have their own tables. All tables are tenant-scoped and `ENABLE`+`FORCE RLS`.

## 7. Actions and events

The `architecture.*` family is declared in SPEC_V2 §10.4. Every action is an `ActionSpec` with one
declaration → HTTP + MCP + OpenAPI + generated client. The events are one subject and one verb
(`architecture_artifact.changed`, `architecture_phase.closed`, `compliance_review.recorded`,
`architecture_waiver.granted`). A phase advance and its contract approval commit in one
transaction with their event.

## 8. The programme

`seed/programme.json` T9 carries the §12 gate text; its expected outputs come from the output
standard. P0's class carries the definition categories; T9's class carries the architecture
categories. The seed is generated (`scripts/build-programme-seed.py`), never hand-edited, and
`--check` is the gate.

## 9. Dependencies

| Relation | Issue | Source |
|---|---|---|
| blocked-by | #72 | `seed/programme.json` T9 `meta.depends_on` = `["T1","T3","P0"]`; the phase nodes, the baseline and the register are T1's |
| blocked-by | #74 | same; the phase contract gates run on the PM's gate routing and closure |
| blocked-by | #122 | the same `P0` edge at issue level: the architecture capability baseline (P0 §3.14) is the Preliminary input the profile seeds |
| blocked-by | #121 | the same `P0` edge at issue level: the blueprint (P0 §3.2) is the target architecture the artefacts elaborate. The P0 project and gate (#120) is closed; the definition units are the open blockers |
| related | #121, #122 | P0's definition set and its architecture capability baseline |
| related | #216 | the MSP layer this capability sits beside; the design study's programme destination |
| related | #221 | the output-standard categories and the seed's architecture rows |
| blocks | #88 | `seed/programme.json` T12 `meta.depends_on` includes `T9` |

## 10. Acceptance and definition of done

Every D-scenario passes at the PR head with a row in `Evidence at completion`; the repo gate is
green at the exact head (`rbuild --gate`); migrations and the ledger pass `--check-lock`; the
console check and the design/docs/coverage gates pass; one PR `Closes #81` merged.

## 11. Not claimed

No TOGAF certification and no full ADM conformance; no ArchiMate editor or live metamodel; no
renderer that derives a diagram from the model; no automatic conformance verdict (a review is a
person's recorded decision with evidence); no portfolio optimiser or schedule solver.

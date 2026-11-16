---
slice: P0
title: Programme definition — mandate, brief, blueprint, business case, benefits, tranches, governance
status: planned
depends_on: []
spec_sections: ["§1", "§4", "§5", "§8.3", "§12", "MSP"]
adds_tables: []
adds_actions: []
adds_events: []
adds_modules: []
parity_ids: []
kind: programme-definition
---

# P0 — Programme definition

Programme project: P0 · issues #120, #121, #122

**Track:** programme · **Capabilities:** architecture, programme management, project management,
business analysis · **Gate:** the definition set is published and adopted; no V2 delivery project
starts before it.

## 1. Objective

Design the programme that delivers the full conveyor, following MSP's *Defining a Programme* and
*Realizing the Benefits*: the mandate, programme brief, blueprint, business case, benefit profiles
and benefits map, projects dossier, programme plan and tranches, governance, stakeholder profiles,
benefits management strategy and realization plan, reuse map and project briefs. P0 is a project of
the programme it defines, so every artefact in this set is an intermediate output stored on P0 in
the console and published on the design site.

The programme does not start T1–T17 until this set is accepted. The gate is the programme's, not a
document's: a delivery project's brief is the contract it is held to, and a benefit's profile is
the claim its transition is measured against.

## 2. Starting state (measured at the P0 baseline)

- V1 is far implemented: 25 measured-built slices, 2 partial (S14, S21); the Rust workspace, the
  action registry, the board/card/run/approval/document aggregates and the public design pack.
- T0's organisation spine merged (#65, migration 0040, `organisation.*` actions, release hash and
  binding pin); its measured coverage row is corrected by this unit's seed change.
- The programme seed has 47 projects and no programme-definition project. T1–T17 are proposed and
  blocked by their SPEC_V2 §12 edges.
- `docs/SPEC_V2.md` is the design record; `docs/SPEC.md` is the V1 build record. Neither is a
  programme plan; P0 produces the plan that adopts them.

## 3. The definition set (the project's outputs)

1. **Mandate** — strategic fit, current position, drivers, deliverables and boundaries, delivery
   and assurance, success and constraints. Approved by the sponsoring group.
2. **Programme brief** — outline vision, benefit categories, cost/timescale/effort, risks and
   stakeholder support, delivery options including doing nothing, candidate projects and timing,
   and an assessment of current operations and performance.
3. **Blueprint** — the target architecture and operating model: aggregates, services, the action
   registry, the data model, the deployment estate. The to-be the projects deliver.
4. **Business case** — why now, the benefit map, the options considered including doing nothing,
   the investment appraisal and cash flow, cost/risk posture, how the investment delivers the
   capabilities the benefits depend on, and the public/opt-in licensing frame.
5. **Benefit profiles and benefits map** — one profile per benefit (§6), and the map from project
   output → capability → operational outcome → benefit/disbenefit → strategic objective. The
   public/opt-in licensing frame is a condition of the benefits, not a footnote.
6. **Projects dossier** — every project with its outputs, resource requirements, timescales,
   interdependencies, initial requirements, high-level budget, contribution to the blueprint and
   the risks affecting delivery.
7. **Programme plan and tranches** — the definitive project list, sequencing, the tranche plan
   (§7), entry/exit criteria and the review dates. It adopts and updates the existing seed; it does
   not invent a parallel list.
8. **Governance & organisation** — roles (sponsoring group, SRO, programme board, programme office,
   programme manager, programme architect, BCM, PMO lead, quality manager), the decision ladder,
   assurance, the unit protocol, and the strategy/plan set: benefits management, information,
   risk and issue, monitoring and control, communications, quality, and resources. The baseline
   trio — management, governance and boundary — is recorded with the definition.
9. **Stakeholder profiles** — concerns, sensitivities, importance, influence, interest, exposure,
   support level and trend, the engagement target, and the benefits and disbenefits distributed to
   each group.
10. **Benefits management strategy and realization plan** — measurement methods and sources, the
    review cycle, who assesses realization and when, transition and handover dates, embedding
    activities, and who continues measurement after programme closure.
11. **Reuse map** — what exists and may be reused: the conveyor V1 tree (tables, actions, modules,
    processes), the estate's absorbed services and modules, the seeded catalogues, the upstream
    crates. For each: reuse candidate, boundary, licence/contract constraint.
12. **Project briefs** — one per delivery project: outcome, boundaries, dependencies, acceptance,
    capabilities, plan link, reuse sources to analyse, and the two inception units below.
13. **Architecture / analysis pack** — current-state analysis, options, risks, dependency map, and
    the architecture artefacts the blueprint rests on, produced to the architecture artefact
    standard (SPEC_V2 §8.1).
14. **Architecture capability baseline (the Preliminary phase)** — the six things an architecture
    project needs before it starts: the architecture principles catalog (statement, rationale,
    implications per principle); the tailored ADM profile (which phases run, which artefacts are
    mandatory per phase and plateau, which contract closes each phase); the artefact standard
    adopted with its repository and type catalogue; the Architecture Board charter, membership and
    decision limits; the governance, contract and waiver baseline; and the maturity (nine
    dimensions, 0–5) and skills (seven categories) baseline with its development milestones. It is
    the programme's default `adm_profile`; a delivery project's brief says which plateau its own
    architecture inception must satisfy.

## 4. The project brief template (mandatory)

Every delivery project's brief carries:

| Field | Content |
|---|---|
| Outcome | One sentence: what is true after the project that is not true now |
| Boundary | Out of scope, and which existing surfaces it must not fork |
| Reuse sources | Paths inside conveyor; estate services/modules; upstream crates. Each with the analysis question |
| Inception units | `analysis` and `architecture`, the project's first two claims |
| Dependencies | Typed: blocked-by, acceptance-blocked-by, blocks, related |
| Acceptance | BDD scenario per part with a `Verify:` command |
| Capabilities | The organisation positions/capabilities that own it |
| Plan | The slice/S## plan path, or the T## SPEC_V2 section |

## 5. The inception standard (per delivery project)

**Analysis** — current state and reuse: for the project's capability, inventory the code that
already exists (inside the conveyor repo, in the wider estate, and upstream), the contract and
licence constraints, the build-vs-reuse decision, and the risks. It cites `path:symbol` inside the
repo; outside, it names the capability, not a private host or URL.

**Architecture** — the project's design, produced to the architecture artefact standard (SPEC_V2
§8.1): the aggregates, tables, actions, modules, integrations, decision record and acceptance, plus
the artefacts its phase and plateau require — the PSA set at inception (the principles it applies,
its requirements, stakeholder map, driver/goal/objective and solution concept) and the HLD and SD
sets as it matures (business services, data entities, application and technology views, process
and migration diagrams). Every artefact is a typed catalog, matrix or diagram carrying the
standard's common metadata — owner, reviewer and review date, version, purpose/stakeholders/scope,
the related project, and its published document or contract — and its links to what it derives
from, realises or reuses. The project operates under the live `architecture_contract` that governs
it; it must fit the blueprint, and a deviation is a decision record, not a silent fork.

Both are documents on the project with `meta.category` set. They are published when accepted. A
project without both published is an inception gap; its build units are not claimed.

## 6. Benefit profiles and the benefits map (mandatory)

The programme is judged on benefits operations realizes after the projects close, not on the
capabilities it delivers. A capability gives a team a new option; whether it uses the option, and
whether use improves performance, are separate questions. A benefit is a **measurable improvement**
resulting from an outcome, valued by at least one stakeholder, that contributes to an organisational
objective.

Every benefit profile carries all six groups; a profile missing a baseline, a target, a measurement
method or an owner is refused:

| Group | Fields |
|---|---|
| Identification and purpose | reference, description, category, objectives, observable outcome |
| Performance | affected KPIs, baseline, target, measurement method, financial value where expressible |
| Cost | cost of realizing the benefit, including the associated business change |
| Conditions | capabilities required, outcomes that must already exist, changes to operating practice |
| Uncertainty and dependencies | related risks and issues, dependencies outside the programme |
| Responsibility and attribution | who realizes it, the benefit owner (BCM), the operational area to which the improvement is attributed |

The **benefits map** records each path — project output → capability → operational outcome →
benefit or disbenefit → strategic objective — and the dependencies between benefits, on outputs and
on capabilities. It answers two questions the programme needs at every review: which benefit is put
at risk when a project slips, and which operational area must change its practice before delivery
creates value.

The baseline is measured while the old practice is still observable. A benefit whose baseline was
never measured cannot claim an improvement, and a project output is not a benefit.

## 7. Tranches and end-of-tranche reviews

A **tranche** is the unit the programme manages and reviews together. The PI cadence is the delivery
rhythm inside it; the tranche review is a programme decision, not a demo.

Each tranche records its number, entry and exit criteria, the baseline it is managed against and its
review date. The end-of-tranche review is an approval: the sponsoring group decides **continue, pause
or stop** on evidence — the baselines, the benefit status against the profiles, the risk and issue
register, and the blueprint alignment. A review with no evidence is refused. A re-baseline goes
through change control; the review does not rewrite the baseline. The first tranche is authorised
only after the definition set is accepted, and no delivery project starts before that authorisation.

## 8. Acceptance

- The definition set is published as documents on P0. The seed's P0 set carries its eight
  categories today; the benefits, dossier, tranche and realization categories (§12 T1/T10) and the
  architecture-principles, artefact-standard, ADM-profile and capability-assessment categories
  (§12 T9) are added by the slice that lands the rows, because an expected output needs a template
  and a category the document vocabulary accepts.
- The programme plan lists 48 projects (P0 + 47) with tranches and dependencies.
- Every benefit has a profile with baseline, target, method and owner, and at least one benefits-map
  path to a strategic objective.
- Every tranche has entry/exit criteria and a review date; the first tranche is authorised.
- Every delivery brief carries reuse sources and the two inception units, names the ADM phase and
  plateau its architecture inception must satisfy, and names the live `architecture_contract` it
  operates under.
- The architecture capability baseline (§3.14) is published on P0: principles catalog, tailored ADM
  profile, artefact-standard adoption, board charter, maturity and skills baseline.
- Every architecture artefact in the pack carries the standard's common metadata and at least one
  artefact link; a delivery project whose design gate has no live contract is refused.
- The Validation page shows P0 at tranche 0 and T1–T17 `blocked_by: [P0]`.
- `check-coverage` passes; the seed regen is idempotent; this plan is indexed by `build-docs`.

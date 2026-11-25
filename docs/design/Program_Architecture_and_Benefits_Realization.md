# Programme architecture and benefits realization

**Status:** a public, reconstructed study. The workshop document this programme was designed from was
input only and was never committed to this public repository; this page reconstructs its substance
from the merged record — `docs/SPEC_V2.md` §8.1 and §8.3, `docs/plans/P0-programme-definition.md`
§3–§7, and the design pack's programme and architecture layers — and cites what it rests on. The
programme state is measured at a stated date; where a fact is a live row, the date is given.

## 1. Purpose and provenance

The conveyor buildout is delivered as a **programme**, not as one large project: a temporary
organisation that exists to deliver benefits through projects, judged on the improvement operations
realizes after those projects close. The design has two interlocking bodies of practice:

- **MSP** supplies the programme's definition and its benefit track: the mandate, brief, blueprint,
  business case, benefits map and profiles, projects dossier, tranches and their reviews, and the
  measured realization of each benefit (SPEC_V2 §8.3).
- **TOGAF's ADM** supplies the architecture capability: the Preliminary phase, the phase rail A–H,
  the typed artefacts and the contracts that close each phase, compliance and waivers (SPEC_V2
  §8.1).

The rigour layer was merged in #216: the V2 rows, the gates, the P0 definition set and the design
study. T9 (#81) then built the architecture capability, T10 (#82) the tranche and benefits rows,
and T11 (#84) the metrics that measure realization. The design pack's `programme` and `adm` screens
are the design study; signed in, they read the live rows through the `programmes.*`, `benefits.*`,
`tranches.*`, `stakeholders.*`, `cadences.*` and `architecture.*` action families.

Two distinctions run through the whole model and are the reason it is rows rather than a document
library:

1. **A capability is not a benefit.** A delivered capability gives a team a new option. Whether the
   team uses it, and whether use improves performance, are separate questions. A benefit is a
   measurable improvement resulting from an outcome, valued by at least one stakeholder, that
   contributes to an organisational objective.
2. **A tranche review is a programme decision, not a demo.** A demo shows a deliverable; a tranche
   review decides **continue, pause or stop** on evidence — the baselines, the benefit status
   against the profiles, the risk register and the blueprint alignment.

## 2. The programme architecture

P0's definition set is the programme's mandate for the buildout. Every item is a row with a parent,
a version and a gate; the list is not a reading list.

| Definition-set item (P0 §3) | V2 row / action family | Gate |
|---|---|---|
| Mandate — strategic fit, position, drivers, boundaries, assurance, success and constraints | a `governed_asset` of the programme-definition project | the sponsoring group approves the mandate |
| Programme brief — vision, benefit categories, cost/timescale/effort, risks, delivery options including doing nothing, candidate projects | `governed_asset(kind='programme_brief')` | the brief is approved and definition is authorised |
| Blueprint — the target architecture and operating model | `governed_asset(kind='blueprint')`; the architecture artefacts | the blueprint contract |
| Business case — the options including doing nothing, the appraisal and cash flow, how the investment delivers the capabilities the benefits depend on | `governed_asset(kind='business_case')` | the investment decision |
| Benefit profiles and the benefits map | `benefit_profile`, `benefit_link`; `benefits.*` | the BCM and SRO accept the profiles and the map |
| Projects dossier — every project with its outputs, resources, timescales, dependencies, budget, contribution and risks | `project` rows plus programme membership | first-tranche authorisation |
| Programme plan and tranches — the definitive project list, sequencing, entry/exit criteria and review dates | the T1 plan spine and `programme_tranche`; `tranches.*` | the end-of-tranche review |
| Governance and organisation — roles (sponsoring group, SRO, programme board, programme office, programme manager, programme architect, BCM, PMO, quality), the decision ladder, assurance, the strategy set and the baseline trio | `governance_body`, positions and authority rows; `programme_baseline(kind='management'\|'governance'\|'boundary')` | the governance baseline |
| Stakeholder profiles | `stakeholder_profile`; `stakeholders.profile_*` | the engagement plan |
| Benefits management strategy and realization plan | `governed_asset(kind='benefits_strategy'\|'benefits_realization_plan')` | transition readiness |
| Reuse map, project briefs, the architecture/analysis pack, the Preliminary capability baseline | the artefact repository and the project documents; the T9 capability | the architecture contract per project |

The programme is also a **ledger of registrations**: a risk, issue, assumption, dependency, benefit
or change is a `project_register_item` of one of six kinds, and a dependency that slips names the
benefits it puts at risk through the benefit-map edges. The tranche is the unit the programme
manages and reviews together; the PI cadence is the delivery rhythm inside it.

## 3. Benefits realization

A benefit profile carries six field groups, and a profile missing a baseline, a target, a
measurement method or an owner is refused:

1. **Identification and purpose** — reference, description, category, objectives, observable outcome.
2. **Performance** — the affected KPIs, the baseline, the target, the measurement method and the
   financial value where it can be expressed.
3. **Cost** — the cost of realizing the benefit, including the business change it needs.
4. **Conditions** — the capabilities required, the outcomes that must already exist, and the changes
   to operating practice.
5. **Uncertainty and dependencies** — related risks and issues, and dependencies outside the
   programme.
6. **Responsibility and attribution** — who realizes it, the benefit owner (the BCM) and the
   operational area to which the improvement is attributed.

The **benefits map** records each path — project output → capability → operational outcome →
benefit or disbenefit → strategic objective — and the dependencies between benefits, on outputs and
on capabilities. It answers two questions at every review: which benefit is put at risk when a
project slips, and which operational area must change its practice before delivery creates value.

The **baseline is measured while the old practice is still observable**. A benefit whose baseline
was never measured cannot claim an improvement, and a project output is not a benefit. The
transition record carries the baseline measurement, readiness, enactment, the post-transition
measurement and the legacy retirement, with a post-closure owner. A profile's measurement fields,
once the profile is approved, change only through a cited, approved change request; a benefit with
an open transition cannot be retired.

The programme metrics (T11) are read-time aggregates over those rows: benefits realised versus
planned, tranche gate outcomes, the benefit-at-risk count, and the never-refused check. An
**unmeasured baseline is reported `unmeasured`, never zero.**

## 4. The TOGAF ADM capability

The ADM is implemented as a capability first and a project second. The **Preliminary phase**
establishes six things before architecture work starts:

- the architecture principles catalogue — statement, rationale and implications per principle;
- the tailored ADM profile — which phases run, which artefacts are mandatory per phase and
  plateau, and which contract closes each phase;
- the artefact standard and its repository — typed catalogues, matrices and diagrams, each with its
  common metadata (owner, reviewer, review date, version, purpose, stakeholders, scope, project,
  and its published document or contract);
- the Architecture Board — its charter, membership and decision limits;
- the governance, contract and waiver baseline;
- the maturity (nine dimensions, 0–5) and skills (seven categories) baselines with their
  development milestones.

The **phase rail A–H** then produces typed artefacts: vision and stakeholders at A; the business
architecture at B; the information-systems architectures at C; the technology architecture at D;
opportunities, solutions and the work packages at E; the migration plan at F; implementation
governance at G; and change management at H, which records exactly one change or kata. A phase
cannot close while a mandatory artefact is still draft, and a delivery project's design gate is
refused when it has no live architecture contract. Reviews record one compliance level; a waiver
names its accepting owner and its review date.

The capability is a **tailored ADM profile** for software delivery, not a claim of TOGAF
certification or full conformance; what is claimed is testable: the artefacts and gates the
organisation uses are rows, the process runs, and a deviation is a decision record rather than a
silent fork.

## 5. The buildout under its own capability — measured 2026-11-24

The programme does not only describe MSP and TOGAF; it runs under them. Measured on the live
programme at the stated date:

- **P0's definition set is published** — 27 of 27 outputs, so the tranche-0 review could be taken on
  evidence.
- **Tranche 0 is closed and its review decision is `continue`**, authorised by the programme owner
  on 2026-11-17; tranches 1–13 are planned. Tranche 1's exit set is S02, T1 and T11.
- **The delivery projects carry published briefs and inception outputs**; the inception gate
  refuses a build run until a project's analysis and architecture are published.
- **The benefit profiles are `draft`, with their baselines `not measured`** and a measurement
  method recorded; the tranche-0 review evidence lists them unmeasured and records that the BCM is
  unstaffed as a risk. That is the honest state, not a zero. The exception is **B7 Deliberated
  cross-department decisions** (T19), whose baseline is now a measurement record — zero deliberated
  decision rows before T19 — with the programme-manager job as its benefit owner. The seed carries
  **seven** profiles: B1–B6 and B7; the snapshot regenerated with this study exports all seven.
- **T19 (decision forums) is built and closed at tranche 4** — the plan and programme artefacts
  landed at #605, the implementation at #606, and the close-out at #650: the five tables, the
  twelve `meetings.*` actions, the `approval_decision` participant extension, the `/meetings`
  destination and the participant-hours roll-up are measured present, and the slice's evidence is
  `docs/plans/T19/evidence.md`. The live gate ran on the deployed revision on 2026-11-24: the
  human seat decided one agenda item in the web room and one over MCP, both `via='meeting'`, and
  the meeting closed on its published minutes; the same 15-check gate passes again on the current
  deployment `7e617673`. The gate exposed and closed two deployment defects in the MCP surface
  (the public host allowlist and per-session pod affinity), both in `gitops-production`. The
  published snapshot now carries T19, its board and B7.
  T19's own TOGAF chain is live and approved: seven typed artefacts
  (`requirements_catalog`, `stakeholder_map`, `driver_goal_objective`, `solution_concept`,
  `business_capability_map`, `data_entity_catalog`, `technology_standards`), their links, a live
  Statement of Architecture Work (`01a20ce0-5183-776c-84bd-327f2301b19b`, board decision
  `01a20ce0-644d-7340-a696-1af10fa8c61e`), a live phase-F delivery contract
  (`01a20cea-91bb-77f4-a880-044c90bb5f0e`), a Phase-G compliance review on each contract (level
  `conformant`), and a published `architecture-artifact` output
  (`01a20ce4-73e4-7009-9ec7-eb2265e9e84e`); `scripts/t19-governance-chain.py --check` reads all 16
  assertions back. The approvals were performed by the tenant's `conveyor-approver` seat under the
  standing delegation, not by the owner personally. `docs/plans/T19/closure-review.md` records the
  decision; the one remaining T19-labelled audit finding is the programme's tranche-4
  authorisation (P0), which belongs to the #526 retrospective chain; the `meeting_type` authoring
  route and the T9/T10 binding are the T19F follow-on (below). The T18 UI lane (#562–#566) is in
  flight and independent.
- **T19F (decision-forum governance) is built and active at tranche 6** — the follow-on T19's
  closure review named. The plan and the MSP/TOGAF/SAFe artefact set are
  `docs/plans/T19F-meeting-bindings.md` and `docs/plans/T19F/`; **#682/#683/#684** delivered the
  plan (PR #686), the versioned policy authoring through T17's route, the board sitting and the
  cadence materialisation (PR #700), and the live proof (PR #711). The close-out (#722) recorded
  the human `authoring.review` decision by FR Test in the signed-in console and took the live gate
  to 12/12: the policy v2 is published, the v1-pinned meeting still resolves v1, the sitting
  decided `via='meeting'` and the cadence materialised once. The project stays `active`: its
  closure review defers closure to the tranche-6 review, so no unledgered P0 is manufactured.
  T19's ledgered tranche-4 P0 remains the #526 chain's; the strict conformance remediation is the
  follow-on unit.
- **Architecture contracts remain a large open gap, one project smaller.** Twelve delivery
  projects (T3, T5–T8, T12, T14–T19) had no live contract at the last audit; T19 now carries one,
  live and board-approved, so the count is eleven. The board's approval is the authority act, and
  the remaining contract requests are tracked by the #526 programme-rework lane.

The units that close the gaps are themselves rows in the programme: T11's metrics (merged
2026-11-19, #404) measure realization; the seed of the buildout's own definition — its benefits,
map, tranches and per-project ADM phase/plateau — is a pending unit; the board's contract
approvals are a pending unit; and the closure and retirement gates (#302, #303) make a project's
close and a profile's retirement evidence-bound.

### 5.1 Decision forums — where MSP, TOGAF and SAFe meet

The meeting is the one mechanism all three frameworks name. A programme board and a tranche review
are MSP's decision and review forums; the Architecture Board and the Design Board are TOGAF's
governing bodies; PI planning, the system demo and inspect & adapt are SAFe's cadence events. V2
models each as a `meeting_type` over the same rows — the convener supplies the schedule, the
meeting supplies the record (D66).

| Framework | Named procedure | Convener | Agenda sources | Decision rule | Outcome |
|---|---|---|---|---|---|
| MSP | programme board | sponsoring group / SRO | programme register, benefit-at-risk, tranche decisions | chair (type policy) | decision / commitment |
| MSP | tranche review | sponsoring group | baselines, benefit status, risk register | unanimous or chair (type policy) | continue / pause / stop |
| TOGAF | Architecture Board sitting | `governance_body` | architecture contracts, compliance reviews, waivers, change requests | majority | decision / action |
| TOGAF | Design Board sitting | `governance_body` | solution designs, design gate, deviations | majority | decision / action |
| SAFe | PI planning | programme manager | backlog by WSJF, capacity, plan approval | majority | decision / commitment |
| SAFe | system demo | programme manager | completed deliverables and evidence | chair | information / decision |
| SAFe | inspect & adapt | coach | retro findings, lessons, kata candidates | majority | action / commitment |
| Operations | incident review | incident commander | incident, postmortem, runbook findings | chair | decision / action |

T19's set is the executable form of this table: `docs/plans/T19-decision-forums.md` (the delivery
plan), `docs/plans/T19/project-brief.md` and `inception-analysis.md` (the project's inception),
`docs/plans/T19/architecture-design.md` (the PSA and HLD set), `docs/plans/T19/msp-profile.md`
(B7, R20 and the tranche-4 evidence), `docs/plans/T19/safe-epic.md` (features, enablers, WSJF and
PI objectives) and `docs/plans/T19/artifact-register.md` (the typed catalogue). The
`governance_body` and `programme_cadence` rows are the consumer seams; T19F
(`docs/plans/T19F-meeting-bindings.md` and its artefact set) binds them explicitly: the body names
its standing type, the cadence row names its forum and materialises one meeting, and a policy
change is a reviewed authoring release.

## 6. The measurement and refusal discipline

The programme's claims rest on four rules that are enforced in code, not prose:

- **Metrics are computed from rows, never stored as opinion.** A change to the source rows changes
  the figure; reading a metric writes nothing.
- **A gate that never refused is flagged.** A gate with recorded decisions and no rejection is
  reported as never-refused with its counts, so a stamp cannot read as health.
- **A declared output that is unpublished blocks closure.** A project cannot close while an output
  it declared is missing.
- **A retirement is a record, not a delete.** A retired benefit or tranche keeps its row and its
  reason; an open transition blocks the retirement of its benefit.

These are the mechanisms that let a tranche review or a benefit realization be taken on evidence.
They are deliberately narrow: rows, gates and read models, with no certification badge and no
second register.

### 6.1 The strict conformance gate, measured 2026-11-25

`docs/design/scripts/publish-data.sh` runs the conformance audit strict (`--strict --fail-on
P0,P1 --baseline docs/evidence/conformance-baseline.json`) before it replaces the pack. The
publish of 2026-11-25T09:52Z passed: **strict audit OK**. The remediation that made it pass was
records, not waivers:

- the five closed projects **T5, T6, T8, T15 and T17** published their declared `closure-review`,
  and **T5, T6, T12, T15 and T17** their declared `evidence`; T8 also published the brief its
  declaration names;
- the ledger gained one time-boxed entry — **T19F `project_in_unauthorised_tranche` P1**,
  disposition `rework`, owner `programme owner`, expires 2027-02-18 — because T19F stays active at
  tranche 6 until the tranche-6 review; T12's tranche-13 P1 is ledgered the same way;
- the baseline's severity for the five closed projects' tranche gaps moved to **P0** (they closed
  before their tranches were authorised — still #526's chain), and eight resolved entries
  (S18/S21/SPEC-V2 closure records; T5/T6/T8 seed drift) were removed;
- one further time-boxed entry accepts a **seed/live drift** that the registry refresh surfaced:
  T14's seed derives `closed` from its closed unit issues while the live row stays `active`;
  closing it in planned tranche 13 would manufacture a tranche P0, so the synchronisation waits on
  the tranche-13 authorisation (#526 chain) or a seed install;
- `scripts/programme_state.py` honours a declared `closure_deferred_to_tranche`, so T19F's
  deferred closure keeps it active instead of deriving `closed` from its closed issues, and
  `seed/programme-definition.json` carries the declaration.

The strict set is 161 P0/P1 gaps (29 P0, 132 P1): each is either fixed or an accepted, owned and
expiring ledger entry. No tranche is authorised by this record.

## 7. What this study does not claim

It does not claim MSP or TOGAF certification, or a compliance badge. It does not claim a live
metamodel, a portfolio optimiser, a schedule solver or a benefit-forecasting model. It does not
replace the charging system: `billing_service` remains the charging authority, and the conveyor's
reconciliation compares its own derived figures with that roll-up and opens an incident on a
difference; the transport and its TLS policy belong to the platform's external-action path. And it
does not claim that the programme's own benefits are yet realized: at the measured date their
baselines are unmeasured, which is exactly what a benefits discipline is supposed to make visible.

## 8. References

- `docs/SPEC_V2.md` §8.1 (TOGAF ADM), §8.3 (MSP), §10.3–§10.4 (the row and action families), §12
  (the T0–T17 slices and their gates).
- `docs/plans/P0-programme-definition.md` — the definition set (§3), the brief template (§4), the
  inception standard (§5), the benefit profiles and map (§6), the tranches and reviews (§7).
- `docs/plans/03-DELIVERY.md` — the unit protocol, including the programme context every unit
  declares (tranche, brief, ADM phase/plateau, benefits).
- `docs/design/README.md` — the design pack's own description of the programme and architecture
  surfaces.
- The units that built the layer: #216 (the rigour layer), #81/T9 (the ADM capability), #82/T10
  (tranches and benefits), #84/T11 (metrics, metering and maturity), #302 (closure gate), #303
  (retirement path).

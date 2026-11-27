# Conformance and standards as row-derived sections

**Status:** design note for #783 — no code in this unit. Claim baseline `main@e6f4c3ac9`
(2026-11-27). The owner's decision (P4) is the merge gate and is not self-issued.

**Why this note exists.** Three Python generators still build the programme's own conformance
record from a mixture of live rows, committed files and published pages, and then write files
back. This note inventories every invariant they enforce, names one owner for each — a live
action, a snapshot section, a committed standard, a one-off instrument, or the unit that closes a
gap — and stages the replacement so no invariant is dropped on the way.

**Measured at the claim baseline** (`docs/design/data`, 2026-11-27):

| Generator | Lines | What it produces now |
|---|---:|---|
| `scripts/audit-programme-conformance.py` | 2,465 | 30 rules → 199 gaps (29 P0, 131 P1, 39 P2); a 160-entry accepted-gap ledger; the rework projection (420 packages, 533 subjects, 33 resolved, 2 suppressed) |
| `scripts/generate-standards.py` | 1,697 | 629 artefact instances (247 evidenced, 186 partial, 196 absent); 232 findings; 629 pages + `standards.json`/`standards.js`/`rework.js` |
| `scripts/governance-chain.py` | 1,288 | 7 live checks and a 6-step apply over `/api/actions` for a T19/T19F project |
| **Total** | **5,450** | the issue's 5,057 is the original `main@0d6db794` measurement; the delta is audit +40 and standards +353, chain unchanged |

The three are chained by `docs/design/scripts/publish-data.sh:59-122`: snapshot → audit (draft) →
standards findings → audit with findings → rework register → snapshot with `--conformance` →
strict audit → copy `conformance.json`/`.md` → standards (final) → `build-docs`. The site then
serves `snapshot.js`, `outputs.js` and the rendered pages (`crates/api/src/design.rs:4-11`,
`crates/pack/src/live.rs:74`); the baked `docs/design/data/*.json` and `docs/standards/*` are a
second copy of what the same rows can answer.

## 1. What is already row-derived — reuse, never duplicate

`crates/store/src/snapshot.rs:26-61` defines the snapshot; `:947-1025` assembles it. These
sections are live and already answer a large part of the inventory:

| Section | Fields the note relies on | Builder |
|---|---|---|
| `programme` | `programme`, `projects` (with `meta.depends_on`, `meta.tranche`, `meta.adm_phase`, `meta.closure_outputs`), `issue_registry`, `feedback` | `snapshot.rs:947`; registry merged by `crates/pack/src/snapshot.rs:207-220` |
| `msp` | `benefit_profiles`, `benefit_links`, `benefit_transitions`, `tranches`, `tranche_reviews`, `programme_baselines`, `cadences`, `wbs_nodes`, `register_items`, `project_dependencies`, `benefit_realisation`, `dependencies`, `tranche_reconciliation` | `snapshot.rs:976-992` |
| `msp.dependencies` | `edges`, `counts`, `untyped`, `unknown_targets`, `cycles` — recomputed from `meta.depends_on` | `snapshot.rs:1230-1307`; Kosaraju at `:1312` |
| `msp.tranche_reconciliation` | per tranche: `authorised`, `review`, `projects.{total,active,closed,archived}`, `flags` (`review_missing`, `projects_closed_before_authorisation`) | `snapshot.rs:1388-1481` |
| `msp.benefit_realisation` | per profile: baseline value/measured_at/placeholder, post value, delta, state | `snapshot.rs:1061-1210` |
| `artefacts` | `governed_assets`, `documents` (published outputs with `category`, `status`, `content_hash`) | `snapshot.rs:993` |
| `architecture` | `profiles` (`status`, `phase_set`, `artifact_requirements`, hash inputs), `standard`, `artifacts` (with `phase_code`, `status`, entries, links, `document_id`), `links`, `contracts`, `compliance_reviews`, `waivers`, `board`, `maturity`, `skills`, `phase_progress` | `snapshot.rs:994-1006` |
| `architecture.phase_progress` | per project: each phase `state` (`evidenced`/`not-evidenced`), `contract`, `compliance`, `next_required` | `snapshot.rs:1516+` |
| `work` | `cards`, `runs`, `approvals`, `approval_decisions` | `snapshot.rs:963-975` |
| `delivery` | per unit: `repo`, `lane`, `issue_ref`, `status`, `steps`, the three `gates`, `closure` (or `not recorded`) | `snapshot.rs:836-888` |
| `programme_health` | `tranches` (reconciliation), `benefits.{profiles,transitions,realised}`, `units.{total,by_status}`, `provenance.{documents,with_source_commit,unrecorded}` | `snapshot.rs:935-944` |
| `register` (pack) | declared-vs-evidenced outputs per project, quoting the committed gap register | `crates/pack/src/register.rs:77`; built at `crates/pack/src/snapshot.rs:197-205` |

`msp.project_dependencies` is the typed `project_dependency` table; `msp.dependencies` is the
declared `meta.depends_on` graph. Both are real graphs; the typed table is the better owner once
it is populated, and the declared graph is the fallback
(`scripts/generate-standards.py:417-458`).

## 2. P1 — the invariant inventory

One row per invariant. The source of truth is a live row, one of the sections above, a committed
standard, or a one-off repository fact. Lines cite the claim baseline.

### 2.1 `audit-programme-conformance.py` — the conformance rules

| ID | Invariant | Enforced at | Source of truth | Consumers |
|---|---|---|---|---|
| A1 | A declared dependency names a project that exists | `:287-313` | `msp.dependencies.unknown_targets` (`snapshot.rs:1279-1293`); typed `msp.project_dependencies` | rework packages, design site |
| A2 | A declared dependency carries `blocks\|needs\|supersedes\|informs` | `:314-324` | `msp.dependencies.untyped` (`snapshot.rs:1271-1278`) | rework packages |
| A3 | The declared dependency graph has no cycle | `:325-331` | `msp.dependencies.cycles` (`snapshot.rs:1294-1307`, Kosaraju `:1312`) | rework packages (P0) |
| A4 | A published cadence section has rows | `:361-372` | `msp.cadences` (`snapshot.rs:984`); the rule only fires when the section exists and is empty | rework packages |
| A5 | Every active project has a WBS node | `:374-391` | `msp.wbs_nodes` (`snapshot.rs:985`) vs `programme.projects[].status` | rework packages |
| A6 | The snapshot header schema matches the standard | `:425-458` | `manifest.schema` written by `crates/pack/src/snapshot.rs` (`SCHEMA`); expected value `sources.snapshot_schema` in `governance/conformance.json` | publish pipeline |
| A7 | P0's declared output categories are published | `:460-479` | `seed/output-standard.json` class `p0` (21 categories) vs `artefacts.documents`; the `register` section already compares these (`crates/pack/src/register.rs`) | publish gate, site posture |
| A8 | A tranche has entry/exit criteria; a closed tranche has a review and a review date | `:480-495` | `msp.tranches.{entry_criteria,exit_criteria,state,review_at}` + `msp.tranche_reviews` | rework packages |
| A9 | A `continue` review authorises the next tranche and names it | `:496-553` | `msp.tranche_reconciliation.authorised`, `review` (`snapshot.rs:1421-1474`); `msp.tranches.authorisation_review_id` | rework packages (P0) |
| A10 | A project's status is compatible with its tranche's state | `:555-589` | `msp.tranche_reconciliation.projects` + `flags.projects_closed_before_authorisation`; `meta.tranche` (`snapshot.rs:1429-1454`) | rework packages |
| A11 | A closed project declares `meta.closure_outputs` | `:591-607` | `programme.projects[].meta.closure_outputs` (`snapshot.rs:947`) | rework packages |
| A12 | A closed project publishes its closure review | `:608-620` | `artefacts.documents` + the project's declared/closure categories | rework packages (P0/P1) |
| A13 | Every declared output is published | `:622-641` | the `register` section (`crates/pack/src/register.rs:77`) + `programme_health.provenance` | publish gate |
| A14 | A benefit profile carries target, method, operational area, owner and a benefits-map edge | `:643-661` | `msp.benefit_profiles` + `msp.benefit_links`; the write is `benefits.create/update/link` | rework packages |
| A15 | A baseline is a measurement record, not a placeholder | `:662-704` | `msp.benefit_realisation.placeholder` and baseline fields (`snapshot.rs:1088-1123`); the write is `benefits.create/update` | rework packages |
| A16a | A realised benefit has a measured baseline, post-measurement and evidence | `:705-723` | `msp.benefit_realisation` (`snapshot.rs:1061-1210`); the write is `benefits.transition_upsert` | rework packages (P0) |
| A16b | A transition's measurements are ordered: baseline before enactment before post | `:724-763` | `msp.benefit_realisation`; the write is `benefits.transition_upsert` | rework packages (P0) |
| A17 | The programme has management, governance and boundary baselines | `:765-778` | `msp.programme_baselines` (`snapshot.rs:983`); the write is `programmes.baseline` / `programmes.baseline_approve` | rework packages |
| A18 | The ADM profile is adopted and matches the pinned hash | `:782-807` | `architecture.profiles` (`snapshot.rs:995`); the pin is `sources.profile_hash` in `governance/conformance.json`; the write is `architecture.profile_tailor` | rework packages (P0) |
| A19 | An active architecture board is recorded | `:808-821` | `architecture.board` (`snapshot.rs:1002`); the writes are `architecture.board_open` and the meeting actions | rework packages |
| A20 | A live v2 project has a live architecture contract | `:823-845` | `architecture.contracts` (`snapshot.rs:999`); the write is `architecture.contracts_request`/`contracts_approve` | rework packages |
| A21 | A live contract carries its approval | `:846-853` | `architecture.contracts.approved_at`; the write is `architecture.contracts_approve` | rework packages |
| A22 | A contract's requester is not its approver | `:854-869` | enforced live at `crates/actions/src/architecture.rs:1097-1105` (`requester_is_approver`); rows expose `requested_by`/`approved_by` | audit history only |
| A23 | A closed delivery project has a Phase-G delivery-contract review; a non-conformant review has an active waiver | `:871-927` | `architecture.compliance_reviews` + `architecture.waivers` (`snapshot.rs:1000-1001`); the writes are `architecture.reviews_record` / `reviews_waive` | rework packages |
| A24 | A waiver carries owner, reason and reassessment date, and an active one is not expired | `:929-955` | `architecture.waivers`; the write is `architecture.reviews_waive` | rework packages |
| A25 | The Preliminary capability has each mandatory artefact type approved | `:957-984` | `architecture.standard` (preliminary rows) + approved `architecture.artifacts` | rework packages |
| A26 | Every project has the approved artefacts its profile's requirements demand up to its phase | `:986-1026` | `architecture.profiles.artifact_requirements` + `architecture.artifacts.status` + `architecture.phase_progress` (`snapshot.rs:1516+` reports evidence, not the required set) | rework packages |
| A27 | The seed's project status matches the live row | `:1030-1046` | `seed/programme.json` (committed mapping) vs `programme.projects[].status` | rework packages |
| A28 | The coverage instrument's status matches the live row | `:1047-1064` | `docs/design/data/coverage.json` (the code-measured instrument) vs the live row | rework packages |
| A29 | The tracking issue names scripts that exist | `:1075-1102` | the GitHub issue registry (`programme.issue_registry`) vs the repository tree | rework packages |
| A30 | The rule matrix is a complete governance record | `:1271-1365` | `governance/conformance.json` (the standard, never a projection); validated against `seed/output-standard.json` and `seed/programme.json`/`programme-definition.json` | the audit itself |
| A31 | The accepted-gap ledger is valid, unexpired, and matches the live gaps | `:1189-1268` | `docs/evidence/conformance-baseline.json` — 160 entries, all `rework`, earliest expiry 2027-02-18 | strict publish gate |
| A32 | The gap register projects into rework packages, duplicates and suppressions, with a scorecard | `:1465-1709` | gaps + the matrix + the findings ledgers; published as `rework.json`/`rework.js` (file input at `crates/pack/src/snapshot.rs:209-211`) | the card factory, the site |
| A33 | The gap register has a Markdown rendering | `:1131-1182` | the gaps; a document renderer, not a record | `docs/evidence/conformance-gap-register.md` |

Supporting invariants of the same script (not separate rules): the self-test harness
(`:1740-2307`), `load_dataset` (`:1105-1128`), and the CLI (`:2313-2461`).

### 2.2 `generate-standards.py` — the standards derivation

| ID | Invariant | Enforced at | Source of truth | Consumers |
|---|---|---|---|---|
| S1 | The subject set is programme, project, tranche, benefit, capability, phase and card rows | `:183-288` | `programme`, `msp`, `architecture`, `work` sections | standards instances |
| S2 | Every standard template applies to every subject; the 629 artefacts carry a status (`evidenced`/`partial`/`absent`) and a gate | `:977-1056`, `:893-964` | `seed/output-standard.json` (the standard, committed) + the sections + published pages | findings, the design site |
| S3 | Each template section resolves to a named row or published document | `:769-872` | the same sections; the 14 computed sources below | artefact status |
| S4 | A published document section is evidenced only when the page carries its heading | `:293-334` | `artefacts.documents` rows + `docs/design/docs/output-*.html` | artefact status |
| S5 | An artefact's gate reads a live contract or approval | `:933-964` | `architecture.contracts` + `work.approvals` | the standards page |
| S6 | Every mandatory artefact that is not evidenced yields a finding, deduplicated against the gap register | `:1086-1159`, `:1061-1083` | the conformance gaps + `COVERED_BY`/`ABSENT_ONLY_COVERED` (`:64-73`) | rework register |
| S7 | An artefact's finding links to the published package that owns it | `:1424-1473` | the `rework` register (`:1380-1395`) | the site |
| S8 | The record's payload, every page's bytes and `standards.js` match their recorded hashes | `:1478-1551` | the generated record + the pack build | the pack's integrity gate |
| S9 | The WSJF order is computed from four typed integer inputs; the score is never stored (§8.2) | `:357-398` | `msp.wbs_nodes.{business_value,time_criticality,risk_reduction,job_size}` | the plan pages |
| S10 | The dependency-graph artefact rows are the typed table, else the declared graph | `:417-458` | `msp.project_dependencies` (typed) / `msp.dependencies` (declared) | the plan pages |
| S11 | A refinement cadence row carries a period; an unfiltered cadence read is not a row | `:460-485` | `msp.cadences` (`kind='refinement'`, `period`) | the cadence page |
| S12 | Kata candidates come from a lesson's typed marker or the kata register | `:487-548` | `msp.lessons[].meta.kata_candidate` or a `kata` section — neither is in the snapshot today (the kata rows are empty) | the I&A page |
| S13 | The gap-analysis rows are the conformance gaps | `:550-623` | the conformance gaps | the gap page |
| S14 | Each of the 629 artefacts is a rendered page with an index and a hash | `:1190-1368` | the instances; a document renderer | the design pack |

Computed sources (the 14 named derivations of S3): `programme_summary`, `benefit_map`,
`architecture_capability`, `phase_artefacts`, `requirements_catalog`, `gap-analysis`,
`dependency_graph`, `refinement_cadence`, `kata_candidates`, `wsjf_order`, `artifact_metadata`,
`artifact_entries`, `artifact_links`, `artifact_document` (`:625-766`). Every one reads a section
listed in §1; none needs a second derivation.

### 2.3 `governance-chain.py` — the live TOGAF chain

| ID | Invariant | Enforced at | Source of truth | Consumers |
|---|---|---|---|---|
| G1 | The standing delegation is published and active | `:1178-1182` | the delegation document row; `documents.publish` | `apply` gate |
| G2 | The live project row is the profile's project (`T19`/`T19F`) | `:1206-1207` | `projects.get`; the `programme` section | `check` |
| G3 | The seven mandatory artefact types are approved | `:60-68`, `:1213-1216` | `architecture.artifacts` rows; `architecture.artifacts_approve` | `check` |
| G4 | A live, approved architecture contract and a live delivery contract exist; the requester is not the approver | `:1218-1226` | `architecture.contracts`; `architecture.contracts_*` (the refusal is live: `crates/actions/src/architecture.rs:1097-1105`) | `check` |
| G5 | The benefit baseline is a measurement record and the benefit has an owner job | `:1228-1232` | `msp.benefit_profiles`; `benefits.*` | `check` |
| G6 | The `architecture-artifact` document is published and carries Metadata/Entries/Links/Document | `:1234-1242` | `artefacts.documents` + the page body; `documents.compose`/`publish` | `check` |
| G7 | The profile's Phase-G compliance reviews exist at the expected levels | `:1244-1260` | `architecture.compliance_reviews`; `architecture.reviews_record` | `check` |

The apply path (`:1176-1193`) is a six-step orchestrator over the same actions: artefacts, links,
Statement of Architecture Work, delivery contract, baseline, reviews, document. It writes no
record of its own; `scripts/programme-record.py` is its transport.

## 3. P2 — the ownership map

Exactly one owner per invariant. Owner kinds: **action** (a live handler is the gate),
**section** (a computed snapshot section is the gate), **standard** (a committed document that is
the rule, not a projection), **keep** (a one-off instrument observing an external/frozen fact or a
renderer), **gap** (no owner yet; the unit that closes it).

| ID | Owner kind | Owner | State |
|---|---|---|---|
| A1 | section | `conformance` reading `msp.dependencies.unknown_targets` | gap — Stage 0 |
| A2 | section | `conformance` reading `msp.dependencies.untyped` | gap — Stage 0 |
| A3 | section | `conformance` reading `msp.dependencies.cycles` | gap — Stage 0 |
| A4 | section | `conformance` reading `msp.cadences` (empty published section) | gap — Stage 0 |
| A5 | section | `conformance` reading `msp.wbs_nodes` + `programme.projects` | gap — Stage 0 |
| A6 | keep | the pack header contract (`manifest.schema` vs `sources.snapshot_schema` in `governance/conformance.json`) | live |
| A7 | section | `register` (`crates/pack/src/register.rs:77`) | live |
| A8 | section | `conformance` reading `msp.tranches` + `msp.tranche_reviews` | gap — Stage 0 |
| A9 | section | `msp.tranche_reconciliation.authorised`/`review` | live; the authorisation link field is a gap in the same section |
| A10 | section | `msp.tranche_reconciliation.flags.projects_closed_before_authorisation` | live |
| A11 | section | `conformance` reading `programme.projects[].meta.closure_outputs` | gap — Stage 0 |
| A12 | section | `conformance` reading `artefacts.documents` | gap — Stage 0 |
| A13 | section | `register` + `programme_health.provenance` | live |
| A14 | action | `benefits.create`/`benefits.update`; `benefits.link` | live |
| A15 | action | `benefits.create`/`benefits.update` | live |
| A16a | action | `benefits.transition_upsert` | live |
| A16b | action | `benefits.transition_upsert` | live |
| A17 | section | `conformance` reading `msp.programme_baselines` | gap — Stage 0 |
| A18 | gap | `architecture.profile_tailor` must compute and pin the profile hash (the standard pin exists; the action does not check it) | gap |
| A19 | section | `conformance` reading `architecture.board` | gap — Stage 0 |
| A20 | section | `conformance` reading `architecture.contracts` | gap — Stage 0 |
| A21 | action | `architecture.contracts_approve` | live |
| A22 | action | `architecture.contracts_approve` (`crates/actions/src/architecture.rs:1097-1105`) | live |
| A23 | section | `conformance` reading `architecture.compliance_reviews` + `waivers` | gap — Stage 0 |
| A24 | action | `architecture.reviews_waive` (verify the handler refuses an invalid waiver) | live |
| A25 | section | `conformance` reading `architecture.standard` (preliminary) + approved `artifacts` | gap — Stage 0 |
| A26 | section | `architecture.phase_progress` gains `missing_artefacts` from the profile | gap — extend the section |
| A27 | keep | the publish pipeline's seed/live check | keep |
| A28 | keep | the design-pack coverage instrument | keep |
| A29 | keep | the issue-registry fact | keep |
| A30 | standard | `governance/conformance.json`; shape check in the design pack | live |
| A31 | gap | accepted-gap rows via `registers.*` | gap — Stage 4 |
| A32 | section | `rework` section computed by the engine | gap — Stage 2 |
| A33 | keep | the document renderer (`build-docs`) | keep |
| S1 | section | `standards` reading the programme/msp/architecture/work subjects | gap — Stage 2 |
| S2 | section | `standards` computing the 629 instances from `seed/output-standard.json` + the sections | gap — Stage 2 |
| S3 | section | `standards` reading the 14 computed sources from §1 | gap — Stage 2 |
| S4 | gap | `documents.compose` must refuse a page missing a template heading (the four-heading rule) | gap |
| S5 | section | `standards` reading `architecture.contracts` + `work.approvals` | gap — Stage 2 |
| S6 | section | `standards` (the covered-key dedupe against the conformance gaps) | gap — Stage 2 |
| S7 | section | `rework` | gap — Stage 2 |
| S8 | keep | the pack's build-integrity check | keep |
| S9 | section | `msp.wbs_nodes` typed integer inputs; the score is computed in the page, never stored | live |
| S10 | section | `msp.project_dependencies` (typed), else `msp.dependencies` | live |
| S11 | section | `msp.cadences` (`kind='refinement'`, `period`) | live |
| S12 | gap | a `kata` section and the `kata.*` rows (the actions exist; the section and rows do not) | gap |
| S13 | section | `conformance` | gap — Stage 0 |
| S14 | keep | the standards page renderer | keep |
| G1 | action | `documents.publish` | live |
| G2 | section | `programme` (the live project row the chain addresses) | live |
| G3 | action | `architecture.artifacts_approve` | live |
| G4 | action | `architecture.contracts_request`/`contracts_approve` | live |
| G5 | action | `benefits.create`/`benefits.update` | live |
| G6 | gap | `documents.compose` must refuse a document missing a template heading | gap |
| G7 | action | `architecture.reviews_record` | live |

**The one-owner rule in words.** A write-time constraint belongs to the action that refuses it
(A14, A15, A16a, A16b, A21-A22, A24, G1, G3-G5, G7). A completeness or consistency check over rows belongs to
the engine's computed section, which reads the sections in §1 and never re-derives them (A1-A5,
A7-A13, A17, A19-A20, A23, A25-A26, S1-S3, S5-S11, S13, G2). An external or frozen fact, a
renderer or a build-integrity check stays a one-off (A6, A27-A30, A33, S8, S14). A gap names the
unit that closes it (A18, A31-A32, S4, S12, G6). No invariant is assigned to a file scheduled for
deletion.

## 4. P3 — the staged replacement

Targets are the measured function spans of the claim baseline; a stage closes when `wc -l` on the
three scripts is at or below its target and every proving test is green at the PR head.

| Stage | Capability live first | Files deleted | Proving test | Lines after |
|---|---|---|---|---|
| 0 | `conformance` snapshot section (the 30 rules as row predicates reading the sections) + `conveyor snapshot --strict` | none | the section's findings equal the current audit's output on the same snapshot; `--strict` exits 1 on the 160 accepted P0/P1 gaps and 0 when they are fixed | 5,450 |
| 1 | Stage 0's strict gate | audit methods `dependencies` (`:287-331`), `tranche_authorisation` (`:496-553`), `project_tranche` (`:555-589`), `benefits` (`:643-763`) and their self-test cases | `--strict` reproduces the same verdict; the four sections carry the same flags; audit `wc -l` ≤ 5,071 | 5,071 |
| 2 | `standards` and `rework` sections (the 629 instances, 232 findings, 420 packages computed by the engine) | `generate-standards.py` except the page renderer (`:1190-1368`), which moves to the pack; the audit's remaining row rules, `build_register` (`:1465-1662`), `_scorecard` (`:1665-1709`) | the standards summary `629/247/186/196/232` and the rework summary `420 packages/418 open/2 suppressed` reproduced from the sections; the pages byte-identical; the design pack green | 2,038 |
| 3 | the `programme-architect`/`solution-architect` playbooks over `/api/actions` (the #849 session channel), plus the G6 heading rule as a compose/handler rule | `governance-chain.py` (`:1-1288`) | the T19/T19F chain applied by the playbook; G1-G7 read back from the sections; no script in the playbook | 750 |
| 4 | accepted-gap rows via `registers.*` (a register of accepted P0/P1 gaps with owner, expiry, disposition) | `docs/evidence/conformance-baseline.json` and `check_strict` (`:1226-1268`) | the 160 entries readable as rows; the strict verdict identical before and after | 630 |

The 5,057 in the issue's P4.2 is the original `main@0d6db794` measurement; the claim baseline is
**5,450** (audit 2,465, standards 1,697, chain 1,288). The targets above are from that 5,450.

**Stage 0 is the load-bearing unit.** It is the only stage that needs new engine code before any
deletion; its section is the gate every later deletion leans on. It must reproduce the current
audit's findings exactly (a golden test against the committed snapshot) before anything is
removed. The `conformance` section reads the sections in §1; it does not re-derive them
(P1.2).

**Stages are independent after Stage 0.** Stage 2 and Stage 3 touch disjoint files; Stage 4 is
independent of both. Each is a separate issue, created only from this note once it is accepted.

## 5. P4 — the decision

**Recommendation: accept.** Adopt Stage 0 as the next unit (a `conformance` section and
`snapshot --strict` in the engine), then Stage 1's deletions, then Stage 2/3/4 as separate units.

The decision options, for the record:

1. **Accept** — the ownership map and the five stages as written; the next unit is Stage 0.
2. **Amend** — e.g., keep the standards pages file-first and move only the conformance rules; or
   move Stage 4 (the ledger) ahead of Stage 2. Name the change; the note is revised before merge.
3. **Reject** — the generators stay; the reason is recorded, and no unit is created from this note.

The two open questions the issue carries are answered in the recommendation:

- **The gap register: a register, not a new table.** The register itself is a projection over
  rows — the `conformance` section computes it. The *accepted-gap ledger* (160 entries) is the
  record that needs rows: a `registers.*` register of accepted gaps with owner, expiry and
  disposition (Stage 4). No new table is proposed.
- **The standards record: a section plus published documents.** The derivation (instances,
  status, findings) is the `standards` section; the pages are outputs rendered from it, with
  provenance (#756). Neither is a second derivation.

## 6. Evidence at completion

| Scenario | Command / test | Result | SHA |
|---|---|---|---|
| P1.1 file exists | `test -f docs/plans/conformance-rows.md` | exit 0 | 40b2f98b4 |
| P1.1 three generators named | `grep -c 'audit-programme-conformance\|generate-standards\|governance-chain' docs/plans/conformance-rows.md` | 10 (≥ 3) | 40b2f98b4 |
| P1.1 cited paths resolve | `git cat-file -e origin/main:<path>` for every path in §1-§2 | 19 paths checked, 0 missing | e6f4c3ac9 |
| P2.1 table count | `grep -c '^\| ' docs/plans/conformance-rows.md` ≥ the invariant count | 145 table rows; 54 invariants in P1 and 54 owner rows in P2 | 40b2f98b4 |
| P2.2 one owner per row | the P2 table, reviewed | 54 rows, one owner each (action, section, standard, keep or gap) | 40b2f98b4 |
| P3.1 stages | the P3 table; `git diff --check`; links resolve | the five-stage table with line targets; `git diff --check` exit 0; 0 Markdown links | 40b2f98b4 |
| P4.2 counts | `grep -n '5,057' docs/plans/conformance-rows.md` and `grep -n '5,450'` | both present (5,057 at §0 and §4; 5,450 at §0, §4 and the stage table) | 40b2f98b4 |
| Docs gate | `python3 docs/design/scripts/build-docs.py --check` | 303 sources, 0 failures; the page and index are committed | 40b2f98b4 |
| Programme gate | `python3 scripts/ci/board.py --repo nonagenticai/conveyor --dor 783` | exit 0 | 40b2f98b4 |
| P4.1 decision | the owner's comment on #783 | pending | — |

## 7. Risks and open points

- **The committed pack lags the live rows.** The 199 gaps / 629 instances / 420 packages above are
  measured on `docs/design/data` at the claim baseline; the live publish (#526's lane) can differ.
  Stage 0's golden test must be taken on both the committed and the live snapshot.
- **The strict audit already fails on the committed pack** (`seed_live_drift T12` and a grown-ledger
  entry) because the committed pack is not the live publish snapshot. That is a known state, not a
  property of this note; the Stage 0 unit must reproduce the *live* verdict and treat the
  committed-pack delta as the stale-badge signal (#867), not as a new rule.
- **The standards page renderer stays.** Rendering 629 HTML pages is a document renderer, not a
  record; it is out of scope for deletion and moves to the pack's own scripts.
- **The kata section is empty today.** S12's owner (the `kata.*` actions) exists, but the snapshot
  has no kata section and the rows are zero. Stage 2 must not claim a kata derivation it cannot
  evidence; the note marks it a gap with the kata section unit.

---
slice: T18
title: The real V2 UI — shell, routes, destinations and the delivery view
status: built
depends_on: [T0, T1, T2, T3, P0]
spec_sections: ["§10.5", "§12 T18", "D64", "D65"]
adds_tables: []
adds_actions: []
adds_events: []
adds_modules: []
parity_ids: []
---

# T18 — The real V2 UI

Programme project: T18 · issue #559 (kickoff) and #562–#566 (the five sequenced delivery units).

**Gate (SPEC_V2 §12 T18):** a signed-in tenant drives portfolio → programme → project → work
package, organisation → release → binding, procedures intake → publish, tools call → ledger,
improve → kata → adoption, operations incident → postmortem, and the ADM phase rail from the app;
every destination's spec asserts empty, error, happy and one refusal; the V2 coverage record
reports `exists` only from a real route and its spec; no production screen renders a fixture.

## 1. Objective

Make the V2 destinations real screens in `apps/web` — the one Leptos CSR/WASM application, served
by the same binary on port 8930 — instead of pages in the static design pack. The app already has
31 routes, 25 screen modules and 31 Playwright specs for the V1 surfaces. T18 adds the V2 routes
around them, with the same shell, the same action registry and the same test harness; the design
pack stays the public programme site at `/design/` and the reviewed reference (D64). The programme
measures the result from the app's route table and specs, not from the mock (D65).

No table, action or event is added. A destination whose owning slice is not built renders **not yet
live**, names the missing action or slice, and waits, typed, on that slice.

## 2. Starting state (measured at `main@490ad555`, 2026-11-22)

| Part | State | Evidence |
|---|---|---|
| The real app | 31 routes, 25 screen modules, 31 specs — all V1 | `apps/web/src/app.rs:139-169`, `apps/web/src/screens/mod.rs`, `apps/web/e2e/` |
| V2 routes | ABSENT | no `/portfolio`, `/programme`, `/organisation`, `/procedures`, `/authoring`, `/plan`, `/improve`, `/operations`, `/tools`, `/design/adm` |
| The mock | 26 page ids in seven nav groups; the L1–L4 delivery view landed 2026-11-21 (#528) | `docs/design/v2.js`, `docs/design/README.md` § "The delivery view" |
| V2 coverage record | 60 rows: 46 `exists`, 6 `extend`, 8 `new`; the destination rows credit the mock page | `docs/evidence/v2-coverage.json`, `scripts/build-v2-coverage.py:DESTINATION_PAGES` |
| Action registry | the destination families exist or are owned by T0–T17 | `docs/SPEC_V2.md` §10.4 |
| Programme | no T18 project or slice | `seed/programme.json`, `seed/programme-definition.json` |

## 3. Scope

**IN** — the shell and route registry; the ten V2 destinations in §10.5; the L1–L4 delivery view;
one Playwright spec per destination; the V2 coverage record measured from the real routes; the
navigation that exposes the V1 surfaces the V2 destinations extend.

**OUT** — new tables, actions, events or ports; SSR; a second frontend or router; a mobile
application; redesigning the design pack; any live tenant write; retiring the V1 screens the mock
labels legacy (they stay until the owning slice retires them).

## 4. Information architecture

### 4.1 Routes

The route table is normative; `docs/SPEC_V2.md` §10.5 carries the same table, and
`scripts/build-v2-coverage.py` reads it.

| Destination | Route (app) | Screens | Owning slice | Spec |
|---|---|---|---|---|
| **Organisation** | `/organisation` | departments/jobs, catalogue + release, bindings, authority matrix, approval policies, refinements, laya config, job performance | T0, T2 | `organisation.spec.ts` |
| **Procedures** | `/procedures` (V1: `/build/processes`, `/build/process-map`) | capability → procedure → step, SOP/RDF/OWL/BPMN intake, PDDL, the knowledge-pack pairing, process map, publish | T16 | `procedures.spec.ts` |
| **Authoring** | `/authoring` | intake queue, drafts, review/publish, dependency status | T17 | `authoring.spec.ts` |
| **Portfolio** | `/portfolio`, `/portfolio/:programme_id` | in-flight set, shared config, dependency graph, WSJF, cadence, capacity, per-project metering, L1/L2 | T1, T10 | `portfolio.spec.ts` |
| **Programme** | `/programme` | definition, benefits map/profiles, tranches/reviews, stakeholders, realisation | T10 | `programme.spec.ts` |
| **Architecture** | `/design/adm` | Preliminary, phases A–H, artefact repository, board/compliance, SoAW/RfAW | T9 | `adm.spec.ts` |
| **Plan** | `/plan` (V1: `/boards`) | WBS, baseline vs forecast, registers, change control, Boards join | T1 | `plan.spec.ts` |
| **Improve** | `/improve` (V1: `/evals/improvement`) | coach, kata six Gets, lessons, adoption | T4, T5 | `improve.spec.ts` |
| **Operations** | `/operations` | incidents, on-call, runbooks, gated writes, postmortems, andon | T7 | `operations.spec.ts` |
| **Tools** | `/tools` | toolkit versions, bindings/grants, calls, probes, BYO, authoring, storage, credentials | T13–T15 | `tools.spec.ts` |

### 4.2 The shell

One `Router`, one sidebar, one top bar in `apps/web/src/app.rs`. Seven nav groups, trimmed to the
destinations that exist: **Portfolio & plan** (Today, Portfolio, Programme, Projects, Outputs, Plan,
Validation, Execution, Decisions, Runs), **Organisation** (Organisation, Jobs, Agents, Tools,
Project types), **Procedures & authoring** (Procedures, Capabilities, Authoring), **Govern &
design** (Assurance, Requirements, Principles, Models, Architecture capability, System architecture,
Documents), **Knowledge & memory** (Knowledge design, Sources, Memory), **Measure & improve**
(Improve, Evals, Benchmarks, Hillclimb, Usage, Metering), **Operations**. An item is shown only
when the principal holds the scope for at least one of its actions or read routes; the existing
`has_action` gate is the rule. The tenant switcher, command palette, theme control and profile sit
in the top bar. Deep links stay honest: a signed-out V2 route is a sign-in gate, a forbidden one a
permission gate.

### 4.3 V1 routes retained

`/`, `/boards`, `/board/:board_id`, `/card/:card_id`, `/runs`, `/runs/:run_id`, `/approvals`,
`/design/requirements`, `/design/requirements/:id`, `/design/models`, `/design/models/:model_id`,
`/architecture`, `/build/processes`, `/build/processes/:process_id`, `/build/process-map`,
`/build/process-map/:map_id`, `/search`, `/measure/usage`, `/measure/prompt-audit`,
`/evals/benchmarks`, `/evals/benchmarks/run/:run_id`, `/evals/improvement`, `/evals`,
`/evals/:eval_id`, `/evals/:eval_id/:tab`, `/settings/members`, `/settings/roles`,
`/settings/inference`, `/settings/admin`, `/copilot`, `/platform`. The V2 screens link into them;
they are not re-implemented.

## 5. Destination contracts

Each contract names the read path, the write path, the states and the spec. Read paths use the
owning slice's existing actions or literal `GET`s; T18 adds neither. Write paths are
`POST /api/actions/{name}/invocations` with the headers §10.5.4 names.

- **Organisation** — reads `industries.list`, `departments.list`, `jobs.get/list`,
  `job_releases.list`, `job_bindings.list`, `approval_policies.list`, `decision_policies.list`,
  `refinements.list`, `laya_configs.list`, `ladder.list`; writes the create/update/release/retire
  verbs those families expose. Empty = no departments; denied = no `organisation:read`. Spec:
  `organisation.spec.ts`.
- **Procedures** — reads `procedures.list|get`, `capabilities.list|get`, `processes.list|get`,
  `approvals.list` and the published pack; writes `procedures.create|update|publish|retire`,
  `procedures.ingest_sop|ingest_process|ingest_rdf`, `procedures.validate|review_source`, and the
  two publication gates through `approvals.request|decide`. An unreviewed source refuses
  publication (`422 source.unapproved`); an invalid judge contract refuses (`422
  procedure.inspection`); a PDDL step the parser rejects refuses (`422 pddl.invalid`) and names
  the step. `capabilities.design|publish` are not registered; T16's ingest creates the
  capability. The PKO workspace was ruled out by the owner on 2026-11-19 (PR #419) and is
  replaced by the knowledge-pack pairing. Spec: `procedures.spec.ts`.
- **Execution** — reads `runs.list|get`; renders the run's steps, gates, process, inputs and tree
  (the provenance and explain-context) at `/execution`. The executor kind, `job_binding_id` and
  `release_hash` are not exposed by the run read model (T0/T4/S21), and the tools panel needs
  `tool_calls.list` (T13); the screen names both gaps. Spec: `execution.spec.ts`.
- **Authoring** — reads `authoring.assets` and the `authoring.review` gates through
  `approvals.list`; writes `authoring.intake|draft|review|publish` and the gate through
  `approvals.request`. The intake queue is the catalogue grouped by `AssetOut.intake_id`: T17
  exposes the intake write-only, so no read returns a recorded brief and the queue's brief read is
  carried by #655. §5.7's order is refused when the draft is attempted
  (`422 asset_dependency_unpublished`); the release re-checks it, but nothing can unpublish a
  dependency. Review records an approved gate whose `meta.target` is the asset id. Spec:
  `authoring.spec.ts`.
- **Portfolio** — reads the computed project/programme set, `programme_config.get`,
  `project_dependencies.list`, `cadences.list`, `metrics.programme`; writes `project_dependencies.*`,
  `programmes.update`, `wbs.*`/`baselines.*` where the Plan owns them. A dependency cycle is refused.
  Spec: `portfolio.spec.ts`.
- **Programme** — reads `programmes.list|get`, `benefits.list`, `benefits.link_list`,
  `tranches.list`, `tranches.review_list`, `stakeholders.profile_list`; writes the create/update/
  transition/review verbs. A benefit without baseline, target, method or owner is refused; a
  transition that never measured its baseline cannot report realised. Spec: `programme.spec.ts`.
- **Architecture** — reads `architecture.artifacts_*`, `repository_search`, `contracts_list`,
  `profile_get`; writes `artifacts_create|update|approve|supersede|link`, `contracts_request|approve`,
  `phases_advance`, `reviews_record|waive`, `board_*`. A phase missing a mandatory artefact is
  refused; a waiver needs an owner and a review date. Spec: `adm.spec.ts`.
- **Plan** — reads `wbs.*`, `baselines.list`, `registers.*`, `project_dependencies.list`; writes
  `wbs.*`, `baselines.take`, `registers.*`. A change without a rationale is refused
  (`422 change_rationale_required`); a new baseline does not move existing run pins. Spec:
  `plan.spec.ts`.
- **Improve** — reads the coach queue, `kata.list`, lessons; writes `coach.triage|remedy|escalate`,
  `kata.open|observe|analyse|propose|measure|adopt|propagate|close`, `lessons.propose|review|promote`.
  A kata without an instrument and a success threshold is not opened; a lesson with one evidence
  reference is refused. Spec: `improve.spec.ts`.
- **Operations** — reads incidents, runbooks, the on-call roster and the ops read actions; writes
  `ops.incident|triage|mitigate|resolve|postmortem` and the gated `ops.k8s_*` set. A destructive
  action without the configured approvals is refused; a host action is refused until T7's mechanism
  exists. Spec: `operations.spec.ts`.
- **Tools** — reads `tools.list|get`, `toolkits.list`, `tool_bindings.list`, `tool_calls.list`;
  writes `tools.probe|call|publish|retire`, `toolkits.import_openapi|publish`,
  `tool_bindings.create|update|retire`, `tool_authoring.*`. A binding that widens its grant gets
  `403 tool_effect_forbidden`; a failed probe unbinds new runs (`409 tool_not_ready`). Spec:
  `tools.spec.ts`.

## 6. The delivery view

One read model at four levels: **L1** the portfolio Gantt of programmes and standalone change
initiatives; **L2** a programme's projects grouped by tranche with the benefit and capability rails;
**L3** a project's kanban of work packages and loose cards; **L4** a work package's own board. A bar
is actuals — first observed card or run activity to completion, or to the snapshot time while in
flight. A planned window is a dashed track only when `meta.plan_start` and `meta.plan_end` both
exist; a row with no dated activity says so. Spend is the ledger's tokens plus the pinned commercial
price; an unpriced row renders `—*` with its count, never `$0`. The public boundary is the tenant
portfolio; the app shows the signed-in tenant's rows. The read model is the same derived
`work`/`measure`/programme sections the public pack publishes, exposed by the owning slices' read
actions; a section with no action is `acceptance-blocked-by` that slice.

## 7. Work packages

The five units are sequenced because `apps/web/src/app.rs` and `screens/mod.rs` are single-writer.
Each is one issue, one branch, one PR (03-DELIVERY's ≤5 split, written reason: file overlap).

### W1 — Shell, route registry, Portfolio and the delivery view (issue #562)

Files: `apps/web/src/app.rs` (router + nav), a new shell/nav module if the current one is inline
(`components.rs` or `shell.rs`), `apps/web/src/screens/{portfolio,programme}.rs`,
`screens/mod.rs`, `apps/web/e2e/{shell,portfolio,programme}.spec.ts`. The route registry gains all
ten V2 paths; only Portfolio/Programme render real data; every other V2 route renders the honest
"not yet live" state naming its owning slice. Acceptance: the delivery view L1–L4 from the tenant's
rows; a signed-out deep link gates; the nav hides an unpermitted item; the first spec asserts empty,
error, happy and one refusal.

### W2 — Organisation, Jobs, Agents, Decisions, Project types, Plan, Validation (issue #563)

Files: `app.rs` (route rows only), `screens/{organisation,jobs,agents,decision_inbox,project_types,
plan,validation}.rs`, `mod.rs`, e2e specs. Acceptance: the authority matrix, the catalogue/release,
the binding staleness refusal, the decision ladder and the WBS/baseline/register change control
render and write through their actions.

### W3 — Procedures, Capabilities, Authoring, Execution, Run history (issue #564)

Files: `app.rs`, `screens/{procedures,execution}.rs`, `mod.rs`,
`e2e/{procedures,authoring,execution}.spec.ts`. Acceptance: SOP/RDF/OWL/BPMN intake, the PDDL step
workspace, the knowledge-pack pairing and the two publication gates from the real T16 actions; the
capability catalogue as a read-only section (T16's ingest creates capabilities); the Authoring
route landed as the honest gate naming T17 — T17 (#87, PR #632) then made it a live read-only
catalogue and #629 delivered the operating half (intake, draft, review, publish), with the queue
derived from the catalogue because the family has no intake read (#655); the execution room renders
the run's provenance and names the missing executor/pin and T13 tool ledger. The PKO workspace is
not restored (owner decision 2026-11-19). Capabilities is not a separate route; `runs.rs` stays the
V1 queue.

### W4 — Tools, Metering, Operations, Improve (issue #565)

Files: `app.rs`, `screens/{tools,metering,operations,improve}.rs`, `mod.rs`, e2e specs. Acceptance:
the toolkit/binding/grant/call ledger and probe; the metering roll-ups and drill-down; the incident
→ mitigation → postmortem path and the gated writes; the coach/kata/lesson path.

### W5 — Architecture, System architecture, Knowledge/memory, Measure and the acceptance (issue #566)

Files: `app.rs`, `screens/{adm,system_architecture,knowledge,sources,memory,measure}.rs`, `mod.rs`,
e2e specs. Acceptance: the ADM phase rail, the artefact repository and compliance/waivers; the
system architecture map; the knowledge/memory workbench; the retained evals/benchmarks/hillclimb/
prompt audit; and the T12 organisation loop driven from the UI as the final spec.

## 8. Acceptance gate

The §12 T18 gate, proved by the five units together: every destination route exists, every
destination has its spec, the V2 coverage record reports `exists` from real routes and specs, and
the T12 loop runs from the app. A unit's own gate is its spec plus `web.yml`; the project closes
when all five are merged and the coverage record has no destination `new`/`extend` without a named
owner. A gate is not a destination: `build-v2-coverage.py` reports `extend` for a `*Gate` view even
when its gate spec exists, naming the owning slice; only a real screen reports `exists`.

## 9. Dependencies

| Relation | Issue / slice | Why |
|---|---|---|
| blocked-by (construction) | T0, T1, T2, T3, P0 | the shell and the first destinations need the spine; P0 authorises the project |
| acceptance-blocked-by | T4, T5, T7, T9, T10, T11, T13, T16, T17 | each destination's live scenarios wait on its owning slice; construction can proceed with the honest "not yet live" state |
| acceptance-blocked-by | T12 | the final loop is T12's acceptance, driven from the UI |
| related | #526 | the other lane's generated record; rebase + regenerate, never hand-merge |

## 10. Verification

- One spec per destination in `apps/web/e2e/`; each asserts empty, error, happy and one refusal.
- `web.yml` is the browser gate on `apps/web/**`; the PR's run is the verdict, not a local compile.
- `python3 scripts/build-v2-coverage.py --check` proves the record matches the routes and specs.
- `node docs/design/scripts/check-coverage.cjs` proves the programme’s project/unit set.
- The `design-pack` job's full command list stays green; `git diff --check` before the push.

## 11. Not claimed

No SSR, no second router, no mobile app, no design tool, no retirement of the design pack, no new
table/action/event, and no live tenant row invented by a PR. A destination without its backend says
so and waits on its slice; the mock is reference, never evidence (D64/D65).

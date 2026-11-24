# Architecture design — T18, the real V2 UI

## Context and boundary

The UI is a client of the conveyor's action registry, not a second system. `apps/web` owns the
routes, screens, navigation and states; the API owns reads and writes; Postgres owns the rows; the
design pack owns the public programme site. One image serves both surfaces: the app at `/`, the
pack at `/design/`. The boundary this design must not cross: the app never reads the published
snapshot as its data source, never renders a fixture, and never calls a backend except through a
registered action or a documented read `GET`.

The blueprint it fits is SPEC_V2 §10.5 (the real-UI specification) and the action families in §10.4.
The V1 routes stand and are linked, not re-implemented.

## Aggregates, tables and actions

**No new table, action or event.** The destination contracts name the existing families they read
and write; where a family is absent, the destination is `acceptance-blocked-by` its owning slice and
renders **not yet live**.

| Destination | Read | Write |
|---|---|---|
| Organisation | `industries.list`, `departments.list`, `jobs.*`, `job_releases.*`, `job_bindings.*`, `approval_policies.*`, `decision_policies.*`, `refinements.*`, `laya_configs.*`, `ladder.list` | the create/update/release/retire verbs of those families |
| Procedures | `procedures.list\|get`, `capabilities.list\|get`, `processes.list\|get`, `approvals.list`, the process map, the published pack | `procedures.create\|update\|publish\|retire`, `procedures.ingest_sop\|ingest_process\|ingest_rdf`, `procedures.validate\|review_source`, `approvals.request\|decide` for the two publication gates; `capabilities.design\|publish` are not registered (T16's ingest creates the capability) |
| Authoring | `authoring.assets`, the draft queue, dependency status | `authoring.intake\|draft\|review\|publish` |
| Portfolio | the computed project/programme set, `programme_config.*`, `project_dependencies.*`, `cadences.*`, `metrics.programme` | `programmes.update`, `project_dependencies.*`, `wbs.*`/`baselines.*` where Plan owns them |
| Programme | `programmes.*`, `benefits.*`, `tranches.*`, `stakeholders.*`, `cadences.*` | the create/update/transition/review verbs |
| Architecture | `architecture.artifacts_*`, `repository_search`, `contracts_list`, `profile_get` | `artifacts_create\|update\|approve\|supersede\|link`, `contracts_request\|approve`, `phases_advance`, `reviews_record\|waive`, `board_*` |
| Plan | `wbs.*`, `baselines.list`, `registers.*`, `project_dependencies.list` | `wbs.*`, `baselines.take`, `registers.*` |
| Improve | the coach queue, `kata.*`, `lessons.*` | `coach.triage\|remedy\|escalate`, `kata.*`, `lessons.propose\|review\|promote` |
| Operations | incidents, runbooks, on-call, the ops read actions | `ops.incident\|triage\|mitigate\|resolve\|postmortem`, the gated `ops.k8s_*` set |
| Tools | `tools.*`, `toolkits.*`, `tool_bindings.*`, `tool_calls.*` | `tools.probe\|call\|publish\|retire`, `toolkits.*`, `tool_bindings.*`, `tool_authoring.*` |

The read model for the delivery view is the existing `programme`/`work`/`measure` derivation at four
levels (portfolio, programme, project, work package), exposed through the owning actions. The SSE
topics are the existing `board:{id}`, `card:{id}`, `run:{id}` and `approval:{id}`; a screen without a
topic polls explicitly.

The **Execution** surface reads `runs.list|get` and writes nothing; the executor kind,
`job_binding_id` and `release_hash` are not exposed by that read model (T0/T4/S21), and the tools
panel waits on `tool_calls.list` (T13). The screen names both gaps and carries the step's input,
output and judge as the provenance. The PKO workspace is not restored: the owner decision of
2026-11-19 replaced it with the knowledge-pack pairing (SPEC_V2 §5.6).

## Decisions

- **D64** — the real UI is the app; the pack is the public site and the reviewed reference.
- **D65** — the V2 coverage instrument measures `apps/web/src/app.rs` and the named specs; `exists`
  requires both.
- **ADR 0009** — CSR stays (ADR 0001), action-only writes, no fixture data, the e2e harness is the
  verification.
- **Routes and nav** — the §10.5 table is the route contract; the seven nav groups are the mock's,
  trimmed to destinations that exist and gated by `has_action`.
- **The single-writer router** — the five units are sequenced on `apps/web/src/app.rs` and
  `screens/mod.rs`; a slice never opens a second PR against the same file.

## Acceptance

- The SPEC_V2 §12 T18 gate: every destination route exists with its spec; a signed-in tenant drives
  the portfolio → programme → project → work package, organisation → release → binding, procedures
  intake → publish, tools call → ledger, improve → kata → adoption, operations incident → postmortem,
  and the ADM phase rail from the app; the coverage record reports `exists` only from a real route
  and its spec, and a `*Gate` view is `extend` naming its owner even when its gate spec exists; no
  production screen renders a fixture.
- Each destination's Playwright spec asserts empty, error, happy and one refusal.
- The T12 organisation loop, driven from the UI, is the final spec.

## Architecture artefacts

The architecture to be registered in the live repository is described here; the rows are the
owner's/live-publish step, `acceptance-blocked-by` #545, and are not fabricated by a PR.

- **Catalogue** — the route/destination catalogue in SPEC_V2 §10.5 and this plan: every destination,
  its route(s), owning slice and spec. Owner: the T18 lane. Reviewer: the programme owner. Review
  date: the T18 closure.
- **Matrix** — the destination × action-family matrix above: what each screen reads and writes, and
  the refusal code that proves the gate.
- **Diagram** — the real-route map: the seven nav groups and the ten destinations, derived from the
  route table; the pack's UI map remains the design reference.
- The typed rows (`architecture.artifacts_create`) enter the live repository with the contract and
  compliance review when the live record is published; this document is their source.

## Migration and rollout

No migration: the routes are additive, the schema and the action registry are unchanged. The rollout
is the kickoff (#559) then T18-1…5 in order; each unit's `web.yml` is the gate and its PR is the one
browser run. Rollback is the unit's revert; the V1 routes are untouched.

## Links

Issue #559 · SPEC_V2 §10.5, §12 T18, D64/D65 · `docs/plans/T18-real-v2-ui.md` ·
`docs/decisions/0013-real-v2-ui.md` · `docs/design/STUDIO-SPEC.md` (design reference).

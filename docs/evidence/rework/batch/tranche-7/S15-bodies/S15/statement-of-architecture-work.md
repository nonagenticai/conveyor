# Statement Of Architecture Work — Capability, knowledge and memory

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a222a7-1881-7d2e-a3b5-928c86cb2b76",
      "approved_at": "2026-11-28T14:55:06.005722+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks: the capability aggregate resolves with its executor and versioned binding; readiness refuses a run with `409 capability_not_ready` before the first step; a cited passage comes from a pinned strata pack; memory is written, consolidated into `strata_rag` and recalled by `source_id`; and a degraded source mid-run reports `knowledge_degraded`, not the pre-run refusal.",
        "assignment": "Deliver the architectural record for a capability aggregate: a capability is a persisted thing with an executor spec, a versioned knowledge binding and memory, which a card's run resolves and readiness-checks before its first step. The assignment is retrospective \u2014 steps 1\u20134 shipped on 2026-11-09 and step 5's actions remain open \u2014 so this statement records the architecture and the remaining work together.",
        "constraints": "No second loop and no second retrieval path. The conveyor owns the binding and strata owns retrieval, so a run that read knowledge by any route other than `knowledge.query` would make the pinned pack advisory. Every new table carries a tenant column with row-level security forced, and documents are immutable \u2014 ingest a new version and prune the old, never update in place.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the capability model; the capability author writes the pack and its readiness declaration; the retrieval owner operates the strata pack. The retrospective author is not the accepting role, so the separation of duties holds.",
        "retrospective": "This statement is written after the first four build steps and before the fifth, which makes it unusually literal: it records what exists and what does not rather than reconstructing a finished shape. The seven remaining actions, the unproved pack-bound retrieval and the unproved run-transactional memory write are named as open work.",
        "scope": "In scope: the Phase A\u2013D artefact set, the capability tables and their versioning, the pre-run readiness check, the pinned retrieval binding, and the memory write, consolidation and recall. Out of scope: the model loop (S13's `agent.run`), the interactive copilot surface (S10), embeddings and answer generation (the executor's), a tenant-authored prompt store, KAG, and S08's dropped `project_capability` ledger."
      },
      "created_at": "2026-11-28T14:55:05.705673+00:00",
      "id": "01a222a7-17ec-751f-bc1c-9e932f9b4763",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S15 Capability, Knowledge and Memory (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:55:06.005722+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

```json
{
  "category": "statement-of-architecture-work",
  "count": 38,
  "rows": [
    {
      "approval_id": null,
      "approved_at": "2026-11-19T07:50:28.603416+00:00",
      "approved_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "body": {
        "acceptance_criteria": [
          "D1 \u2014 the artefact standard and repository: a catalog's entries are rows, an approved artefact is immutable and a supersede writes version 2, and a reuse link is searchable across plateaus.",
          "D2 \u2014 the Preliminary capability: the tailored profile names each phase's mandatory set by plateau, the board is a governance body row, and a maturity assessment carries all nine dimensions and a skills assessment all seven categories.",
          "D3 \u2014 the architecture project and phase gates: an approved artefact set and a live contract advance a phase, a draft artefact refuses the advance naming the artefact, and a contract change re-opens the gate.",
          "D4 \u2014 delivery compliance: a design gate without a live contract refuses, a superseded contract refuses naming its status, and one compliance level per review and a dated waiver without an owner are refused.",
          "D5 \u2014 change management: a review's exactly-one outcome is a change row or a kata candidate, and a review with no verdict cannot close the phase.",
          "D6 \u2014 the console surface: the Architecture destination renders its tabs from the live aggregates and each interaction records state."
        ],
        "assignment": "Deliver the TOGAF ADM capability as rows and actions in the one workspace: the artefact standard and repository, the Preliminary capability, the architecture project and its phase gates, delivery compliance, change management, and its console surface.",
        "responsibilities": "The programme architect (the maintainer sessions under the owner's authority) delivers and evidences each deliverable; the Architecture Board reviews and approves; the programme owner accepts the capability.",
        "scope": "The eleven tables of migration 0054 and the 26 architecture.* actions that expose them; the four ADM contract kinds; the seeded ten-type artefact standard; the Architecture Board; the maturity and skills baselines; and the Architecture capability screen (D6). The design is docs/plans/T9-architecture-capability.md."
      },
      "created_at": "2026-11-19T07:42:46.140382+00:00",
      "id": "01a1f2c2-0e1a-787f-9011-46b424d1ec71",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d35-7bd1-945d-cea12c5dfd6d",
      "requested_by": null,
      "status": "superseded",
      "superseded_by_id": "01a2023e-4e59-75ad-b2ab-fd7d014511d2",
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 the TOGAF ADM capability (T9)",
      "updated_at": "2026-11-22T07:52:45.884024+00:00",
      "version": 3
    },
    {
      "approval_id": "01a1fdd2-74eb-7fe3-a424-cdf02d021931",
      "approved_at": "2026-11-21T11:16:38.828544+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/P0.toml is green against the live record at the published SHA",
        "assignment": "Rework the programme definition project P0 by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526).",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "manifest": "acceptance/programme/P0.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the 21 programme-definition outputs, the charter and scope baseline, the benefit and governance baselines, the tranche-0 record and the closure review"
      },
      "created_at": "2026-11-21T11:16:21.046667+00:00",
      "id": "01a1fdd2-5041-7f80-8032-3305751d498c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1dcb8-d24b-76de-b901-56fd56e001bc",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 P0 programme definition (retrospective, U5a)",
      "updated_at": "2026-11-21T11:16:38.828544+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff2f-c76b-7bfe-b71a-a39294a95644",
      "approved_at": "2026-11-21T17:38:09.721121+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S01.toml is green against the live record at the published SHA; the decision record marks the unmeasured values not recoverable.",
        "assignment": "Rework the UI-stack spike by the reconstituted lifecycle (U5a, #526); the decision itself was made 2026-11-08.",
        "manifest": "acceptance/programme/S01.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "ADR 0001 and its measured table: interaction TTI, bundle, first render, drag frames, SSE\u2192DOM, memory, cold build and auth."
      },
      "created_at": "2026-11-21T17:36:38.122067+00:00",
      "id": "01a1ff2e-7974-74cd-be6d-377e3c0071a4",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-724c-9bae-af96b2d926b1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S01 (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:38:09.721121+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff37-f693-7336-8c6d-fe98f6f5c7b7",
      "approved_at": "2026-11-21T17:47:05.975622+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S00.toml is green against the live record at the published SHA; the inventory's 244 dropped rows are the gate's evidence.",
        "assignment": "Rework the usage inventory project by the reconstituted lifecycle (U5a, #526); the inventory itself was delivered 2026-11-07/08.",
        "manifest": "acceptance/programme/S00.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The 1,300-row parity inventory: 218 used, 295 rare, 543 unused, 244 dropped, across fr (798), RM (314), Planka (96) and n8n (92)."
      },
      "created_at": "2026-11-21T17:46:54.771712+00:00",
      "id": "01a1ff37-e240-779b-aab5-74a3fa86ecad",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S00 (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:47:05.975622+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff3c-8f0f-7326-b8a7-2de4d2eb6597",
      "approved_at": "2026-11-21T17:52:06.626934+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "Four, each with its check: (1) the public site renders the programme from Postgres \u2014 `data/snapshot.js` carries the programme's rows and the public sidebar names it only when the snapshot does (#470, 7d4e0d27); (2) a published output appears at its stable URL \u2014 165 committed output pages, every one named by `docs/design/docs/outputs.js` (#516, 06ba0fe3); (3) signed out there is no API request and no write control \u2014 `node docs/design/scripts/check-console.cjs`; (4) the pack's claims are checked like code \u2014 `build-coverage.py --check`, `build-docs.py --check` and the surface checks in the `design-pack` job of `.github/workflows/ci-public.yml`. The project's own gate is `acceptance/programme/DESIGN-PUBLISH.toml`.",
        "assignment": "Deliver to the programme owner, the Architecture Board and every outside reader the published studio design pack that makes the SPEC_V2 operating model readable and challengeable from a browser, and keep it live from the application image. This is the Phase A architecture work of a programme-level project in tranche 0, held to `docs/design/STUDIO-SPEC.md`. Recorded retrospectively on 2026-11-21: the pack was built and published by units #32, #33, #36, #37, #89, #97, #103 and #149, all closed.",
        "manifest": "acceptance/programme/DESIGN-PUBLISH.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The Phase A artefact set at plateau psa \u2014 stakeholder map (matrix), driver/goal/objective catalog, requirements catalog and solution concept (diagram) \u2014 with the Phase B\u2013D catalogues: business capability map, data entity catalog and technology standards. The published surfaces: `studio.html` offline, `https://conveyor.nonagentic.ai/design/` from the app image, `/design/data/snapshot.js` read from the tenant's rows, and `/design/docs/output-<id>.html` for every published output. Out of scope: any V2 backend aggregate, any signed-out write path, and the retired homepage mirror."
      },
      "created_at": "2026-11-21T17:51:55.933191+00:00",
      "id": "01a1ff3c-7aad-7dda-820e-1c7d85fe70a8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 DESIGN-PUBLISH: the published studio design pack",
      "updated_at": "2026-11-21T17:52:06.626934+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff41-2c98-7646-b835-7c7a96e1ca6e",
      "approved_at": "2026-11-21T17:57:08.762631+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest `acceptance/programme/SPEC-V2.toml` is green: the closure review exists, every declared closure output (`evidence`, `closure-review`) is published, and the project re-closes through the one gate decided by an approver other than its requester. The Phase-G compliance review records exactly one standard level against this contract and cites the published standing delegation. The seven A\u2013D artefacts are approved rows with their entries; no outcome-level claim substitutes for them.",
        "assignment": "Produce and publish SPEC V2 as the programme's adopted operating model, and prove it through the programme's executable acceptance. The work is the document and its decision set \u2014 `docs/SPEC_V2.md`, decisions D28\u2013D63 \u2014 with the ownership, organisational, work, decision, tool and rigour models it defines. This Statement of Architecture Work is recorded retrospectively on 2026-11-21 for the tranche-0 rework (issue #526) under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the project's real, contemporaneous statement was never recorded and is not recoverable.",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The V2 document and its decisions, and the phase A\u2013D architecture artefacts this contract governs: the stakeholder map, the driver/goal/objective catalog, the requirements catalog and the solution concept (phase A); the business capability map (phase B); the data entity/data component catalog (phase C); the technology standards catalog (phase D). Not in scope: V1's S-slices (not re-opened), the V2 delivery slices T0\u2013T17 (their own projects), and `docs/SPEC_LEGACY_KAG.md` (D54)."
      },
      "created_at": "2026-11-21T17:56:58.859998+00:00",
      "id": "01a1ff41-19f9-77bb-aa4a-f416f0d758e3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 SPEC-V2, the organisational operating model (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:57:08.762631+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff45-c6f7-7a7d-ab9f-0c9c8293147e",
      "approved_at": "2026-11-21T18:02:10.160888+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T0.toml is green against the live record at the published SHA: a job is created, a release is published, a binding pins its hash, a run on a retired binding refuses 409 job_binding_stale, no concept has two stores, and a test-only third executor kind registers without editing the run path. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T0 (SPEC_V2 \u00a712, issue #34) by the reconstituted TOGAF/MSP lifecycle: produce and publish the Phase A\u2013D mandatory artefacts, the charter and the delivery contract, the Phase-G compliance review and the closure review, declare the closure outputs and re-close through the one gate.",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/T0.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the model and its invariants (SPEC_V2 \u00a73 P1\u2013P11, \u00a74.1\u2013\u00a74.6, \u00a710.1, \u00a710.3): the organisation spine \u2014 industry, ordered departments, job_catalogue, the immutable job_release and the tenant-scoped job_binding; the decision-ladder types; and the internal Executor contract. The action contracts are those the registry serves and served-spec.lock.json pins. Execution behaviour is T2; the plan spine is T1; the tool platform is T13."
      },
      "created_at": "2026-11-21T18:02:00.126911+00:00",
      "id": "01a1ff45-b2ce-7276-8a88-cf3d744e7976",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7734-98da-fb7f1e2386c7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T0 Model and contracts (retrospective, U5a)",
      "updated_at": "2026-11-21T18:02:10.160888+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff84-57c3-7f67-9229-94bf0188732d",
      "approved_at": "2026-11-21T19:10:30.370289+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S02.toml is green against the published snapshot at the recorded SHA, with the S2 gate proven: two tenants isolated including RLS, and login works. The long-job lease/recovery proof under two workers was issue #2's P2; it is proved by `apps/server/tests/jobs_lease_two_workers.rs` at d367a4a9, and issue #2 closes with the lane PR.",
        "assignment": "Rework the delivered S02 skeleton by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526, tranche 1), recording the A\u2013D artefacts, the delivery contract and the Phase-G compliance review, and closing the project once the long-job proof issue #2 named is landed in this batch.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The ten-member workspace and one `conveyor` binary delivered by S02: eleven platform tables in `migrations/0001_platform.sql`, forced RLS on the nine tenant-owned tables, OIDC and development authentication, the fail-closed `app_licence` gate, the fenced job runner, the replaying `/api/events` stream, and the declared `shell/login` surface for S12."
      },
      "created_at": "2026-11-21T19:10:20.569282+00:00",
      "id": "01a1ff84-4425-75d9-8772-08e09cf71069",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S02 Skeleton (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:10:30.370289+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff89-1794-79d0-8707-f7614987ee7c",
      "approved_at": "2026-11-21T19:15:42.114015+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T1 gate: one project with a baseline, a change request and a refused self-approval; WSJF recomputes from inputs; the portfolio view lists the in-flight set; a dependency cycle is refused; a benefit profile without baseline, target, method or owner is refused; a benefit-map edge names its five levels. The ten issue-#72 scenarios are proved by the named real-Postgres tests in crates/actions/tests/plan_spine.rs and crates/api/tests/approval_dual.rs; the rework's acceptance instrument is acceptance/programme/T1.toml. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T1 (SPEC_V2 \u00a712, issue #72) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the plan spine and programme-definition rows of SPEC_V2 \u00a712: wbs_node, project_baseline, project_register_item, project_dependency, programme_cadence, programme_tranche, benefit_profile, benefit_link, benefit_transition, stakeholder_profile, programme_baseline, and the widened governed_asset kinds; the spine action families; and the requester/executor exclusion in approvals.decide. Out of scope: the decision ladder (T2), the PM job (T3), cadence materialisation and PI planning and the benefits-realization procedure (T10), metrics (T11), and any migration of V1's outline tree."
      },
      "created_at": "2026-11-21T19:15:31.506201+00:00",
      "id": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T1 Plan spine (retrospective, U5a)",
      "updated_at": "2026-11-21T19:15:42.114015+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff8d-b4cd-74f9-9f32-2b9f6cae33cd",
      "approved_at": "2026-11-21T19:20:43.741951+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T11.toml is green against the published snapshot at the re-close: every metric and every metering level recomputes from rows; the reconciliation with billing_service is exact at six decimals or opens an incident; a C0\u2013C5 level moves only when the measured evidence crosses its recorded threshold, naming that threshold version; a gate that never refused is flagged. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T11 (SPEC_V2 \u00a77.5, \u00a710.4, \u00a712; issues #84 and #458) by the reconstituted TOGAF/MSP lifecycle: produce and approve the Phase A\u2013D mandatory artefacts, this Statement of Architecture Work, the delivery contract, the Phase-G compliance review and the closure review; publish the declared outputs; and re-close the project through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The metrics, metering and maturity capability delivered by #84 (commit 183f5111, 2026-11-19) and the live Metering screen delivered by #458 (commit 0a8a6e95, 2026-11-20): migration 0059_metrics.sql, crates/domain/src/metrics.rs, crates/store/src/metrics.rs, crates/actions/src/metrics/mod.rs, the eight metrics actions, the billing.summary reconciliation input, the incident card type, the served-spec lock and the console screen. Charging, eval/benchmark-ledger quality metrics, knowledge metrics without ledgers and per-tool health are out of scope."
      },
      "created_at": "2026-11-21T19:20:34.079029+00:00",
      "id": "01a1ff8d-a0ad-7a91-b026-d9a2575daa3f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T11 Metrics, metering and maturity (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:20:43.741951+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2005a-8c6e-708a-84df-ab16d175072e",
      "approved_at": "2026-11-21T23:04:28.797665+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-21T23:04:18.681795+00:00",
      "id": "01a2005a-7886-7f2d-9b49-53d174544ee9",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:14.119814+00:00",
      "version": 3
    },
    {
      "approval_id": "01a20061-d19f-7269-9f5c-113f3aeb0001",
      "approved_at": "2026-11-21T23:12:26.539242+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC \u00a78 S3 gate: proptest on transitions and schema review. Property (a) \u2014 no transition leaves list.kind and archived_at contradictory, proved by board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error. Property (b) \u2014 claim not granted twice without release or expiry, proved as tested by claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic, with the lease-exclusivity half recorded as the P1 package v1-s03-1. Property (c) \u2014 one pending approval per (run, gate), proved by approval_race.rs::sixteen_racers_create_exactly_one_pending_approval. Property (d) \u2014 terminal run and dense revision, proved by transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense. Tenant isolation is proved by rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema review against data-arch \u00a72.3/\u00a72.4 admits the P2 package v1-s03-2. The rework's acceptance instrument is acceptance/programme/S03.toml; the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V1 slice S03 (SPEC \u00a76, \u00a78 S3) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate on the P0-only gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the baseline domain of SPEC \u00a75.3 and \u00a76: the fourteen tables of migrations/0002_domain.sql (programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state), the may_move matrix and the run/approval/asset state machines, the store `_tx` modules with TenantId, and the catalogue_entry.kind vocabulary. Out of scope: every route and action (S04\u2013S06), the board satellites (S05), channels and delivery (S06), outlines, quality and project_request (S07), models, views and portfolio registers (S08), documents and secrets (S09), conversations (S10), procedures (S11), workflow (S13) and the capability columns (S15)."
      },
      "created_at": "2026-11-21T23:12:15.079018+00:00",
      "id": "01a20061-bd73-7a89-b08c-bd8b867eb061",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S03 Domain and schema (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T23:12:26.539242+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20204-34e9-7eea-87d1-5b8265816c62",
      "approved_at": "2026-11-22T06:49:22.301988+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/PLATFORM.toml is green against the live record at the published SHA; the register's PLATFORM P0s are cleared or owned by a named executable package.",
        "assignment": "Hold the programme's definition, standards, gates and record; keep every project's acceptance manifest executable.",
        "manifest": "acceptance/programme/PLATFORM.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The programme capability: the one tenant, the programme definition set, the tranche plan, the artefact standard and the acceptance manifests; the programme record as reworked under #526."
      },
      "created_at": "2026-11-22T06:48:35.177183+00:00",
      "id": "01a20203-883b-7231-b0d3-dee948cf90e4",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work - PLATFORM (programme capability, retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T06:49:22.301988+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2023f-11ac-7fe6-bf6f-101ced8f4ba2",
      "approved_at": "2026-11-22T07:53:45.939115+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": [
          "D1 \u2014 the artefact standard and repository: a catalog's entries are rows, an approved artefact is immutable and a supersede writes version 2, and a reuse link is searchable across plateaus.",
          "D2 \u2014 the Preliminary capability: the tailored profile names each phase's mandatory set by plateau, the board is a governance body row, and a maturity assessment carries all nine dimensions and a skills assessment all seven categories.",
          "D3 \u2014 the architecture project and phase gates: an approved artefact set and a live contract advance a phase, a draft artefact refuses the advance naming the artefact, and a contract change re-opens the gate.",
          "D4 \u2014 delivery compliance: a design gate without a live contract refuses, a superseded contract refuses naming its status, and one compliance level per review and a dated waiver without an owner are refused.",
          "D5 \u2014 change management: a review's exactly-one outcome is a change row or a kata candidate, and a review with no verdict cannot close the phase.",
          "D6 \u2014 the console surface: the Architecture destination renders its tabs from the live aggregates and each interaction records state."
        ],
        "assignment": "Deliver the TOGAF ADM capability as rows and actions in the one workspace: the artefact standard and repository, the Preliminary capability, the architecture project and its phase gates, delivery compliance, change management, and its console surface.",
        "responsibilities": "The programme architect (the maintainer sessions under the owner's authority) delivers and evidences each deliverable; the Architecture Board reviews and approves; the programme owner accepts the capability.",
        "scope": "The eleven tables of migration 0054 and the 26 architecture.* actions that expose them; the four ADM contract kinds; the seeded ten-type artefact standard; the Architecture Board; the maturity and skills baselines; and the Architecture capability screen (D6). The design is docs/plans/T9-architecture-capability.md."
      },
      "created_at": "2026-11-22T07:52:45.884024+00:00",
      "id": "01a2023e-4e59-75ad-b2ab-fd7d014511d2",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d35-7bd1-945d-cea12c5dfd6d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": "01a1f2c2-0e1a-787f-9011-46b424d1ec71",
      "title": "Statement of Architecture Work - the TOGAF ADM capability (T9) - re-issued 2026-11-22",
      "updated_at": "2026-11-22T07:53:45.939115+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20256-80ac-72f0-a310-d114488f1d7c",
      "approved_at": "2026-11-22T08:19:17.038367+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": [
          {
            "criterion": "HTTP and MCP cannot diverge",
            "id": "a",
            "proof": "All 20 (16 internal + 4 fixtures.*) both ways; stripping request_id/duration_ms, http.body == mcp.structuredContent."
          },
          {
            "criterion": "One value, three projections",
            "id": "b",
            "proof": "spec_hash: manifest == resolved operation == MCP tool."
          },
          {
            "criterion": "Duplicates fail closed",
            "id": "c",
            "proof": "Exit 1 on a duplicate name, an MCP collision or an out-of-charset name."
          },
          {
            "criterion": "Schema-invalid input",
            "id": "d",
            "proof": "RFC 9457 with populated errors[], never a 500."
          },
          {
            "criterion": "Idempotent replay",
            "id": "e",
            "proof": "Two identical POSTs with one key: same status and body, one action_invocation."
          },
          {
            "criterion": "The lock catches drift; breaking ingest refused",
            "id": "f",
            "proof": "A renamed field makes --check-lock exit 1 naming the operation; the removed-field fixture exits 2 (422) while the approved version serves."
          }
        ],
        "assignment": {
          "depends_on": [
            "S02",
            "S03"
          ],
          "gate": "SPEC section 8 S4 - 20 actions identical over HTTP and MCP",
          "issue": "#13",
          "plan_status": "planned",
          "programme_project": "S04",
          "spec_sections": [
            "section 5.5",
            "section 7",
            "section 8 S4",
            "section 3 D11"
          ],
          "title": "Action registry - derive, manifest, OpenAPI, MCP, invocation, external ingest"
        },
        "manifest": {
          "adds_actions": [
            "actions.list",
            "actions.describe",
            "me.get",
            "tenants.get",
            "tenants.update_settings",
            "members.list",
            "members.set_role",
            "departments.list",
            "departments.create",
            "audit.list",
            "jobs.list",
            "jobs.retry",
            "schedules.list",
            "schedules.upsert",
            "events.list",
            "licences.get",
            "fixtures.echo",
            "fixtures.slow",
            "fixtures.fail",
            "fixtures.timeout"
          ],
          "adds_events": [],
          "adds_modules": [
            "actions"
          ],
          "adds_tables": [
            "action_registry"
          ],
          "deliverables": [
            "crates/actions-derive/src/lib.rs",
            "crates/actions/src/{spec,registry,invoke,mcp,openapi,lock,ingest}.rs",
            "crates/actions/tests/{registry,parity_http_mcp,idempotency,timeout,ingest_diff,external_actions}.rs",
            "crates/actions/tests/fixtures/external/openapi.json",
            "crates/{api,store}/src/actions.rs",
            "migrations/0003_action_registry.sql",
            "served-spec.lock.json"
          ],
          "source_pins": {
            "agent": "83b8de7e",
            "fr": "4a579e91",
            "fr_source_map": "6f7c5097",
            "pinned": "2026-11-07"
          }
        },
        "responsibilities": [
          {
            "responsibility": "Build crates/actions-derive and crates/actions, wire the boot assertion before TcpListener::bind, and add migrations/0003_action_registry.sql.",
            "role": "S04 delivery"
          },
          {
            "responsibility": "Supplies begin_scoped, audit_event, action_invocation, idempotency_key, event and /api/me.",
            "role": "S02"
          },
          {
            "responsibility": "Supplies the 14 domain aggregates and transitions used by the 16 internal actions.",
            "role": "S03"
          },
          {
            "responsibility": "Add domain actions and their own events in the same transaction; later slices add actions, never a surface.",
            "role": "S05-S11"
          },
          {
            "responsibility": "Provides the kind: workflow engine; until then workflow returns 501.",
            "role": "S13"
          },
          {
            "responsibility": "Supplies channel-mcp delivery rows and the signed mcp_server channel (S06-alpha).",
            "role": "S06"
          }
        ],
        "retrospective": "Planned, not yet delivered. Decisions closed here: S4-D1 derive crate is the 10th member; S4-D2 one literal route per action plus a spec_exempt fallback; S4-D3 enable/disable is approved_at; S4-D4 an external name colliding with any name is refused at ingest; S4-D5 idempotency scope is the action name; S4-D6 scope-filtered MCP tools with the refused tool absent; S4-D7 workflow returns 501 until S13. Traps to carry: four artifacts drifting is the fr lesson; Router::merge drops paths; MCP tool names are not action names; a timeout is the caller's; idempotency and the mutation must share a transaction; circular actions hit invocation_depth 8 and 409 action-recursion-limit.",
        "scope": "IN: ActionSpec plus #[action]; the compile-time registry and boot assertion; GET /api/actions; POST /api/actions/{name}/invocations with idempotency, timeout, audit, outbox; rmcp at /mcp; the OpenAPI fold and lock; openapiv3 ingest; migrations/0003_action_registry.sql; 16 internal plus 4 fixtures.* actions. OUT: domain actions (cards.move, ...) are S05-S11's; kind: workflow (S13); channel-mcp delivery rows (S06); the signed mcp_server channel (S06-alpha)."
      },
      "created_at": "2026-11-22T08:19:08.182233+00:00",
      "id": "01a20256-6ede-7d70-aae8-5dbfe681a1b7",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-7eb6-a62a-f420e742595c",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work - S04 Action registry (retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T08:19:17.038367+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20273-4f0f-77e6-8005-9744d1e3a574",
      "approved_at": "2026-11-22T08:50:49.577591+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": "# Statement of Architecture Work \u2014 T13 Tool platform\n\n## Context\nSPEC_V2 \u00a711 replaces ad-hoc backend calls with one governed path. The conveyor already owns actions, secrets, entitlements, audit and a durable queue, so a second catalogue/queue/runner (the predecessor MCP server) is refused (D44, D59). T13 is the slice that makes the one path real.\n\n## Scope\nThe broker `tools.call` and its seven steps; the four catalogue/ledger tables; effect-class gating; the closed-schema input check; credential aliases over sealed secrets; the direct MinIO storage adapter with the mandatory tenant prefix; the coding toolkit and one kag-era toolkit; V1 external-action migration; six gate proofs.\n\n## Approach\nImplement the broker and storage layers in the conveyor workspace, prove each refusal before dispatch at the wire (a counting stub), record one `tool_call` and one `tool.called` event per call, and record every artefact of this rework retrospectively from \u00a711 and the tests rather than from memory.\n\n## Deliverables\n`crates/actions/tests/tool_platform.rs` (the six proofs and the extra broker tests); `conveyor_actions::storage::{JobStorage, TENANT_PREFIX, sha256_hex}`; `conveyor_store::tool::{calls_for_run, ToolCallRow}`; `conveyor_actions::prompt::compile_for_audit`; the reused `conveyor_store::secret::create_secret_tx`; tables `tool`, `toolkit`, `tool_binding`, `tool_call`; action `tools.call`; event `tool.called`.\n\n## Constraints and standards\nRust workspace; PostgreSQL 18; MinIO (S3-compatible) as the platform store; a closed JSON-Schema subset (additionalProperties false, enums, patterns, bounds); OpenAPI 3.1 and streamable-HTTP MCP as the only two BYO forms; idempotency keys on writes and bounded polls.\n\n## Acceptance\nThe six \u00a712 clauses and their named tests, as written in `acceptance/programme/T13.toml`; the manifest is the gate, not this document.\n\n## Risks\nThe suite serialises because the destination URL is process-global; the storage test path is test-only until live MinIO wiring exists; the catalogue write path is deferred to T14; a production binding writer is still absent.\n\n## Decisions closed by this rework\nThe binding row is the broker's authority; refusals are proved on the wire; `tool_call` is not `action_invocation`; the storage adapter builds every key, so traversal and hash mismatches are refused.\n",
      "created_at": "2026-11-22T08:48:20.084131+00:00",
      "id": "01a20271-2a37-7329-b159-28031ddcb966",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T13 Tool platform (retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T08:50:49.577591+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20287-813e-7abd-83c5-ddf3bb9e98b3",
      "approved_at": "2026-11-22T09:12:50.020166+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T10 gate: PI planning materialises cards from the backlog by WSJF within WIP; a deliverable without evidence cannot demo; retro produces lessons; an end-of-tranche review with no baselines, benefit status or risk evidence is refused; a transition that never measured its baseline cannot report the benefit realized. Clauses 2, 3 and 5 are proved by named tests; clause 1 is proved for a project backlog and not for the programme-wide backlog (v1-p82-5); clause 4 is unproved at HEAD because free-text placeholders satisfy the evidence check (v1-p83-1) and the baseline requirement is not gated (v1-p83-4). The rework's acceptance instrument is `acceptance/programme/T10.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T10 (SPEC_V2 sections 8.2 and 8.3 and section 12, issues #82 and #502) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the SAFe cadence and the MSP tranche machinery of SPEC_V2 sections 8.2 and 8.3: the cadence state machine, WSJF and WIP-bounded materialisation behind an approved PI plan, backlog refinement, the demo credit, the retro lesson requirement, the tranche review and its evidence, and the benefit transition with its baseline measurement. Out of scope: the decision ladder (T2), the plan-spine baseline contract (T1), the coach card (T4) except the retro card, metrics (T11) and the one-organisation-loop acceptance (T12)."
      },
      "created_at": "2026-11-22T09:12:38.058104+00:00",
      "id": "01a20287-696c-76a4-a2db-28c6092b5813",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-70c1-a5e8-f50956c5e3f9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T10 SAFe cadence and MSP tranches (retrospective, U5a)",
      "updated_at": "2026-11-22T09:12:50.020166+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2028b-f63c-7c27-b580-d4cba2c1e70f",
      "approved_at": "2026-11-22T09:17:42.179607+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T4 gate: an injected failure yields exactly one coach card and one remedy; a repeat signature escalates; no coach card about a coach card; a systemic class opens a kata. Clauses 2 and 3 are proved by named tests in `crates/actions/tests/coach.rs`; clause 1 is proved for a first interruption but refuted as a universal by v1-p6-4; clause 4 is proved only as a `kata.open` process start and v1-p6-2 shows no kata record, instrument or threshold is created. The rework's acceptance instrument is `acceptance/programme/T4.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T4 (SPEC_V2 sections 6.2, 6.4 and 12, issue #75) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the interruption taxonomy, the closed remedy set, per-signature uniqueness and the depth cap of SPEC_V2 section 6.2, plus the systemic-class path of section 6.4: the `coach_interruption` and `coach_remedy` tables, the `coach.interrupt` action, the seeded coach process and the real-Postgres suite `crates/actions/tests/coach.rs`. Out of scope: the board and pull path's `err.card` producer, the kata's own lifecycle and measurement (T5), the PM job (T3), operations (T7) and the tool platform (T13)."
      },
      "created_at": "2026-11-22T09:17:30.02765+00:00",
      "id": "01a2028b-dde6-7ed4-9e47-6e582f2341d8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T4 Kanban coach (retrospective, U5a)",
      "updated_at": "2026-11-22T09:17:42.179607+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20ce0-644d-7340-a696-1af10fa8c61e",
      "approved_at": "2026-11-24T09:26:06.517885+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The six \u00a712 gate clauses plus D1\u2013D8; the live gate recorded in docs/plans/T19/evidence.md; the closure decision in docs/plans/T19/closure-review.md.",
        "assignment": "Build T19 \u2014 decision forums \u2014 to the \u00a712 gate: a meeting is a card whose agenda items resolve approvals, whose participants hold typed positions, and whose outcomes are derived arithmetic over those positions.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the SRO accepts the benefit B7 and the tranche evidence.",
        "scope": "The meeting aggregate, the approval_decision participant extension, the minutes governed-asset kind, the twelve meetings.* actions, the Meetings destination and the two consumer seams (governance_body, programme_cadence)."
      },
      "created_at": "2026-11-24T09:25:57.096759+00:00",
      "id": "01a20ce0-5183-776c-84bd-327f2301b19b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b9b8-7214-abc6-f2c4f75a1fc1",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T19 decision forums (2026-11-24)",
      "updated_at": "2026-11-24T09:26:06.517885+00:00",
      "version": 2
    },
    {
      "approval_id": "01a210bd-cdb2-7f2f-ac9a-c314fb1856af",
      "approved_at": "2026-11-25T03:26:48.7374+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The T19F plan's P1\u2013P5 scenarios, the live gate recorded in docs/plans/T19F/evidence.md, and the closure review in docs/plans/T19F/closure-review.md that defers project closure to the tranche-6 review.",
        "assignment": "Build T19F \u2014 the forum bindings \u2014 to the plan's P1\u2013P5: a meeting_type version is written only by the reviewed authoring route, a board subject is decided in a sitting with via='meeting' rows, and a cadence row materialises its own meeting exactly once.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the programme manager owns B7's measurement; the SRO accepts the tranche-6 evidence.",
        "scope": "Migration 0073; the authoring publish hook and meetings.type_list/type_get; architecture.board_sit_open/board_sit_decide; cadences.materialise; the mock-UI and real-app surfaces; the programme record and its evidence."
      },
      "created_at": "2026-11-25T03:26:38.984805+00:00",
      "id": "01a210bd-ba25-78c1-a363-3eea5aa71335",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T19F forum bindings (2026-11-25)",
      "updated_at": "2026-11-25T03:26:48.7374+00:00",
      "version": 2
    },
    {
      "approval_id": "01a217fb-a503-7db9-b47b-55a111c1c8de",
      "approved_at": "2026-11-26T13:11:37.614131+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate in SPEC_V2 \u00a712 is executable: a PM runs a request end to end; the PM cannot approve its own gate; a baseline change requires a change request. The close gate reads the project's declared outputs \u2014 project-brief and evidence \u2014 plus the closure-review output, a live board-approved delivery contract, a Phase-G review citing the standing delegation, the approved scope baseline, the seven mandatory A\u2013D artefacts with their registers, WBS inputs 8, 8, 5, 3, a demo credit per accepted deliverable and at least one tranche lesson. This content pack is checked offline by scripts/rework-content-check.py --project T3 before the factory reads it, and the factory then records the contract, review, artefacts and closure against the snapshot.",
        "assignment": "Rework T3 by the reconstituted TOGAF/MSP lifecycle: author the charter, this Statement of Architecture Work, the seven mandatory Phase A\u2013D artefacts, the delivery contract, the WBS deliverable and the Phase-G review, and re-publish the project-brief and evidence outputs. The architecture work is the PM job itself \u2014 one project run from charter through WBS, scope baseline, staffing, gate routing and closure, with the approver kept distinct from the requester and every baseline change carried by a change request \u2014 recorded retrospectively and dated 2026-11-26.",
        "contract": "This statement is authored retrospectively under the delegation and is approved through the same board gate it describes; its approval is recorded by the factory from this pack, not asserted here. It is complemented by the delivery contract whose commitments, compliance and approval fields this pack authors, on the approved scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa (kind scope, taken 2026-11-26, approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545), and by the Phase-G review the factory records from this pack. The tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9 decided continue on 2026-11-26 and authorised tranche 4; the reopen this pack answers is dated 2026-11-26T07:44:41Z and its reason is the missing contract, review and baseline record.",
        "responsibilities": "The programme architect authors the pack under the #526 rework delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14; the Architecture Board reviews the seven A\u2013D artefacts, which this pack records as approved. The programme owner is the approver of the delivery contract and of the closure: the scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa is approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545, and the closure gate recorded for T3 was decided by an approver distinct from its requester, which is the gate's own rule. The PM runs the request and may not approve a gate it raised.",
        "retrospective": true,
        "scope": "In scope: the charter and its approved gate; the WBS with the four typed WSJF inputs 8, 8, 5, 3; the scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa taken on 2026-11-26 and approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545; the staffing plan; the gate router that refuses self-approval and records every decision; the change request that must precede a baseline change; the closure record with its closure-review output; the seven A\u2013D artefacts and the project registers. Out of scope: the T2 decision ladder's policy engine, the T4 coach, the T5 kata, the T13 tool platform, the T1 plan spine's implementation and every V1 slice; the rework completes the record and adds no scope."
      },
      "created_at": "2026-11-26T13:11:37.331166+00:00",
      "id": "01a217fb-a475-7824-b01c-bfcfcb4680d3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7915-8833-75f4dc31cc07",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T3 PM job (retrospective, 2026-11-26)",
      "updated_at": "2026-11-26T13:11:37.614131+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2180f-168b-79af-af7f-e4cc18948e7b",
      "approved_at": "2026-11-26T13:32:51.863706+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-26T13:32:51.58027+00:00",
      "id": "01a2180f-15fe-794d-aba4-395c2f06a32f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:51.863706+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d8-8c6d-715e-ad85-da38d5f3351c",
      "approved_at": "2026-11-26T21:52:31.994413+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S05.toml` and plan \u00a77: (a) replay fidelity \u2014 1,000 cards/10 lists, `admin diff` prints zero deltas; (b) one winner \u2014 two clients with the same `If-Match` get one 200 and one 412 carrying the current representation, `version` +1; (c) 1k load \u2014 `GET /api/boards/{id}/cards?limit=200` p95 \u2264 120 ms at 20 rps, 1,000 cards in five pages \u2264 500 ms; (d) webhook shape \u2014 `{event,data,prevData,user}` exactly, `event` the internal register type and `data.list_id`/`prevData.list_id` the lanes; (e) SSE \u2014 a second client on `board:{id}` gets `card.moved` \u2264 200 ms live and on replay; (f) no second store \u2014 one `comment`, one `board_membership`, no `card_membership`, no `attachment`. The instrument is `acceptance/programme/S05.toml`; the closure gate is decided by an approver other than its requester. The snapshot records no Phase-G review of these properties; this rework records that gap rather than claiming a pass.",
        "assignment": "Rework the V1 slice S05 (SPEC \u00a78 S5) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the acceptance instrument.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the board slice of the plan: the nine tables (board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription), the 39 actions, the read projections and cursors, the SSE topics `board:{id}` and the resolved `card:{id}`, the webhook emitter and its `webhook.deliver` job, and the key-referenced `board-export/v1` CLI. Out of scope: attachments and card-level memberships, claims/assignee/stopwatch writes (S06), webhook delivery rows as actions, and the UI (S12)."
      },
      "created_at": "2026-11-26T21:52:31.682863+00:00",
      "id": "01a219d8-8bc7-71a2-8dbe-33702699eac1",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S05 Board (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:52:31.994413+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d9-303c-7fdc-b217-ec349e82329b",
      "approved_at": "2026-11-26T21:53:13.933646+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "Acceptance criteria": "Phase A closes when this statement is approved, the seven A\u2013D artefacts are approved in standard order, and the seven artefacts each carry metadata, entries, links and a document section with authored prose. Delivery acceptance is the plan \u00a77 gate: (a) three revisions with max(seq)=3 and a compare showing the status change; (b) an idempotent seed returning {0,0,N}; (c) both GIN indexes used under enable_seqscan=off with rising idx_scan; (d) relevance where B meets \u00a75.6's recall@5 \u2265 0.75, MRR \u2265 0.60 and B \u2265 C; (e) traceability in both directions with zero cross-tenant rows; (f) quality scoring read-only; (g) no orphan over the 28 RM modules. The record-level acceptance is `acceptance/programme/S07.toml`, whose 17 checks are 15 P0 and two P1.",
        "Assignment": "The assignment is to give the conveyor one governed requirements surface: requirements, principles and decisions share a single `governed_asset` lifecycle carried by S03, every mutation writes one immutable `revision`, `requirements.compare` diffs two revisions in Rust, quality is scored adviserily on a revision, and search ranks assets, cards and catalogue entries through SQL. The work is asked for by the tranche-4 sponsoring group and accepted by the Architecture Board; this statement is authored retrospectively on 2026-11-24 under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 for project 01a1d039-0d2c-7322-b2f9-4dd041153894.",
        "Contract": "The contract record is the delivery contract of project 01a1d039-0d2c-7322-b2f9-4dd041153894, version 1, approved by the Architecture Board on 2026-11-24 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. Its compliance clause is plan \u00a77 and the acceptance instrument `acceptance/programme/S07.toml`; its manifest names that instrument; its acceptance date is the snapshot of 2026-11-24, when the project row is closed at version 7 and tranche 4 (01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce) is authorised by the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9. The close gate is decided by an approver other than the requester.",
        "Responsibilities": "The programme architect (conveyor-e2e) is responsible for the plan, the seven artefacts, this statement, the delivery contract and the retrospective pack. The Architecture Board (conveyor-architecture-board) reviews and approves the artefact set and the Phase-G review. The S03 and S04 owners are consulted because `governed_asset`, `revision`, `catalogue_entry`, `trace_link` and the action registry are their surfaces; S08 and S12 are informed as consumers. The approver of the closure gate is deliberately a different principal from the requester under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14.",
        "Scope": "The phase set is A to D: the stakeholder map, the driver/goal/objective catalog, the requirements catalog, the solution concept diagram, the business capability map, the data entity/data component catalog and the technology standards catalog. The boundary is the plan \u00a75 material: the five \u00a75.1 tables, the 32 actions, the `search` module, the catalogue seed, the quality rubric and job, the eval set and the traceability actions. The project does not build the TOC API, ArchiMate or UML renders, project CRUD, batch bulk operations, RM's promote/project-copy or any UI, and it does not recreate `outline`/`outline_node`, which migration 0011 dropped."
      },
      "created_at": "2026-11-26T21:53:13.637379+00:00",
      "id": "01a219d9-2fa8-703a-99f2-fb7e7572e467",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S07 Requirements and search (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:53:13.933646+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d9-f717-73ba-9c86-18adeeecd4ef",
      "approved_at": "2026-11-26T21:54:04.836362+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The plan \u00a77 gate: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, then delete yields GET 404 and object_store::head NotFound, asserted in documents_roundtrip; (b) cap \u2014 10,485,760 bytes accepted and one byte more refused 413 application/problem+json with a stable type in documents_limits; (c) secret \u2014 rotate to version 2 and GET returns no plaintext, value key or ciphertext in secrets_never_revealed; (d) workers \u2014 the page renders live rows and a stopped service yields a banner with the breaker open after five failures; (e) reports \u2014 every number equals its SQL count on the same snapshot in reports_match_sql; (f) externals \u2014 GET /api/actions carries timeout_ms on every external and zero reqwest:: outside the executor in externals_declared; (g) billing \u2014 no write action and no metering event in the manifest. The same instrument also requires migration 0013 applied twice as a no-op, FORCE RLS on all four tables, and the nine declared event types. The snapshot records no run of these properties.",
        "assignment": "Rework the V1 slice S09 (SPEC \u00a78 S9) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, this charter, this Statement of Architecture Work and the delivery contract, take the scope baseline with one WBS deliverable, record a retrospective Phase-G review at level consistent, and publish the expected outputs project-brief and evidence. Because all nine linked issues are recorded closed, this rework re-closes the project at level consistent through the acceptance manifest, with the closure and closure review dated 2026-11-24.",
        "contract": "This Statement establishes the delivery contract against acceptance/programme/S09.toml, whose gate text names the seven properties above. Compliance is assessed clause by clause at the 2026-11-24 retrospective snapshot; the project's published evidence page is 01a1e687-7682-753d-b60e-1f00243b13a3 and its pre-rework audit pack is docs/evidence/rework/S09.json, which records verdict red on the missing baseline, charter, contract, Phase-G review and closure outputs. The rework supplies the lifecycle record and re-closes the project retrospectively on the P0 checks of the manifest; the remaining verification limits are carried in the closure review, the review and the evidence output rather than converted into a pass.",
        "responsibilities": "The programme architect (conveyor-e2e) owns the slice plan and this retrospective set; the Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract; the approver (conveyor-approver) decides the Phase-G level; the S03 team supplies tenancy, trace_link and revision, and the S04 team supplies the action registry with its OpenAPI and MCP projection. Operators own the KEK source in the pod environment and the MinIO bucket policy.",
        "scope": "the four tables document, repository, project_repository and secret in migrations/0013_documents.sql; the integrations module (object_store, external, breaker, ingest); documents.create/get/list/download/update/delete/report, 26 of the front matter's 27 actions in all; the nine event types; MinIO bucket conveyor-documents with key t/<tenant>/<document>/<version>; the KEK/HKDF/XChaCha20-Poly1305 secret envelope; repositories as references; the read-only workers, billing and observability displays; report queries over local tables; and the strata handoff. Out of scope: billing logic and metering writes, worker scheduling, artifact objects, code analysis, licence issuance, ingestion and embeddings as services, screens (S12) and project_request.repository_url (S07)."
      },
      "created_at": "2026-11-26T21:54:04.526424+00:00",
      "id": "01a219d9-f671-708a-b28f-063e7e79e6ec",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S09 Documents and integrations (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:54:04.836362+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219dd-4701-7342-9adf-f2c47331c4af",
      "approved_at": "2026-11-26T21:57:41.900644+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T14 gate, executed on the live estate through `acceptance/programme/T14.toml`: (a) each published toolkit has a probe and one live call; (b) `scientific_processor` is scaled up for its probe and one live call, then scaled back, with a receipt; (c) a zero-use report exists; (d) a backend outage unbinds new runs while in-flight runs finish. The snapshot carries no executed receipt for any of the four clauses, so this rework records them as the project's open verification item rather than a pass; the level written is consistent, never non_conformant without a waiver, and no waiver exists.",
        "assignment": "Rework the V2 slice T14 (SPEC_V2 \u00a712) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, record the Phase-G compliance review at the level the record supports, publish the expected outputs (`project-brief`, `evidence`) and leave the project active with its gate recorded as open verification rather than closing it.",
        "contract": "No live Statement of Architecture Work and no live delivery contract exist for T14 at the snapshot; this pair is supplied retrospectively by the 2026-11-24 rework and is the contract of record until a Board-approved one supersedes it. It binds the project to the four gate clauses above, to broker-only calls inherited from T13, to draft status for the named draft services, and to a receipt per claim; it does not authorise closure. The standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 is the authority under which the rework acts.",
        "responsibilities": "the conveyor-approver approves the Phase A\u2013D set and the delivery contract; the conveyor-architecture-board governs the standards; the programme architect (conveyor-e2e) owns T14 and this retrospective pack; the T13 tool-platform team owns the tool family, the broker, the effect classes and the credential profiles; the estate service owners grant the live-call window and the outage drill; the SRO is accountable for the tranche outcome.",
        "retrospective": "written by the 2026-11-24 tranche-4 rework; it reconstructs the record and does not attest contemporaneous intent.",
        "scope": "the estate-toolkit import wave on T13's tool platform: the `tool`, `toolkit`, `tool_binding` and `tool_call` rows for the coding fleet, strata, billing and licence reads, webscraper, media_discovery, scientific_processor, lean4_prover, video-compiler, the w2v orchestrator and the ops read tools; `evidence_review`, `discovery` :8807 and the planned AHTAPol services as `draft`; the probe and the one live call per published toolkit; the `scientific_processor` scale-up and scale-back receipt; the zero-use report; and the backend-outage unbind test. Out of scope: the broker itself (T13), tool authoring and BYO (T15), and the UI."
      },
      "created_at": "2026-11-26T21:57:41.6166+00:00",
      "id": "01a219dd-4673-70da-b701-3f43675b4fa6",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T14 Estate toolkits (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:57:41.900644+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219dd-edc0-7da7-a7fd-00eafabf0eae",
      "approved_at": "2026-11-26T21:58:24.588139+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The pack passes `python3 scripts/rework-content-check.py --project T16 --content docs/evidence/rework/tranche-4/rework-content-T16.json --snapshot /tmp/conv526-w1-snap/data --wsjf 10,8,7,8 --closes false` with exit 0; every cited identifier exists in the snapshot; the seven artefacts carry the existing titles, phase, plateau and kind in the required order; each artefact body carries Metadata, Entries, Links and Document as authored prose; the project brief and evidence carry their template sections with substance; and no closure key appears. The project's own gate remains acceptance/programme/T16.toml, and this review's verdict is consistent.",
        "assignment": "Under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, the retrospective completes T16's governance record against acceptance/programme/T16.toml for the procedure-ingestion slice of SPEC_V2 \u00a712. The assignment is record completion, not rework of the built capability: author the charter, the Phase A\u2013D artefact set, this Statement of Architecture Work, the delivery contract, the Phase-G review and the published project brief and evidence, and leave the project active for a separate close decision.",
        "contract": "The record is delivered under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 against scope baseline 01a216d0-f529-7bcb-81a7-d429d008d638 (approved 2026-11-24T16:58:21Z) and tranche 4 01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce. The delivery contract below records compliance, acceptance, approval, manifest and retrospective limits. Any change to scope or gate returns to the Architecture Board; no waiver is claimed or required, and the project stays active for the tranche-4 review to decide continue.",
        "responsibilities": "The programme architect authors the record under the delegation; the Architecture Board reviews contract and Phase-G conformance; procedure owners and publishers own the sources and the publication decision; delivery teams consume the ingestion and PDDL surface; tenants own their rows and sources; the programme owner owns the outcome and any close decision. scripts/rework-project.py performs the live writes, and scripts/rework-content-check.py refuses this pack before any write if it is structurally wrong or cites an identifier absent from the snapshot.",
        "scope": "In scope: SOP intake; RDF/OWL/Turtle as a strata knowledge pack bound by knowledge_pack_ref; BPMN, Visio and draw.io into PDDL procedure drafts; capability, agent and job design from an SOP; the source-review and human publication gates; re-ingest versioning; bound PDDL execution; and the governance records named in the assignment. Out of scope: any runtime RDF store, graph store or reasoner (decision D53, owner 2026-11-19), T17's tenant-wide generalisation, T19's meetings records, the /procedures destination work of issue #448, and new ingestion scope."
      },
      "created_at": "2026-11-26T21:58:24.307416+00:00",
      "id": "01a219dd-ed35-79eb-b898-3323854d5411",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d38-76fb-9394-b75934d16fcd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T16 SOP, RDF/OWL and process-document ingestion (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:58:24.588139+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219de-7e7e-72db-b751-a17bdd9b013d",
      "approved_at": "2026-11-26T21:59:01.639909+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T18 gate, judged by the fifteen checks of acceptance/programme/T18.toml: every destination route exists with its spec, each spec asserts empty, error, happy and one refusal, the coverage record reports exists only from a real route and a present spec with a *Gate reported as extend naming its owner, no production screen renders a fixture, and a signed-in tenant drives portfolio \u2192 programme \u2192 project \u2192 work package, organisation \u2192 release \u2192 binding, procedures intake \u2192 publish, tools call \u2192 ledger, improve \u2192 kata \u2192 adoption, operations incident \u2192 postmortem and the ADM phase rail from the app, with the T12 loop as the final spec.",
        "assignment": "Rework the V2 delivery slice T18 (SPEC_V2 \u00a710.5 and \u00a712 T18) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter, the Statement of Architecture Work and the delivery contract, take the scope baseline with the one WBS deliverable, record the retrospective Phase-G compliance review, publish the expected outputs project-brief, inception-analysis and architecture-design plus the evidence output, and leave the project active because T18-4 and T18-5 are not delivered. The work is retrospective: it reconstructs the governance record from the project row, the plan, the route table and the issue registry, and it attests no contemporaneous intent.",
        "contract": "The delivery contract record is requested and approved live by scripts/rework-project.py under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 and is titled 'Delivery contract \u2014 T18 The real V2 UI (retrospective, 2026-11-24)'. It commits the lane to the \u00a712 gate and to the two open units as typed dependencies, and it records that this retrospective neither closes the project nor waives any check. The contract's approval is retrospective; the historical snapshot has no live contract, which is one of the failed checks in the prior acceptance record.",
        "responsibilities": "The T18 lane under the programme architect (conveyor-e2e) is responsible for the route table, the screen modules, the specs and the coverage instrument. The Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract. The sponsoring group is accountable for the tranche-4 authorisation and for keeping the project active. The owning slices T4, T5, T7, T9, T10, T11, T13, T16 and T17 are responsible for the actions each destination calls, and T12 owns the final organisation loop.",
        "retrospective": true,
        "scope": "the shell, route registry and navigation; the ten V2 destinations of SPEC_V2 \u00a710.5 and the execution room; the L1\u2013L4 delivery view; one Playwright spec per destination; the V2 coverage instrument measured from apps/web/src/app.rs and the named specs; and the T12 organisation loop as the final acceptance. Out of scope: new tables, actions, events or ports; SSR, a second router or frontend, or a mobile app; retiring or rebuilding the design pack; the V1 routes, which are linked rather than re-implemented; and any production fixture or live tenant write inside the retrospective."
      },
      "created_at": "2026-11-26T21:59:01.357241+00:00",
      "id": "01a219de-7def-7827-8a21-98428640f502",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T18 The real V2 UI (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:59:01.639909+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21a39-aae9-7d90-9069-b0576ed463d4",
      "approved_at": "2026-11-26T23:38:36.773008+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S06.toml` and plan \u00a77: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 `cards.move(done)` yields `run.succeeded`, `approval.approved` and one audit, invocation and event row each; (b) dual control \u2014 `required_decisions=2`, one approve stays pending, the same approver again is 409, a second approver closes; (c) retry \u2014 `kill -9` mid-run leads to `run.reap`, a crashed run, then `attempt=2` with two rows and never two live runs; (d) andon \u2014 engaged gives 409 and a pending `run.execute` for 60 s, released resumes within one poll; (e) delivery retry \u2014 a stub 500 gives `attempts=5`, `state=failed`, one row; (f) no timers \u2014 `rg 'setInterval' crates apps` is empty and every cadence is a `schedule` row; (g) human vs agent \u2014 `decided_by_kind` is `human` for one decision and `agent` for the other. The instrument is `acceptance/programme/S06.toml`; the close gate is decided by an approver other than its requester. The snapshot records no run of these properties and no Phase-G review; this rework records the level `non_conformant` rather than claiming a pass.",
        "assignment": "Rework the V1 slice S06 (SPEC \u00a78 S6) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure decision, and re-close through the acceptance instrument once its properties run.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the control-plane slice of the plan: the six tables of `02-DATA-ARCHITECTURE \u00a72.6` (`channel_binding`, `channel_preference`, `message`, `delivery`, `audience`, `audience_member` \u2014 the last two PENDING-S0), the 17 actions, the 17 events, the SSE topics `run:{id}`, `approval:{id}` and `tenant`, the Telegram in/out path with its sender-identity resolution, the andon gauge and poller pause, and the job kinds that replace 11 of fr's 14 timers (three deleted). Out of scope: the signed mcp_server channel (S06a), MCP tool surfacing (S10), `project_request.timeout` (S07), the UI (S12) and the audience screens (PENDING-S0)."
      },
      "created_at": "2026-11-26T23:38:36.518039+00:00",
      "id": "01a21a39-aa69-7167-8696-e615aa3b012f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06 Control Plane (retrospective, 2026-11-27)",
      "updated_at": "2026-11-26T23:38:36.773008+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c16-879a-765f-9e1b-c788a9c1ef4a",
      "approved_at": "2026-11-27T08:19:28.421371+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The S08 gate is the slice gate of SPEC \u00a78 S8: the Part A round-trip from principle to ADR to TOC to register to deliverable to capability to stakeholder, the TOC cycle refusal, the absence of a duplicate principles/decisions surface, three golden models in SVG/PNG/PDF, the profile's write-time 422, the XMI round-trip, the post-commit validation job, deterministic ids, the two retention producers and the render byte-route parity.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the S08 slice: the governance surface over S03's spine and the modelling surface over seven new tables, with a deterministic renderer and a profile-scoped XMI exchange. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the S08 slice plan.",
        "constraints": "The build must not introduce a second model store, must not use the registry as the validation authority, must not call load_system_fonts, must not query JSONB geometry in SQL, must not validate inside the mutation transaction, and must not trust an XMI document: size, depth, duplicate ids and a DOCTYPE are refused. These constraints are the plan's traps and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set, the contract and the profile; S03 supplies the spine tables and RLS; S04 supplies the registry and invocation route; S07 supplies the outline and the principles/decisions actions; S12 receives the declared trace/architecture surface; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the slice plan, the measured \u00a77.2 completion record and the record's project row. It records the assignment and the acceptance the build was measured against; it does not claim that a contemporaneous board approved this text on 2026-11-08, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are the 68 actions, the seven S08 tables, the two retention producers, the two byte routes and the seven Phase A\u2013D artefacts. Out of scope are the UI, model search, project CRUD, principle-conflict verdicts, architecture containers, change-set/review/publication splits, graph layout and OCL/Profile/SysML; each exclusion is named in the slice plan's \u00a73 so a later reader cannot mistake absence for omission."
      },
      "created_at": "2026-11-27T08:19:28.107129+00:00",
      "id": "01a21c16-86ee-7106-8eab-4155e8b4b141",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2c-7bad-8e6c-b5c5d1fd40e2",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S08 Governance and Modelling (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:19:28.421371+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c1b-b5de-788c-9918-d57c95016a17",
      "approved_at": "2026-11-27T08:25:07.941014+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The S17 gate is the slice gate of SPEC \u00a78 S17: the banked parity oracle 555/555, gold 50/50 and empty 0/50, the golden eval scripts 50/50, the exact McNemar vectors, RLS on all four tables, the fail-closed sandbox, deterministic Job names and the 1 MiB ConfigMap cap, no answers to producers, no benchmark action in AGENT_TOOLS, and the live sandbox lifecycle recorded on compute-1.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the S17 slice: one measurement loop over four tables, a hashed seed artifact, a pure grader and exact McNemar, a k8s Job sandbox and a single run driver. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the S17 slice plan.",
        "constraints": "The build must not import Python into the product or the image, must not let a producer see answers, must not widen the sandbox config to a default namespace, must not exceed the 1 MiB ConfigMap cap or the 256 KiB patch cap, must not let the conveyor arm start before S18, and must not repoint the grading node. These constraints are the plan's traps and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set, the contract and the measurement standard; S02 supplies the job/event/audit tables; S03 supplies the RLS spine and the document row; S04 supplies the registry and scopes; S09 supplies the object store the eval log is written to; S18 takes the conveyor arm; S12 receives the declared screens; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the slice plan, docs/evidence/S17-CORE.md and the record's project row. It records the assignment and the acceptance the build was measured against; it does not claim that a contemporaneous board approved this text on 2026-11-09, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are the four benchmark tables, the seed artifact and its image lock, the standalone seed generator, the grader and 12 parsers, the exact McNemar, the sandbox adapter, the gold/empty/baseline arms, the eight actions, two scopes, two events, and the declared S12 screens. Out of scope are the conveyor arm (S18), model hosting, HF calls, the old CLI, a second service and Python; each exclusion is named in the slice plan's \u00a73."
      },
      "created_at": "2026-11-27T08:25:07.66928+00:00",
      "id": "01a21c1b-b557-7b0a-ba9d-bc9a2de9247d",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d30-794f-bd53-c72986b18e60",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S17 Native Benchmark Harness (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:25:07.941014+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c1d-aa55-7091-ae4b-93328e410035",
      "approved_at": "2026-11-27T08:27:16.055256+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The T17 gate: a brief plus documents draft the catalogue; documents yield linked procedure and policy drafts; an unpublished dependency is refused; a reviewed catalogue publishes with its decisions; no asset publishes itself; a non-human decision cannot complete a gate; no asset widens a tool grant; and the real-app E2E creates a draft and sees the row render.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the T17 slice: the authoring lifecycle over two tables, a human review gate, a published-dependency check and a publish-through-the-target rule. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the T17 inception set and SPEC_V2 \u00a75.7/\u00a710.4\u2013\u00a710.5.",
        "constraints": "The build must not write the target tables, must not let an asset publish itself, must not let a non-human decision complete a gate, must not widen a tool grant, must not duplicate the governed_asset lifecycle and must not refuse the dependency order later than the draft attempt. These constraints are the inception brief's risks and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set and the contract; T3 supplies the decision ladder and the approval primitives; T16 owns live ingestion and the SOP human gate; T13 owns tools and bindings; the web team owns the Authoring screen and the E2E; the tenant authors and reviewers take the human decisions; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the inception set, the evidence and closure-review documents and the record's project row. It records the assignment and the acceptance the delivery was measured against; it does not claim that a contemporaneous board approved this text, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are intake, drafting, review and publication of organisation/department/job/procedure/policy assets, the two authoring tables, the state machine, the review gate, the published-dependency refusal and the Authoring screen. Out of scope are writing the target tables, the SOP/RDF/BPMN converter (T16), tools and bindings (T13), a governed_asset fork and a new template table except asset_template; each exclusion is named in the inception brief's Boundary."
      },
      "created_at": "2026-11-27T08:27:15.79094+00:00",
      "id": "01a21c1d-a9d1-70d8-8a6a-901c21e4dd7b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d38-7db1-b685-d76bd4cebf47",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T17 Tenant Asset Authoring (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:27:16.055256+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c26-afda-78c1-aea9-920275ed65c4",
      "approved_at": "2026-11-27T08:37:07.301597+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The \u00a712 T7 gate: an injected pod failure is triaged, mitigated by an approved write, resolved and postmortemed; a destructive action without the tenant's configured approvals is refused; restart, scale, rollback and flag-flip are callable. The executed suite maps each clause to a named test in T7-acceptance.md; the live cluster script is deploy-bound and is not claimed as run.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the T7 record: the incident lifecycle over the existing card and approval primitives, the four gated cluster writes, the postmortem/kata loop and the /operations console. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by SPEC_V2 \u00a76.5, \u00a79.2 and \u00a712 T7.",
        "constraints": "The build must not let an ops write reach the cluster without an approval, a recorded dry run and two distinct human approvers; must not let the mitigation wrapper bypass the gate; must not let an incident that needed no write stay unresolvable; must not widen a tool grant; and must not claim the live cluster proof before the deployed image carries the gate. These constraints are the finding acceptance and are carried into the contract.",
        "responsibilities": "The programme architect owns the record and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set and the contract; T2 supplies the job/event/audit rows; T4 supplies the registry and the destructive effect; T13 owns tools; the web team owns the /operations screen; the on-call operators take the incident decisions; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the SPEC row, the issue #78 delivery, the acceptance evidence and the two fixes at ca155fada. It records the assignment and the acceptance the delivery was measured against; it does not claim a contemporaneous board approval, and it carries the live-proof limitation into the Phase-G review.",
        "scope": "In scope are the five incident actions, the four write verbs and their gate, the k8s read family, the incident meta lifecycle and the /operations screen. Out of scope are tools and bindings (T13), the agent tool surface (the ops writes are not agent-reachable) and a second ops service; each exclusion is deliberate."
      },
      "created_at": "2026-11-27T08:37:06.989963+00:00",
      "id": "01a21c26-af30-7c3e-ad6d-39f7ae1937ea",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d34-7f89-a0be-f3de3c9f540e",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T7 Operations (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:37:07.301597+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2210c-1cd2-7eb0-a3d2-c90c96b7d6e7",
      "approved_at": "2026-11-28T07:26:11.809378+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is one real agent task over `/mcp`: create a project, read its progress, open and answer an approval, close the project, with `decided_by_kind='agent'` recorded, no `nonagenticai_mcp_server` component in the path, and no `X-MCP-Binding` header. The local proof is `crates/api/tests/mcp_agent.rs::one_licence_principal_task_creates_reads_decides_and_closes`; the deployed agent-app run is S14's gate and is recorded as deferred.",
        "assignment": "The architecture work assigns S06b the agent's direct interface to the conveyor: define the tool key every MCP tool carries, resolve a machine principal from a licence bearer, propagate that principal into attribution, and add the approval-request action an agent needs to escalate. This statement governs the retrospective record of work completed on 2026-11-09 and re-read at the 2026-11-27 snapshot.",
        "constraints": "The one-membership rule rejects zero or several memberships, so the tenant is taken from the licence. A JWT-shaped token that fails JWT verification never falls through to licence introspection. The licence service is on the request path, so its verdict is cached for at most 60 seconds, and an unreachable service refuses with `503 licence_unavailable` rather than admitting. The cache is keyed by the token hash, never by tenant.",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. The agent platform is consulted as the consumer, and the tenant admin is informed because the tool allowlist is the authorization surface.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the 2026-11-09 delivery was recorded only in the slice plan and the evidence file. The statement records what the tree and the suites prove today and names the deferred live step instead of implying it happened.",
        "scope": "In scope are `_meta.operation` on every registered tool, the licence introspection in `crates/auth/src/machine.rs` with its 60-second capped cache and fail-closed rule, `Principal.kind` across human, agent and service, `approvals.request` over `insert_approval_tx`, and the end-to-end `/mcp` session in `crates/api/tests/mcp_agent.rs`. Out of scope are the `fr` toolkit, MCP tasks (S06c), the copilot surface (S10) and any signed HMAC channel (S06a, superseded)."
      },
      "created_at": "2026-11-28T07:26:11.487462+00:00",
      "id": "01a2210c-1c22-792c-8f82-18dc219fba9c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06b Agent Interface (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:26:11.809378+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2210e-f7bb-70ff-8d4d-5d6f216ed585",
      "approved_at": "2026-11-28T07:29:18.922843+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is a scripted session: open a conversation, ask for a card move, receive a proposal and one pending approval with zero action invocations; approve as an authorized member and see exactly one invocation, the card moved and the turn completed citing that invocation; reconnect with `Last-Event-ID` and receive the turn once; see a low-scope principal's two surfaces omit the same tool; see an unavailable router fail the turn with zero invocations; and see the coverage gate fail an unmapped operation by name. The local proofs are `crates/api/tests/copilot_session.rs`, `copilot_proposal.rs`, `copilot_approve.rs`, `copilot_replay.rs`, `copilot_surface.rs`, `copilot_turn.rs` and `crates/actions/tests/copilot_coverage.rs`.",
        "assignment": "The architecture work assigns S10 the copilot's one action surface: project the S04 registry by principal, turn a destructive tool call into an S06 approval, run the conversation as two tables and an event stream, and keep model access outside the conveyor. This statement governs the retrospective record of work built in the S10 slice and re-read at the 2026-11-27 snapshot.",
        "constraints": "S10-D1 keeps the surface a query over `ActionSpec`, so no second registry or tool table exists. S10-D2 makes a proposal an approval created through S06's `insert_approval_tx`, with one pending proposal per turn enforced by the partial unique index on `(tenant_id, meta->>'turn_id')`. S10-D3 makes execution a job with a conditional claim, a 120-second lease, re-checked proposer scopes and the proposal idempotency key. S10-D4 makes only terminal turns durable, with the terminal meta, the proposal and the `conversation_turn.created` event committed together. S10-D5 makes `completions.create` fail closed: an outage, timeout or open breaker ends the turn `failed` and runs no tool. S10-D6 declares `copilot.chat` the only coverage exemption. S10-D7 keeps AG-UI as a serializer over the native stream.",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. S12 is consulted as the renderer of the declared screens, operations is consulted on model availability and key custody, and the tenant admin is consulted because the channel grants and the approval decision are the authorization surface.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the S10 row was created on 2026-11-12 and the build was recorded in the slice plan and the receipt. The statement records what the tree and the suites prove today, names the pre-pack receipt as red on lifecycle checks, and leaves the deployed UI and router proof to S12 and S14 instead of implying it happened.",
        "scope": "In scope are the `conversation` and `conversation_turn` tables with forced row-level security, the turn runner and its eight-round cap, the `agent_surface` projection over the action registry, proposals carried as `approval(kind='copilot.action')` with sealed inputs, the conditional execution claim and sweep, the SSE topic `conversation:{id}` with `Last-Event-ID` replay, `completions.create` as an external action with a 60-second timeout and breaker, the AG-UI serializer, and the screens declared as data for S12. Out of scope are inference-router (D14), the `/mcp` transport and scope filtering (S04), the approval state machine (S06), fr's 34 AG-UI action modules, the agent's principal and tool contract (S06b), and chat UI rendering (S12)."
      },
      "created_at": "2026-11-28T07:29:18.626402+00:00",
      "id": "01a2210e-f724-7ebf-8b08-74d6c0f24f4c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S10 Copilot / AG-UI (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:29:18.922843+00:00",
      "version": 2
    },
    {
      "approval_id": "01a22110-1c69-7458-a9be-c5550ac1ee8a",
      "approved_at": "2026-11-28T07:30:33.839885+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is author \u2192 publish \u2192 execute a process run: an SOP and a three-step process are created and published, a card starts the run, the second step parks on an approval, the approval resumes it, and the run finishes with three succeeded run_step rows, non-empty input and output, and one approval. The negative proofs are an unresolved template (422 refusal, never an empty string), an unknown action refused at publish, and a retry that appends a second attempt row. The local proof is the suite set in crates/actions/tests plus the domain unit tests; the plan's \u00a77 two-process rehearsal is the live gate and is recorded as not executed in this pack.",
        "assignment": "The architecture work assigns S11 the executable-documentation capability: define a versioned process as an ordered list of steps, give each step a registry action and a resolved input template, attach a versioned SOP to each step, validate the whole draft at publish, and execute a published process as a run with one immutable run_step per attempt. This statement governs the retrospective record of work built on 2026-11-09 and re-read at the 2026-11-27 snapshot.",
        "constraints": "The store forbids a second published or draft row per (tenant_id, key). Publish validates in one transaction against the live manifest and refuses an unknown action, a step naming process_runs.*, a forward step reference, a retry on a non-idempotent action, an unresolved gate kind and an SOP whose inspection is empty. The resolver is one pure function with the roots run, steps, card and procedure; an absent path refuses and a null resolves. The executor claims a queued step with a status-guarded update, persists the resolved input at claim, and re-invokes a replayed attempt under the same idempotency key derived from (run_id, position, attempt).",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the store, resolver, actions and executor evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. The process author is consulted because tenant authorship is the capability's consumer, and S13 and S12 are informed because they consume the run tree and the declared read models.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the 2026-11-09 delivery was recorded in the slice plan and in the test tree. The statement records what the tree and the suites prove today and names the deferred live gate, the zero seed and the S15 seam instead of implying they happened.",
        "scope": "In scope are migrations 0015 (procedure, procedure_step, run_step), 0016 (the draft index), 0017 (lock_version), 0021 (the run capability seam), 0022 (the kata_executor role) and 0035 (the rename to process/process_step, the SOP table and process_map); the process, procedure (SOP) and process_map actions; publish validation and the frozen revision; the resolver in crates/domain/src/template.rs; and the executor in crates/actions/src/process/step.rs dispatched as the process.step job. Out of scope are matrix legality (S03), approval transport (S06), sub-processes and the engine (S13), screens (S12), PDL (never) and the deferred default seed."
      },
      "created_at": "2026-11-28T07:30:33.564273+00:00",
      "id": "01a22110-1bde-74d0-8b3e-e693e6c7291b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S11 Processes, Steps and SOPs (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:30:33.839885+00:00",
      "version": 2
    },
    {
      "approval_id": "01a222a6-2960-71b2-8edc-797253376758",
      "approved_at": "2026-11-28T14:54:04.782673+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks, each with its own target: exactly one task-capable action read from the compiled registry; the project-scoped `inputRequests`; the answer written as `approvals.decide`; a terminal result that never says `failed` on the wire; one `-32602` for every ownership failure; and the parity suite proving the task surface and the HTTP action agree.",
        "assignment": "Deliver the architectural record for durable agent tasks: a client that declares the MCP task extension drives a long-running project \u2014 starting it, polling it, answering the approval it waits on and reading a terminal result \u2014 without a second store and without a service between the client and the conveyor. The assignment is retrospective: the product shipped on 2026-11-09 and this statement records the architecture it was built to.",
        "constraints": "No new table and no new service. The run state machine is the task lifecycle, so the reaper had to learn to skip project executions and the one-live-run property had to become a partial unique index because the existing index does not dedupe nulls. Every constraint in this project is a consequence of the reuse decision rather than an independent choice.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the agent interface; the MCP client author is the consumer whose conformance the contract is written against. The retrospective author is not the accepting role, so the separation of duties holds even though one session wrote the product record and this set.",
        "retrospective": "This statement is written after the fact and declares so. The slice plan's build record gives the shipped state; the gate's compiled-registry assertion is the one the plan had to correct, because its first form grepped a flag no serialised surface carries. That correction is recorded here rather than tidied away.",
        "scope": "In scope: the Phase A\u2013D artefact set, the task-to-run identity, the project-scoped `inputRequests` listing, the answer path through `approvals.decide`, and the two indexes migration 0023 adds. Out of scope: the agent's own task support against external MCP servers, decommissioning `mcp_server`, SSE or push delivery, a progress percentage, and any new table."
      },
      "created_at": "2026-11-28T14:54:04.488688+00:00",
      "id": "01a222a6-28cb-7ab4-b430-124b3ec82cb8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06c MCP Tasks (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:54:04.782673+00:00",
      "version": 2
    },
    {
      "approval_id": "01a222a7-1881-7d2e-a3b5-928c86cb2b76",
      "approved_at": "2026-11-28T14:55:06.005722+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks: the capability aggregate resolves with its executor and versioned binding; readiness refuses a run with `409 capability_not_ready` before the first step; a cited passage comes from a pinned strata pack; memory is written, consolidated into `strata_rag` and recalled by `source_id`; and a degraded source mid-run reports `knowledge_degraded`, not the pre-run refusal.",
        "assignment": "Deliver the architectural record for a capability aggregate: a capability is a persisted thing with an executor spec, a versioned knowledge binding and memory, which a card's run resolves and readiness-checks before its first step. The assignment is retrospective \u2014 steps 1\u20134 shipped on 2026-11-09 and step 5's actions remain open \u2014 so this statement records the architecture and the remaining work together.",
        "constraints": "No second loop and no second retrieval path. The conveyor owns the binding and strata owns retrieval, so a run that read knowledge by any route other than `knowledge.query` would make the pinned pack advisory. Every new table carries a tenant column with row-level security forced, and documents are immutable \u2014 ingest a new version and prune the old, never update in place.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the capability model; the capability author writes the pack and its readiness declaration; the retrieval owner operates the strata pack. The retrospective author is not the accepting role, so the separation of duties holds.",
        "retrospective": "This statement is written after the first four build steps and before the fifth, which makes it unusually literal: it records what exists and what does not rather than reconstructing a finished shape. The seven remaining actions, the unproved pack-bound retrieval and the unproved run-transactional memory write are named as open work.",
        "scope": "In scope: the Phase A\u2013D artefact set, the capability tables and their versioning, the pre-run readiness check, the pinned retrieval binding, and the memory write, consolidation and recall. Out of scope: the model loop (S13's `agent.run`), the interactive copilot surface (S10), embeddings and answer generation (the executor's), a tenant-authored prompt store, KAG, and S08's dropped `project_capability` ledger."
      },
      "created_at": "2026-11-28T14:55:05.705673+00:00",
      "id": "01a222a7-17ec-751f-bc1c-9e932f9b4763",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S15 Capability, Knowledge and Memory (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:55:06.005722+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Scope

The boundaries, the artefacts expected and the phase set.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a222a7-1881-7d2e-a3b5-928c86cb2b76",
      "approved_at": "2026-11-28T14:55:06.005722+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks: the capability aggregate resolves with its executor and versioned binding; readiness refuses a run with `409 capability_not_ready` before the first step; a cited passage comes from a pinned strata pack; memory is written, consolidated into `strata_rag` and recalled by `source_id`; and a degraded source mid-run reports `knowledge_degraded`, not the pre-run refusal.",
        "assignment": "Deliver the architectural record for a capability aggregate: a capability is a persisted thing with an executor spec, a versioned knowledge binding and memory, which a card's run resolves and readiness-checks before its first step. The assignment is retrospective \u2014 steps 1\u20134 shipped on 2026-11-09 and step 5's actions remain open \u2014 so this statement records the architecture and the remaining work together.",
        "constraints": "No second loop and no second retrieval path. The conveyor owns the binding and strata owns retrieval, so a run that read knowledge by any route other than `knowledge.query` would make the pinned pack advisory. Every new table carries a tenant column with row-level security forced, and documents are immutable \u2014 ingest a new version and prune the old, never update in place.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the capability model; the capability author writes the pack and its readiness declaration; the retrieval owner operates the strata pack. The retrospective author is not the accepting role, so the separation of duties holds.",
        "retrospective": "This statement is written after the first four build steps and before the fifth, which makes it unusually literal: it records what exists and what does not rather than reconstructing a finished shape. The seven remaining actions, the unproved pack-bound retrieval and the unproved run-transactional memory write are named as open work.",
        "scope": "In scope: the Phase A\u2013D artefact set, the capability tables and their versioning, the pre-run readiness check, the pinned retrieval binding, and the memory write, consolidation and recall. Out of scope: the model loop (S13's `agent.run`), the interactive copilot surface (S10), embeddings and answer generation (the executor's), a tenant-authored prompt store, KAG, and S08's dropped `project_capability` ledger."
      },
      "created_at": "2026-11-28T14:55:05.705673+00:00",
      "id": "01a222a7-17ec-751f-bc1c-9e932f9b4763",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S15 Capability, Knowledge and Memory (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:55:06.005722+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

```json
{
  "category": "statement-of-architecture-work",
  "count": 38,
  "rows": [
    {
      "approval_id": null,
      "approved_at": "2026-11-19T07:50:28.603416+00:00",
      "approved_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "body": {
        "acceptance_criteria": [
          "D1 \u2014 the artefact standard and repository: a catalog's entries are rows, an approved artefact is immutable and a supersede writes version 2, and a reuse link is searchable across plateaus.",
          "D2 \u2014 the Preliminary capability: the tailored profile names each phase's mandatory set by plateau, the board is a governance body row, and a maturity assessment carries all nine dimensions and a skills assessment all seven categories.",
          "D3 \u2014 the architecture project and phase gates: an approved artefact set and a live contract advance a phase, a draft artefact refuses the advance naming the artefact, and a contract change re-opens the gate.",
          "D4 \u2014 delivery compliance: a design gate without a live contract refuses, a superseded contract refuses naming its status, and one compliance level per review and a dated waiver without an owner are refused.",
          "D5 \u2014 change management: a review's exactly-one outcome is a change row or a kata candidate, and a review with no verdict cannot close the phase.",
          "D6 \u2014 the console surface: the Architecture destination renders its tabs from the live aggregates and each interaction records state."
        ],
        "assignment": "Deliver the TOGAF ADM capability as rows and actions in the one workspace: the artefact standard and repository, the Preliminary capability, the architecture project and its phase gates, delivery compliance, change management, and its console surface.",
        "responsibilities": "The programme architect (the maintainer sessions under the owner's authority) delivers and evidences each deliverable; the Architecture Board reviews and approves; the programme owner accepts the capability.",
        "scope": "The eleven tables of migration 0054 and the 26 architecture.* actions that expose them; the four ADM contract kinds; the seeded ten-type artefact standard; the Architecture Board; the maturity and skills baselines; and the Architecture capability screen (D6). The design is docs/plans/T9-architecture-capability.md."
      },
      "created_at": "2026-11-19T07:42:46.140382+00:00",
      "id": "01a1f2c2-0e1a-787f-9011-46b424d1ec71",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d35-7bd1-945d-cea12c5dfd6d",
      "requested_by": null,
      "status": "superseded",
      "superseded_by_id": "01a2023e-4e59-75ad-b2ab-fd7d014511d2",
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 the TOGAF ADM capability (T9)",
      "updated_at": "2026-11-22T07:52:45.884024+00:00",
      "version": 3
    },
    {
      "approval_id": "01a1fdd2-74eb-7fe3-a424-cdf02d021931",
      "approved_at": "2026-11-21T11:16:38.828544+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/P0.toml is green against the live record at the published SHA",
        "assignment": "Rework the programme definition project P0 by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526).",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "manifest": "acceptance/programme/P0.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the 21 programme-definition outputs, the charter and scope baseline, the benefit and governance baselines, the tranche-0 record and the closure review"
      },
      "created_at": "2026-11-21T11:16:21.046667+00:00",
      "id": "01a1fdd2-5041-7f80-8032-3305751d498c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1dcb8-d24b-76de-b901-56fd56e001bc",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 P0 programme definition (retrospective, U5a)",
      "updated_at": "2026-11-21T11:16:38.828544+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff2f-c76b-7bfe-b71a-a39294a95644",
      "approved_at": "2026-11-21T17:38:09.721121+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S01.toml is green against the live record at the published SHA; the decision record marks the unmeasured values not recoverable.",
        "assignment": "Rework the UI-stack spike by the reconstituted lifecycle (U5a, #526); the decision itself was made 2026-11-08.",
        "manifest": "acceptance/programme/S01.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "ADR 0001 and its measured table: interaction TTI, bundle, first render, drag frames, SSE\u2192DOM, memory, cold build and auth."
      },
      "created_at": "2026-11-21T17:36:38.122067+00:00",
      "id": "01a1ff2e-7974-74cd-be6d-377e3c0071a4",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-724c-9bae-af96b2d926b1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S01 (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:38:09.721121+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff37-f693-7336-8c6d-fe98f6f5c7b7",
      "approved_at": "2026-11-21T17:47:05.975622+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S00.toml is green against the live record at the published SHA; the inventory's 244 dropped rows are the gate's evidence.",
        "assignment": "Rework the usage inventory project by the reconstituted lifecycle (U5a, #526); the inventory itself was delivered 2026-11-07/08.",
        "manifest": "acceptance/programme/S00.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The 1,300-row parity inventory: 218 used, 295 rare, 543 unused, 244 dropped, across fr (798), RM (314), Planka (96) and n8n (92)."
      },
      "created_at": "2026-11-21T17:46:54.771712+00:00",
      "id": "01a1ff37-e240-779b-aab5-74a3fa86ecad",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S00 (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:47:05.975622+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff3c-8f0f-7326-b8a7-2de4d2eb6597",
      "approved_at": "2026-11-21T17:52:06.626934+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "Four, each with its check: (1) the public site renders the programme from Postgres \u2014 `data/snapshot.js` carries the programme's rows and the public sidebar names it only when the snapshot does (#470, 7d4e0d27); (2) a published output appears at its stable URL \u2014 165 committed output pages, every one named by `docs/design/docs/outputs.js` (#516, 06ba0fe3); (3) signed out there is no API request and no write control \u2014 `node docs/design/scripts/check-console.cjs`; (4) the pack's claims are checked like code \u2014 `build-coverage.py --check`, `build-docs.py --check` and the surface checks in the `design-pack` job of `.github/workflows/ci-public.yml`. The project's own gate is `acceptance/programme/DESIGN-PUBLISH.toml`.",
        "assignment": "Deliver to the programme owner, the Architecture Board and every outside reader the published studio design pack that makes the SPEC_V2 operating model readable and challengeable from a browser, and keep it live from the application image. This is the Phase A architecture work of a programme-level project in tranche 0, held to `docs/design/STUDIO-SPEC.md`. Recorded retrospectively on 2026-11-21: the pack was built and published by units #32, #33, #36, #37, #89, #97, #103 and #149, all closed.",
        "manifest": "acceptance/programme/DESIGN-PUBLISH.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The Phase A artefact set at plateau psa \u2014 stakeholder map (matrix), driver/goal/objective catalog, requirements catalog and solution concept (diagram) \u2014 with the Phase B\u2013D catalogues: business capability map, data entity catalog and technology standards. The published surfaces: `studio.html` offline, `https://conveyor.nonagentic.ai/design/` from the app image, `/design/data/snapshot.js` read from the tenant's rows, and `/design/docs/output-<id>.html` for every published output. Out of scope: any V2 backend aggregate, any signed-out write path, and the retired homepage mirror."
      },
      "created_at": "2026-11-21T17:51:55.933191+00:00",
      "id": "01a1ff3c-7aad-7dda-820e-1c7d85fe70a8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 DESIGN-PUBLISH: the published studio design pack",
      "updated_at": "2026-11-21T17:52:06.626934+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff41-2c98-7646-b835-7c7a96e1ca6e",
      "approved_at": "2026-11-21T17:57:08.762631+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest `acceptance/programme/SPEC-V2.toml` is green: the closure review exists, every declared closure output (`evidence`, `closure-review`) is published, and the project re-closes through the one gate decided by an approver other than its requester. The Phase-G compliance review records exactly one standard level against this contract and cites the published standing delegation. The seven A\u2013D artefacts are approved rows with their entries; no outcome-level claim substitutes for them.",
        "assignment": "Produce and publish SPEC V2 as the programme's adopted operating model, and prove it through the programme's executable acceptance. The work is the document and its decision set \u2014 `docs/SPEC_V2.md`, decisions D28\u2013D63 \u2014 with the ownership, organisational, work, decision, tool and rigour models it defines. This Statement of Architecture Work is recorded retrospectively on 2026-11-21 for the tranche-0 rework (issue #526) under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the project's real, contemporaneous statement was never recorded and is not recoverable.",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The V2 document and its decisions, and the phase A\u2013D architecture artefacts this contract governs: the stakeholder map, the driver/goal/objective catalog, the requirements catalog and the solution concept (phase A); the business capability map (phase B); the data entity/data component catalog (phase C); the technology standards catalog (phase D). Not in scope: V1's S-slices (not re-opened), the V2 delivery slices T0\u2013T17 (their own projects), and `docs/SPEC_LEGACY_KAG.md` (D54)."
      },
      "created_at": "2026-11-21T17:56:58.859998+00:00",
      "id": "01a1ff41-19f9-77bb-aa4a-f416f0d758e3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 SPEC-V2, the organisational operating model (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:57:08.762631+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff45-c6f7-7a7d-ab9f-0c9c8293147e",
      "approved_at": "2026-11-21T18:02:10.160888+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T0.toml is green against the live record at the published SHA: a job is created, a release is published, a binding pins its hash, a run on a retired binding refuses 409 job_binding_stale, no concept has two stores, and a test-only third executor kind registers without editing the run path. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T0 (SPEC_V2 \u00a712, issue #34) by the reconstituted TOGAF/MSP lifecycle: produce and publish the Phase A\u2013D mandatory artefacts, the charter and the delivery contract, the Phase-G compliance review and the closure review, declare the closure outputs and re-close through the one gate.",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/T0.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the model and its invariants (SPEC_V2 \u00a73 P1\u2013P11, \u00a74.1\u2013\u00a74.6, \u00a710.1, \u00a710.3): the organisation spine \u2014 industry, ordered departments, job_catalogue, the immutable job_release and the tenant-scoped job_binding; the decision-ladder types; and the internal Executor contract. The action contracts are those the registry serves and served-spec.lock.json pins. Execution behaviour is T2; the plan spine is T1; the tool platform is T13."
      },
      "created_at": "2026-11-21T18:02:00.126911+00:00",
      "id": "01a1ff45-b2ce-7276-8a88-cf3d744e7976",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7734-98da-fb7f1e2386c7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T0 Model and contracts (retrospective, U5a)",
      "updated_at": "2026-11-21T18:02:10.160888+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff84-57c3-7f67-9229-94bf0188732d",
      "approved_at": "2026-11-21T19:10:30.370289+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S02.toml is green against the published snapshot at the recorded SHA, with the S2 gate proven: two tenants isolated including RLS, and login works. The long-job lease/recovery proof under two workers was issue #2's P2; it is proved by `apps/server/tests/jobs_lease_two_workers.rs` at d367a4a9, and issue #2 closes with the lane PR.",
        "assignment": "Rework the delivered S02 skeleton by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526, tranche 1), recording the A\u2013D artefacts, the delivery contract and the Phase-G compliance review, and closing the project once the long-job proof issue #2 named is landed in this batch.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The ten-member workspace and one `conveyor` binary delivered by S02: eleven platform tables in `migrations/0001_platform.sql`, forced RLS on the nine tenant-owned tables, OIDC and development authentication, the fail-closed `app_licence` gate, the fenced job runner, the replaying `/api/events` stream, and the declared `shell/login` surface for S12."
      },
      "created_at": "2026-11-21T19:10:20.569282+00:00",
      "id": "01a1ff84-4425-75d9-8772-08e09cf71069",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S02 Skeleton (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:10:30.370289+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff89-1794-79d0-8707-f7614987ee7c",
      "approved_at": "2026-11-21T19:15:42.114015+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T1 gate: one project with a baseline, a change request and a refused self-approval; WSJF recomputes from inputs; the portfolio view lists the in-flight set; a dependency cycle is refused; a benefit profile without baseline, target, method or owner is refused; a benefit-map edge names its five levels. The ten issue-#72 scenarios are proved by the named real-Postgres tests in crates/actions/tests/plan_spine.rs and crates/api/tests/approval_dual.rs; the rework's acceptance instrument is acceptance/programme/T1.toml. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T1 (SPEC_V2 \u00a712, issue #72) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the plan spine and programme-definition rows of SPEC_V2 \u00a712: wbs_node, project_baseline, project_register_item, project_dependency, programme_cadence, programme_tranche, benefit_profile, benefit_link, benefit_transition, stakeholder_profile, programme_baseline, and the widened governed_asset kinds; the spine action families; and the requester/executor exclusion in approvals.decide. Out of scope: the decision ladder (T2), the PM job (T3), cadence materialisation and PI planning and the benefits-realization procedure (T10), metrics (T11), and any migration of V1's outline tree."
      },
      "created_at": "2026-11-21T19:15:31.506201+00:00",
      "id": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T1 Plan spine (retrospective, U5a)",
      "updated_at": "2026-11-21T19:15:42.114015+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff8d-b4cd-74f9-9f32-2b9f6cae33cd",
      "approved_at": "2026-11-21T19:20:43.741951+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T11.toml is green against the published snapshot at the re-close: every metric and every metering level recomputes from rows; the reconciliation with billing_service is exact at six decimals or opens an incident; a C0\u2013C5 level moves only when the measured evidence crosses its recorded threshold, naming that threshold version; a gate that never refused is flagged. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T11 (SPEC_V2 \u00a77.5, \u00a710.4, \u00a712; issues #84 and #458) by the reconstituted TOGAF/MSP lifecycle: produce and approve the Phase A\u2013D mandatory artefacts, this Statement of Architecture Work, the delivery contract, the Phase-G compliance review and the closure review; publish the declared outputs; and re-close the project through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The metrics, metering and maturity capability delivered by #84 (commit 183f5111, 2026-11-19) and the live Metering screen delivered by #458 (commit 0a8a6e95, 2026-11-20): migration 0059_metrics.sql, crates/domain/src/metrics.rs, crates/store/src/metrics.rs, crates/actions/src/metrics/mod.rs, the eight metrics actions, the billing.summary reconciliation input, the incident card type, the served-spec lock and the console screen. Charging, eval/benchmark-ledger quality metrics, knowledge metrics without ledgers and per-tool health are out of scope."
      },
      "created_at": "2026-11-21T19:20:34.079029+00:00",
      "id": "01a1ff8d-a0ad-7a91-b026-d9a2575daa3f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T11 Metrics, metering and maturity (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:20:43.741951+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2005a-8c6e-708a-84df-ab16d175072e",
      "approved_at": "2026-11-21T23:04:28.797665+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-21T23:04:18.681795+00:00",
      "id": "01a2005a-7886-7f2d-9b49-53d174544ee9",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:14.119814+00:00",
      "version": 3
    },
    {
      "approval_id": "01a20061-d19f-7269-9f5c-113f3aeb0001",
      "approved_at": "2026-11-21T23:12:26.539242+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC \u00a78 S3 gate: proptest on transitions and schema review. Property (a) \u2014 no transition leaves list.kind and archived_at contradictory, proved by board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error. Property (b) \u2014 claim not granted twice without release or expiry, proved as tested by claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic, with the lease-exclusivity half recorded as the P1 package v1-s03-1. Property (c) \u2014 one pending approval per (run, gate), proved by approval_race.rs::sixteen_racers_create_exactly_one_pending_approval. Property (d) \u2014 terminal run and dense revision, proved by transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense. Tenant isolation is proved by rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema review against data-arch \u00a72.3/\u00a72.4 admits the P2 package v1-s03-2. The rework's acceptance instrument is acceptance/programme/S03.toml; the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V1 slice S03 (SPEC \u00a76, \u00a78 S3) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate on the P0-only gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the baseline domain of SPEC \u00a75.3 and \u00a76: the fourteen tables of migrations/0002_domain.sql (programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state), the may_move matrix and the run/approval/asset state machines, the store `_tx` modules with TenantId, and the catalogue_entry.kind vocabulary. Out of scope: every route and action (S04\u2013S06), the board satellites (S05), channels and delivery (S06), outlines, quality and project_request (S07), models, views and portfolio registers (S08), documents and secrets (S09), conversations (S10), procedures (S11), workflow (S13) and the capability columns (S15)."
      },
      "created_at": "2026-11-21T23:12:15.079018+00:00",
      "id": "01a20061-bd73-7a89-b08c-bd8b867eb061",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S03 Domain and schema (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T23:12:26.539242+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20204-34e9-7eea-87d1-5b8265816c62",
      "approved_at": "2026-11-22T06:49:22.301988+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/PLATFORM.toml is green against the live record at the published SHA; the register's PLATFORM P0s are cleared or owned by a named executable package.",
        "assignment": "Hold the programme's definition, standards, gates and record; keep every project's acceptance manifest executable.",
        "manifest": "acceptance/programme/PLATFORM.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The programme capability: the one tenant, the programme definition set, the tranche plan, the artefact standard and the acceptance manifests; the programme record as reworked under #526."
      },
      "created_at": "2026-11-22T06:48:35.177183+00:00",
      "id": "01a20203-883b-7231-b0d3-dee948cf90e4",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work - PLATFORM (programme capability, retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T06:49:22.301988+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2023f-11ac-7fe6-bf6f-101ced8f4ba2",
      "approved_at": "2026-11-22T07:53:45.939115+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": [
          "D1 \u2014 the artefact standard and repository: a catalog's entries are rows, an approved artefact is immutable and a supersede writes version 2, and a reuse link is searchable across plateaus.",
          "D2 \u2014 the Preliminary capability: the tailored profile names each phase's mandatory set by plateau, the board is a governance body row, and a maturity assessment carries all nine dimensions and a skills assessment all seven categories.",
          "D3 \u2014 the architecture project and phase gates: an approved artefact set and a live contract advance a phase, a draft artefact refuses the advance naming the artefact, and a contract change re-opens the gate.",
          "D4 \u2014 delivery compliance: a design gate without a live contract refuses, a superseded contract refuses naming its status, and one compliance level per review and a dated waiver without an owner are refused.",
          "D5 \u2014 change management: a review's exactly-one outcome is a change row or a kata candidate, and a review with no verdict cannot close the phase.",
          "D6 \u2014 the console surface: the Architecture destination renders its tabs from the live aggregates and each interaction records state."
        ],
        "assignment": "Deliver the TOGAF ADM capability as rows and actions in the one workspace: the artefact standard and repository, the Preliminary capability, the architecture project and its phase gates, delivery compliance, change management, and its console surface.",
        "responsibilities": "The programme architect (the maintainer sessions under the owner's authority) delivers and evidences each deliverable; the Architecture Board reviews and approves; the programme owner accepts the capability.",
        "scope": "The eleven tables of migration 0054 and the 26 architecture.* actions that expose them; the four ADM contract kinds; the seeded ten-type artefact standard; the Architecture Board; the maturity and skills baselines; and the Architecture capability screen (D6). The design is docs/plans/T9-architecture-capability.md."
      },
      "created_at": "2026-11-22T07:52:45.884024+00:00",
      "id": "01a2023e-4e59-75ad-b2ab-fd7d014511d2",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d35-7bd1-945d-cea12c5dfd6d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": "01a1f2c2-0e1a-787f-9011-46b424d1ec71",
      "title": "Statement of Architecture Work - the TOGAF ADM capability (T9) - re-issued 2026-11-22",
      "updated_at": "2026-11-22T07:53:45.939115+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20256-80ac-72f0-a310-d114488f1d7c",
      "approved_at": "2026-11-22T08:19:17.038367+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": [
          {
            "criterion": "HTTP and MCP cannot diverge",
            "id": "a",
            "proof": "All 20 (16 internal + 4 fixtures.*) both ways; stripping request_id/duration_ms, http.body == mcp.structuredContent."
          },
          {
            "criterion": "One value, three projections",
            "id": "b",
            "proof": "spec_hash: manifest == resolved operation == MCP tool."
          },
          {
            "criterion": "Duplicates fail closed",
            "id": "c",
            "proof": "Exit 1 on a duplicate name, an MCP collision or an out-of-charset name."
          },
          {
            "criterion": "Schema-invalid input",
            "id": "d",
            "proof": "RFC 9457 with populated errors[], never a 500."
          },
          {
            "criterion": "Idempotent replay",
            "id": "e",
            "proof": "Two identical POSTs with one key: same status and body, one action_invocation."
          },
          {
            "criterion": "The lock catches drift; breaking ingest refused",
            "id": "f",
            "proof": "A renamed field makes --check-lock exit 1 naming the operation; the removed-field fixture exits 2 (422) while the approved version serves."
          }
        ],
        "assignment": {
          "depends_on": [
            "S02",
            "S03"
          ],
          "gate": "SPEC section 8 S4 - 20 actions identical over HTTP and MCP",
          "issue": "#13",
          "plan_status": "planned",
          "programme_project": "S04",
          "spec_sections": [
            "section 5.5",
            "section 7",
            "section 8 S4",
            "section 3 D11"
          ],
          "title": "Action registry - derive, manifest, OpenAPI, MCP, invocation, external ingest"
        },
        "manifest": {
          "adds_actions": [
            "actions.list",
            "actions.describe",
            "me.get",
            "tenants.get",
            "tenants.update_settings",
            "members.list",
            "members.set_role",
            "departments.list",
            "departments.create",
            "audit.list",
            "jobs.list",
            "jobs.retry",
            "schedules.list",
            "schedules.upsert",
            "events.list",
            "licences.get",
            "fixtures.echo",
            "fixtures.slow",
            "fixtures.fail",
            "fixtures.timeout"
          ],
          "adds_events": [],
          "adds_modules": [
            "actions"
          ],
          "adds_tables": [
            "action_registry"
          ],
          "deliverables": [
            "crates/actions-derive/src/lib.rs",
            "crates/actions/src/{spec,registry,invoke,mcp,openapi,lock,ingest}.rs",
            "crates/actions/tests/{registry,parity_http_mcp,idempotency,timeout,ingest_diff,external_actions}.rs",
            "crates/actions/tests/fixtures/external/openapi.json",
            "crates/{api,store}/src/actions.rs",
            "migrations/0003_action_registry.sql",
            "served-spec.lock.json"
          ],
          "source_pins": {
            "agent": "83b8de7e",
            "fr": "4a579e91",
            "fr_source_map": "6f7c5097",
            "pinned": "2026-11-07"
          }
        },
        "responsibilities": [
          {
            "responsibility": "Build crates/actions-derive and crates/actions, wire the boot assertion before TcpListener::bind, and add migrations/0003_action_registry.sql.",
            "role": "S04 delivery"
          },
          {
            "responsibility": "Supplies begin_scoped, audit_event, action_invocation, idempotency_key, event and /api/me.",
            "role": "S02"
          },
          {
            "responsibility": "Supplies the 14 domain aggregates and transitions used by the 16 internal actions.",
            "role": "S03"
          },
          {
            "responsibility": "Add domain actions and their own events in the same transaction; later slices add actions, never a surface.",
            "role": "S05-S11"
          },
          {
            "responsibility": "Provides the kind: workflow engine; until then workflow returns 501.",
            "role": "S13"
          },
          {
            "responsibility": "Supplies channel-mcp delivery rows and the signed mcp_server channel (S06-alpha).",
            "role": "S06"
          }
        ],
        "retrospective": "Planned, not yet delivered. Decisions closed here: S4-D1 derive crate is the 10th member; S4-D2 one literal route per action plus a spec_exempt fallback; S4-D3 enable/disable is approved_at; S4-D4 an external name colliding with any name is refused at ingest; S4-D5 idempotency scope is the action name; S4-D6 scope-filtered MCP tools with the refused tool absent; S4-D7 workflow returns 501 until S13. Traps to carry: four artifacts drifting is the fr lesson; Router::merge drops paths; MCP tool names are not action names; a timeout is the caller's; idempotency and the mutation must share a transaction; circular actions hit invocation_depth 8 and 409 action-recursion-limit.",
        "scope": "IN: ActionSpec plus #[action]; the compile-time registry and boot assertion; GET /api/actions; POST /api/actions/{name}/invocations with idempotency, timeout, audit, outbox; rmcp at /mcp; the OpenAPI fold and lock; openapiv3 ingest; migrations/0003_action_registry.sql; 16 internal plus 4 fixtures.* actions. OUT: domain actions (cards.move, ...) are S05-S11's; kind: workflow (S13); channel-mcp delivery rows (S06); the signed mcp_server channel (S06-alpha)."
      },
      "created_at": "2026-11-22T08:19:08.182233+00:00",
      "id": "01a20256-6ede-7d70-aae8-5dbfe681a1b7",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-7eb6-a62a-f420e742595c",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work - S04 Action registry (retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T08:19:17.038367+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20273-4f0f-77e6-8005-9744d1e3a574",
      "approved_at": "2026-11-22T08:50:49.577591+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": "# Statement of Architecture Work \u2014 T13 Tool platform\n\n## Context\nSPEC_V2 \u00a711 replaces ad-hoc backend calls with one governed path. The conveyor already owns actions, secrets, entitlements, audit and a durable queue, so a second catalogue/queue/runner (the predecessor MCP server) is refused (D44, D59). T13 is the slice that makes the one path real.\n\n## Scope\nThe broker `tools.call` and its seven steps; the four catalogue/ledger tables; effect-class gating; the closed-schema input check; credential aliases over sealed secrets; the direct MinIO storage adapter with the mandatory tenant prefix; the coding toolkit and one kag-era toolkit; V1 external-action migration; six gate proofs.\n\n## Approach\nImplement the broker and storage layers in the conveyor workspace, prove each refusal before dispatch at the wire (a counting stub), record one `tool_call` and one `tool.called` event per call, and record every artefact of this rework retrospectively from \u00a711 and the tests rather than from memory.\n\n## Deliverables\n`crates/actions/tests/tool_platform.rs` (the six proofs and the extra broker tests); `conveyor_actions::storage::{JobStorage, TENANT_PREFIX, sha256_hex}`; `conveyor_store::tool::{calls_for_run, ToolCallRow}`; `conveyor_actions::prompt::compile_for_audit`; the reused `conveyor_store::secret::create_secret_tx`; tables `tool`, `toolkit`, `tool_binding`, `tool_call`; action `tools.call`; event `tool.called`.\n\n## Constraints and standards\nRust workspace; PostgreSQL 18; MinIO (S3-compatible) as the platform store; a closed JSON-Schema subset (additionalProperties false, enums, patterns, bounds); OpenAPI 3.1 and streamable-HTTP MCP as the only two BYO forms; idempotency keys on writes and bounded polls.\n\n## Acceptance\nThe six \u00a712 clauses and their named tests, as written in `acceptance/programme/T13.toml`; the manifest is the gate, not this document.\n\n## Risks\nThe suite serialises because the destination URL is process-global; the storage test path is test-only until live MinIO wiring exists; the catalogue write path is deferred to T14; a production binding writer is still absent.\n\n## Decisions closed by this rework\nThe binding row is the broker's authority; refusals are proved on the wire; `tool_call` is not `action_invocation`; the storage adapter builds every key, so traversal and hash mismatches are refused.\n",
      "created_at": "2026-11-22T08:48:20.084131+00:00",
      "id": "01a20271-2a37-7329-b159-28031ddcb966",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T13 Tool platform (retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T08:50:49.577591+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20287-813e-7abd-83c5-ddf3bb9e98b3",
      "approved_at": "2026-11-22T09:12:50.020166+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T10 gate: PI planning materialises cards from the backlog by WSJF within WIP; a deliverable without evidence cannot demo; retro produces lessons; an end-of-tranche review with no baselines, benefit status or risk evidence is refused; a transition that never measured its baseline cannot report the benefit realized. Clauses 2, 3 and 5 are proved by named tests; clause 1 is proved for a project backlog and not for the programme-wide backlog (v1-p82-5); clause 4 is unproved at HEAD because free-text placeholders satisfy the evidence check (v1-p83-1) and the baseline requirement is not gated (v1-p83-4). The rework's acceptance instrument is `acceptance/programme/T10.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T10 (SPEC_V2 sections 8.2 and 8.3 and section 12, issues #82 and #502) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the SAFe cadence and the MSP tranche machinery of SPEC_V2 sections 8.2 and 8.3: the cadence state machine, WSJF and WIP-bounded materialisation behind an approved PI plan, backlog refinement, the demo credit, the retro lesson requirement, the tranche review and its evidence, and the benefit transition with its baseline measurement. Out of scope: the decision ladder (T2), the plan-spine baseline contract (T1), the coach card (T4) except the retro card, metrics (T11) and the one-organisation-loop acceptance (T12)."
      },
      "created_at": "2026-11-22T09:12:38.058104+00:00",
      "id": "01a20287-696c-76a4-a2db-28c6092b5813",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-70c1-a5e8-f50956c5e3f9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T10 SAFe cadence and MSP tranches (retrospective, U5a)",
      "updated_at": "2026-11-22T09:12:50.020166+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2028b-f63c-7c27-b580-d4cba2c1e70f",
      "approved_at": "2026-11-22T09:17:42.179607+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T4 gate: an injected failure yields exactly one coach card and one remedy; a repeat signature escalates; no coach card about a coach card; a systemic class opens a kata. Clauses 2 and 3 are proved by named tests in `crates/actions/tests/coach.rs`; clause 1 is proved for a first interruption but refuted as a universal by v1-p6-4; clause 4 is proved only as a `kata.open` process start and v1-p6-2 shows no kata record, instrument or threshold is created. The rework's acceptance instrument is `acceptance/programme/T4.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T4 (SPEC_V2 sections 6.2, 6.4 and 12, issue #75) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the interruption taxonomy, the closed remedy set, per-signature uniqueness and the depth cap of SPEC_V2 section 6.2, plus the systemic-class path of section 6.4: the `coach_interruption` and `coach_remedy` tables, the `coach.interrupt` action, the seeded coach process and the real-Postgres suite `crates/actions/tests/coach.rs`. Out of scope: the board and pull path's `err.card` producer, the kata's own lifecycle and measurement (T5), the PM job (T3), operations (T7) and the tool platform (T13)."
      },
      "created_at": "2026-11-22T09:17:30.02765+00:00",
      "id": "01a2028b-dde6-7ed4-9e47-6e582f2341d8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T4 Kanban coach (retrospective, U5a)",
      "updated_at": "2026-11-22T09:17:42.179607+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20ce0-644d-7340-a696-1af10fa8c61e",
      "approved_at": "2026-11-24T09:26:06.517885+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The six \u00a712 gate clauses plus D1\u2013D8; the live gate recorded in docs/plans/T19/evidence.md; the closure decision in docs/plans/T19/closure-review.md.",
        "assignment": "Build T19 \u2014 decision forums \u2014 to the \u00a712 gate: a meeting is a card whose agenda items resolve approvals, whose participants hold typed positions, and whose outcomes are derived arithmetic over those positions.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the SRO accepts the benefit B7 and the tranche evidence.",
        "scope": "The meeting aggregate, the approval_decision participant extension, the minutes governed-asset kind, the twelve meetings.* actions, the Meetings destination and the two consumer seams (governance_body, programme_cadence)."
      },
      "created_at": "2026-11-24T09:25:57.096759+00:00",
      "id": "01a20ce0-5183-776c-84bd-327f2301b19b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b9b8-7214-abc6-f2c4f75a1fc1",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T19 decision forums (2026-11-24)",
      "updated_at": "2026-11-24T09:26:06.517885+00:00",
      "version": 2
    },
    {
      "approval_id": "01a210bd-cdb2-7f2f-ac9a-c314fb1856af",
      "approved_at": "2026-11-25T03:26:48.7374+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The T19F plan's P1\u2013P5 scenarios, the live gate recorded in docs/plans/T19F/evidence.md, and the closure review in docs/plans/T19F/closure-review.md that defers project closure to the tranche-6 review.",
        "assignment": "Build T19F \u2014 the forum bindings \u2014 to the plan's P1\u2013P5: a meeting_type version is written only by the reviewed authoring route, a board subject is decided in a sitting with via='meeting' rows, and a cadence row materialises its own meeting exactly once.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the programme manager owns B7's measurement; the SRO accepts the tranche-6 evidence.",
        "scope": "Migration 0073; the authoring publish hook and meetings.type_list/type_get; architecture.board_sit_open/board_sit_decide; cadences.materialise; the mock-UI and real-app surfaces; the programme record and its evidence."
      },
      "created_at": "2026-11-25T03:26:38.984805+00:00",
      "id": "01a210bd-ba25-78c1-a363-3eea5aa71335",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T19F forum bindings (2026-11-25)",
      "updated_at": "2026-11-25T03:26:48.7374+00:00",
      "version": 2
    },
    {
      "approval_id": "01a217fb-a503-7db9-b47b-55a111c1c8de",
      "approved_at": "2026-11-26T13:11:37.614131+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate in SPEC_V2 \u00a712 is executable: a PM runs a request end to end; the PM cannot approve its own gate; a baseline change requires a change request. The close gate reads the project's declared outputs \u2014 project-brief and evidence \u2014 plus the closure-review output, a live board-approved delivery contract, a Phase-G review citing the standing delegation, the approved scope baseline, the seven mandatory A\u2013D artefacts with their registers, WBS inputs 8, 8, 5, 3, a demo credit per accepted deliverable and at least one tranche lesson. This content pack is checked offline by scripts/rework-content-check.py --project T3 before the factory reads it, and the factory then records the contract, review, artefacts and closure against the snapshot.",
        "assignment": "Rework T3 by the reconstituted TOGAF/MSP lifecycle: author the charter, this Statement of Architecture Work, the seven mandatory Phase A\u2013D artefacts, the delivery contract, the WBS deliverable and the Phase-G review, and re-publish the project-brief and evidence outputs. The architecture work is the PM job itself \u2014 one project run from charter through WBS, scope baseline, staffing, gate routing and closure, with the approver kept distinct from the requester and every baseline change carried by a change request \u2014 recorded retrospectively and dated 2026-11-26.",
        "contract": "This statement is authored retrospectively under the delegation and is approved through the same board gate it describes; its approval is recorded by the factory from this pack, not asserted here. It is complemented by the delivery contract whose commitments, compliance and approval fields this pack authors, on the approved scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa (kind scope, taken 2026-11-26, approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545), and by the Phase-G review the factory records from this pack. The tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9 decided continue on 2026-11-26 and authorised tranche 4; the reopen this pack answers is dated 2026-11-26T07:44:41Z and its reason is the missing contract, review and baseline record.",
        "responsibilities": "The programme architect authors the pack under the #526 rework delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14; the Architecture Board reviews the seven A\u2013D artefacts, which this pack records as approved. The programme owner is the approver of the delivery contract and of the closure: the scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa is approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545, and the closure gate recorded for T3 was decided by an approver distinct from its requester, which is the gate's own rule. The PM runs the request and may not approve a gate it raised.",
        "retrospective": true,
        "scope": "In scope: the charter and its approved gate; the WBS with the four typed WSJF inputs 8, 8, 5, 3; the scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa taken on 2026-11-26 and approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545; the staffing plan; the gate router that refuses self-approval and records every decision; the change request that must precede a baseline change; the closure record with its closure-review output; the seven A\u2013D artefacts and the project registers. Out of scope: the T2 decision ladder's policy engine, the T4 coach, the T5 kata, the T13 tool platform, the T1 plan spine's implementation and every V1 slice; the rework completes the record and adds no scope."
      },
      "created_at": "2026-11-26T13:11:37.331166+00:00",
      "id": "01a217fb-a475-7824-b01c-bfcfcb4680d3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7915-8833-75f4dc31cc07",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T3 PM job (retrospective, 2026-11-26)",
      "updated_at": "2026-11-26T13:11:37.614131+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2180f-168b-79af-af7f-e4cc18948e7b",
      "approved_at": "2026-11-26T13:32:51.863706+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-26T13:32:51.58027+00:00",
      "id": "01a2180f-15fe-794d-aba4-395c2f06a32f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:51.863706+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d8-8c6d-715e-ad85-da38d5f3351c",
      "approved_at": "2026-11-26T21:52:31.994413+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S05.toml` and plan \u00a77: (a) replay fidelity \u2014 1,000 cards/10 lists, `admin diff` prints zero deltas; (b) one winner \u2014 two clients with the same `If-Match` get one 200 and one 412 carrying the current representation, `version` +1; (c) 1k load \u2014 `GET /api/boards/{id}/cards?limit=200` p95 \u2264 120 ms at 20 rps, 1,000 cards in five pages \u2264 500 ms; (d) webhook shape \u2014 `{event,data,prevData,user}` exactly, `event` the internal register type and `data.list_id`/`prevData.list_id` the lanes; (e) SSE \u2014 a second client on `board:{id}` gets `card.moved` \u2264 200 ms live and on replay; (f) no second store \u2014 one `comment`, one `board_membership`, no `card_membership`, no `attachment`. The instrument is `acceptance/programme/S05.toml`; the closure gate is decided by an approver other than its requester. The snapshot records no Phase-G review of these properties; this rework records that gap rather than claiming a pass.",
        "assignment": "Rework the V1 slice S05 (SPEC \u00a78 S5) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the acceptance instrument.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the board slice of the plan: the nine tables (board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription), the 39 actions, the read projections and cursors, the SSE topics `board:{id}` and the resolved `card:{id}`, the webhook emitter and its `webhook.deliver` job, and the key-referenced `board-export/v1` CLI. Out of scope: attachments and card-level memberships, claims/assignee/stopwatch writes (S06), webhook delivery rows as actions, and the UI (S12)."
      },
      "created_at": "2026-11-26T21:52:31.682863+00:00",
      "id": "01a219d8-8bc7-71a2-8dbe-33702699eac1",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S05 Board (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:52:31.994413+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d9-303c-7fdc-b217-ec349e82329b",
      "approved_at": "2026-11-26T21:53:13.933646+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "Acceptance criteria": "Phase A closes when this statement is approved, the seven A\u2013D artefacts are approved in standard order, and the seven artefacts each carry metadata, entries, links and a document section with authored prose. Delivery acceptance is the plan \u00a77 gate: (a) three revisions with max(seq)=3 and a compare showing the status change; (b) an idempotent seed returning {0,0,N}; (c) both GIN indexes used under enable_seqscan=off with rising idx_scan; (d) relevance where B meets \u00a75.6's recall@5 \u2265 0.75, MRR \u2265 0.60 and B \u2265 C; (e) traceability in both directions with zero cross-tenant rows; (f) quality scoring read-only; (g) no orphan over the 28 RM modules. The record-level acceptance is `acceptance/programme/S07.toml`, whose 17 checks are 15 P0 and two P1.",
        "Assignment": "The assignment is to give the conveyor one governed requirements surface: requirements, principles and decisions share a single `governed_asset` lifecycle carried by S03, every mutation writes one immutable `revision`, `requirements.compare` diffs two revisions in Rust, quality is scored adviserily on a revision, and search ranks assets, cards and catalogue entries through SQL. The work is asked for by the tranche-4 sponsoring group and accepted by the Architecture Board; this statement is authored retrospectively on 2026-11-24 under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 for project 01a1d039-0d2c-7322-b2f9-4dd041153894.",
        "Contract": "The contract record is the delivery contract of project 01a1d039-0d2c-7322-b2f9-4dd041153894, version 1, approved by the Architecture Board on 2026-11-24 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. Its compliance clause is plan \u00a77 and the acceptance instrument `acceptance/programme/S07.toml`; its manifest names that instrument; its acceptance date is the snapshot of 2026-11-24, when the project row is closed at version 7 and tranche 4 (01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce) is authorised by the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9. The close gate is decided by an approver other than the requester.",
        "Responsibilities": "The programme architect (conveyor-e2e) is responsible for the plan, the seven artefacts, this statement, the delivery contract and the retrospective pack. The Architecture Board (conveyor-architecture-board) reviews and approves the artefact set and the Phase-G review. The S03 and S04 owners are consulted because `governed_asset`, `revision`, `catalogue_entry`, `trace_link` and the action registry are their surfaces; S08 and S12 are informed as consumers. The approver of the closure gate is deliberately a different principal from the requester under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14.",
        "Scope": "The phase set is A to D: the stakeholder map, the driver/goal/objective catalog, the requirements catalog, the solution concept diagram, the business capability map, the data entity/data component catalog and the technology standards catalog. The boundary is the plan \u00a75 material: the five \u00a75.1 tables, the 32 actions, the `search` module, the catalogue seed, the quality rubric and job, the eval set and the traceability actions. The project does not build the TOC API, ArchiMate or UML renders, project CRUD, batch bulk operations, RM's promote/project-copy or any UI, and it does not recreate `outline`/`outline_node`, which migration 0011 dropped."
      },
      "created_at": "2026-11-26T21:53:13.637379+00:00",
      "id": "01a219d9-2fa8-703a-99f2-fb7e7572e467",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S07 Requirements and search (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:53:13.933646+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d9-f717-73ba-9c86-18adeeecd4ef",
      "approved_at": "2026-11-26T21:54:04.836362+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The plan \u00a77 gate: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, then delete yields GET 404 and object_store::head NotFound, asserted in documents_roundtrip; (b) cap \u2014 10,485,760 bytes accepted and one byte more refused 413 application/problem+json with a stable type in documents_limits; (c) secret \u2014 rotate to version 2 and GET returns no plaintext, value key or ciphertext in secrets_never_revealed; (d) workers \u2014 the page renders live rows and a stopped service yields a banner with the breaker open after five failures; (e) reports \u2014 every number equals its SQL count on the same snapshot in reports_match_sql; (f) externals \u2014 GET /api/actions carries timeout_ms on every external and zero reqwest:: outside the executor in externals_declared; (g) billing \u2014 no write action and no metering event in the manifest. The same instrument also requires migration 0013 applied twice as a no-op, FORCE RLS on all four tables, and the nine declared event types. The snapshot records no run of these properties.",
        "assignment": "Rework the V1 slice S09 (SPEC \u00a78 S9) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, this charter, this Statement of Architecture Work and the delivery contract, take the scope baseline with one WBS deliverable, record a retrospective Phase-G review at level consistent, and publish the expected outputs project-brief and evidence. Because all nine linked issues are recorded closed, this rework re-closes the project at level consistent through the acceptance manifest, with the closure and closure review dated 2026-11-24.",
        "contract": "This Statement establishes the delivery contract against acceptance/programme/S09.toml, whose gate text names the seven properties above. Compliance is assessed clause by clause at the 2026-11-24 retrospective snapshot; the project's published evidence page is 01a1e687-7682-753d-b60e-1f00243b13a3 and its pre-rework audit pack is docs/evidence/rework/S09.json, which records verdict red on the missing baseline, charter, contract, Phase-G review and closure outputs. The rework supplies the lifecycle record and re-closes the project retrospectively on the P0 checks of the manifest; the remaining verification limits are carried in the closure review, the review and the evidence output rather than converted into a pass.",
        "responsibilities": "The programme architect (conveyor-e2e) owns the slice plan and this retrospective set; the Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract; the approver (conveyor-approver) decides the Phase-G level; the S03 team supplies tenancy, trace_link and revision, and the S04 team supplies the action registry with its OpenAPI and MCP projection. Operators own the KEK source in the pod environment and the MinIO bucket policy.",
        "scope": "the four tables document, repository, project_repository and secret in migrations/0013_documents.sql; the integrations module (object_store, external, breaker, ingest); documents.create/get/list/download/update/delete/report, 26 of the front matter's 27 actions in all; the nine event types; MinIO bucket conveyor-documents with key t/<tenant>/<document>/<version>; the KEK/HKDF/XChaCha20-Poly1305 secret envelope; repositories as references; the read-only workers, billing and observability displays; report queries over local tables; and the strata handoff. Out of scope: billing logic and metering writes, worker scheduling, artifact objects, code analysis, licence issuance, ingestion and embeddings as services, screens (S12) and project_request.repository_url (S07)."
      },
      "created_at": "2026-11-26T21:54:04.526424+00:00",
      "id": "01a219d9-f671-708a-b28f-063e7e79e6ec",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S09 Documents and integrations (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:54:04.836362+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219dd-4701-7342-9adf-f2c47331c4af",
      "approved_at": "2026-11-26T21:57:41.900644+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T14 gate, executed on the live estate through `acceptance/programme/T14.toml`: (a) each published toolkit has a probe and one live call; (b) `scientific_processor` is scaled up for its probe and one live call, then scaled back, with a receipt; (c) a zero-use report exists; (d) a backend outage unbinds new runs while in-flight runs finish. The snapshot carries no executed receipt for any of the four clauses, so this rework records them as the project's open verification item rather than a pass; the level written is consistent, never non_conformant without a waiver, and no waiver exists.",
        "assignment": "Rework the V2 slice T14 (SPEC_V2 \u00a712) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, record the Phase-G compliance review at the level the record supports, publish the expected outputs (`project-brief`, `evidence`) and leave the project active with its gate recorded as open verification rather than closing it.",
        "contract": "No live Statement of Architecture Work and no live delivery contract exist for T14 at the snapshot; this pair is supplied retrospectively by the 2026-11-24 rework and is the contract of record until a Board-approved one supersedes it. It binds the project to the four gate clauses above, to broker-only calls inherited from T13, to draft status for the named draft services, and to a receipt per claim; it does not authorise closure. The standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 is the authority under which the rework acts.",
        "responsibilities": "the conveyor-approver approves the Phase A\u2013D set and the delivery contract; the conveyor-architecture-board governs the standards; the programme architect (conveyor-e2e) owns T14 and this retrospective pack; the T13 tool-platform team owns the tool family, the broker, the effect classes and the credential profiles; the estate service owners grant the live-call window and the outage drill; the SRO is accountable for the tranche outcome.",
        "retrospective": "written by the 2026-11-24 tranche-4 rework; it reconstructs the record and does not attest contemporaneous intent.",
        "scope": "the estate-toolkit import wave on T13's tool platform: the `tool`, `toolkit`, `tool_binding` and `tool_call` rows for the coding fleet, strata, billing and licence reads, webscraper, media_discovery, scientific_processor, lean4_prover, video-compiler, the w2v orchestrator and the ops read tools; `evidence_review`, `discovery` :8807 and the planned AHTAPol services as `draft`; the probe and the one live call per published toolkit; the `scientific_processor` scale-up and scale-back receipt; the zero-use report; and the backend-outage unbind test. Out of scope: the broker itself (T13), tool authoring and BYO (T15), and the UI."
      },
      "created_at": "2026-11-26T21:57:41.6166+00:00",
      "id": "01a219dd-4673-70da-b701-3f43675b4fa6",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T14 Estate toolkits (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:57:41.900644+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219dd-edc0-7da7-a7fd-00eafabf0eae",
      "approved_at": "2026-11-26T21:58:24.588139+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The pack passes `python3 scripts/rework-content-check.py --project T16 --content docs/evidence/rework/tranche-4/rework-content-T16.json --snapshot /tmp/conv526-w1-snap/data --wsjf 10,8,7,8 --closes false` with exit 0; every cited identifier exists in the snapshot; the seven artefacts carry the existing titles, phase, plateau and kind in the required order; each artefact body carries Metadata, Entries, Links and Document as authored prose; the project brief and evidence carry their template sections with substance; and no closure key appears. The project's own gate remains acceptance/programme/T16.toml, and this review's verdict is consistent.",
        "assignment": "Under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, the retrospective completes T16's governance record against acceptance/programme/T16.toml for the procedure-ingestion slice of SPEC_V2 \u00a712. The assignment is record completion, not rework of the built capability: author the charter, the Phase A\u2013D artefact set, this Statement of Architecture Work, the delivery contract, the Phase-G review and the published project brief and evidence, and leave the project active for a separate close decision.",
        "contract": "The record is delivered under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 against scope baseline 01a216d0-f529-7bcb-81a7-d429d008d638 (approved 2026-11-24T16:58:21Z) and tranche 4 01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce. The delivery contract below records compliance, acceptance, approval, manifest and retrospective limits. Any change to scope or gate returns to the Architecture Board; no waiver is claimed or required, and the project stays active for the tranche-4 review to decide continue.",
        "responsibilities": "The programme architect authors the record under the delegation; the Architecture Board reviews contract and Phase-G conformance; procedure owners and publishers own the sources and the publication decision; delivery teams consume the ingestion and PDDL surface; tenants own their rows and sources; the programme owner owns the outcome and any close decision. scripts/rework-project.py performs the live writes, and scripts/rework-content-check.py refuses this pack before any write if it is structurally wrong or cites an identifier absent from the snapshot.",
        "scope": "In scope: SOP intake; RDF/OWL/Turtle as a strata knowledge pack bound by knowledge_pack_ref; BPMN, Visio and draw.io into PDDL procedure drafts; capability, agent and job design from an SOP; the source-review and human publication gates; re-ingest versioning; bound PDDL execution; and the governance records named in the assignment. Out of scope: any runtime RDF store, graph store or reasoner (decision D53, owner 2026-11-19), T17's tenant-wide generalisation, T19's meetings records, the /procedures destination work of issue #448, and new ingestion scope."
      },
      "created_at": "2026-11-26T21:58:24.307416+00:00",
      "id": "01a219dd-ed35-79eb-b898-3323854d5411",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d38-76fb-9394-b75934d16fcd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T16 SOP, RDF/OWL and process-document ingestion (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:58:24.588139+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219de-7e7e-72db-b751-a17bdd9b013d",
      "approved_at": "2026-11-26T21:59:01.639909+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T18 gate, judged by the fifteen checks of acceptance/programme/T18.toml: every destination route exists with its spec, each spec asserts empty, error, happy and one refusal, the coverage record reports exists only from a real route and a present spec with a *Gate reported as extend naming its owner, no production screen renders a fixture, and a signed-in tenant drives portfolio \u2192 programme \u2192 project \u2192 work package, organisation \u2192 release \u2192 binding, procedures intake \u2192 publish, tools call \u2192 ledger, improve \u2192 kata \u2192 adoption, operations incident \u2192 postmortem and the ADM phase rail from the app, with the T12 loop as the final spec.",
        "assignment": "Rework the V2 delivery slice T18 (SPEC_V2 \u00a710.5 and \u00a712 T18) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter, the Statement of Architecture Work and the delivery contract, take the scope baseline with the one WBS deliverable, record the retrospective Phase-G compliance review, publish the expected outputs project-brief, inception-analysis and architecture-design plus the evidence output, and leave the project active because T18-4 and T18-5 are not delivered. The work is retrospective: it reconstructs the governance record from the project row, the plan, the route table and the issue registry, and it attests no contemporaneous intent.",
        "contract": "The delivery contract record is requested and approved live by scripts/rework-project.py under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 and is titled 'Delivery contract \u2014 T18 The real V2 UI (retrospective, 2026-11-24)'. It commits the lane to the \u00a712 gate and to the two open units as typed dependencies, and it records that this retrospective neither closes the project nor waives any check. The contract's approval is retrospective; the historical snapshot has no live contract, which is one of the failed checks in the prior acceptance record.",
        "responsibilities": "The T18 lane under the programme architect (conveyor-e2e) is responsible for the route table, the screen modules, the specs and the coverage instrument. The Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract. The sponsoring group is accountable for the tranche-4 authorisation and for keeping the project active. The owning slices T4, T5, T7, T9, T10, T11, T13, T16 and T17 are responsible for the actions each destination calls, and T12 owns the final organisation loop.",
        "retrospective": true,
        "scope": "the shell, route registry and navigation; the ten V2 destinations of SPEC_V2 \u00a710.5 and the execution room; the L1\u2013L4 delivery view; one Playwright spec per destination; the V2 coverage instrument measured from apps/web/src/app.rs and the named specs; and the T12 organisation loop as the final acceptance. Out of scope: new tables, actions, events or ports; SSR, a second router or frontend, or a mobile app; retiring or rebuilding the design pack; the V1 routes, which are linked rather than re-implemented; and any production fixture or live tenant write inside the retrospective."
      },
      "created_at": "2026-11-26T21:59:01.357241+00:00",
      "id": "01a219de-7def-7827-8a21-98428640f502",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T18 The real V2 UI (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:59:01.639909+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21a39-aae9-7d90-9069-b0576ed463d4",
      "approved_at": "2026-11-26T23:38:36.773008+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S06.toml` and plan \u00a77: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 `cards.move(done)` yields `run.succeeded`, `approval.approved` and one audit, invocation and event row each; (b) dual control \u2014 `required_decisions=2`, one approve stays pending, the same approver again is 409, a second approver closes; (c) retry \u2014 `kill -9` mid-run leads to `run.reap`, a crashed run, then `attempt=2` with two rows and never two live runs; (d) andon \u2014 engaged gives 409 and a pending `run.execute` for 60 s, released resumes within one poll; (e) delivery retry \u2014 a stub 500 gives `attempts=5`, `state=failed`, one row; (f) no timers \u2014 `rg 'setInterval' crates apps` is empty and every cadence is a `schedule` row; (g) human vs agent \u2014 `decided_by_kind` is `human` for one decision and `agent` for the other. The instrument is `acceptance/programme/S06.toml`; the close gate is decided by an approver other than its requester. The snapshot records no run of these properties and no Phase-G review; this rework records the level `non_conformant` rather than claiming a pass.",
        "assignment": "Rework the V1 slice S06 (SPEC \u00a78 S6) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure decision, and re-close through the acceptance instrument once its properties run.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the control-plane slice of the plan: the six tables of `02-DATA-ARCHITECTURE \u00a72.6` (`channel_binding`, `channel_preference`, `message`, `delivery`, `audience`, `audience_member` \u2014 the last two PENDING-S0), the 17 actions, the 17 events, the SSE topics `run:{id}`, `approval:{id}` and `tenant`, the Telegram in/out path with its sender-identity resolution, the andon gauge and poller pause, and the job kinds that replace 11 of fr's 14 timers (three deleted). Out of scope: the signed mcp_server channel (S06a), MCP tool surfacing (S10), `project_request.timeout` (S07), the UI (S12) and the audience screens (PENDING-S0)."
      },
      "created_at": "2026-11-26T23:38:36.518039+00:00",
      "id": "01a21a39-aa69-7167-8696-e615aa3b012f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06 Control Plane (retrospective, 2026-11-27)",
      "updated_at": "2026-11-26T23:38:36.773008+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c16-879a-765f-9e1b-c788a9c1ef4a",
      "approved_at": "2026-11-27T08:19:28.421371+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The S08 gate is the slice gate of SPEC \u00a78 S8: the Part A round-trip from principle to ADR to TOC to register to deliverable to capability to stakeholder, the TOC cycle refusal, the absence of a duplicate principles/decisions surface, three golden models in SVG/PNG/PDF, the profile's write-time 422, the XMI round-trip, the post-commit validation job, deterministic ids, the two retention producers and the render byte-route parity.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the S08 slice: the governance surface over S03's spine and the modelling surface over seven new tables, with a deterministic renderer and a profile-scoped XMI exchange. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the S08 slice plan.",
        "constraints": "The build must not introduce a second model store, must not use the registry as the validation authority, must not call load_system_fonts, must not query JSONB geometry in SQL, must not validate inside the mutation transaction, and must not trust an XMI document: size, depth, duplicate ids and a DOCTYPE are refused. These constraints are the plan's traps and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set, the contract and the profile; S03 supplies the spine tables and RLS; S04 supplies the registry and invocation route; S07 supplies the outline and the principles/decisions actions; S12 receives the declared trace/architecture surface; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the slice plan, the measured \u00a77.2 completion record and the record's project row. It records the assignment and the acceptance the build was measured against; it does not claim that a contemporaneous board approved this text on 2026-11-08, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are the 68 actions, the seven S08 tables, the two retention producers, the two byte routes and the seven Phase A\u2013D artefacts. Out of scope are the UI, model search, project CRUD, principle-conflict verdicts, architecture containers, change-set/review/publication splits, graph layout and OCL/Profile/SysML; each exclusion is named in the slice plan's \u00a73 so a later reader cannot mistake absence for omission."
      },
      "created_at": "2026-11-27T08:19:28.107129+00:00",
      "id": "01a21c16-86ee-7106-8eab-4155e8b4b141",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2c-7bad-8e6c-b5c5d1fd40e2",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S08 Governance and Modelling (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:19:28.421371+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c1b-b5de-788c-9918-d57c95016a17",
      "approved_at": "2026-11-27T08:25:07.941014+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The S17 gate is the slice gate of SPEC \u00a78 S17: the banked parity oracle 555/555, gold 50/50 and empty 0/50, the golden eval scripts 50/50, the exact McNemar vectors, RLS on all four tables, the fail-closed sandbox, deterministic Job names and the 1 MiB ConfigMap cap, no answers to producers, no benchmark action in AGENT_TOOLS, and the live sandbox lifecycle recorded on compute-1.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the S17 slice: one measurement loop over four tables, a hashed seed artifact, a pure grader and exact McNemar, a k8s Job sandbox and a single run driver. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the S17 slice plan.",
        "constraints": "The build must not import Python into the product or the image, must not let a producer see answers, must not widen the sandbox config to a default namespace, must not exceed the 1 MiB ConfigMap cap or the 256 KiB patch cap, must not let the conveyor arm start before S18, and must not repoint the grading node. These constraints are the plan's traps and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set, the contract and the measurement standard; S02 supplies the job/event/audit tables; S03 supplies the RLS spine and the document row; S04 supplies the registry and scopes; S09 supplies the object store the eval log is written to; S18 takes the conveyor arm; S12 receives the declared screens; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the slice plan, docs/evidence/S17-CORE.md and the record's project row. It records the assignment and the acceptance the build was measured against; it does not claim that a contemporaneous board approved this text on 2026-11-09, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are the four benchmark tables, the seed artifact and its image lock, the standalone seed generator, the grader and 12 parsers, the exact McNemar, the sandbox adapter, the gold/empty/baseline arms, the eight actions, two scopes, two events, and the declared S12 screens. Out of scope are the conveyor arm (S18), model hosting, HF calls, the old CLI, a second service and Python; each exclusion is named in the slice plan's \u00a73."
      },
      "created_at": "2026-11-27T08:25:07.66928+00:00",
      "id": "01a21c1b-b557-7b0a-ba9d-bc9a2de9247d",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d30-794f-bd53-c72986b18e60",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S17 Native Benchmark Harness (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:25:07.941014+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c1d-aa55-7091-ae4b-93328e410035",
      "approved_at": "2026-11-27T08:27:16.055256+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The T17 gate: a brief plus documents draft the catalogue; documents yield linked procedure and policy drafts; an unpublished dependency is refused; a reviewed catalogue publishes with its decisions; no asset publishes itself; a non-human decision cannot complete a gate; no asset widens a tool grant; and the real-app E2E creates a draft and sees the row render.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the T17 slice: the authoring lifecycle over two tables, a human review gate, a published-dependency check and a publish-through-the-target rule. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the T17 inception set and SPEC_V2 \u00a75.7/\u00a710.4\u2013\u00a710.5.",
        "constraints": "The build must not write the target tables, must not let an asset publish itself, must not let a non-human decision complete a gate, must not widen a tool grant, must not duplicate the governed_asset lifecycle and must not refuse the dependency order later than the draft attempt. These constraints are the inception brief's risks and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set and the contract; T3 supplies the decision ladder and the approval primitives; T16 owns live ingestion and the SOP human gate; T13 owns tools and bindings; the web team owns the Authoring screen and the E2E; the tenant authors and reviewers take the human decisions; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the inception set, the evidence and closure-review documents and the record's project row. It records the assignment and the acceptance the delivery was measured against; it does not claim that a contemporaneous board approved this text, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are intake, drafting, review and publication of organisation/department/job/procedure/policy assets, the two authoring tables, the state machine, the review gate, the published-dependency refusal and the Authoring screen. Out of scope are writing the target tables, the SOP/RDF/BPMN converter (T16), tools and bindings (T13), a governed_asset fork and a new template table except asset_template; each exclusion is named in the inception brief's Boundary."
      },
      "created_at": "2026-11-27T08:27:15.79094+00:00",
      "id": "01a21c1d-a9d1-70d8-8a6a-901c21e4dd7b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d38-7db1-b685-d76bd4cebf47",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T17 Tenant Asset Authoring (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:27:16.055256+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c26-afda-78c1-aea9-920275ed65c4",
      "approved_at": "2026-11-27T08:37:07.301597+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The \u00a712 T7 gate: an injected pod failure is triaged, mitigated by an approved write, resolved and postmortemed; a destructive action without the tenant's configured approvals is refused; restart, scale, rollback and flag-flip are callable. The executed suite maps each clause to a named test in T7-acceptance.md; the live cluster script is deploy-bound and is not claimed as run.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the T7 record: the incident lifecycle over the existing card and approval primitives, the four gated cluster writes, the postmortem/kata loop and the /operations console. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by SPEC_V2 \u00a76.5, \u00a79.2 and \u00a712 T7.",
        "constraints": "The build must not let an ops write reach the cluster without an approval, a recorded dry run and two distinct human approvers; must not let the mitigation wrapper bypass the gate; must not let an incident that needed no write stay unresolvable; must not widen a tool grant; and must not claim the live cluster proof before the deployed image carries the gate. These constraints are the finding acceptance and are carried into the contract.",
        "responsibilities": "The programme architect owns the record and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set and the contract; T2 supplies the job/event/audit rows; T4 supplies the registry and the destructive effect; T13 owns tools; the web team owns the /operations screen; the on-call operators take the incident decisions; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the SPEC row, the issue #78 delivery, the acceptance evidence and the two fixes at ca155fada. It records the assignment and the acceptance the delivery was measured against; it does not claim a contemporaneous board approval, and it carries the live-proof limitation into the Phase-G review.",
        "scope": "In scope are the five incident actions, the four write verbs and their gate, the k8s read family, the incident meta lifecycle and the /operations screen. Out of scope are tools and bindings (T13), the agent tool surface (the ops writes are not agent-reachable) and a second ops service; each exclusion is deliberate."
      },
      "created_at": "2026-11-27T08:37:06.989963+00:00",
      "id": "01a21c26-af30-7c3e-ad6d-39f7ae1937ea",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d34-7f89-a0be-f3de3c9f540e",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T7 Operations (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:37:07.301597+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2210c-1cd2-7eb0-a3d2-c90c96b7d6e7",
      "approved_at": "2026-11-28T07:26:11.809378+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is one real agent task over `/mcp`: create a project, read its progress, open and answer an approval, close the project, with `decided_by_kind='agent'` recorded, no `nonagenticai_mcp_server` component in the path, and no `X-MCP-Binding` header. The local proof is `crates/api/tests/mcp_agent.rs::one_licence_principal_task_creates_reads_decides_and_closes`; the deployed agent-app run is S14's gate and is recorded as deferred.",
        "assignment": "The architecture work assigns S06b the agent's direct interface to the conveyor: define the tool key every MCP tool carries, resolve a machine principal from a licence bearer, propagate that principal into attribution, and add the approval-request action an agent needs to escalate. This statement governs the retrospective record of work completed on 2026-11-09 and re-read at the 2026-11-27 snapshot.",
        "constraints": "The one-membership rule rejects zero or several memberships, so the tenant is taken from the licence. A JWT-shaped token that fails JWT verification never falls through to licence introspection. The licence service is on the request path, so its verdict is cached for at most 60 seconds, and an unreachable service refuses with `503 licence_unavailable` rather than admitting. The cache is keyed by the token hash, never by tenant.",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. The agent platform is consulted as the consumer, and the tenant admin is informed because the tool allowlist is the authorization surface.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the 2026-11-09 delivery was recorded only in the slice plan and the evidence file. The statement records what the tree and the suites prove today and names the deferred live step instead of implying it happened.",
        "scope": "In scope are `_meta.operation` on every registered tool, the licence introspection in `crates/auth/src/machine.rs` with its 60-second capped cache and fail-closed rule, `Principal.kind` across human, agent and service, `approvals.request` over `insert_approval_tx`, and the end-to-end `/mcp` session in `crates/api/tests/mcp_agent.rs`. Out of scope are the `fr` toolkit, MCP tasks (S06c), the copilot surface (S10) and any signed HMAC channel (S06a, superseded)."
      },
      "created_at": "2026-11-28T07:26:11.487462+00:00",
      "id": "01a2210c-1c22-792c-8f82-18dc219fba9c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06b Agent Interface (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:26:11.809378+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2210e-f7bb-70ff-8d4d-5d6f216ed585",
      "approved_at": "2026-11-28T07:29:18.922843+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is a scripted session: open a conversation, ask for a card move, receive a proposal and one pending approval with zero action invocations; approve as an authorized member and see exactly one invocation, the card moved and the turn completed citing that invocation; reconnect with `Last-Event-ID` and receive the turn once; see a low-scope principal's two surfaces omit the same tool; see an unavailable router fail the turn with zero invocations; and see the coverage gate fail an unmapped operation by name. The local proofs are `crates/api/tests/copilot_session.rs`, `copilot_proposal.rs`, `copilot_approve.rs`, `copilot_replay.rs`, `copilot_surface.rs`, `copilot_turn.rs` and `crates/actions/tests/copilot_coverage.rs`.",
        "assignment": "The architecture work assigns S10 the copilot's one action surface: project the S04 registry by principal, turn a destructive tool call into an S06 approval, run the conversation as two tables and an event stream, and keep model access outside the conveyor. This statement governs the retrospective record of work built in the S10 slice and re-read at the 2026-11-27 snapshot.",
        "constraints": "S10-D1 keeps the surface a query over `ActionSpec`, so no second registry or tool table exists. S10-D2 makes a proposal an approval created through S06's `insert_approval_tx`, with one pending proposal per turn enforced by the partial unique index on `(tenant_id, meta->>'turn_id')`. S10-D3 makes execution a job with a conditional claim, a 120-second lease, re-checked proposer scopes and the proposal idempotency key. S10-D4 makes only terminal turns durable, with the terminal meta, the proposal and the `conversation_turn.created` event committed together. S10-D5 makes `completions.create` fail closed: an outage, timeout or open breaker ends the turn `failed` and runs no tool. S10-D6 declares `copilot.chat` the only coverage exemption. S10-D7 keeps AG-UI as a serializer over the native stream.",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. S12 is consulted as the renderer of the declared screens, operations is consulted on model availability and key custody, and the tenant admin is consulted because the channel grants and the approval decision are the authorization surface.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the S10 row was created on 2026-11-12 and the build was recorded in the slice plan and the receipt. The statement records what the tree and the suites prove today, names the pre-pack receipt as red on lifecycle checks, and leaves the deployed UI and router proof to S12 and S14 instead of implying it happened.",
        "scope": "In scope are the `conversation` and `conversation_turn` tables with forced row-level security, the turn runner and its eight-round cap, the `agent_surface` projection over the action registry, proposals carried as `approval(kind='copilot.action')` with sealed inputs, the conditional execution claim and sweep, the SSE topic `conversation:{id}` with `Last-Event-ID` replay, `completions.create` as an external action with a 60-second timeout and breaker, the AG-UI serializer, and the screens declared as data for S12. Out of scope are inference-router (D14), the `/mcp` transport and scope filtering (S04), the approval state machine (S06), fr's 34 AG-UI action modules, the agent's principal and tool contract (S06b), and chat UI rendering (S12)."
      },
      "created_at": "2026-11-28T07:29:18.626402+00:00",
      "id": "01a2210e-f724-7ebf-8b08-74d6c0f24f4c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S10 Copilot / AG-UI (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:29:18.922843+00:00",
      "version": 2
    },
    {
      "approval_id": "01a22110-1c69-7458-a9be-c5550ac1ee8a",
      "approved_at": "2026-11-28T07:30:33.839885+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is author \u2192 publish \u2192 execute a process run: an SOP and a three-step process are created and published, a card starts the run, the second step parks on an approval, the approval resumes it, and the run finishes with three succeeded run_step rows, non-empty input and output, and one approval. The negative proofs are an unresolved template (422 refusal, never an empty string), an unknown action refused at publish, and a retry that appends a second attempt row. The local proof is the suite set in crates/actions/tests plus the domain unit tests; the plan's \u00a77 two-process rehearsal is the live gate and is recorded as not executed in this pack.",
        "assignment": "The architecture work assigns S11 the executable-documentation capability: define a versioned process as an ordered list of steps, give each step a registry action and a resolved input template, attach a versioned SOP to each step, validate the whole draft at publish, and execute a published process as a run with one immutable run_step per attempt. This statement governs the retrospective record of work built on 2026-11-09 and re-read at the 2026-11-27 snapshot.",
        "constraints": "The store forbids a second published or draft row per (tenant_id, key). Publish validates in one transaction against the live manifest and refuses an unknown action, a step naming process_runs.*, a forward step reference, a retry on a non-idempotent action, an unresolved gate kind and an SOP whose inspection is empty. The resolver is one pure function with the roots run, steps, card and procedure; an absent path refuses and a null resolves. The executor claims a queued step with a status-guarded update, persists the resolved input at claim, and re-invokes a replayed attempt under the same idempotency key derived from (run_id, position, attempt).",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the store, resolver, actions and executor evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. The process author is consulted because tenant authorship is the capability's consumer, and S13 and S12 are informed because they consume the run tree and the declared read models.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the 2026-11-09 delivery was recorded in the slice plan and in the test tree. The statement records what the tree and the suites prove today and names the deferred live gate, the zero seed and the S15 seam instead of implying they happened.",
        "scope": "In scope are migrations 0015 (procedure, procedure_step, run_step), 0016 (the draft index), 0017 (lock_version), 0021 (the run capability seam), 0022 (the kata_executor role) and 0035 (the rename to process/process_step, the SOP table and process_map); the process, procedure (SOP) and process_map actions; publish validation and the frozen revision; the resolver in crates/domain/src/template.rs; and the executor in crates/actions/src/process/step.rs dispatched as the process.step job. Out of scope are matrix legality (S03), approval transport (S06), sub-processes and the engine (S13), screens (S12), PDL (never) and the deferred default seed."
      },
      "created_at": "2026-11-28T07:30:33.564273+00:00",
      "id": "01a22110-1bde-74d0-8b3e-e693e6c7291b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S11 Processes, Steps and SOPs (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:30:33.839885+00:00",
      "version": 2
    },
    {
      "approval_id": "01a222a6-2960-71b2-8edc-797253376758",
      "approved_at": "2026-11-28T14:54:04.782673+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks, each with its own target: exactly one task-capable action read from the compiled registry; the project-scoped `inputRequests`; the answer written as `approvals.decide`; a terminal result that never says `failed` on the wire; one `-32602` for every ownership failure; and the parity suite proving the task surface and the HTTP action agree.",
        "assignment": "Deliver the architectural record for durable agent tasks: a client that declares the MCP task extension drives a long-running project \u2014 starting it, polling it, answering the approval it waits on and reading a terminal result \u2014 without a second store and without a service between the client and the conveyor. The assignment is retrospective: the product shipped on 2026-11-09 and this statement records the architecture it was built to.",
        "constraints": "No new table and no new service. The run state machine is the task lifecycle, so the reaper had to learn to skip project executions and the one-live-run property had to become a partial unique index because the existing index does not dedupe nulls. Every constraint in this project is a consequence of the reuse decision rather than an independent choice.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the agent interface; the MCP client author is the consumer whose conformance the contract is written against. The retrospective author is not the accepting role, so the separation of duties holds even though one session wrote the product record and this set.",
        "retrospective": "This statement is written after the fact and declares so. The slice plan's build record gives the shipped state; the gate's compiled-registry assertion is the one the plan had to correct, because its first form grepped a flag no serialised surface carries. That correction is recorded here rather than tidied away.",
        "scope": "In scope: the Phase A\u2013D artefact set, the task-to-run identity, the project-scoped `inputRequests` listing, the answer path through `approvals.decide`, and the two indexes migration 0023 adds. Out of scope: the agent's own task support against external MCP servers, decommissioning `mcp_server`, SSE or push delivery, a progress percentage, and any new table."
      },
      "created_at": "2026-11-28T14:54:04.488688+00:00",
      "id": "01a222a6-28cb-7ab4-b430-124b3ec82cb8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06c MCP Tasks (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:54:04.782673+00:00",
      "version": 2
    },
    {
      "approval_id": "01a222a7-1881-7d2e-a3b5-928c86cb2b76",
      "approved_at": "2026-11-28T14:55:06.005722+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks: the capability aggregate resolves with its executor and versioned binding; readiness refuses a run with `409 capability_not_ready` before the first step; a cited passage comes from a pinned strata pack; memory is written, consolidated into `strata_rag` and recalled by `source_id`; and a degraded source mid-run reports `knowledge_degraded`, not the pre-run refusal.",
        "assignment": "Deliver the architectural record for a capability aggregate: a capability is a persisted thing with an executor spec, a versioned knowledge binding and memory, which a card's run resolves and readiness-checks before its first step. The assignment is retrospective \u2014 steps 1\u20134 shipped on 2026-11-09 and step 5's actions remain open \u2014 so this statement records the architecture and the remaining work together.",
        "constraints": "No second loop and no second retrieval path. The conveyor owns the binding and strata owns retrieval, so a run that read knowledge by any route other than `knowledge.query` would make the pinned pack advisory. Every new table carries a tenant column with row-level security forced, and documents are immutable \u2014 ingest a new version and prune the old, never update in place.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the capability model; the capability author writes the pack and its readiness declaration; the retrieval owner operates the strata pack. The retrospective author is not the accepting role, so the separation of duties holds.",
        "retrospective": "This statement is written after the first four build steps and before the fifth, which makes it unusually literal: it records what exists and what does not rather than reconstructing a finished shape. The seven remaining actions, the unproved pack-bound retrieval and the unproved run-transactional memory write are named as open work.",
        "scope": "In scope: the Phase A\u2013D artefact set, the capability tables and their versioning, the pre-run readiness check, the pinned retrieval binding, and the memory write, consolidation and recall. Out of scope: the model loop (S13's `agent.run`), the interactive copilot surface (S10), embeddings and answer generation (the executor's), a tenant-authored prompt store, KAG, and S08's dropped `project_capability` ledger."
      },
      "created_at": "2026-11-28T14:55:05.705673+00:00",
      "id": "01a222a7-17ec-751f-bc1c-9e932f9b4763",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S15 Capability, Knowledge and Memory (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:55:06.005722+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Responsibilities

Who does the work and who accepts it.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a222a7-1881-7d2e-a3b5-928c86cb2b76",
      "approved_at": "2026-11-28T14:55:06.005722+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks: the capability aggregate resolves with its executor and versioned binding; readiness refuses a run with `409 capability_not_ready` before the first step; a cited passage comes from a pinned strata pack; memory is written, consolidated into `strata_rag` and recalled by `source_id`; and a degraded source mid-run reports `knowledge_degraded`, not the pre-run refusal.",
        "assignment": "Deliver the architectural record for a capability aggregate: a capability is a persisted thing with an executor spec, a versioned knowledge binding and memory, which a card's run resolves and readiness-checks before its first step. The assignment is retrospective \u2014 steps 1\u20134 shipped on 2026-11-09 and step 5's actions remain open \u2014 so this statement records the architecture and the remaining work together.",
        "constraints": "No second loop and no second retrieval path. The conveyor owns the binding and strata owns retrieval, so a run that read knowledge by any route other than `knowledge.query` would make the pinned pack advisory. Every new table carries a tenant column with row-level security forced, and documents are immutable \u2014 ingest a new version and prune the old, never update in place.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the capability model; the capability author writes the pack and its readiness declaration; the retrieval owner operates the strata pack. The retrospective author is not the accepting role, so the separation of duties holds.",
        "retrospective": "This statement is written after the first four build steps and before the fifth, which makes it unusually literal: it records what exists and what does not rather than reconstructing a finished shape. The seven remaining actions, the unproved pack-bound retrieval and the unproved run-transactional memory write are named as open work.",
        "scope": "In scope: the Phase A\u2013D artefact set, the capability tables and their versioning, the pre-run readiness check, the pinned retrieval binding, and the memory write, consolidation and recall. Out of scope: the model loop (S13's `agent.run`), the interactive copilot surface (S10), embeddings and answer generation (the executor's), a tenant-authored prompt store, KAG, and S08's dropped `project_capability` ledger."
      },
      "created_at": "2026-11-28T14:55:05.705673+00:00",
      "id": "01a222a7-17ec-751f-bc1c-9e932f9b4763",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S15 Capability, Knowledge and Memory (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:55:06.005722+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

```json
{
  "category": "statement-of-architecture-work",
  "count": 38,
  "rows": [
    {
      "approval_id": null,
      "approved_at": "2026-11-19T07:50:28.603416+00:00",
      "approved_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "body": {
        "acceptance_criteria": [
          "D1 \u2014 the artefact standard and repository: a catalog's entries are rows, an approved artefact is immutable and a supersede writes version 2, and a reuse link is searchable across plateaus.",
          "D2 \u2014 the Preliminary capability: the tailored profile names each phase's mandatory set by plateau, the board is a governance body row, and a maturity assessment carries all nine dimensions and a skills assessment all seven categories.",
          "D3 \u2014 the architecture project and phase gates: an approved artefact set and a live contract advance a phase, a draft artefact refuses the advance naming the artefact, and a contract change re-opens the gate.",
          "D4 \u2014 delivery compliance: a design gate without a live contract refuses, a superseded contract refuses naming its status, and one compliance level per review and a dated waiver without an owner are refused.",
          "D5 \u2014 change management: a review's exactly-one outcome is a change row or a kata candidate, and a review with no verdict cannot close the phase.",
          "D6 \u2014 the console surface: the Architecture destination renders its tabs from the live aggregates and each interaction records state."
        ],
        "assignment": "Deliver the TOGAF ADM capability as rows and actions in the one workspace: the artefact standard and repository, the Preliminary capability, the architecture project and its phase gates, delivery compliance, change management, and its console surface.",
        "responsibilities": "The programme architect (the maintainer sessions under the owner's authority) delivers and evidences each deliverable; the Architecture Board reviews and approves; the programme owner accepts the capability.",
        "scope": "The eleven tables of migration 0054 and the 26 architecture.* actions that expose them; the four ADM contract kinds; the seeded ten-type artefact standard; the Architecture Board; the maturity and skills baselines; and the Architecture capability screen (D6). The design is docs/plans/T9-architecture-capability.md."
      },
      "created_at": "2026-11-19T07:42:46.140382+00:00",
      "id": "01a1f2c2-0e1a-787f-9011-46b424d1ec71",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d35-7bd1-945d-cea12c5dfd6d",
      "requested_by": null,
      "status": "superseded",
      "superseded_by_id": "01a2023e-4e59-75ad-b2ab-fd7d014511d2",
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 the TOGAF ADM capability (T9)",
      "updated_at": "2026-11-22T07:52:45.884024+00:00",
      "version": 3
    },
    {
      "approval_id": "01a1fdd2-74eb-7fe3-a424-cdf02d021931",
      "approved_at": "2026-11-21T11:16:38.828544+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/P0.toml is green against the live record at the published SHA",
        "assignment": "Rework the programme definition project P0 by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526).",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "manifest": "acceptance/programme/P0.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the 21 programme-definition outputs, the charter and scope baseline, the benefit and governance baselines, the tranche-0 record and the closure review"
      },
      "created_at": "2026-11-21T11:16:21.046667+00:00",
      "id": "01a1fdd2-5041-7f80-8032-3305751d498c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1dcb8-d24b-76de-b901-56fd56e001bc",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 P0 programme definition (retrospective, U5a)",
      "updated_at": "2026-11-21T11:16:38.828544+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff2f-c76b-7bfe-b71a-a39294a95644",
      "approved_at": "2026-11-21T17:38:09.721121+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S01.toml is green against the live record at the published SHA; the decision record marks the unmeasured values not recoverable.",
        "assignment": "Rework the UI-stack spike by the reconstituted lifecycle (U5a, #526); the decision itself was made 2026-11-08.",
        "manifest": "acceptance/programme/S01.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "ADR 0001 and its measured table: interaction TTI, bundle, first render, drag frames, SSE\u2192DOM, memory, cold build and auth."
      },
      "created_at": "2026-11-21T17:36:38.122067+00:00",
      "id": "01a1ff2e-7974-74cd-be6d-377e3c0071a4",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-724c-9bae-af96b2d926b1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S01 (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:38:09.721121+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff37-f693-7336-8c6d-fe98f6f5c7b7",
      "approved_at": "2026-11-21T17:47:05.975622+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S00.toml is green against the live record at the published SHA; the inventory's 244 dropped rows are the gate's evidence.",
        "assignment": "Rework the usage inventory project by the reconstituted lifecycle (U5a, #526); the inventory itself was delivered 2026-11-07/08.",
        "manifest": "acceptance/programme/S00.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The 1,300-row parity inventory: 218 used, 295 rare, 543 unused, 244 dropped, across fr (798), RM (314), Planka (96) and n8n (92)."
      },
      "created_at": "2026-11-21T17:46:54.771712+00:00",
      "id": "01a1ff37-e240-779b-aab5-74a3fa86ecad",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S00 (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:47:05.975622+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff3c-8f0f-7326-b8a7-2de4d2eb6597",
      "approved_at": "2026-11-21T17:52:06.626934+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "Four, each with its check: (1) the public site renders the programme from Postgres \u2014 `data/snapshot.js` carries the programme's rows and the public sidebar names it only when the snapshot does (#470, 7d4e0d27); (2) a published output appears at its stable URL \u2014 165 committed output pages, every one named by `docs/design/docs/outputs.js` (#516, 06ba0fe3); (3) signed out there is no API request and no write control \u2014 `node docs/design/scripts/check-console.cjs`; (4) the pack's claims are checked like code \u2014 `build-coverage.py --check`, `build-docs.py --check` and the surface checks in the `design-pack` job of `.github/workflows/ci-public.yml`. The project's own gate is `acceptance/programme/DESIGN-PUBLISH.toml`.",
        "assignment": "Deliver to the programme owner, the Architecture Board and every outside reader the published studio design pack that makes the SPEC_V2 operating model readable and challengeable from a browser, and keep it live from the application image. This is the Phase A architecture work of a programme-level project in tranche 0, held to `docs/design/STUDIO-SPEC.md`. Recorded retrospectively on 2026-11-21: the pack was built and published by units #32, #33, #36, #37, #89, #97, #103 and #149, all closed.",
        "manifest": "acceptance/programme/DESIGN-PUBLISH.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The Phase A artefact set at plateau psa \u2014 stakeholder map (matrix), driver/goal/objective catalog, requirements catalog and solution concept (diagram) \u2014 with the Phase B\u2013D catalogues: business capability map, data entity catalog and technology standards. The published surfaces: `studio.html` offline, `https://conveyor.nonagentic.ai/design/` from the app image, `/design/data/snapshot.js` read from the tenant's rows, and `/design/docs/output-<id>.html` for every published output. Out of scope: any V2 backend aggregate, any signed-out write path, and the retired homepage mirror."
      },
      "created_at": "2026-11-21T17:51:55.933191+00:00",
      "id": "01a1ff3c-7aad-7dda-820e-1c7d85fe70a8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 DESIGN-PUBLISH: the published studio design pack",
      "updated_at": "2026-11-21T17:52:06.626934+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff41-2c98-7646-b835-7c7a96e1ca6e",
      "approved_at": "2026-11-21T17:57:08.762631+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest `acceptance/programme/SPEC-V2.toml` is green: the closure review exists, every declared closure output (`evidence`, `closure-review`) is published, and the project re-closes through the one gate decided by an approver other than its requester. The Phase-G compliance review records exactly one standard level against this contract and cites the published standing delegation. The seven A\u2013D artefacts are approved rows with their entries; no outcome-level claim substitutes for them.",
        "assignment": "Produce and publish SPEC V2 as the programme's adopted operating model, and prove it through the programme's executable acceptance. The work is the document and its decision set \u2014 `docs/SPEC_V2.md`, decisions D28\u2013D63 \u2014 with the ownership, organisational, work, decision, tool and rigour models it defines. This Statement of Architecture Work is recorded retrospectively on 2026-11-21 for the tranche-0 rework (issue #526) under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the project's real, contemporaneous statement was never recorded and is not recoverable.",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The V2 document and its decisions, and the phase A\u2013D architecture artefacts this contract governs: the stakeholder map, the driver/goal/objective catalog, the requirements catalog and the solution concept (phase A); the business capability map (phase B); the data entity/data component catalog (phase C); the technology standards catalog (phase D). Not in scope: V1's S-slices (not re-opened), the V2 delivery slices T0\u2013T17 (their own projects), and `docs/SPEC_LEGACY_KAG.md` (D54)."
      },
      "created_at": "2026-11-21T17:56:58.859998+00:00",
      "id": "01a1ff41-19f9-77bb-aa4a-f416f0d758e3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 SPEC-V2, the organisational operating model (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:57:08.762631+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff45-c6f7-7a7d-ab9f-0c9c8293147e",
      "approved_at": "2026-11-21T18:02:10.160888+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T0.toml is green against the live record at the published SHA: a job is created, a release is published, a binding pins its hash, a run on a retired binding refuses 409 job_binding_stale, no concept has two stores, and a test-only third executor kind registers without editing the run path. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T0 (SPEC_V2 \u00a712, issue #34) by the reconstituted TOGAF/MSP lifecycle: produce and publish the Phase A\u2013D mandatory artefacts, the charter and the delivery contract, the Phase-G compliance review and the closure review, declare the closure outputs and re-close through the one gate.",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/T0.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the model and its invariants (SPEC_V2 \u00a73 P1\u2013P11, \u00a74.1\u2013\u00a74.6, \u00a710.1, \u00a710.3): the organisation spine \u2014 industry, ordered departments, job_catalogue, the immutable job_release and the tenant-scoped job_binding; the decision-ladder types; and the internal Executor contract. The action contracts are those the registry serves and served-spec.lock.json pins. Execution behaviour is T2; the plan spine is T1; the tool platform is T13."
      },
      "created_at": "2026-11-21T18:02:00.126911+00:00",
      "id": "01a1ff45-b2ce-7276-8a88-cf3d744e7976",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7734-98da-fb7f1e2386c7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T0 Model and contracts (retrospective, U5a)",
      "updated_at": "2026-11-21T18:02:10.160888+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff84-57c3-7f67-9229-94bf0188732d",
      "approved_at": "2026-11-21T19:10:30.370289+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S02.toml is green against the published snapshot at the recorded SHA, with the S2 gate proven: two tenants isolated including RLS, and login works. The long-job lease/recovery proof under two workers was issue #2's P2; it is proved by `apps/server/tests/jobs_lease_two_workers.rs` at d367a4a9, and issue #2 closes with the lane PR.",
        "assignment": "Rework the delivered S02 skeleton by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526, tranche 1), recording the A\u2013D artefacts, the delivery contract and the Phase-G compliance review, and closing the project once the long-job proof issue #2 named is landed in this batch.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The ten-member workspace and one `conveyor` binary delivered by S02: eleven platform tables in `migrations/0001_platform.sql`, forced RLS on the nine tenant-owned tables, OIDC and development authentication, the fail-closed `app_licence` gate, the fenced job runner, the replaying `/api/events` stream, and the declared `shell/login` surface for S12."
      },
      "created_at": "2026-11-21T19:10:20.569282+00:00",
      "id": "01a1ff84-4425-75d9-8772-08e09cf71069",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S02 Skeleton (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:10:30.370289+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff89-1794-79d0-8707-f7614987ee7c",
      "approved_at": "2026-11-21T19:15:42.114015+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T1 gate: one project with a baseline, a change request and a refused self-approval; WSJF recomputes from inputs; the portfolio view lists the in-flight set; a dependency cycle is refused; a benefit profile without baseline, target, method or owner is refused; a benefit-map edge names its five levels. The ten issue-#72 scenarios are proved by the named real-Postgres tests in crates/actions/tests/plan_spine.rs and crates/api/tests/approval_dual.rs; the rework's acceptance instrument is acceptance/programme/T1.toml. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T1 (SPEC_V2 \u00a712, issue #72) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the plan spine and programme-definition rows of SPEC_V2 \u00a712: wbs_node, project_baseline, project_register_item, project_dependency, programme_cadence, programme_tranche, benefit_profile, benefit_link, benefit_transition, stakeholder_profile, programme_baseline, and the widened governed_asset kinds; the spine action families; and the requester/executor exclusion in approvals.decide. Out of scope: the decision ladder (T2), the PM job (T3), cadence materialisation and PI planning and the benefits-realization procedure (T10), metrics (T11), and any migration of V1's outline tree."
      },
      "created_at": "2026-11-21T19:15:31.506201+00:00",
      "id": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T1 Plan spine (retrospective, U5a)",
      "updated_at": "2026-11-21T19:15:42.114015+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff8d-b4cd-74f9-9f32-2b9f6cae33cd",
      "approved_at": "2026-11-21T19:20:43.741951+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T11.toml is green against the published snapshot at the re-close: every metric and every metering level recomputes from rows; the reconciliation with billing_service is exact at six decimals or opens an incident; a C0\u2013C5 level moves only when the measured evidence crosses its recorded threshold, naming that threshold version; a gate that never refused is flagged. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T11 (SPEC_V2 \u00a77.5, \u00a710.4, \u00a712; issues #84 and #458) by the reconstituted TOGAF/MSP lifecycle: produce and approve the Phase A\u2013D mandatory artefacts, this Statement of Architecture Work, the delivery contract, the Phase-G compliance review and the closure review; publish the declared outputs; and re-close the project through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The metrics, metering and maturity capability delivered by #84 (commit 183f5111, 2026-11-19) and the live Metering screen delivered by #458 (commit 0a8a6e95, 2026-11-20): migration 0059_metrics.sql, crates/domain/src/metrics.rs, crates/store/src/metrics.rs, crates/actions/src/metrics/mod.rs, the eight metrics actions, the billing.summary reconciliation input, the incident card type, the served-spec lock and the console screen. Charging, eval/benchmark-ledger quality metrics, knowledge metrics without ledgers and per-tool health are out of scope."
      },
      "created_at": "2026-11-21T19:20:34.079029+00:00",
      "id": "01a1ff8d-a0ad-7a91-b026-d9a2575daa3f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T11 Metrics, metering and maturity (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:20:43.741951+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2005a-8c6e-708a-84df-ab16d175072e",
      "approved_at": "2026-11-21T23:04:28.797665+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-21T23:04:18.681795+00:00",
      "id": "01a2005a-7886-7f2d-9b49-53d174544ee9",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:14.119814+00:00",
      "version": 3
    },
    {
      "approval_id": "01a20061-d19f-7269-9f5c-113f3aeb0001",
      "approved_at": "2026-11-21T23:12:26.539242+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC \u00a78 S3 gate: proptest on transitions and schema review. Property (a) \u2014 no transition leaves list.kind and archived_at contradictory, proved by board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error. Property (b) \u2014 claim not granted twice without release or expiry, proved as tested by claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic, with the lease-exclusivity half recorded as the P1 package v1-s03-1. Property (c) \u2014 one pending approval per (run, gate), proved by approval_race.rs::sixteen_racers_create_exactly_one_pending_approval. Property (d) \u2014 terminal run and dense revision, proved by transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense. Tenant isolation is proved by rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema review against data-arch \u00a72.3/\u00a72.4 admits the P2 package v1-s03-2. The rework's acceptance instrument is acceptance/programme/S03.toml; the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V1 slice S03 (SPEC \u00a76, \u00a78 S3) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate on the P0-only gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the baseline domain of SPEC \u00a75.3 and \u00a76: the fourteen tables of migrations/0002_domain.sql (programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state), the may_move matrix and the run/approval/asset state machines, the store `_tx` modules with TenantId, and the catalogue_entry.kind vocabulary. Out of scope: every route and action (S04\u2013S06), the board satellites (S05), channels and delivery (S06), outlines, quality and project_request (S07), models, views and portfolio registers (S08), documents and secrets (S09), conversations (S10), procedures (S11), workflow (S13) and the capability columns (S15)."
      },
      "created_at": "2026-11-21T23:12:15.079018+00:00",
      "id": "01a20061-bd73-7a89-b08c-bd8b867eb061",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S03 Domain and schema (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T23:12:26.539242+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20204-34e9-7eea-87d1-5b8265816c62",
      "approved_at": "2026-11-22T06:49:22.301988+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/PLATFORM.toml is green against the live record at the published SHA; the register's PLATFORM P0s are cleared or owned by a named executable package.",
        "assignment": "Hold the programme's definition, standards, gates and record; keep every project's acceptance manifest executable.",
        "manifest": "acceptance/programme/PLATFORM.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The programme capability: the one tenant, the programme definition set, the tranche plan, the artefact standard and the acceptance manifests; the programme record as reworked under #526."
      },
      "created_at": "2026-11-22T06:48:35.177183+00:00",
      "id": "01a20203-883b-7231-b0d3-dee948cf90e4",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work - PLATFORM (programme capability, retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T06:49:22.301988+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2023f-11ac-7fe6-bf6f-101ced8f4ba2",
      "approved_at": "2026-11-22T07:53:45.939115+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": [
          "D1 \u2014 the artefact standard and repository: a catalog's entries are rows, an approved artefact is immutable and a supersede writes version 2, and a reuse link is searchable across plateaus.",
          "D2 \u2014 the Preliminary capability: the tailored profile names each phase's mandatory set by plateau, the board is a governance body row, and a maturity assessment carries all nine dimensions and a skills assessment all seven categories.",
          "D3 \u2014 the architecture project and phase gates: an approved artefact set and a live contract advance a phase, a draft artefact refuses the advance naming the artefact, and a contract change re-opens the gate.",
          "D4 \u2014 delivery compliance: a design gate without a live contract refuses, a superseded contract refuses naming its status, and one compliance level per review and a dated waiver without an owner are refused.",
          "D5 \u2014 change management: a review's exactly-one outcome is a change row or a kata candidate, and a review with no verdict cannot close the phase.",
          "D6 \u2014 the console surface: the Architecture destination renders its tabs from the live aggregates and each interaction records state."
        ],
        "assignment": "Deliver the TOGAF ADM capability as rows and actions in the one workspace: the artefact standard and repository, the Preliminary capability, the architecture project and its phase gates, delivery compliance, change management, and its console surface.",
        "responsibilities": "The programme architect (the maintainer sessions under the owner's authority) delivers and evidences each deliverable; the Architecture Board reviews and approves; the programme owner accepts the capability.",
        "scope": "The eleven tables of migration 0054 and the 26 architecture.* actions that expose them; the four ADM contract kinds; the seeded ten-type artefact standard; the Architecture Board; the maturity and skills baselines; and the Architecture capability screen (D6). The design is docs/plans/T9-architecture-capability.md."
      },
      "created_at": "2026-11-22T07:52:45.884024+00:00",
      "id": "01a2023e-4e59-75ad-b2ab-fd7d014511d2",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d35-7bd1-945d-cea12c5dfd6d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": "01a1f2c2-0e1a-787f-9011-46b424d1ec71",
      "title": "Statement of Architecture Work - the TOGAF ADM capability (T9) - re-issued 2026-11-22",
      "updated_at": "2026-11-22T07:53:45.939115+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20256-80ac-72f0-a310-d114488f1d7c",
      "approved_at": "2026-11-22T08:19:17.038367+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": [
          {
            "criterion": "HTTP and MCP cannot diverge",
            "id": "a",
            "proof": "All 20 (16 internal + 4 fixtures.*) both ways; stripping request_id/duration_ms, http.body == mcp.structuredContent."
          },
          {
            "criterion": "One value, three projections",
            "id": "b",
            "proof": "spec_hash: manifest == resolved operation == MCP tool."
          },
          {
            "criterion": "Duplicates fail closed",
            "id": "c",
            "proof": "Exit 1 on a duplicate name, an MCP collision or an out-of-charset name."
          },
          {
            "criterion": "Schema-invalid input",
            "id": "d",
            "proof": "RFC 9457 with populated errors[], never a 500."
          },
          {
            "criterion": "Idempotent replay",
            "id": "e",
            "proof": "Two identical POSTs with one key: same status and body, one action_invocation."
          },
          {
            "criterion": "The lock catches drift; breaking ingest refused",
            "id": "f",
            "proof": "A renamed field makes --check-lock exit 1 naming the operation; the removed-field fixture exits 2 (422) while the approved version serves."
          }
        ],
        "assignment": {
          "depends_on": [
            "S02",
            "S03"
          ],
          "gate": "SPEC section 8 S4 - 20 actions identical over HTTP and MCP",
          "issue": "#13",
          "plan_status": "planned",
          "programme_project": "S04",
          "spec_sections": [
            "section 5.5",
            "section 7",
            "section 8 S4",
            "section 3 D11"
          ],
          "title": "Action registry - derive, manifest, OpenAPI, MCP, invocation, external ingest"
        },
        "manifest": {
          "adds_actions": [
            "actions.list",
            "actions.describe",
            "me.get",
            "tenants.get",
            "tenants.update_settings",
            "members.list",
            "members.set_role",
            "departments.list",
            "departments.create",
            "audit.list",
            "jobs.list",
            "jobs.retry",
            "schedules.list",
            "schedules.upsert",
            "events.list",
            "licences.get",
            "fixtures.echo",
            "fixtures.slow",
            "fixtures.fail",
            "fixtures.timeout"
          ],
          "adds_events": [],
          "adds_modules": [
            "actions"
          ],
          "adds_tables": [
            "action_registry"
          ],
          "deliverables": [
            "crates/actions-derive/src/lib.rs",
            "crates/actions/src/{spec,registry,invoke,mcp,openapi,lock,ingest}.rs",
            "crates/actions/tests/{registry,parity_http_mcp,idempotency,timeout,ingest_diff,external_actions}.rs",
            "crates/actions/tests/fixtures/external/openapi.json",
            "crates/{api,store}/src/actions.rs",
            "migrations/0003_action_registry.sql",
            "served-spec.lock.json"
          ],
          "source_pins": {
            "agent": "83b8de7e",
            "fr": "4a579e91",
            "fr_source_map": "6f7c5097",
            "pinned": "2026-11-07"
          }
        },
        "responsibilities": [
          {
            "responsibility": "Build crates/actions-derive and crates/actions, wire the boot assertion before TcpListener::bind, and add migrations/0003_action_registry.sql.",
            "role": "S04 delivery"
          },
          {
            "responsibility": "Supplies begin_scoped, audit_event, action_invocation, idempotency_key, event and /api/me.",
            "role": "S02"
          },
          {
            "responsibility": "Supplies the 14 domain aggregates and transitions used by the 16 internal actions.",
            "role": "S03"
          },
          {
            "responsibility": "Add domain actions and their own events in the same transaction; later slices add actions, never a surface.",
            "role": "S05-S11"
          },
          {
            "responsibility": "Provides the kind: workflow engine; until then workflow returns 501.",
            "role": "S13"
          },
          {
            "responsibility": "Supplies channel-mcp delivery rows and the signed mcp_server channel (S06-alpha).",
            "role": "S06"
          }
        ],
        "retrospective": "Planned, not yet delivered. Decisions closed here: S4-D1 derive crate is the 10th member; S4-D2 one literal route per action plus a spec_exempt fallback; S4-D3 enable/disable is approved_at; S4-D4 an external name colliding with any name is refused at ingest; S4-D5 idempotency scope is the action name; S4-D6 scope-filtered MCP tools with the refused tool absent; S4-D7 workflow returns 501 until S13. Traps to carry: four artifacts drifting is the fr lesson; Router::merge drops paths; MCP tool names are not action names; a timeout is the caller's; idempotency and the mutation must share a transaction; circular actions hit invocation_depth 8 and 409 action-recursion-limit.",
        "scope": "IN: ActionSpec plus #[action]; the compile-time registry and boot assertion; GET /api/actions; POST /api/actions/{name}/invocations with idempotency, timeout, audit, outbox; rmcp at /mcp; the OpenAPI fold and lock; openapiv3 ingest; migrations/0003_action_registry.sql; 16 internal plus 4 fixtures.* actions. OUT: domain actions (cards.move, ...) are S05-S11's; kind: workflow (S13); channel-mcp delivery rows (S06); the signed mcp_server channel (S06-alpha)."
      },
      "created_at": "2026-11-22T08:19:08.182233+00:00",
      "id": "01a20256-6ede-7d70-aae8-5dbfe681a1b7",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-7eb6-a62a-f420e742595c",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work - S04 Action registry (retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T08:19:17.038367+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20273-4f0f-77e6-8005-9744d1e3a574",
      "approved_at": "2026-11-22T08:50:49.577591+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": "# Statement of Architecture Work \u2014 T13 Tool platform\n\n## Context\nSPEC_V2 \u00a711 replaces ad-hoc backend calls with one governed path. The conveyor already owns actions, secrets, entitlements, audit and a durable queue, so a second catalogue/queue/runner (the predecessor MCP server) is refused (D44, D59). T13 is the slice that makes the one path real.\n\n## Scope\nThe broker `tools.call` and its seven steps; the four catalogue/ledger tables; effect-class gating; the closed-schema input check; credential aliases over sealed secrets; the direct MinIO storage adapter with the mandatory tenant prefix; the coding toolkit and one kag-era toolkit; V1 external-action migration; six gate proofs.\n\n## Approach\nImplement the broker and storage layers in the conveyor workspace, prove each refusal before dispatch at the wire (a counting stub), record one `tool_call` and one `tool.called` event per call, and record every artefact of this rework retrospectively from \u00a711 and the tests rather than from memory.\n\n## Deliverables\n`crates/actions/tests/tool_platform.rs` (the six proofs and the extra broker tests); `conveyor_actions::storage::{JobStorage, TENANT_PREFIX, sha256_hex}`; `conveyor_store::tool::{calls_for_run, ToolCallRow}`; `conveyor_actions::prompt::compile_for_audit`; the reused `conveyor_store::secret::create_secret_tx`; tables `tool`, `toolkit`, `tool_binding`, `tool_call`; action `tools.call`; event `tool.called`.\n\n## Constraints and standards\nRust workspace; PostgreSQL 18; MinIO (S3-compatible) as the platform store; a closed JSON-Schema subset (additionalProperties false, enums, patterns, bounds); OpenAPI 3.1 and streamable-HTTP MCP as the only two BYO forms; idempotency keys on writes and bounded polls.\n\n## Acceptance\nThe six \u00a712 clauses and their named tests, as written in `acceptance/programme/T13.toml`; the manifest is the gate, not this document.\n\n## Risks\nThe suite serialises because the destination URL is process-global; the storage test path is test-only until live MinIO wiring exists; the catalogue write path is deferred to T14; a production binding writer is still absent.\n\n## Decisions closed by this rework\nThe binding row is the broker's authority; refusals are proved on the wire; `tool_call` is not `action_invocation`; the storage adapter builds every key, so traversal and hash mismatches are refused.\n",
      "created_at": "2026-11-22T08:48:20.084131+00:00",
      "id": "01a20271-2a37-7329-b159-28031ddcb966",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T13 Tool platform (retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T08:50:49.577591+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20287-813e-7abd-83c5-ddf3bb9e98b3",
      "approved_at": "2026-11-22T09:12:50.020166+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T10 gate: PI planning materialises cards from the backlog by WSJF within WIP; a deliverable without evidence cannot demo; retro produces lessons; an end-of-tranche review with no baselines, benefit status or risk evidence is refused; a transition that never measured its baseline cannot report the benefit realized. Clauses 2, 3 and 5 are proved by named tests; clause 1 is proved for a project backlog and not for the programme-wide backlog (v1-p82-5); clause 4 is unproved at HEAD because free-text placeholders satisfy the evidence check (v1-p83-1) and the baseline requirement is not gated (v1-p83-4). The rework's acceptance instrument is `acceptance/programme/T10.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T10 (SPEC_V2 sections 8.2 and 8.3 and section 12, issues #82 and #502) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the SAFe cadence and the MSP tranche machinery of SPEC_V2 sections 8.2 and 8.3: the cadence state machine, WSJF and WIP-bounded materialisation behind an approved PI plan, backlog refinement, the demo credit, the retro lesson requirement, the tranche review and its evidence, and the benefit transition with its baseline measurement. Out of scope: the decision ladder (T2), the plan-spine baseline contract (T1), the coach card (T4) except the retro card, metrics (T11) and the one-organisation-loop acceptance (T12)."
      },
      "created_at": "2026-11-22T09:12:38.058104+00:00",
      "id": "01a20287-696c-76a4-a2db-28c6092b5813",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-70c1-a5e8-f50956c5e3f9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T10 SAFe cadence and MSP tranches (retrospective, U5a)",
      "updated_at": "2026-11-22T09:12:50.020166+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2028b-f63c-7c27-b580-d4cba2c1e70f",
      "approved_at": "2026-11-22T09:17:42.179607+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T4 gate: an injected failure yields exactly one coach card and one remedy; a repeat signature escalates; no coach card about a coach card; a systemic class opens a kata. Clauses 2 and 3 are proved by named tests in `crates/actions/tests/coach.rs`; clause 1 is proved for a first interruption but refuted as a universal by v1-p6-4; clause 4 is proved only as a `kata.open` process start and v1-p6-2 shows no kata record, instrument or threshold is created. The rework's acceptance instrument is `acceptance/programme/T4.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T4 (SPEC_V2 sections 6.2, 6.4 and 12, issue #75) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the interruption taxonomy, the closed remedy set, per-signature uniqueness and the depth cap of SPEC_V2 section 6.2, plus the systemic-class path of section 6.4: the `coach_interruption` and `coach_remedy` tables, the `coach.interrupt` action, the seeded coach process and the real-Postgres suite `crates/actions/tests/coach.rs`. Out of scope: the board and pull path's `err.card` producer, the kata's own lifecycle and measurement (T5), the PM job (T3), operations (T7) and the tool platform (T13)."
      },
      "created_at": "2026-11-22T09:17:30.02765+00:00",
      "id": "01a2028b-dde6-7ed4-9e47-6e582f2341d8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T4 Kanban coach (retrospective, U5a)",
      "updated_at": "2026-11-22T09:17:42.179607+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20ce0-644d-7340-a696-1af10fa8c61e",
      "approved_at": "2026-11-24T09:26:06.517885+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The six \u00a712 gate clauses plus D1\u2013D8; the live gate recorded in docs/plans/T19/evidence.md; the closure decision in docs/plans/T19/closure-review.md.",
        "assignment": "Build T19 \u2014 decision forums \u2014 to the \u00a712 gate: a meeting is a card whose agenda items resolve approvals, whose participants hold typed positions, and whose outcomes are derived arithmetic over those positions.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the SRO accepts the benefit B7 and the tranche evidence.",
        "scope": "The meeting aggregate, the approval_decision participant extension, the minutes governed-asset kind, the twelve meetings.* actions, the Meetings destination and the two consumer seams (governance_body, programme_cadence)."
      },
      "created_at": "2026-11-24T09:25:57.096759+00:00",
      "id": "01a20ce0-5183-776c-84bd-327f2301b19b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b9b8-7214-abc6-f2c4f75a1fc1",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T19 decision forums (2026-11-24)",
      "updated_at": "2026-11-24T09:26:06.517885+00:00",
      "version": 2
    },
    {
      "approval_id": "01a210bd-cdb2-7f2f-ac9a-c314fb1856af",
      "approved_at": "2026-11-25T03:26:48.7374+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The T19F plan's P1\u2013P5 scenarios, the live gate recorded in docs/plans/T19F/evidence.md, and the closure review in docs/plans/T19F/closure-review.md that defers project closure to the tranche-6 review.",
        "assignment": "Build T19F \u2014 the forum bindings \u2014 to the plan's P1\u2013P5: a meeting_type version is written only by the reviewed authoring route, a board subject is decided in a sitting with via='meeting' rows, and a cadence row materialises its own meeting exactly once.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the programme manager owns B7's measurement; the SRO accepts the tranche-6 evidence.",
        "scope": "Migration 0073; the authoring publish hook and meetings.type_list/type_get; architecture.board_sit_open/board_sit_decide; cadences.materialise; the mock-UI and real-app surfaces; the programme record and its evidence."
      },
      "created_at": "2026-11-25T03:26:38.984805+00:00",
      "id": "01a210bd-ba25-78c1-a363-3eea5aa71335",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T19F forum bindings (2026-11-25)",
      "updated_at": "2026-11-25T03:26:48.7374+00:00",
      "version": 2
    },
    {
      "approval_id": "01a217fb-a503-7db9-b47b-55a111c1c8de",
      "approved_at": "2026-11-26T13:11:37.614131+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate in SPEC_V2 \u00a712 is executable: a PM runs a request end to end; the PM cannot approve its own gate; a baseline change requires a change request. The close gate reads the project's declared outputs \u2014 project-brief and evidence \u2014 plus the closure-review output, a live board-approved delivery contract, a Phase-G review citing the standing delegation, the approved scope baseline, the seven mandatory A\u2013D artefacts with their registers, WBS inputs 8, 8, 5, 3, a demo credit per accepted deliverable and at least one tranche lesson. This content pack is checked offline by scripts/rework-content-check.py --project T3 before the factory reads it, and the factory then records the contract, review, artefacts and closure against the snapshot.",
        "assignment": "Rework T3 by the reconstituted TOGAF/MSP lifecycle: author the charter, this Statement of Architecture Work, the seven mandatory Phase A\u2013D artefacts, the delivery contract, the WBS deliverable and the Phase-G review, and re-publish the project-brief and evidence outputs. The architecture work is the PM job itself \u2014 one project run from charter through WBS, scope baseline, staffing, gate routing and closure, with the approver kept distinct from the requester and every baseline change carried by a change request \u2014 recorded retrospectively and dated 2026-11-26.",
        "contract": "This statement is authored retrospectively under the delegation and is approved through the same board gate it describes; its approval is recorded by the factory from this pack, not asserted here. It is complemented by the delivery contract whose commitments, compliance and approval fields this pack authors, on the approved scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa (kind scope, taken 2026-11-26, approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545), and by the Phase-G review the factory records from this pack. The tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9 decided continue on 2026-11-26 and authorised tranche 4; the reopen this pack answers is dated 2026-11-26T07:44:41Z and its reason is the missing contract, review and baseline record.",
        "responsibilities": "The programme architect authors the pack under the #526 rework delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14; the Architecture Board reviews the seven A\u2013D artefacts, which this pack records as approved. The programme owner is the approver of the delivery contract and of the closure: the scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa is approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545, and the closure gate recorded for T3 was decided by an approver distinct from its requester, which is the gate's own rule. The PM runs the request and may not approve a gate it raised.",
        "retrospective": true,
        "scope": "In scope: the charter and its approved gate; the WBS with the four typed WSJF inputs 8, 8, 5, 3; the scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa taken on 2026-11-26 and approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545; the staffing plan; the gate router that refuses self-approval and records every decision; the change request that must precede a baseline change; the closure record with its closure-review output; the seven A\u2013D artefacts and the project registers. Out of scope: the T2 decision ladder's policy engine, the T4 coach, the T5 kata, the T13 tool platform, the T1 plan spine's implementation and every V1 slice; the rework completes the record and adds no scope."
      },
      "created_at": "2026-11-26T13:11:37.331166+00:00",
      "id": "01a217fb-a475-7824-b01c-bfcfcb4680d3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7915-8833-75f4dc31cc07",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T3 PM job (retrospective, 2026-11-26)",
      "updated_at": "2026-11-26T13:11:37.614131+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2180f-168b-79af-af7f-e4cc18948e7b",
      "approved_at": "2026-11-26T13:32:51.863706+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-26T13:32:51.58027+00:00",
      "id": "01a2180f-15fe-794d-aba4-395c2f06a32f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:51.863706+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d8-8c6d-715e-ad85-da38d5f3351c",
      "approved_at": "2026-11-26T21:52:31.994413+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S05.toml` and plan \u00a77: (a) replay fidelity \u2014 1,000 cards/10 lists, `admin diff` prints zero deltas; (b) one winner \u2014 two clients with the same `If-Match` get one 200 and one 412 carrying the current representation, `version` +1; (c) 1k load \u2014 `GET /api/boards/{id}/cards?limit=200` p95 \u2264 120 ms at 20 rps, 1,000 cards in five pages \u2264 500 ms; (d) webhook shape \u2014 `{event,data,prevData,user}` exactly, `event` the internal register type and `data.list_id`/`prevData.list_id` the lanes; (e) SSE \u2014 a second client on `board:{id}` gets `card.moved` \u2264 200 ms live and on replay; (f) no second store \u2014 one `comment`, one `board_membership`, no `card_membership`, no `attachment`. The instrument is `acceptance/programme/S05.toml`; the closure gate is decided by an approver other than its requester. The snapshot records no Phase-G review of these properties; this rework records that gap rather than claiming a pass.",
        "assignment": "Rework the V1 slice S05 (SPEC \u00a78 S5) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the acceptance instrument.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the board slice of the plan: the nine tables (board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription), the 39 actions, the read projections and cursors, the SSE topics `board:{id}` and the resolved `card:{id}`, the webhook emitter and its `webhook.deliver` job, and the key-referenced `board-export/v1` CLI. Out of scope: attachments and card-level memberships, claims/assignee/stopwatch writes (S06), webhook delivery rows as actions, and the UI (S12)."
      },
      "created_at": "2026-11-26T21:52:31.682863+00:00",
      "id": "01a219d8-8bc7-71a2-8dbe-33702699eac1",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S05 Board (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:52:31.994413+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d9-303c-7fdc-b217-ec349e82329b",
      "approved_at": "2026-11-26T21:53:13.933646+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "Acceptance criteria": "Phase A closes when this statement is approved, the seven A\u2013D artefacts are approved in standard order, and the seven artefacts each carry metadata, entries, links and a document section with authored prose. Delivery acceptance is the plan \u00a77 gate: (a) three revisions with max(seq)=3 and a compare showing the status change; (b) an idempotent seed returning {0,0,N}; (c) both GIN indexes used under enable_seqscan=off with rising idx_scan; (d) relevance where B meets \u00a75.6's recall@5 \u2265 0.75, MRR \u2265 0.60 and B \u2265 C; (e) traceability in both directions with zero cross-tenant rows; (f) quality scoring read-only; (g) no orphan over the 28 RM modules. The record-level acceptance is `acceptance/programme/S07.toml`, whose 17 checks are 15 P0 and two P1.",
        "Assignment": "The assignment is to give the conveyor one governed requirements surface: requirements, principles and decisions share a single `governed_asset` lifecycle carried by S03, every mutation writes one immutable `revision`, `requirements.compare` diffs two revisions in Rust, quality is scored adviserily on a revision, and search ranks assets, cards and catalogue entries through SQL. The work is asked for by the tranche-4 sponsoring group and accepted by the Architecture Board; this statement is authored retrospectively on 2026-11-24 under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 for project 01a1d039-0d2c-7322-b2f9-4dd041153894.",
        "Contract": "The contract record is the delivery contract of project 01a1d039-0d2c-7322-b2f9-4dd041153894, version 1, approved by the Architecture Board on 2026-11-24 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. Its compliance clause is plan \u00a77 and the acceptance instrument `acceptance/programme/S07.toml`; its manifest names that instrument; its acceptance date is the snapshot of 2026-11-24, when the project row is closed at version 7 and tranche 4 (01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce) is authorised by the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9. The close gate is decided by an approver other than the requester.",
        "Responsibilities": "The programme architect (conveyor-e2e) is responsible for the plan, the seven artefacts, this statement, the delivery contract and the retrospective pack. The Architecture Board (conveyor-architecture-board) reviews and approves the artefact set and the Phase-G review. The S03 and S04 owners are consulted because `governed_asset`, `revision`, `catalogue_entry`, `trace_link` and the action registry are their surfaces; S08 and S12 are informed as consumers. The approver of the closure gate is deliberately a different principal from the requester under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14.",
        "Scope": "The phase set is A to D: the stakeholder map, the driver/goal/objective catalog, the requirements catalog, the solution concept diagram, the business capability map, the data entity/data component catalog and the technology standards catalog. The boundary is the plan \u00a75 material: the five \u00a75.1 tables, the 32 actions, the `search` module, the catalogue seed, the quality rubric and job, the eval set and the traceability actions. The project does not build the TOC API, ArchiMate or UML renders, project CRUD, batch bulk operations, RM's promote/project-copy or any UI, and it does not recreate `outline`/`outline_node`, which migration 0011 dropped."
      },
      "created_at": "2026-11-26T21:53:13.637379+00:00",
      "id": "01a219d9-2fa8-703a-99f2-fb7e7572e467",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S07 Requirements and search (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:53:13.933646+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d9-f717-73ba-9c86-18adeeecd4ef",
      "approved_at": "2026-11-26T21:54:04.836362+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The plan \u00a77 gate: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, then delete yields GET 404 and object_store::head NotFound, asserted in documents_roundtrip; (b) cap \u2014 10,485,760 bytes accepted and one byte more refused 413 application/problem+json with a stable type in documents_limits; (c) secret \u2014 rotate to version 2 and GET returns no plaintext, value key or ciphertext in secrets_never_revealed; (d) workers \u2014 the page renders live rows and a stopped service yields a banner with the breaker open after five failures; (e) reports \u2014 every number equals its SQL count on the same snapshot in reports_match_sql; (f) externals \u2014 GET /api/actions carries timeout_ms on every external and zero reqwest:: outside the executor in externals_declared; (g) billing \u2014 no write action and no metering event in the manifest. The same instrument also requires migration 0013 applied twice as a no-op, FORCE RLS on all four tables, and the nine declared event types. The snapshot records no run of these properties.",
        "assignment": "Rework the V1 slice S09 (SPEC \u00a78 S9) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, this charter, this Statement of Architecture Work and the delivery contract, take the scope baseline with one WBS deliverable, record a retrospective Phase-G review at level consistent, and publish the expected outputs project-brief and evidence. Because all nine linked issues are recorded closed, this rework re-closes the project at level consistent through the acceptance manifest, with the closure and closure review dated 2026-11-24.",
        "contract": "This Statement establishes the delivery contract against acceptance/programme/S09.toml, whose gate text names the seven properties above. Compliance is assessed clause by clause at the 2026-11-24 retrospective snapshot; the project's published evidence page is 01a1e687-7682-753d-b60e-1f00243b13a3 and its pre-rework audit pack is docs/evidence/rework/S09.json, which records verdict red on the missing baseline, charter, contract, Phase-G review and closure outputs. The rework supplies the lifecycle record and re-closes the project retrospectively on the P0 checks of the manifest; the remaining verification limits are carried in the closure review, the review and the evidence output rather than converted into a pass.",
        "responsibilities": "The programme architect (conveyor-e2e) owns the slice plan and this retrospective set; the Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract; the approver (conveyor-approver) decides the Phase-G level; the S03 team supplies tenancy, trace_link and revision, and the S04 team supplies the action registry with its OpenAPI and MCP projection. Operators own the KEK source in the pod environment and the MinIO bucket policy.",
        "scope": "the four tables document, repository, project_repository and secret in migrations/0013_documents.sql; the integrations module (object_store, external, breaker, ingest); documents.create/get/list/download/update/delete/report, 26 of the front matter's 27 actions in all; the nine event types; MinIO bucket conveyor-documents with key t/<tenant>/<document>/<version>; the KEK/HKDF/XChaCha20-Poly1305 secret envelope; repositories as references; the read-only workers, billing and observability displays; report queries over local tables; and the strata handoff. Out of scope: billing logic and metering writes, worker scheduling, artifact objects, code analysis, licence issuance, ingestion and embeddings as services, screens (S12) and project_request.repository_url (S07)."
      },
      "created_at": "2026-11-26T21:54:04.526424+00:00",
      "id": "01a219d9-f671-708a-b28f-063e7e79e6ec",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S09 Documents and integrations (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:54:04.836362+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219dd-4701-7342-9adf-f2c47331c4af",
      "approved_at": "2026-11-26T21:57:41.900644+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T14 gate, executed on the live estate through `acceptance/programme/T14.toml`: (a) each published toolkit has a probe and one live call; (b) `scientific_processor` is scaled up for its probe and one live call, then scaled back, with a receipt; (c) a zero-use report exists; (d) a backend outage unbinds new runs while in-flight runs finish. The snapshot carries no executed receipt for any of the four clauses, so this rework records them as the project's open verification item rather than a pass; the level written is consistent, never non_conformant without a waiver, and no waiver exists.",
        "assignment": "Rework the V2 slice T14 (SPEC_V2 \u00a712) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, record the Phase-G compliance review at the level the record supports, publish the expected outputs (`project-brief`, `evidence`) and leave the project active with its gate recorded as open verification rather than closing it.",
        "contract": "No live Statement of Architecture Work and no live delivery contract exist for T14 at the snapshot; this pair is supplied retrospectively by the 2026-11-24 rework and is the contract of record until a Board-approved one supersedes it. It binds the project to the four gate clauses above, to broker-only calls inherited from T13, to draft status for the named draft services, and to a receipt per claim; it does not authorise closure. The standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 is the authority under which the rework acts.",
        "responsibilities": "the conveyor-approver approves the Phase A\u2013D set and the delivery contract; the conveyor-architecture-board governs the standards; the programme architect (conveyor-e2e) owns T14 and this retrospective pack; the T13 tool-platform team owns the tool family, the broker, the effect classes and the credential profiles; the estate service owners grant the live-call window and the outage drill; the SRO is accountable for the tranche outcome.",
        "retrospective": "written by the 2026-11-24 tranche-4 rework; it reconstructs the record and does not attest contemporaneous intent.",
        "scope": "the estate-toolkit import wave on T13's tool platform: the `tool`, `toolkit`, `tool_binding` and `tool_call` rows for the coding fleet, strata, billing and licence reads, webscraper, media_discovery, scientific_processor, lean4_prover, video-compiler, the w2v orchestrator and the ops read tools; `evidence_review`, `discovery` :8807 and the planned AHTAPol services as `draft`; the probe and the one live call per published toolkit; the `scientific_processor` scale-up and scale-back receipt; the zero-use report; and the backend-outage unbind test. Out of scope: the broker itself (T13), tool authoring and BYO (T15), and the UI."
      },
      "created_at": "2026-11-26T21:57:41.6166+00:00",
      "id": "01a219dd-4673-70da-b701-3f43675b4fa6",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T14 Estate toolkits (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:57:41.900644+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219dd-edc0-7da7-a7fd-00eafabf0eae",
      "approved_at": "2026-11-26T21:58:24.588139+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The pack passes `python3 scripts/rework-content-check.py --project T16 --content docs/evidence/rework/tranche-4/rework-content-T16.json --snapshot /tmp/conv526-w1-snap/data --wsjf 10,8,7,8 --closes false` with exit 0; every cited identifier exists in the snapshot; the seven artefacts carry the existing titles, phase, plateau and kind in the required order; each artefact body carries Metadata, Entries, Links and Document as authored prose; the project brief and evidence carry their template sections with substance; and no closure key appears. The project's own gate remains acceptance/programme/T16.toml, and this review's verdict is consistent.",
        "assignment": "Under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, the retrospective completes T16's governance record against acceptance/programme/T16.toml for the procedure-ingestion slice of SPEC_V2 \u00a712. The assignment is record completion, not rework of the built capability: author the charter, the Phase A\u2013D artefact set, this Statement of Architecture Work, the delivery contract, the Phase-G review and the published project brief and evidence, and leave the project active for a separate close decision.",
        "contract": "The record is delivered under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 against scope baseline 01a216d0-f529-7bcb-81a7-d429d008d638 (approved 2026-11-24T16:58:21Z) and tranche 4 01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce. The delivery contract below records compliance, acceptance, approval, manifest and retrospective limits. Any change to scope or gate returns to the Architecture Board; no waiver is claimed or required, and the project stays active for the tranche-4 review to decide continue.",
        "responsibilities": "The programme architect authors the record under the delegation; the Architecture Board reviews contract and Phase-G conformance; procedure owners and publishers own the sources and the publication decision; delivery teams consume the ingestion and PDDL surface; tenants own their rows and sources; the programme owner owns the outcome and any close decision. scripts/rework-project.py performs the live writes, and scripts/rework-content-check.py refuses this pack before any write if it is structurally wrong or cites an identifier absent from the snapshot.",
        "scope": "In scope: SOP intake; RDF/OWL/Turtle as a strata knowledge pack bound by knowledge_pack_ref; BPMN, Visio and draw.io into PDDL procedure drafts; capability, agent and job design from an SOP; the source-review and human publication gates; re-ingest versioning; bound PDDL execution; and the governance records named in the assignment. Out of scope: any runtime RDF store, graph store or reasoner (decision D53, owner 2026-11-19), T17's tenant-wide generalisation, T19's meetings records, the /procedures destination work of issue #448, and new ingestion scope."
      },
      "created_at": "2026-11-26T21:58:24.307416+00:00",
      "id": "01a219dd-ed35-79eb-b898-3323854d5411",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d38-76fb-9394-b75934d16fcd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T16 SOP, RDF/OWL and process-document ingestion (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:58:24.588139+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219de-7e7e-72db-b751-a17bdd9b013d",
      "approved_at": "2026-11-26T21:59:01.639909+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T18 gate, judged by the fifteen checks of acceptance/programme/T18.toml: every destination route exists with its spec, each spec asserts empty, error, happy and one refusal, the coverage record reports exists only from a real route and a present spec with a *Gate reported as extend naming its owner, no production screen renders a fixture, and a signed-in tenant drives portfolio \u2192 programme \u2192 project \u2192 work package, organisation \u2192 release \u2192 binding, procedures intake \u2192 publish, tools call \u2192 ledger, improve \u2192 kata \u2192 adoption, operations incident \u2192 postmortem and the ADM phase rail from the app, with the T12 loop as the final spec.",
        "assignment": "Rework the V2 delivery slice T18 (SPEC_V2 \u00a710.5 and \u00a712 T18) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter, the Statement of Architecture Work and the delivery contract, take the scope baseline with the one WBS deliverable, record the retrospective Phase-G compliance review, publish the expected outputs project-brief, inception-analysis and architecture-design plus the evidence output, and leave the project active because T18-4 and T18-5 are not delivered. The work is retrospective: it reconstructs the governance record from the project row, the plan, the route table and the issue registry, and it attests no contemporaneous intent.",
        "contract": "The delivery contract record is requested and approved live by scripts/rework-project.py under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 and is titled 'Delivery contract \u2014 T18 The real V2 UI (retrospective, 2026-11-24)'. It commits the lane to the \u00a712 gate and to the two open units as typed dependencies, and it records that this retrospective neither closes the project nor waives any check. The contract's approval is retrospective; the historical snapshot has no live contract, which is one of the failed checks in the prior acceptance record.",
        "responsibilities": "The T18 lane under the programme architect (conveyor-e2e) is responsible for the route table, the screen modules, the specs and the coverage instrument. The Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract. The sponsoring group is accountable for the tranche-4 authorisation and for keeping the project active. The owning slices T4, T5, T7, T9, T10, T11, T13, T16 and T17 are responsible for the actions each destination calls, and T12 owns the final organisation loop.",
        "retrospective": true,
        "scope": "the shell, route registry and navigation; the ten V2 destinations of SPEC_V2 \u00a710.5 and the execution room; the L1\u2013L4 delivery view; one Playwright spec per destination; the V2 coverage instrument measured from apps/web/src/app.rs and the named specs; and the T12 organisation loop as the final acceptance. Out of scope: new tables, actions, events or ports; SSR, a second router or frontend, or a mobile app; retiring or rebuilding the design pack; the V1 routes, which are linked rather than re-implemented; and any production fixture or live tenant write inside the retrospective."
      },
      "created_at": "2026-11-26T21:59:01.357241+00:00",
      "id": "01a219de-7def-7827-8a21-98428640f502",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T18 The real V2 UI (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:59:01.639909+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21a39-aae9-7d90-9069-b0576ed463d4",
      "approved_at": "2026-11-26T23:38:36.773008+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S06.toml` and plan \u00a77: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 `cards.move(done)` yields `run.succeeded`, `approval.approved` and one audit, invocation and event row each; (b) dual control \u2014 `required_decisions=2`, one approve stays pending, the same approver again is 409, a second approver closes; (c) retry \u2014 `kill -9` mid-run leads to `run.reap`, a crashed run, then `attempt=2` with two rows and never two live runs; (d) andon \u2014 engaged gives 409 and a pending `run.execute` for 60 s, released resumes within one poll; (e) delivery retry \u2014 a stub 500 gives `attempts=5`, `state=failed`, one row; (f) no timers \u2014 `rg 'setInterval' crates apps` is empty and every cadence is a `schedule` row; (g) human vs agent \u2014 `decided_by_kind` is `human` for one decision and `agent` for the other. The instrument is `acceptance/programme/S06.toml`; the close gate is decided by an approver other than its requester. The snapshot records no run of these properties and no Phase-G review; this rework records the level `non_conformant` rather than claiming a pass.",
        "assignment": "Rework the V1 slice S06 (SPEC \u00a78 S6) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure decision, and re-close through the acceptance instrument once its properties run.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the control-plane slice of the plan: the six tables of `02-DATA-ARCHITECTURE \u00a72.6` (`channel_binding`, `channel_preference`, `message`, `delivery`, `audience`, `audience_member` \u2014 the last two PENDING-S0), the 17 actions, the 17 events, the SSE topics `run:{id}`, `approval:{id}` and `tenant`, the Telegram in/out path with its sender-identity resolution, the andon gauge and poller pause, and the job kinds that replace 11 of fr's 14 timers (three deleted). Out of scope: the signed mcp_server channel (S06a), MCP tool surfacing (S10), `project_request.timeout` (S07), the UI (S12) and the audience screens (PENDING-S0)."
      },
      "created_at": "2026-11-26T23:38:36.518039+00:00",
      "id": "01a21a39-aa69-7167-8696-e615aa3b012f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06 Control Plane (retrospective, 2026-11-27)",
      "updated_at": "2026-11-26T23:38:36.773008+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c16-879a-765f-9e1b-c788a9c1ef4a",
      "approved_at": "2026-11-27T08:19:28.421371+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The S08 gate is the slice gate of SPEC \u00a78 S8: the Part A round-trip from principle to ADR to TOC to register to deliverable to capability to stakeholder, the TOC cycle refusal, the absence of a duplicate principles/decisions surface, three golden models in SVG/PNG/PDF, the profile's write-time 422, the XMI round-trip, the post-commit validation job, deterministic ids, the two retention producers and the render byte-route parity.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the S08 slice: the governance surface over S03's spine and the modelling surface over seven new tables, with a deterministic renderer and a profile-scoped XMI exchange. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the S08 slice plan.",
        "constraints": "The build must not introduce a second model store, must not use the registry as the validation authority, must not call load_system_fonts, must not query JSONB geometry in SQL, must not validate inside the mutation transaction, and must not trust an XMI document: size, depth, duplicate ids and a DOCTYPE are refused. These constraints are the plan's traps and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set, the contract and the profile; S03 supplies the spine tables and RLS; S04 supplies the registry and invocation route; S07 supplies the outline and the principles/decisions actions; S12 receives the declared trace/architecture surface; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the slice plan, the measured \u00a77.2 completion record and the record's project row. It records the assignment and the acceptance the build was measured against; it does not claim that a contemporaneous board approved this text on 2026-11-08, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are the 68 actions, the seven S08 tables, the two retention producers, the two byte routes and the seven Phase A\u2013D artefacts. Out of scope are the UI, model search, project CRUD, principle-conflict verdicts, architecture containers, change-set/review/publication splits, graph layout and OCL/Profile/SysML; each exclusion is named in the slice plan's \u00a73 so a later reader cannot mistake absence for omission."
      },
      "created_at": "2026-11-27T08:19:28.107129+00:00",
      "id": "01a21c16-86ee-7106-8eab-4155e8b4b141",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2c-7bad-8e6c-b5c5d1fd40e2",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S08 Governance and Modelling (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:19:28.421371+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c1b-b5de-788c-9918-d57c95016a17",
      "approved_at": "2026-11-27T08:25:07.941014+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The S17 gate is the slice gate of SPEC \u00a78 S17: the banked parity oracle 555/555, gold 50/50 and empty 0/50, the golden eval scripts 50/50, the exact McNemar vectors, RLS on all four tables, the fail-closed sandbox, deterministic Job names and the 1 MiB ConfigMap cap, no answers to producers, no benchmark action in AGENT_TOOLS, and the live sandbox lifecycle recorded on compute-1.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the S17 slice: one measurement loop over four tables, a hashed seed artifact, a pure grader and exact McNemar, a k8s Job sandbox and a single run driver. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the S17 slice plan.",
        "constraints": "The build must not import Python into the product or the image, must not let a producer see answers, must not widen the sandbox config to a default namespace, must not exceed the 1 MiB ConfigMap cap or the 256 KiB patch cap, must not let the conveyor arm start before S18, and must not repoint the grading node. These constraints are the plan's traps and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set, the contract and the measurement standard; S02 supplies the job/event/audit tables; S03 supplies the RLS spine and the document row; S04 supplies the registry and scopes; S09 supplies the object store the eval log is written to; S18 takes the conveyor arm; S12 receives the declared screens; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the slice plan, docs/evidence/S17-CORE.md and the record's project row. It records the assignment and the acceptance the build was measured against; it does not claim that a contemporaneous board approved this text on 2026-11-09, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are the four benchmark tables, the seed artifact and its image lock, the standalone seed generator, the grader and 12 parsers, the exact McNemar, the sandbox adapter, the gold/empty/baseline arms, the eight actions, two scopes, two events, and the declared S12 screens. Out of scope are the conveyor arm (S18), model hosting, HF calls, the old CLI, a second service and Python; each exclusion is named in the slice plan's \u00a73."
      },
      "created_at": "2026-11-27T08:25:07.66928+00:00",
      "id": "01a21c1b-b557-7b0a-ba9d-bc9a2de9247d",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d30-794f-bd53-c72986b18e60",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S17 Native Benchmark Harness (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:25:07.941014+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c1d-aa55-7091-ae4b-93328e410035",
      "approved_at": "2026-11-27T08:27:16.055256+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The T17 gate: a brief plus documents draft the catalogue; documents yield linked procedure and policy drafts; an unpublished dependency is refused; a reviewed catalogue publishes with its decisions; no asset publishes itself; a non-human decision cannot complete a gate; no asset widens a tool grant; and the real-app E2E creates a draft and sees the row render.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the T17 slice: the authoring lifecycle over two tables, a human review gate, a published-dependency check and a publish-through-the-target rule. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the T17 inception set and SPEC_V2 \u00a75.7/\u00a710.4\u2013\u00a710.5.",
        "constraints": "The build must not write the target tables, must not let an asset publish itself, must not let a non-human decision complete a gate, must not widen a tool grant, must not duplicate the governed_asset lifecycle and must not refuse the dependency order later than the draft attempt. These constraints are the inception brief's risks and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set and the contract; T3 supplies the decision ladder and the approval primitives; T16 owns live ingestion and the SOP human gate; T13 owns tools and bindings; the web team owns the Authoring screen and the E2E; the tenant authors and reviewers take the human decisions; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the inception set, the evidence and closure-review documents and the record's project row. It records the assignment and the acceptance the delivery was measured against; it does not claim that a contemporaneous board approved this text, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are intake, drafting, review and publication of organisation/department/job/procedure/policy assets, the two authoring tables, the state machine, the review gate, the published-dependency refusal and the Authoring screen. Out of scope are writing the target tables, the SOP/RDF/BPMN converter (T16), tools and bindings (T13), a governed_asset fork and a new template table except asset_template; each exclusion is named in the inception brief's Boundary."
      },
      "created_at": "2026-11-27T08:27:15.79094+00:00",
      "id": "01a21c1d-a9d1-70d8-8a6a-901c21e4dd7b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d38-7db1-b685-d76bd4cebf47",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T17 Tenant Asset Authoring (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:27:16.055256+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c26-afda-78c1-aea9-920275ed65c4",
      "approved_at": "2026-11-27T08:37:07.301597+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The \u00a712 T7 gate: an injected pod failure is triaged, mitigated by an approved write, resolved and postmortemed; a destructive action without the tenant's configured approvals is refused; restart, scale, rollback and flag-flip are callable. The executed suite maps each clause to a named test in T7-acceptance.md; the live cluster script is deploy-bound and is not claimed as run.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the T7 record: the incident lifecycle over the existing card and approval primitives, the four gated cluster writes, the postmortem/kata loop and the /operations console. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by SPEC_V2 \u00a76.5, \u00a79.2 and \u00a712 T7.",
        "constraints": "The build must not let an ops write reach the cluster without an approval, a recorded dry run and two distinct human approvers; must not let the mitigation wrapper bypass the gate; must not let an incident that needed no write stay unresolvable; must not widen a tool grant; and must not claim the live cluster proof before the deployed image carries the gate. These constraints are the finding acceptance and are carried into the contract.",
        "responsibilities": "The programme architect owns the record and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set and the contract; T2 supplies the job/event/audit rows; T4 supplies the registry and the destructive effect; T13 owns tools; the web team owns the /operations screen; the on-call operators take the incident decisions; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the SPEC row, the issue #78 delivery, the acceptance evidence and the two fixes at ca155fada. It records the assignment and the acceptance the delivery was measured against; it does not claim a contemporaneous board approval, and it carries the live-proof limitation into the Phase-G review.",
        "scope": "In scope are the five incident actions, the four write verbs and their gate, the k8s read family, the incident meta lifecycle and the /operations screen. Out of scope are tools and bindings (T13), the agent tool surface (the ops writes are not agent-reachable) and a second ops service; each exclusion is deliberate."
      },
      "created_at": "2026-11-27T08:37:06.989963+00:00",
      "id": "01a21c26-af30-7c3e-ad6d-39f7ae1937ea",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d34-7f89-a0be-f3de3c9f540e",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T7 Operations (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:37:07.301597+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2210c-1cd2-7eb0-a3d2-c90c96b7d6e7",
      "approved_at": "2026-11-28T07:26:11.809378+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is one real agent task over `/mcp`: create a project, read its progress, open and answer an approval, close the project, with `decided_by_kind='agent'` recorded, no `nonagenticai_mcp_server` component in the path, and no `X-MCP-Binding` header. The local proof is `crates/api/tests/mcp_agent.rs::one_licence_principal_task_creates_reads_decides_and_closes`; the deployed agent-app run is S14's gate and is recorded as deferred.",
        "assignment": "The architecture work assigns S06b the agent's direct interface to the conveyor: define the tool key every MCP tool carries, resolve a machine principal from a licence bearer, propagate that principal into attribution, and add the approval-request action an agent needs to escalate. This statement governs the retrospective record of work completed on 2026-11-09 and re-read at the 2026-11-27 snapshot.",
        "constraints": "The one-membership rule rejects zero or several memberships, so the tenant is taken from the licence. A JWT-shaped token that fails JWT verification never falls through to licence introspection. The licence service is on the request path, so its verdict is cached for at most 60 seconds, and an unreachable service refuses with `503 licence_unavailable` rather than admitting. The cache is keyed by the token hash, never by tenant.",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. The agent platform is consulted as the consumer, and the tenant admin is informed because the tool allowlist is the authorization surface.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the 2026-11-09 delivery was recorded only in the slice plan and the evidence file. The statement records what the tree and the suites prove today and names the deferred live step instead of implying it happened.",
        "scope": "In scope are `_meta.operation` on every registered tool, the licence introspection in `crates/auth/src/machine.rs` with its 60-second capped cache and fail-closed rule, `Principal.kind` across human, agent and service, `approvals.request` over `insert_approval_tx`, and the end-to-end `/mcp` session in `crates/api/tests/mcp_agent.rs`. Out of scope are the `fr` toolkit, MCP tasks (S06c), the copilot surface (S10) and any signed HMAC channel (S06a, superseded)."
      },
      "created_at": "2026-11-28T07:26:11.487462+00:00",
      "id": "01a2210c-1c22-792c-8f82-18dc219fba9c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06b Agent Interface (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:26:11.809378+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2210e-f7bb-70ff-8d4d-5d6f216ed585",
      "approved_at": "2026-11-28T07:29:18.922843+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is a scripted session: open a conversation, ask for a card move, receive a proposal and one pending approval with zero action invocations; approve as an authorized member and see exactly one invocation, the card moved and the turn completed citing that invocation; reconnect with `Last-Event-ID` and receive the turn once; see a low-scope principal's two surfaces omit the same tool; see an unavailable router fail the turn with zero invocations; and see the coverage gate fail an unmapped operation by name. The local proofs are `crates/api/tests/copilot_session.rs`, `copilot_proposal.rs`, `copilot_approve.rs`, `copilot_replay.rs`, `copilot_surface.rs`, `copilot_turn.rs` and `crates/actions/tests/copilot_coverage.rs`.",
        "assignment": "The architecture work assigns S10 the copilot's one action surface: project the S04 registry by principal, turn a destructive tool call into an S06 approval, run the conversation as two tables and an event stream, and keep model access outside the conveyor. This statement governs the retrospective record of work built in the S10 slice and re-read at the 2026-11-27 snapshot.",
        "constraints": "S10-D1 keeps the surface a query over `ActionSpec`, so no second registry or tool table exists. S10-D2 makes a proposal an approval created through S06's `insert_approval_tx`, with one pending proposal per turn enforced by the partial unique index on `(tenant_id, meta->>'turn_id')`. S10-D3 makes execution a job with a conditional claim, a 120-second lease, re-checked proposer scopes and the proposal idempotency key. S10-D4 makes only terminal turns durable, with the terminal meta, the proposal and the `conversation_turn.created` event committed together. S10-D5 makes `completions.create` fail closed: an outage, timeout or open breaker ends the turn `failed` and runs no tool. S10-D6 declares `copilot.chat` the only coverage exemption. S10-D7 keeps AG-UI as a serializer over the native stream.",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. S12 is consulted as the renderer of the declared screens, operations is consulted on model availability and key custody, and the tenant admin is consulted because the channel grants and the approval decision are the authorization surface.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the S10 row was created on 2026-11-12 and the build was recorded in the slice plan and the receipt. The statement records what the tree and the suites prove today, names the pre-pack receipt as red on lifecycle checks, and leaves the deployed UI and router proof to S12 and S14 instead of implying it happened.",
        "scope": "In scope are the `conversation` and `conversation_turn` tables with forced row-level security, the turn runner and its eight-round cap, the `agent_surface` projection over the action registry, proposals carried as `approval(kind='copilot.action')` with sealed inputs, the conditional execution claim and sweep, the SSE topic `conversation:{id}` with `Last-Event-ID` replay, `completions.create` as an external action with a 60-second timeout and breaker, the AG-UI serializer, and the screens declared as data for S12. Out of scope are inference-router (D14), the `/mcp` transport and scope filtering (S04), the approval state machine (S06), fr's 34 AG-UI action modules, the agent's principal and tool contract (S06b), and chat UI rendering (S12)."
      },
      "created_at": "2026-11-28T07:29:18.626402+00:00",
      "id": "01a2210e-f724-7ebf-8b08-74d6c0f24f4c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S10 Copilot / AG-UI (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:29:18.922843+00:00",
      "version": 2
    },
    {
      "approval_id": "01a22110-1c69-7458-a9be-c5550ac1ee8a",
      "approved_at": "2026-11-28T07:30:33.839885+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is author \u2192 publish \u2192 execute a process run: an SOP and a three-step process are created and published, a card starts the run, the second step parks on an approval, the approval resumes it, and the run finishes with three succeeded run_step rows, non-empty input and output, and one approval. The negative proofs are an unresolved template (422 refusal, never an empty string), an unknown action refused at publish, and a retry that appends a second attempt row. The local proof is the suite set in crates/actions/tests plus the domain unit tests; the plan's \u00a77 two-process rehearsal is the live gate and is recorded as not executed in this pack.",
        "assignment": "The architecture work assigns S11 the executable-documentation capability: define a versioned process as an ordered list of steps, give each step a registry action and a resolved input template, attach a versioned SOP to each step, validate the whole draft at publish, and execute a published process as a run with one immutable run_step per attempt. This statement governs the retrospective record of work built on 2026-11-09 and re-read at the 2026-11-27 snapshot.",
        "constraints": "The store forbids a second published or draft row per (tenant_id, key). Publish validates in one transaction against the live manifest and refuses an unknown action, a step naming process_runs.*, a forward step reference, a retry on a non-idempotent action, an unresolved gate kind and an SOP whose inspection is empty. The resolver is one pure function with the roots run, steps, card and procedure; an absent path refuses and a null resolves. The executor claims a queued step with a status-guarded update, persists the resolved input at claim, and re-invokes a replayed attempt under the same idempotency key derived from (run_id, position, attempt).",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the store, resolver, actions and executor evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. The process author is consulted because tenant authorship is the capability's consumer, and S13 and S12 are informed because they consume the run tree and the declared read models.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the 2026-11-09 delivery was recorded in the slice plan and in the test tree. The statement records what the tree and the suites prove today and names the deferred live gate, the zero seed and the S15 seam instead of implying they happened.",
        "scope": "In scope are migrations 0015 (procedure, procedure_step, run_step), 0016 (the draft index), 0017 (lock_version), 0021 (the run capability seam), 0022 (the kata_executor role) and 0035 (the rename to process/process_step, the SOP table and process_map); the process, procedure (SOP) and process_map actions; publish validation and the frozen revision; the resolver in crates/domain/src/template.rs; and the executor in crates/actions/src/process/step.rs dispatched as the process.step job. Out of scope are matrix legality (S03), approval transport (S06), sub-processes and the engine (S13), screens (S12), PDL (never) and the deferred default seed."
      },
      "created_at": "2026-11-28T07:30:33.564273+00:00",
      "id": "01a22110-1bde-74d0-8b3e-e693e6c7291b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S11 Processes, Steps and SOPs (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:30:33.839885+00:00",
      "version": 2
    },
    {
      "approval_id": "01a222a6-2960-71b2-8edc-797253376758",
      "approved_at": "2026-11-28T14:54:04.782673+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks, each with its own target: exactly one task-capable action read from the compiled registry; the project-scoped `inputRequests`; the answer written as `approvals.decide`; a terminal result that never says `failed` on the wire; one `-32602` for every ownership failure; and the parity suite proving the task surface and the HTTP action agree.",
        "assignment": "Deliver the architectural record for durable agent tasks: a client that declares the MCP task extension drives a long-running project \u2014 starting it, polling it, answering the approval it waits on and reading a terminal result \u2014 without a second store and without a service between the client and the conveyor. The assignment is retrospective: the product shipped on 2026-11-09 and this statement records the architecture it was built to.",
        "constraints": "No new table and no new service. The run state machine is the task lifecycle, so the reaper had to learn to skip project executions and the one-live-run property had to become a partial unique index because the existing index does not dedupe nulls. Every constraint in this project is a consequence of the reuse decision rather than an independent choice.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the agent interface; the MCP client author is the consumer whose conformance the contract is written against. The retrospective author is not the accepting role, so the separation of duties holds even though one session wrote the product record and this set.",
        "retrospective": "This statement is written after the fact and declares so. The slice plan's build record gives the shipped state; the gate's compiled-registry assertion is the one the plan had to correct, because its first form grepped a flag no serialised surface carries. That correction is recorded here rather than tidied away.",
        "scope": "In scope: the Phase A\u2013D artefact set, the task-to-run identity, the project-scoped `inputRequests` listing, the answer path through `approvals.decide`, and the two indexes migration 0023 adds. Out of scope: the agent's own task support against external MCP servers, decommissioning `mcp_server`, SSE or push delivery, a progress percentage, and any new table."
      },
      "created_at": "2026-11-28T14:54:04.488688+00:00",
      "id": "01a222a6-28cb-7ab4-b430-124b3ec82cb8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06c MCP Tasks (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:54:04.782673+00:00",
      "version": 2
    },
    {
      "approval_id": "01a222a7-1881-7d2e-a3b5-928c86cb2b76",
      "approved_at": "2026-11-28T14:55:06.005722+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks: the capability aggregate resolves with its executor and versioned binding; readiness refuses a run with `409 capability_not_ready` before the first step; a cited passage comes from a pinned strata pack; memory is written, consolidated into `strata_rag` and recalled by `source_id`; and a degraded source mid-run reports `knowledge_degraded`, not the pre-run refusal.",
        "assignment": "Deliver the architectural record for a capability aggregate: a capability is a persisted thing with an executor spec, a versioned knowledge binding and memory, which a card's run resolves and readiness-checks before its first step. The assignment is retrospective \u2014 steps 1\u20134 shipped on 2026-11-09 and step 5's actions remain open \u2014 so this statement records the architecture and the remaining work together.",
        "constraints": "No second loop and no second retrieval path. The conveyor owns the binding and strata owns retrieval, so a run that read knowledge by any route other than `knowledge.query` would make the pinned pack advisory. Every new table carries a tenant column with row-level security forced, and documents are immutable \u2014 ingest a new version and prune the old, never update in place.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the capability model; the capability author writes the pack and its readiness declaration; the retrieval owner operates the strata pack. The retrospective author is not the accepting role, so the separation of duties holds.",
        "retrospective": "This statement is written after the first four build steps and before the fifth, which makes it unusually literal: it records what exists and what does not rather than reconstructing a finished shape. The seven remaining actions, the unproved pack-bound retrieval and the unproved run-transactional memory write are named as open work.",
        "scope": "In scope: the Phase A\u2013D artefact set, the capability tables and their versioning, the pre-run readiness check, the pinned retrieval binding, and the memory write, consolidation and recall. Out of scope: the model loop (S13's `agent.run`), the interactive copilot surface (S10), embeddings and answer generation (the executor's), a tenant-authored prompt store, KAG, and S08's dropped `project_capability` ledger."
      },
      "created_at": "2026-11-28T14:55:05.705673+00:00",
      "id": "01a222a7-17ec-751f-bc1c-9e932f9b4763",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S15 Capability, Knowledge and Memory (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:55:06.005722+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Acceptance criteria

The conditions the architecture must meet to close Phase A.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a222a7-1881-7d2e-a3b5-928c86cb2b76",
      "approved_at": "2026-11-28T14:55:06.005722+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks: the capability aggregate resolves with its executor and versioned binding; readiness refuses a run with `409 capability_not_ready` before the first step; a cited passage comes from a pinned strata pack; memory is written, consolidated into `strata_rag` and recalled by `source_id`; and a degraded source mid-run reports `knowledge_degraded`, not the pre-run refusal.",
        "assignment": "Deliver the architectural record for a capability aggregate: a capability is a persisted thing with an executor spec, a versioned knowledge binding and memory, which a card's run resolves and readiness-checks before its first step. The assignment is retrospective \u2014 steps 1\u20134 shipped on 2026-11-09 and step 5's actions remain open \u2014 so this statement records the architecture and the remaining work together.",
        "constraints": "No second loop and no second retrieval path. The conveyor owns the binding and strata owns retrieval, so a run that read knowledge by any route other than `knowledge.query` would make the pinned pack advisory. Every new table carries a tenant column with row-level security forced, and documents are immutable \u2014 ingest a new version and prune the old, never update in place.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the capability model; the capability author writes the pack and its readiness declaration; the retrieval owner operates the strata pack. The retrospective author is not the accepting role, so the separation of duties holds.",
        "retrospective": "This statement is written after the first four build steps and before the fifth, which makes it unusually literal: it records what exists and what does not rather than reconstructing a finished shape. The seven remaining actions, the unproved pack-bound retrieval and the unproved run-transactional memory write are named as open work.",
        "scope": "In scope: the Phase A\u2013D artefact set, the capability tables and their versioning, the pre-run readiness check, the pinned retrieval binding, and the memory write, consolidation and recall. Out of scope: the model loop (S13's `agent.run`), the interactive copilot surface (S10), embeddings and answer generation (the executor's), a tenant-authored prompt store, KAG, and S08's dropped `project_capability` ledger."
      },
      "created_at": "2026-11-28T14:55:05.705673+00:00",
      "id": "01a222a7-17ec-751f-bc1c-9e932f9b4763",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S15 Capability, Knowledge and Memory (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:55:06.005722+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

```json
{
  "category": "statement-of-architecture-work",
  "count": 38,
  "rows": [
    {
      "approval_id": null,
      "approved_at": "2026-11-19T07:50:28.603416+00:00",
      "approved_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "body": {
        "acceptance_criteria": [
          "D1 \u2014 the artefact standard and repository: a catalog's entries are rows, an approved artefact is immutable and a supersede writes version 2, and a reuse link is searchable across plateaus.",
          "D2 \u2014 the Preliminary capability: the tailored profile names each phase's mandatory set by plateau, the board is a governance body row, and a maturity assessment carries all nine dimensions and a skills assessment all seven categories.",
          "D3 \u2014 the architecture project and phase gates: an approved artefact set and a live contract advance a phase, a draft artefact refuses the advance naming the artefact, and a contract change re-opens the gate.",
          "D4 \u2014 delivery compliance: a design gate without a live contract refuses, a superseded contract refuses naming its status, and one compliance level per review and a dated waiver without an owner are refused.",
          "D5 \u2014 change management: a review's exactly-one outcome is a change row or a kata candidate, and a review with no verdict cannot close the phase.",
          "D6 \u2014 the console surface: the Architecture destination renders its tabs from the live aggregates and each interaction records state."
        ],
        "assignment": "Deliver the TOGAF ADM capability as rows and actions in the one workspace: the artefact standard and repository, the Preliminary capability, the architecture project and its phase gates, delivery compliance, change management, and its console surface.",
        "responsibilities": "The programme architect (the maintainer sessions under the owner's authority) delivers and evidences each deliverable; the Architecture Board reviews and approves; the programme owner accepts the capability.",
        "scope": "The eleven tables of migration 0054 and the 26 architecture.* actions that expose them; the four ADM contract kinds; the seeded ten-type artefact standard; the Architecture Board; the maturity and skills baselines; and the Architecture capability screen (D6). The design is docs/plans/T9-architecture-capability.md."
      },
      "created_at": "2026-11-19T07:42:46.140382+00:00",
      "id": "01a1f2c2-0e1a-787f-9011-46b424d1ec71",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d35-7bd1-945d-cea12c5dfd6d",
      "requested_by": null,
      "status": "superseded",
      "superseded_by_id": "01a2023e-4e59-75ad-b2ab-fd7d014511d2",
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 the TOGAF ADM capability (T9)",
      "updated_at": "2026-11-22T07:52:45.884024+00:00",
      "version": 3
    },
    {
      "approval_id": "01a1fdd2-74eb-7fe3-a424-cdf02d021931",
      "approved_at": "2026-11-21T11:16:38.828544+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/P0.toml is green against the live record at the published SHA",
        "assignment": "Rework the programme definition project P0 by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526).",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "manifest": "acceptance/programme/P0.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the 21 programme-definition outputs, the charter and scope baseline, the benefit and governance baselines, the tranche-0 record and the closure review"
      },
      "created_at": "2026-11-21T11:16:21.046667+00:00",
      "id": "01a1fdd2-5041-7f80-8032-3305751d498c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1dcb8-d24b-76de-b901-56fd56e001bc",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 P0 programme definition (retrospective, U5a)",
      "updated_at": "2026-11-21T11:16:38.828544+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff2f-c76b-7bfe-b71a-a39294a95644",
      "approved_at": "2026-11-21T17:38:09.721121+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S01.toml is green against the live record at the published SHA; the decision record marks the unmeasured values not recoverable.",
        "assignment": "Rework the UI-stack spike by the reconstituted lifecycle (U5a, #526); the decision itself was made 2026-11-08.",
        "manifest": "acceptance/programme/S01.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "ADR 0001 and its measured table: interaction TTI, bundle, first render, drag frames, SSE\u2192DOM, memory, cold build and auth."
      },
      "created_at": "2026-11-21T17:36:38.122067+00:00",
      "id": "01a1ff2e-7974-74cd-be6d-377e3c0071a4",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-724c-9bae-af96b2d926b1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S01 (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:38:09.721121+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff37-f693-7336-8c6d-fe98f6f5c7b7",
      "approved_at": "2026-11-21T17:47:05.975622+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S00.toml is green against the live record at the published SHA; the inventory's 244 dropped rows are the gate's evidence.",
        "assignment": "Rework the usage inventory project by the reconstituted lifecycle (U5a, #526); the inventory itself was delivered 2026-11-07/08.",
        "manifest": "acceptance/programme/S00.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The 1,300-row parity inventory: 218 used, 295 rare, 543 unused, 244 dropped, across fr (798), RM (314), Planka (96) and n8n (92)."
      },
      "created_at": "2026-11-21T17:46:54.771712+00:00",
      "id": "01a1ff37-e240-779b-aab5-74a3fa86ecad",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S00 (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:47:05.975622+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff3c-8f0f-7326-b8a7-2de4d2eb6597",
      "approved_at": "2026-11-21T17:52:06.626934+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "Four, each with its check: (1) the public site renders the programme from Postgres \u2014 `data/snapshot.js` carries the programme's rows and the public sidebar names it only when the snapshot does (#470, 7d4e0d27); (2) a published output appears at its stable URL \u2014 165 committed output pages, every one named by `docs/design/docs/outputs.js` (#516, 06ba0fe3); (3) signed out there is no API request and no write control \u2014 `node docs/design/scripts/check-console.cjs`; (4) the pack's claims are checked like code \u2014 `build-coverage.py --check`, `build-docs.py --check` and the surface checks in the `design-pack` job of `.github/workflows/ci-public.yml`. The project's own gate is `acceptance/programme/DESIGN-PUBLISH.toml`.",
        "assignment": "Deliver to the programme owner, the Architecture Board and every outside reader the published studio design pack that makes the SPEC_V2 operating model readable and challengeable from a browser, and keep it live from the application image. This is the Phase A architecture work of a programme-level project in tranche 0, held to `docs/design/STUDIO-SPEC.md`. Recorded retrospectively on 2026-11-21: the pack was built and published by units #32, #33, #36, #37, #89, #97, #103 and #149, all closed.",
        "manifest": "acceptance/programme/DESIGN-PUBLISH.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The Phase A artefact set at plateau psa \u2014 stakeholder map (matrix), driver/goal/objective catalog, requirements catalog and solution concept (diagram) \u2014 with the Phase B\u2013D catalogues: business capability map, data entity catalog and technology standards. The published surfaces: `studio.html` offline, `https://conveyor.nonagentic.ai/design/` from the app image, `/design/data/snapshot.js` read from the tenant's rows, and `/design/docs/output-<id>.html` for every published output. Out of scope: any V2 backend aggregate, any signed-out write path, and the retired homepage mirror."
      },
      "created_at": "2026-11-21T17:51:55.933191+00:00",
      "id": "01a1ff3c-7aad-7dda-820e-1c7d85fe70a8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 DESIGN-PUBLISH: the published studio design pack",
      "updated_at": "2026-11-21T17:52:06.626934+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff41-2c98-7646-b835-7c7a96e1ca6e",
      "approved_at": "2026-11-21T17:57:08.762631+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest `acceptance/programme/SPEC-V2.toml` is green: the closure review exists, every declared closure output (`evidence`, `closure-review`) is published, and the project re-closes through the one gate decided by an approver other than its requester. The Phase-G compliance review records exactly one standard level against this contract and cites the published standing delegation. The seven A\u2013D artefacts are approved rows with their entries; no outcome-level claim substitutes for them.",
        "assignment": "Produce and publish SPEC V2 as the programme's adopted operating model, and prove it through the programme's executable acceptance. The work is the document and its decision set \u2014 `docs/SPEC_V2.md`, decisions D28\u2013D63 \u2014 with the ownership, organisational, work, decision, tool and rigour models it defines. This Statement of Architecture Work is recorded retrospectively on 2026-11-21 for the tranche-0 rework (issue #526) under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the project's real, contemporaneous statement was never recorded and is not recoverable.",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The V2 document and its decisions, and the phase A\u2013D architecture artefacts this contract governs: the stakeholder map, the driver/goal/objective catalog, the requirements catalog and the solution concept (phase A); the business capability map (phase B); the data entity/data component catalog (phase C); the technology standards catalog (phase D). Not in scope: V1's S-slices (not re-opened), the V2 delivery slices T0\u2013T17 (their own projects), and `docs/SPEC_LEGACY_KAG.md` (D54)."
      },
      "created_at": "2026-11-21T17:56:58.859998+00:00",
      "id": "01a1ff41-19f9-77bb-aa4a-f416f0d758e3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 SPEC-V2, the organisational operating model (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:57:08.762631+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff45-c6f7-7a7d-ab9f-0c9c8293147e",
      "approved_at": "2026-11-21T18:02:10.160888+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T0.toml is green against the live record at the published SHA: a job is created, a release is published, a binding pins its hash, a run on a retired binding refuses 409 job_binding_stale, no concept has two stores, and a test-only third executor kind registers without editing the run path. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T0 (SPEC_V2 \u00a712, issue #34) by the reconstituted TOGAF/MSP lifecycle: produce and publish the Phase A\u2013D mandatory artefacts, the charter and the delivery contract, the Phase-G compliance review and the closure review, declare the closure outputs and re-close through the one gate.",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/T0.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the model and its invariants (SPEC_V2 \u00a73 P1\u2013P11, \u00a74.1\u2013\u00a74.6, \u00a710.1, \u00a710.3): the organisation spine \u2014 industry, ordered departments, job_catalogue, the immutable job_release and the tenant-scoped job_binding; the decision-ladder types; and the internal Executor contract. The action contracts are those the registry serves and served-spec.lock.json pins. Execution behaviour is T2; the plan spine is T1; the tool platform is T13."
      },
      "created_at": "2026-11-21T18:02:00.126911+00:00",
      "id": "01a1ff45-b2ce-7276-8a88-cf3d744e7976",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7734-98da-fb7f1e2386c7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T0 Model and contracts (retrospective, U5a)",
      "updated_at": "2026-11-21T18:02:10.160888+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff84-57c3-7f67-9229-94bf0188732d",
      "approved_at": "2026-11-21T19:10:30.370289+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S02.toml is green against the published snapshot at the recorded SHA, with the S2 gate proven: two tenants isolated including RLS, and login works. The long-job lease/recovery proof under two workers was issue #2's P2; it is proved by `apps/server/tests/jobs_lease_two_workers.rs` at d367a4a9, and issue #2 closes with the lane PR.",
        "assignment": "Rework the delivered S02 skeleton by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526, tranche 1), recording the A\u2013D artefacts, the delivery contract and the Phase-G compliance review, and closing the project once the long-job proof issue #2 named is landed in this batch.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The ten-member workspace and one `conveyor` binary delivered by S02: eleven platform tables in `migrations/0001_platform.sql`, forced RLS on the nine tenant-owned tables, OIDC and development authentication, the fail-closed `app_licence` gate, the fenced job runner, the replaying `/api/events` stream, and the declared `shell/login` surface for S12."
      },
      "created_at": "2026-11-21T19:10:20.569282+00:00",
      "id": "01a1ff84-4425-75d9-8772-08e09cf71069",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S02 Skeleton (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:10:30.370289+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff89-1794-79d0-8707-f7614987ee7c",
      "approved_at": "2026-11-21T19:15:42.114015+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T1 gate: one project with a baseline, a change request and a refused self-approval; WSJF recomputes from inputs; the portfolio view lists the in-flight set; a dependency cycle is refused; a benefit profile without baseline, target, method or owner is refused; a benefit-map edge names its five levels. The ten issue-#72 scenarios are proved by the named real-Postgres tests in crates/actions/tests/plan_spine.rs and crates/api/tests/approval_dual.rs; the rework's acceptance instrument is acceptance/programme/T1.toml. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T1 (SPEC_V2 \u00a712, issue #72) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the plan spine and programme-definition rows of SPEC_V2 \u00a712: wbs_node, project_baseline, project_register_item, project_dependency, programme_cadence, programme_tranche, benefit_profile, benefit_link, benefit_transition, stakeholder_profile, programme_baseline, and the widened governed_asset kinds; the spine action families; and the requester/executor exclusion in approvals.decide. Out of scope: the decision ladder (T2), the PM job (T3), cadence materialisation and PI planning and the benefits-realization procedure (T10), metrics (T11), and any migration of V1's outline tree."
      },
      "created_at": "2026-11-21T19:15:31.506201+00:00",
      "id": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T1 Plan spine (retrospective, U5a)",
      "updated_at": "2026-11-21T19:15:42.114015+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff8d-b4cd-74f9-9f32-2b9f6cae33cd",
      "approved_at": "2026-11-21T19:20:43.741951+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T11.toml is green against the published snapshot at the re-close: every metric and every metering level recomputes from rows; the reconciliation with billing_service is exact at six decimals or opens an incident; a C0\u2013C5 level moves only when the measured evidence crosses its recorded threshold, naming that threshold version; a gate that never refused is flagged. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T11 (SPEC_V2 \u00a77.5, \u00a710.4, \u00a712; issues #84 and #458) by the reconstituted TOGAF/MSP lifecycle: produce and approve the Phase A\u2013D mandatory artefacts, this Statement of Architecture Work, the delivery contract, the Phase-G compliance review and the closure review; publish the declared outputs; and re-close the project through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The metrics, metering and maturity capability delivered by #84 (commit 183f5111, 2026-11-19) and the live Metering screen delivered by #458 (commit 0a8a6e95, 2026-11-20): migration 0059_metrics.sql, crates/domain/src/metrics.rs, crates/store/src/metrics.rs, crates/actions/src/metrics/mod.rs, the eight metrics actions, the billing.summary reconciliation input, the incident card type, the served-spec lock and the console screen. Charging, eval/benchmark-ledger quality metrics, knowledge metrics without ledgers and per-tool health are out of scope."
      },
      "created_at": "2026-11-21T19:20:34.079029+00:00",
      "id": "01a1ff8d-a0ad-7a91-b026-d9a2575daa3f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T11 Metrics, metering and maturity (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:20:43.741951+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2005a-8c6e-708a-84df-ab16d175072e",
      "approved_at": "2026-11-21T23:04:28.797665+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-21T23:04:18.681795+00:00",
      "id": "01a2005a-7886-7f2d-9b49-53d174544ee9",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:14.119814+00:00",
      "version": 3
    },
    {
      "approval_id": "01a20061-d19f-7269-9f5c-113f3aeb0001",
      "approved_at": "2026-11-21T23:12:26.539242+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC \u00a78 S3 gate: proptest on transitions and schema review. Property (a) \u2014 no transition leaves list.kind and archived_at contradictory, proved by board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error. Property (b) \u2014 claim not granted twice without release or expiry, proved as tested by claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic, with the lease-exclusivity half recorded as the P1 package v1-s03-1. Property (c) \u2014 one pending approval per (run, gate), proved by approval_race.rs::sixteen_racers_create_exactly_one_pending_approval. Property (d) \u2014 terminal run and dense revision, proved by transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense. Tenant isolation is proved by rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema review against data-arch \u00a72.3/\u00a72.4 admits the P2 package v1-s03-2. The rework's acceptance instrument is acceptance/programme/S03.toml; the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V1 slice S03 (SPEC \u00a76, \u00a78 S3) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate on the P0-only gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the baseline domain of SPEC \u00a75.3 and \u00a76: the fourteen tables of migrations/0002_domain.sql (programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state), the may_move matrix and the run/approval/asset state machines, the store `_tx` modules with TenantId, and the catalogue_entry.kind vocabulary. Out of scope: every route and action (S04\u2013S06), the board satellites (S05), channels and delivery (S06), outlines, quality and project_request (S07), models, views and portfolio registers (S08), documents and secrets (S09), conversations (S10), procedures (S11), workflow (S13) and the capability columns (S15)."
      },
      "created_at": "2026-11-21T23:12:15.079018+00:00",
      "id": "01a20061-bd73-7a89-b08c-bd8b867eb061",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S03 Domain and schema (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T23:12:26.539242+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20204-34e9-7eea-87d1-5b8265816c62",
      "approved_at": "2026-11-22T06:49:22.301988+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/PLATFORM.toml is green against the live record at the published SHA; the register's PLATFORM P0s are cleared or owned by a named executable package.",
        "assignment": "Hold the programme's definition, standards, gates and record; keep every project's acceptance manifest executable.",
        "manifest": "acceptance/programme/PLATFORM.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The programme capability: the one tenant, the programme definition set, the tranche plan, the artefact standard and the acceptance manifests; the programme record as reworked under #526."
      },
      "created_at": "2026-11-22T06:48:35.177183+00:00",
      "id": "01a20203-883b-7231-b0d3-dee948cf90e4",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work - PLATFORM (programme capability, retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T06:49:22.301988+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2023f-11ac-7fe6-bf6f-101ced8f4ba2",
      "approved_at": "2026-11-22T07:53:45.939115+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": [
          "D1 \u2014 the artefact standard and repository: a catalog's entries are rows, an approved artefact is immutable and a supersede writes version 2, and a reuse link is searchable across plateaus.",
          "D2 \u2014 the Preliminary capability: the tailored profile names each phase's mandatory set by plateau, the board is a governance body row, and a maturity assessment carries all nine dimensions and a skills assessment all seven categories.",
          "D3 \u2014 the architecture project and phase gates: an approved artefact set and a live contract advance a phase, a draft artefact refuses the advance naming the artefact, and a contract change re-opens the gate.",
          "D4 \u2014 delivery compliance: a design gate without a live contract refuses, a superseded contract refuses naming its status, and one compliance level per review and a dated waiver without an owner are refused.",
          "D5 \u2014 change management: a review's exactly-one outcome is a change row or a kata candidate, and a review with no verdict cannot close the phase.",
          "D6 \u2014 the console surface: the Architecture destination renders its tabs from the live aggregates and each interaction records state."
        ],
        "assignment": "Deliver the TOGAF ADM capability as rows and actions in the one workspace: the artefact standard and repository, the Preliminary capability, the architecture project and its phase gates, delivery compliance, change management, and its console surface.",
        "responsibilities": "The programme architect (the maintainer sessions under the owner's authority) delivers and evidences each deliverable; the Architecture Board reviews and approves; the programme owner accepts the capability.",
        "scope": "The eleven tables of migration 0054 and the 26 architecture.* actions that expose them; the four ADM contract kinds; the seeded ten-type artefact standard; the Architecture Board; the maturity and skills baselines; and the Architecture capability screen (D6). The design is docs/plans/T9-architecture-capability.md."
      },
      "created_at": "2026-11-22T07:52:45.884024+00:00",
      "id": "01a2023e-4e59-75ad-b2ab-fd7d014511d2",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d35-7bd1-945d-cea12c5dfd6d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": "01a1f2c2-0e1a-787f-9011-46b424d1ec71",
      "title": "Statement of Architecture Work - the TOGAF ADM capability (T9) - re-issued 2026-11-22",
      "updated_at": "2026-11-22T07:53:45.939115+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20256-80ac-72f0-a310-d114488f1d7c",
      "approved_at": "2026-11-22T08:19:17.038367+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": [
          {
            "criterion": "HTTP and MCP cannot diverge",
            "id": "a",
            "proof": "All 20 (16 internal + 4 fixtures.*) both ways; stripping request_id/duration_ms, http.body == mcp.structuredContent."
          },
          {
            "criterion": "One value, three projections",
            "id": "b",
            "proof": "spec_hash: manifest == resolved operation == MCP tool."
          },
          {
            "criterion": "Duplicates fail closed",
            "id": "c",
            "proof": "Exit 1 on a duplicate name, an MCP collision or an out-of-charset name."
          },
          {
            "criterion": "Schema-invalid input",
            "id": "d",
            "proof": "RFC 9457 with populated errors[], never a 500."
          },
          {
            "criterion": "Idempotent replay",
            "id": "e",
            "proof": "Two identical POSTs with one key: same status and body, one action_invocation."
          },
          {
            "criterion": "The lock catches drift; breaking ingest refused",
            "id": "f",
            "proof": "A renamed field makes --check-lock exit 1 naming the operation; the removed-field fixture exits 2 (422) while the approved version serves."
          }
        ],
        "assignment": {
          "depends_on": [
            "S02",
            "S03"
          ],
          "gate": "SPEC section 8 S4 - 20 actions identical over HTTP and MCP",
          "issue": "#13",
          "plan_status": "planned",
          "programme_project": "S04",
          "spec_sections": [
            "section 5.5",
            "section 7",
            "section 8 S4",
            "section 3 D11"
          ],
          "title": "Action registry - derive, manifest, OpenAPI, MCP, invocation, external ingest"
        },
        "manifest": {
          "adds_actions": [
            "actions.list",
            "actions.describe",
            "me.get",
            "tenants.get",
            "tenants.update_settings",
            "members.list",
            "members.set_role",
            "departments.list",
            "departments.create",
            "audit.list",
            "jobs.list",
            "jobs.retry",
            "schedules.list",
            "schedules.upsert",
            "events.list",
            "licences.get",
            "fixtures.echo",
            "fixtures.slow",
            "fixtures.fail",
            "fixtures.timeout"
          ],
          "adds_events": [],
          "adds_modules": [
            "actions"
          ],
          "adds_tables": [
            "action_registry"
          ],
          "deliverables": [
            "crates/actions-derive/src/lib.rs",
            "crates/actions/src/{spec,registry,invoke,mcp,openapi,lock,ingest}.rs",
            "crates/actions/tests/{registry,parity_http_mcp,idempotency,timeout,ingest_diff,external_actions}.rs",
            "crates/actions/tests/fixtures/external/openapi.json",
            "crates/{api,store}/src/actions.rs",
            "migrations/0003_action_registry.sql",
            "served-spec.lock.json"
          ],
          "source_pins": {
            "agent": "83b8de7e",
            "fr": "4a579e91",
            "fr_source_map": "6f7c5097",
            "pinned": "2026-11-07"
          }
        },
        "responsibilities": [
          {
            "responsibility": "Build crates/actions-derive and crates/actions, wire the boot assertion before TcpListener::bind, and add migrations/0003_action_registry.sql.",
            "role": "S04 delivery"
          },
          {
            "responsibility": "Supplies begin_scoped, audit_event, action_invocation, idempotency_key, event and /api/me.",
            "role": "S02"
          },
          {
            "responsibility": "Supplies the 14 domain aggregates and transitions used by the 16 internal actions.",
            "role": "S03"
          },
          {
            "responsibility": "Add domain actions and their own events in the same transaction; later slices add actions, never a surface.",
            "role": "S05-S11"
          },
          {
            "responsibility": "Provides the kind: workflow engine; until then workflow returns 501.",
            "role": "S13"
          },
          {
            "responsibility": "Supplies channel-mcp delivery rows and the signed mcp_server channel (S06-alpha).",
            "role": "S06"
          }
        ],
        "retrospective": "Planned, not yet delivered. Decisions closed here: S4-D1 derive crate is the 10th member; S4-D2 one literal route per action plus a spec_exempt fallback; S4-D3 enable/disable is approved_at; S4-D4 an external name colliding with any name is refused at ingest; S4-D5 idempotency scope is the action name; S4-D6 scope-filtered MCP tools with the refused tool absent; S4-D7 workflow returns 501 until S13. Traps to carry: four artifacts drifting is the fr lesson; Router::merge drops paths; MCP tool names are not action names; a timeout is the caller's; idempotency and the mutation must share a transaction; circular actions hit invocation_depth 8 and 409 action-recursion-limit.",
        "scope": "IN: ActionSpec plus #[action]; the compile-time registry and boot assertion; GET /api/actions; POST /api/actions/{name}/invocations with idempotency, timeout, audit, outbox; rmcp at /mcp; the OpenAPI fold and lock; openapiv3 ingest; migrations/0003_action_registry.sql; 16 internal plus 4 fixtures.* actions. OUT: domain actions (cards.move, ...) are S05-S11's; kind: workflow (S13); channel-mcp delivery rows (S06); the signed mcp_server channel (S06-alpha)."
      },
      "created_at": "2026-11-22T08:19:08.182233+00:00",
      "id": "01a20256-6ede-7d70-aae8-5dbfe681a1b7",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-7eb6-a62a-f420e742595c",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work - S04 Action registry (retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T08:19:17.038367+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20273-4f0f-77e6-8005-9744d1e3a574",
      "approved_at": "2026-11-22T08:50:49.577591+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": "# Statement of Architecture Work \u2014 T13 Tool platform\n\n## Context\nSPEC_V2 \u00a711 replaces ad-hoc backend calls with one governed path. The conveyor already owns actions, secrets, entitlements, audit and a durable queue, so a second catalogue/queue/runner (the predecessor MCP server) is refused (D44, D59). T13 is the slice that makes the one path real.\n\n## Scope\nThe broker `tools.call` and its seven steps; the four catalogue/ledger tables; effect-class gating; the closed-schema input check; credential aliases over sealed secrets; the direct MinIO storage adapter with the mandatory tenant prefix; the coding toolkit and one kag-era toolkit; V1 external-action migration; six gate proofs.\n\n## Approach\nImplement the broker and storage layers in the conveyor workspace, prove each refusal before dispatch at the wire (a counting stub), record one `tool_call` and one `tool.called` event per call, and record every artefact of this rework retrospectively from \u00a711 and the tests rather than from memory.\n\n## Deliverables\n`crates/actions/tests/tool_platform.rs` (the six proofs and the extra broker tests); `conveyor_actions::storage::{JobStorage, TENANT_PREFIX, sha256_hex}`; `conveyor_store::tool::{calls_for_run, ToolCallRow}`; `conveyor_actions::prompt::compile_for_audit`; the reused `conveyor_store::secret::create_secret_tx`; tables `tool`, `toolkit`, `tool_binding`, `tool_call`; action `tools.call`; event `tool.called`.\n\n## Constraints and standards\nRust workspace; PostgreSQL 18; MinIO (S3-compatible) as the platform store; a closed JSON-Schema subset (additionalProperties false, enums, patterns, bounds); OpenAPI 3.1 and streamable-HTTP MCP as the only two BYO forms; idempotency keys on writes and bounded polls.\n\n## Acceptance\nThe six \u00a712 clauses and their named tests, as written in `acceptance/programme/T13.toml`; the manifest is the gate, not this document.\n\n## Risks\nThe suite serialises because the destination URL is process-global; the storage test path is test-only until live MinIO wiring exists; the catalogue write path is deferred to T14; a production binding writer is still absent.\n\n## Decisions closed by this rework\nThe binding row is the broker's authority; refusals are proved on the wire; `tool_call` is not `action_invocation`; the storage adapter builds every key, so traversal and hash mismatches are refused.\n",
      "created_at": "2026-11-22T08:48:20.084131+00:00",
      "id": "01a20271-2a37-7329-b159-28031ddcb966",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T13 Tool platform (retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T08:50:49.577591+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20287-813e-7abd-83c5-ddf3bb9e98b3",
      "approved_at": "2026-11-22T09:12:50.020166+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T10 gate: PI planning materialises cards from the backlog by WSJF within WIP; a deliverable without evidence cannot demo; retro produces lessons; an end-of-tranche review with no baselines, benefit status or risk evidence is refused; a transition that never measured its baseline cannot report the benefit realized. Clauses 2, 3 and 5 are proved by named tests; clause 1 is proved for a project backlog and not for the programme-wide backlog (v1-p82-5); clause 4 is unproved at HEAD because free-text placeholders satisfy the evidence check (v1-p83-1) and the baseline requirement is not gated (v1-p83-4). The rework's acceptance instrument is `acceptance/programme/T10.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T10 (SPEC_V2 sections 8.2 and 8.3 and section 12, issues #82 and #502) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the SAFe cadence and the MSP tranche machinery of SPEC_V2 sections 8.2 and 8.3: the cadence state machine, WSJF and WIP-bounded materialisation behind an approved PI plan, backlog refinement, the demo credit, the retro lesson requirement, the tranche review and its evidence, and the benefit transition with its baseline measurement. Out of scope: the decision ladder (T2), the plan-spine baseline contract (T1), the coach card (T4) except the retro card, metrics (T11) and the one-organisation-loop acceptance (T12)."
      },
      "created_at": "2026-11-22T09:12:38.058104+00:00",
      "id": "01a20287-696c-76a4-a2db-28c6092b5813",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-70c1-a5e8-f50956c5e3f9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T10 SAFe cadence and MSP tranches (retrospective, U5a)",
      "updated_at": "2026-11-22T09:12:50.020166+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2028b-f63c-7c27-b580-d4cba2c1e70f",
      "approved_at": "2026-11-22T09:17:42.179607+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T4 gate: an injected failure yields exactly one coach card and one remedy; a repeat signature escalates; no coach card about a coach card; a systemic class opens a kata. Clauses 2 and 3 are proved by named tests in `crates/actions/tests/coach.rs`; clause 1 is proved for a first interruption but refuted as a universal by v1-p6-4; clause 4 is proved only as a `kata.open` process start and v1-p6-2 shows no kata record, instrument or threshold is created. The rework's acceptance instrument is `acceptance/programme/T4.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T4 (SPEC_V2 sections 6.2, 6.4 and 12, issue #75) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the interruption taxonomy, the closed remedy set, per-signature uniqueness and the depth cap of SPEC_V2 section 6.2, plus the systemic-class path of section 6.4: the `coach_interruption` and `coach_remedy` tables, the `coach.interrupt` action, the seeded coach process and the real-Postgres suite `crates/actions/tests/coach.rs`. Out of scope: the board and pull path's `err.card` producer, the kata's own lifecycle and measurement (T5), the PM job (T3), operations (T7) and the tool platform (T13)."
      },
      "created_at": "2026-11-22T09:17:30.02765+00:00",
      "id": "01a2028b-dde6-7ed4-9e47-6e582f2341d8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T4 Kanban coach (retrospective, U5a)",
      "updated_at": "2026-11-22T09:17:42.179607+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20ce0-644d-7340-a696-1af10fa8c61e",
      "approved_at": "2026-11-24T09:26:06.517885+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The six \u00a712 gate clauses plus D1\u2013D8; the live gate recorded in docs/plans/T19/evidence.md; the closure decision in docs/plans/T19/closure-review.md.",
        "assignment": "Build T19 \u2014 decision forums \u2014 to the \u00a712 gate: a meeting is a card whose agenda items resolve approvals, whose participants hold typed positions, and whose outcomes are derived arithmetic over those positions.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the SRO accepts the benefit B7 and the tranche evidence.",
        "scope": "The meeting aggregate, the approval_decision participant extension, the minutes governed-asset kind, the twelve meetings.* actions, the Meetings destination and the two consumer seams (governance_body, programme_cadence)."
      },
      "created_at": "2026-11-24T09:25:57.096759+00:00",
      "id": "01a20ce0-5183-776c-84bd-327f2301b19b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b9b8-7214-abc6-f2c4f75a1fc1",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T19 decision forums (2026-11-24)",
      "updated_at": "2026-11-24T09:26:06.517885+00:00",
      "version": 2
    },
    {
      "approval_id": "01a210bd-cdb2-7f2f-ac9a-c314fb1856af",
      "approved_at": "2026-11-25T03:26:48.7374+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The T19F plan's P1\u2013P5 scenarios, the live gate recorded in docs/plans/T19F/evidence.md, and the closure review in docs/plans/T19F/closure-review.md that defers project closure to the tranche-6 review.",
        "assignment": "Build T19F \u2014 the forum bindings \u2014 to the plan's P1\u2013P5: a meeting_type version is written only by the reviewed authoring route, a board subject is decided in a sitting with via='meeting' rows, and a cadence row materialises its own meeting exactly once.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the programme manager owns B7's measurement; the SRO accepts the tranche-6 evidence.",
        "scope": "Migration 0073; the authoring publish hook and meetings.type_list/type_get; architecture.board_sit_open/board_sit_decide; cadences.materialise; the mock-UI and real-app surfaces; the programme record and its evidence."
      },
      "created_at": "2026-11-25T03:26:38.984805+00:00",
      "id": "01a210bd-ba25-78c1-a363-3eea5aa71335",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T19F forum bindings (2026-11-25)",
      "updated_at": "2026-11-25T03:26:48.7374+00:00",
      "version": 2
    },
    {
      "approval_id": "01a217fb-a503-7db9-b47b-55a111c1c8de",
      "approved_at": "2026-11-26T13:11:37.614131+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate in SPEC_V2 \u00a712 is executable: a PM runs a request end to end; the PM cannot approve its own gate; a baseline change requires a change request. The close gate reads the project's declared outputs \u2014 project-brief and evidence \u2014 plus the closure-review output, a live board-approved delivery contract, a Phase-G review citing the standing delegation, the approved scope baseline, the seven mandatory A\u2013D artefacts with their registers, WBS inputs 8, 8, 5, 3, a demo credit per accepted deliverable and at least one tranche lesson. This content pack is checked offline by scripts/rework-content-check.py --project T3 before the factory reads it, and the factory then records the contract, review, artefacts and closure against the snapshot.",
        "assignment": "Rework T3 by the reconstituted TOGAF/MSP lifecycle: author the charter, this Statement of Architecture Work, the seven mandatory Phase A\u2013D artefacts, the delivery contract, the WBS deliverable and the Phase-G review, and re-publish the project-brief and evidence outputs. The architecture work is the PM job itself \u2014 one project run from charter through WBS, scope baseline, staffing, gate routing and closure, with the approver kept distinct from the requester and every baseline change carried by a change request \u2014 recorded retrospectively and dated 2026-11-26.",
        "contract": "This statement is authored retrospectively under the delegation and is approved through the same board gate it describes; its approval is recorded by the factory from this pack, not asserted here. It is complemented by the delivery contract whose commitments, compliance and approval fields this pack authors, on the approved scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa (kind scope, taken 2026-11-26, approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545), and by the Phase-G review the factory records from this pack. The tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9 decided continue on 2026-11-26 and authorised tranche 4; the reopen this pack answers is dated 2026-11-26T07:44:41Z and its reason is the missing contract, review and baseline record.",
        "responsibilities": "The programme architect authors the pack under the #526 rework delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14; the Architecture Board reviews the seven A\u2013D artefacts, which this pack records as approved. The programme owner is the approver of the delivery contract and of the closure: the scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa is approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545, and the closure gate recorded for T3 was decided by an approver distinct from its requester, which is the gate's own rule. The PM runs the request and may not approve a gate it raised.",
        "retrospective": true,
        "scope": "In scope: the charter and its approved gate; the WBS with the four typed WSJF inputs 8, 8, 5, 3; the scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa taken on 2026-11-26 and approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545; the staffing plan; the gate router that refuses self-approval and records every decision; the change request that must precede a baseline change; the closure record with its closure-review output; the seven A\u2013D artefacts and the project registers. Out of scope: the T2 decision ladder's policy engine, the T4 coach, the T5 kata, the T13 tool platform, the T1 plan spine's implementation and every V1 slice; the rework completes the record and adds no scope."
      },
      "created_at": "2026-11-26T13:11:37.331166+00:00",
      "id": "01a217fb-a475-7824-b01c-bfcfcb4680d3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7915-8833-75f4dc31cc07",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T3 PM job (retrospective, 2026-11-26)",
      "updated_at": "2026-11-26T13:11:37.614131+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2180f-168b-79af-af7f-e4cc18948e7b",
      "approved_at": "2026-11-26T13:32:51.863706+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-26T13:32:51.58027+00:00",
      "id": "01a2180f-15fe-794d-aba4-395c2f06a32f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:51.863706+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d8-8c6d-715e-ad85-da38d5f3351c",
      "approved_at": "2026-11-26T21:52:31.994413+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S05.toml` and plan \u00a77: (a) replay fidelity \u2014 1,000 cards/10 lists, `admin diff` prints zero deltas; (b) one winner \u2014 two clients with the same `If-Match` get one 200 and one 412 carrying the current representation, `version` +1; (c) 1k load \u2014 `GET /api/boards/{id}/cards?limit=200` p95 \u2264 120 ms at 20 rps, 1,000 cards in five pages \u2264 500 ms; (d) webhook shape \u2014 `{event,data,prevData,user}` exactly, `event` the internal register type and `data.list_id`/`prevData.list_id` the lanes; (e) SSE \u2014 a second client on `board:{id}` gets `card.moved` \u2264 200 ms live and on replay; (f) no second store \u2014 one `comment`, one `board_membership`, no `card_membership`, no `attachment`. The instrument is `acceptance/programme/S05.toml`; the closure gate is decided by an approver other than its requester. The snapshot records no Phase-G review of these properties; this rework records that gap rather than claiming a pass.",
        "assignment": "Rework the V1 slice S05 (SPEC \u00a78 S5) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the acceptance instrument.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the board slice of the plan: the nine tables (board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription), the 39 actions, the read projections and cursors, the SSE topics `board:{id}` and the resolved `card:{id}`, the webhook emitter and its `webhook.deliver` job, and the key-referenced `board-export/v1` CLI. Out of scope: attachments and card-level memberships, claims/assignee/stopwatch writes (S06), webhook delivery rows as actions, and the UI (S12)."
      },
      "created_at": "2026-11-26T21:52:31.682863+00:00",
      "id": "01a219d8-8bc7-71a2-8dbe-33702699eac1",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S05 Board (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:52:31.994413+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d9-303c-7fdc-b217-ec349e82329b",
      "approved_at": "2026-11-26T21:53:13.933646+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "Acceptance criteria": "Phase A closes when this statement is approved, the seven A\u2013D artefacts are approved in standard order, and the seven artefacts each carry metadata, entries, links and a document section with authored prose. Delivery acceptance is the plan \u00a77 gate: (a) three revisions with max(seq)=3 and a compare showing the status change; (b) an idempotent seed returning {0,0,N}; (c) both GIN indexes used under enable_seqscan=off with rising idx_scan; (d) relevance where B meets \u00a75.6's recall@5 \u2265 0.75, MRR \u2265 0.60 and B \u2265 C; (e) traceability in both directions with zero cross-tenant rows; (f) quality scoring read-only; (g) no orphan over the 28 RM modules. The record-level acceptance is `acceptance/programme/S07.toml`, whose 17 checks are 15 P0 and two P1.",
        "Assignment": "The assignment is to give the conveyor one governed requirements surface: requirements, principles and decisions share a single `governed_asset` lifecycle carried by S03, every mutation writes one immutable `revision`, `requirements.compare` diffs two revisions in Rust, quality is scored adviserily on a revision, and search ranks assets, cards and catalogue entries through SQL. The work is asked for by the tranche-4 sponsoring group and accepted by the Architecture Board; this statement is authored retrospectively on 2026-11-24 under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 for project 01a1d039-0d2c-7322-b2f9-4dd041153894.",
        "Contract": "The contract record is the delivery contract of project 01a1d039-0d2c-7322-b2f9-4dd041153894, version 1, approved by the Architecture Board on 2026-11-24 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. Its compliance clause is plan \u00a77 and the acceptance instrument `acceptance/programme/S07.toml`; its manifest names that instrument; its acceptance date is the snapshot of 2026-11-24, when the project row is closed at version 7 and tranche 4 (01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce) is authorised by the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9. The close gate is decided by an approver other than the requester.",
        "Responsibilities": "The programme architect (conveyor-e2e) is responsible for the plan, the seven artefacts, this statement, the delivery contract and the retrospective pack. The Architecture Board (conveyor-architecture-board) reviews and approves the artefact set and the Phase-G review. The S03 and S04 owners are consulted because `governed_asset`, `revision`, `catalogue_entry`, `trace_link` and the action registry are their surfaces; S08 and S12 are informed as consumers. The approver of the closure gate is deliberately a different principal from the requester under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14.",
        "Scope": "The phase set is A to D: the stakeholder map, the driver/goal/objective catalog, the requirements catalog, the solution concept diagram, the business capability map, the data entity/data component catalog and the technology standards catalog. The boundary is the plan \u00a75 material: the five \u00a75.1 tables, the 32 actions, the `search` module, the catalogue seed, the quality rubric and job, the eval set and the traceability actions. The project does not build the TOC API, ArchiMate or UML renders, project CRUD, batch bulk operations, RM's promote/project-copy or any UI, and it does not recreate `outline`/`outline_node`, which migration 0011 dropped."
      },
      "created_at": "2026-11-26T21:53:13.637379+00:00",
      "id": "01a219d9-2fa8-703a-99f2-fb7e7572e467",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S07 Requirements and search (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:53:13.933646+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d9-f717-73ba-9c86-18adeeecd4ef",
      "approved_at": "2026-11-26T21:54:04.836362+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The plan \u00a77 gate: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, then delete yields GET 404 and object_store::head NotFound, asserted in documents_roundtrip; (b) cap \u2014 10,485,760 bytes accepted and one byte more refused 413 application/problem+json with a stable type in documents_limits; (c) secret \u2014 rotate to version 2 and GET returns no plaintext, value key or ciphertext in secrets_never_revealed; (d) workers \u2014 the page renders live rows and a stopped service yields a banner with the breaker open after five failures; (e) reports \u2014 every number equals its SQL count on the same snapshot in reports_match_sql; (f) externals \u2014 GET /api/actions carries timeout_ms on every external and zero reqwest:: outside the executor in externals_declared; (g) billing \u2014 no write action and no metering event in the manifest. The same instrument also requires migration 0013 applied twice as a no-op, FORCE RLS on all four tables, and the nine declared event types. The snapshot records no run of these properties.",
        "assignment": "Rework the V1 slice S09 (SPEC \u00a78 S9) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, this charter, this Statement of Architecture Work and the delivery contract, take the scope baseline with one WBS deliverable, record a retrospective Phase-G review at level consistent, and publish the expected outputs project-brief and evidence. Because all nine linked issues are recorded closed, this rework re-closes the project at level consistent through the acceptance manifest, with the closure and closure review dated 2026-11-24.",
        "contract": "This Statement establishes the delivery contract against acceptance/programme/S09.toml, whose gate text names the seven properties above. Compliance is assessed clause by clause at the 2026-11-24 retrospective snapshot; the project's published evidence page is 01a1e687-7682-753d-b60e-1f00243b13a3 and its pre-rework audit pack is docs/evidence/rework/S09.json, which records verdict red on the missing baseline, charter, contract, Phase-G review and closure outputs. The rework supplies the lifecycle record and re-closes the project retrospectively on the P0 checks of the manifest; the remaining verification limits are carried in the closure review, the review and the evidence output rather than converted into a pass.",
        "responsibilities": "The programme architect (conveyor-e2e) owns the slice plan and this retrospective set; the Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract; the approver (conveyor-approver) decides the Phase-G level; the S03 team supplies tenancy, trace_link and revision, and the S04 team supplies the action registry with its OpenAPI and MCP projection. Operators own the KEK source in the pod environment and the MinIO bucket policy.",
        "scope": "the four tables document, repository, project_repository and secret in migrations/0013_documents.sql; the integrations module (object_store, external, breaker, ingest); documents.create/get/list/download/update/delete/report, 26 of the front matter's 27 actions in all; the nine event types; MinIO bucket conveyor-documents with key t/<tenant>/<document>/<version>; the KEK/HKDF/XChaCha20-Poly1305 secret envelope; repositories as references; the read-only workers, billing and observability displays; report queries over local tables; and the strata handoff. Out of scope: billing logic and metering writes, worker scheduling, artifact objects, code analysis, licence issuance, ingestion and embeddings as services, screens (S12) and project_request.repository_url (S07)."
      },
      "created_at": "2026-11-26T21:54:04.526424+00:00",
      "id": "01a219d9-f671-708a-b28f-063e7e79e6ec",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S09 Documents and integrations (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:54:04.836362+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219dd-4701-7342-9adf-f2c47331c4af",
      "approved_at": "2026-11-26T21:57:41.900644+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T14 gate, executed on the live estate through `acceptance/programme/T14.toml`: (a) each published toolkit has a probe and one live call; (b) `scientific_processor` is scaled up for its probe and one live call, then scaled back, with a receipt; (c) a zero-use report exists; (d) a backend outage unbinds new runs while in-flight runs finish. The snapshot carries no executed receipt for any of the four clauses, so this rework records them as the project's open verification item rather than a pass; the level written is consistent, never non_conformant without a waiver, and no waiver exists.",
        "assignment": "Rework the V2 slice T14 (SPEC_V2 \u00a712) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, record the Phase-G compliance review at the level the record supports, publish the expected outputs (`project-brief`, `evidence`) and leave the project active with its gate recorded as open verification rather than closing it.",
        "contract": "No live Statement of Architecture Work and no live delivery contract exist for T14 at the snapshot; this pair is supplied retrospectively by the 2026-11-24 rework and is the contract of record until a Board-approved one supersedes it. It binds the project to the four gate clauses above, to broker-only calls inherited from T13, to draft status for the named draft services, and to a receipt per claim; it does not authorise closure. The standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 is the authority under which the rework acts.",
        "responsibilities": "the conveyor-approver approves the Phase A\u2013D set and the delivery contract; the conveyor-architecture-board governs the standards; the programme architect (conveyor-e2e) owns T14 and this retrospective pack; the T13 tool-platform team owns the tool family, the broker, the effect classes and the credential profiles; the estate service owners grant the live-call window and the outage drill; the SRO is accountable for the tranche outcome.",
        "retrospective": "written by the 2026-11-24 tranche-4 rework; it reconstructs the record and does not attest contemporaneous intent.",
        "scope": "the estate-toolkit import wave on T13's tool platform: the `tool`, `toolkit`, `tool_binding` and `tool_call` rows for the coding fleet, strata, billing and licence reads, webscraper, media_discovery, scientific_processor, lean4_prover, video-compiler, the w2v orchestrator and the ops read tools; `evidence_review`, `discovery` :8807 and the planned AHTAPol services as `draft`; the probe and the one live call per published toolkit; the `scientific_processor` scale-up and scale-back receipt; the zero-use report; and the backend-outage unbind test. Out of scope: the broker itself (T13), tool authoring and BYO (T15), and the UI."
      },
      "created_at": "2026-11-26T21:57:41.6166+00:00",
      "id": "01a219dd-4673-70da-b701-3f43675b4fa6",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T14 Estate toolkits (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:57:41.900644+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219dd-edc0-7da7-a7fd-00eafabf0eae",
      "approved_at": "2026-11-26T21:58:24.588139+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The pack passes `python3 scripts/rework-content-check.py --project T16 --content docs/evidence/rework/tranche-4/rework-content-T16.json --snapshot /tmp/conv526-w1-snap/data --wsjf 10,8,7,8 --closes false` with exit 0; every cited identifier exists in the snapshot; the seven artefacts carry the existing titles, phase, plateau and kind in the required order; each artefact body carries Metadata, Entries, Links and Document as authored prose; the project brief and evidence carry their template sections with substance; and no closure key appears. The project's own gate remains acceptance/programme/T16.toml, and this review's verdict is consistent.",
        "assignment": "Under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, the retrospective completes T16's governance record against acceptance/programme/T16.toml for the procedure-ingestion slice of SPEC_V2 \u00a712. The assignment is record completion, not rework of the built capability: author the charter, the Phase A\u2013D artefact set, this Statement of Architecture Work, the delivery contract, the Phase-G review and the published project brief and evidence, and leave the project active for a separate close decision.",
        "contract": "The record is delivered under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 against scope baseline 01a216d0-f529-7bcb-81a7-d429d008d638 (approved 2026-11-24T16:58:21Z) and tranche 4 01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce. The delivery contract below records compliance, acceptance, approval, manifest and retrospective limits. Any change to scope or gate returns to the Architecture Board; no waiver is claimed or required, and the project stays active for the tranche-4 review to decide continue.",
        "responsibilities": "The programme architect authors the record under the delegation; the Architecture Board reviews contract and Phase-G conformance; procedure owners and publishers own the sources and the publication decision; delivery teams consume the ingestion and PDDL surface; tenants own their rows and sources; the programme owner owns the outcome and any close decision. scripts/rework-project.py performs the live writes, and scripts/rework-content-check.py refuses this pack before any write if it is structurally wrong or cites an identifier absent from the snapshot.",
        "scope": "In scope: SOP intake; RDF/OWL/Turtle as a strata knowledge pack bound by knowledge_pack_ref; BPMN, Visio and draw.io into PDDL procedure drafts; capability, agent and job design from an SOP; the source-review and human publication gates; re-ingest versioning; bound PDDL execution; and the governance records named in the assignment. Out of scope: any runtime RDF store, graph store or reasoner (decision D53, owner 2026-11-19), T17's tenant-wide generalisation, T19's meetings records, the /procedures destination work of issue #448, and new ingestion scope."
      },
      "created_at": "2026-11-26T21:58:24.307416+00:00",
      "id": "01a219dd-ed35-79eb-b898-3323854d5411",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d38-76fb-9394-b75934d16fcd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T16 SOP, RDF/OWL and process-document ingestion (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:58:24.588139+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219de-7e7e-72db-b751-a17bdd9b013d",
      "approved_at": "2026-11-26T21:59:01.639909+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T18 gate, judged by the fifteen checks of acceptance/programme/T18.toml: every destination route exists with its spec, each spec asserts empty, error, happy and one refusal, the coverage record reports exists only from a real route and a present spec with a *Gate reported as extend naming its owner, no production screen renders a fixture, and a signed-in tenant drives portfolio \u2192 programme \u2192 project \u2192 work package, organisation \u2192 release \u2192 binding, procedures intake \u2192 publish, tools call \u2192 ledger, improve \u2192 kata \u2192 adoption, operations incident \u2192 postmortem and the ADM phase rail from the app, with the T12 loop as the final spec.",
        "assignment": "Rework the V2 delivery slice T18 (SPEC_V2 \u00a710.5 and \u00a712 T18) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter, the Statement of Architecture Work and the delivery contract, take the scope baseline with the one WBS deliverable, record the retrospective Phase-G compliance review, publish the expected outputs project-brief, inception-analysis and architecture-design plus the evidence output, and leave the project active because T18-4 and T18-5 are not delivered. The work is retrospective: it reconstructs the governance record from the project row, the plan, the route table and the issue registry, and it attests no contemporaneous intent.",
        "contract": "The delivery contract record is requested and approved live by scripts/rework-project.py under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 and is titled 'Delivery contract \u2014 T18 The real V2 UI (retrospective, 2026-11-24)'. It commits the lane to the \u00a712 gate and to the two open units as typed dependencies, and it records that this retrospective neither closes the project nor waives any check. The contract's approval is retrospective; the historical snapshot has no live contract, which is one of the failed checks in the prior acceptance record.",
        "responsibilities": "The T18 lane under the programme architect (conveyor-e2e) is responsible for the route table, the screen modules, the specs and the coverage instrument. The Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract. The sponsoring group is accountable for the tranche-4 authorisation and for keeping the project active. The owning slices T4, T5, T7, T9, T10, T11, T13, T16 and T17 are responsible for the actions each destination calls, and T12 owns the final organisation loop.",
        "retrospective": true,
        "scope": "the shell, route registry and navigation; the ten V2 destinations of SPEC_V2 \u00a710.5 and the execution room; the L1\u2013L4 delivery view; one Playwright spec per destination; the V2 coverage instrument measured from apps/web/src/app.rs and the named specs; and the T12 organisation loop as the final acceptance. Out of scope: new tables, actions, events or ports; SSR, a second router or frontend, or a mobile app; retiring or rebuilding the design pack; the V1 routes, which are linked rather than re-implemented; and any production fixture or live tenant write inside the retrospective."
      },
      "created_at": "2026-11-26T21:59:01.357241+00:00",
      "id": "01a219de-7def-7827-8a21-98428640f502",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T18 The real V2 UI (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:59:01.639909+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21a39-aae9-7d90-9069-b0576ed463d4",
      "approved_at": "2026-11-26T23:38:36.773008+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S06.toml` and plan \u00a77: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 `cards.move(done)` yields `run.succeeded`, `approval.approved` and one audit, invocation and event row each; (b) dual control \u2014 `required_decisions=2`, one approve stays pending, the same approver again is 409, a second approver closes; (c) retry \u2014 `kill -9` mid-run leads to `run.reap`, a crashed run, then `attempt=2` with two rows and never two live runs; (d) andon \u2014 engaged gives 409 and a pending `run.execute` for 60 s, released resumes within one poll; (e) delivery retry \u2014 a stub 500 gives `attempts=5`, `state=failed`, one row; (f) no timers \u2014 `rg 'setInterval' crates apps` is empty and every cadence is a `schedule` row; (g) human vs agent \u2014 `decided_by_kind` is `human` for one decision and `agent` for the other. The instrument is `acceptance/programme/S06.toml`; the close gate is decided by an approver other than its requester. The snapshot records no run of these properties and no Phase-G review; this rework records the level `non_conformant` rather than claiming a pass.",
        "assignment": "Rework the V1 slice S06 (SPEC \u00a78 S6) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure decision, and re-close through the acceptance instrument once its properties run.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the control-plane slice of the plan: the six tables of `02-DATA-ARCHITECTURE \u00a72.6` (`channel_binding`, `channel_preference`, `message`, `delivery`, `audience`, `audience_member` \u2014 the last two PENDING-S0), the 17 actions, the 17 events, the SSE topics `run:{id}`, `approval:{id}` and `tenant`, the Telegram in/out path with its sender-identity resolution, the andon gauge and poller pause, and the job kinds that replace 11 of fr's 14 timers (three deleted). Out of scope: the signed mcp_server channel (S06a), MCP tool surfacing (S10), `project_request.timeout` (S07), the UI (S12) and the audience screens (PENDING-S0)."
      },
      "created_at": "2026-11-26T23:38:36.518039+00:00",
      "id": "01a21a39-aa69-7167-8696-e615aa3b012f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06 Control Plane (retrospective, 2026-11-27)",
      "updated_at": "2026-11-26T23:38:36.773008+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c16-879a-765f-9e1b-c788a9c1ef4a",
      "approved_at": "2026-11-27T08:19:28.421371+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The S08 gate is the slice gate of SPEC \u00a78 S8: the Part A round-trip from principle to ADR to TOC to register to deliverable to capability to stakeholder, the TOC cycle refusal, the absence of a duplicate principles/decisions surface, three golden models in SVG/PNG/PDF, the profile's write-time 422, the XMI round-trip, the post-commit validation job, deterministic ids, the two retention producers and the render byte-route parity.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the S08 slice: the governance surface over S03's spine and the modelling surface over seven new tables, with a deterministic renderer and a profile-scoped XMI exchange. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the S08 slice plan.",
        "constraints": "The build must not introduce a second model store, must not use the registry as the validation authority, must not call load_system_fonts, must not query JSONB geometry in SQL, must not validate inside the mutation transaction, and must not trust an XMI document: size, depth, duplicate ids and a DOCTYPE are refused. These constraints are the plan's traps and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set, the contract and the profile; S03 supplies the spine tables and RLS; S04 supplies the registry and invocation route; S07 supplies the outline and the principles/decisions actions; S12 receives the declared trace/architecture surface; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the slice plan, the measured \u00a77.2 completion record and the record's project row. It records the assignment and the acceptance the build was measured against; it does not claim that a contemporaneous board approved this text on 2026-11-08, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are the 68 actions, the seven S08 tables, the two retention producers, the two byte routes and the seven Phase A\u2013D artefacts. Out of scope are the UI, model search, project CRUD, principle-conflict verdicts, architecture containers, change-set/review/publication splits, graph layout and OCL/Profile/SysML; each exclusion is named in the slice plan's \u00a73 so a later reader cannot mistake absence for omission."
      },
      "created_at": "2026-11-27T08:19:28.107129+00:00",
      "id": "01a21c16-86ee-7106-8eab-4155e8b4b141",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2c-7bad-8e6c-b5c5d1fd40e2",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S08 Governance and Modelling (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:19:28.421371+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c1b-b5de-788c-9918-d57c95016a17",
      "approved_at": "2026-11-27T08:25:07.941014+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The S17 gate is the slice gate of SPEC \u00a78 S17: the banked parity oracle 555/555, gold 50/50 and empty 0/50, the golden eval scripts 50/50, the exact McNemar vectors, RLS on all four tables, the fail-closed sandbox, deterministic Job names and the 1 MiB ConfigMap cap, no answers to producers, no benchmark action in AGENT_TOOLS, and the live sandbox lifecycle recorded on compute-1.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the S17 slice: one measurement loop over four tables, a hashed seed artifact, a pure grader and exact McNemar, a k8s Job sandbox and a single run driver. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the S17 slice plan.",
        "constraints": "The build must not import Python into the product or the image, must not let a producer see answers, must not widen the sandbox config to a default namespace, must not exceed the 1 MiB ConfigMap cap or the 256 KiB patch cap, must not let the conveyor arm start before S18, and must not repoint the grading node. These constraints are the plan's traps and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set, the contract and the measurement standard; S02 supplies the job/event/audit tables; S03 supplies the RLS spine and the document row; S04 supplies the registry and scopes; S09 supplies the object store the eval log is written to; S18 takes the conveyor arm; S12 receives the declared screens; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the slice plan, docs/evidence/S17-CORE.md and the record's project row. It records the assignment and the acceptance the build was measured against; it does not claim that a contemporaneous board approved this text on 2026-11-09, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are the four benchmark tables, the seed artifact and its image lock, the standalone seed generator, the grader and 12 parsers, the exact McNemar, the sandbox adapter, the gold/empty/baseline arms, the eight actions, two scopes, two events, and the declared S12 screens. Out of scope are the conveyor arm (S18), model hosting, HF calls, the old CLI, a second service and Python; each exclusion is named in the slice plan's \u00a73."
      },
      "created_at": "2026-11-27T08:25:07.66928+00:00",
      "id": "01a21c1b-b557-7b0a-ba9d-bc9a2de9247d",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d30-794f-bd53-c72986b18e60",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S17 Native Benchmark Harness (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:25:07.941014+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c1d-aa55-7091-ae4b-93328e410035",
      "approved_at": "2026-11-27T08:27:16.055256+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The T17 gate: a brief plus documents draft the catalogue; documents yield linked procedure and policy drafts; an unpublished dependency is refused; a reviewed catalogue publishes with its decisions; no asset publishes itself; a non-human decision cannot complete a gate; no asset widens a tool grant; and the real-app E2E creates a draft and sees the row render.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the T17 slice: the authoring lifecycle over two tables, a human review gate, a published-dependency check and a publish-through-the-target rule. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the T17 inception set and SPEC_V2 \u00a75.7/\u00a710.4\u2013\u00a710.5.",
        "constraints": "The build must not write the target tables, must not let an asset publish itself, must not let a non-human decision complete a gate, must not widen a tool grant, must not duplicate the governed_asset lifecycle and must not refuse the dependency order later than the draft attempt. These constraints are the inception brief's risks and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set and the contract; T3 supplies the decision ladder and the approval primitives; T16 owns live ingestion and the SOP human gate; T13 owns tools and bindings; the web team owns the Authoring screen and the E2E; the tenant authors and reviewers take the human decisions; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the inception set, the evidence and closure-review documents and the record's project row. It records the assignment and the acceptance the delivery was measured against; it does not claim that a contemporaneous board approved this text, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are intake, drafting, review and publication of organisation/department/job/procedure/policy assets, the two authoring tables, the state machine, the review gate, the published-dependency refusal and the Authoring screen. Out of scope are writing the target tables, the SOP/RDF/BPMN converter (T16), tools and bindings (T13), a governed_asset fork and a new template table except asset_template; each exclusion is named in the inception brief's Boundary."
      },
      "created_at": "2026-11-27T08:27:15.79094+00:00",
      "id": "01a21c1d-a9d1-70d8-8a6a-901c21e4dd7b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d38-7db1-b685-d76bd4cebf47",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T17 Tenant Asset Authoring (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:27:16.055256+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c26-afda-78c1-aea9-920275ed65c4",
      "approved_at": "2026-11-27T08:37:07.301597+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The \u00a712 T7 gate: an injected pod failure is triaged, mitigated by an approved write, resolved and postmortemed; a destructive action without the tenant's configured approvals is refused; restart, scale, rollback and flag-flip are callable. The executed suite maps each clause to a named test in T7-acceptance.md; the live cluster script is deploy-bound and is not claimed as run.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the T7 record: the incident lifecycle over the existing card and approval primitives, the four gated cluster writes, the postmortem/kata loop and the /operations console. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by SPEC_V2 \u00a76.5, \u00a79.2 and \u00a712 T7.",
        "constraints": "The build must not let an ops write reach the cluster without an approval, a recorded dry run and two distinct human approvers; must not let the mitigation wrapper bypass the gate; must not let an incident that needed no write stay unresolvable; must not widen a tool grant; and must not claim the live cluster proof before the deployed image carries the gate. These constraints are the finding acceptance and are carried into the contract.",
        "responsibilities": "The programme architect owns the record and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set and the contract; T2 supplies the job/event/audit rows; T4 supplies the registry and the destructive effect; T13 owns tools; the web team owns the /operations screen; the on-call operators take the incident decisions; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the SPEC row, the issue #78 delivery, the acceptance evidence and the two fixes at ca155fada. It records the assignment and the acceptance the delivery was measured against; it does not claim a contemporaneous board approval, and it carries the live-proof limitation into the Phase-G review.",
        "scope": "In scope are the five incident actions, the four write verbs and their gate, the k8s read family, the incident meta lifecycle and the /operations screen. Out of scope are tools and bindings (T13), the agent tool surface (the ops writes are not agent-reachable) and a second ops service; each exclusion is deliberate."
      },
      "created_at": "2026-11-27T08:37:06.989963+00:00",
      "id": "01a21c26-af30-7c3e-ad6d-39f7ae1937ea",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d34-7f89-a0be-f3de3c9f540e",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T7 Operations (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:37:07.301597+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2210c-1cd2-7eb0-a3d2-c90c96b7d6e7",
      "approved_at": "2026-11-28T07:26:11.809378+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is one real agent task over `/mcp`: create a project, read its progress, open and answer an approval, close the project, with `decided_by_kind='agent'` recorded, no `nonagenticai_mcp_server` component in the path, and no `X-MCP-Binding` header. The local proof is `crates/api/tests/mcp_agent.rs::one_licence_principal_task_creates_reads_decides_and_closes`; the deployed agent-app run is S14's gate and is recorded as deferred.",
        "assignment": "The architecture work assigns S06b the agent's direct interface to the conveyor: define the tool key every MCP tool carries, resolve a machine principal from a licence bearer, propagate that principal into attribution, and add the approval-request action an agent needs to escalate. This statement governs the retrospective record of work completed on 2026-11-09 and re-read at the 2026-11-27 snapshot.",
        "constraints": "The one-membership rule rejects zero or several memberships, so the tenant is taken from the licence. A JWT-shaped token that fails JWT verification never falls through to licence introspection. The licence service is on the request path, so its verdict is cached for at most 60 seconds, and an unreachable service refuses with `503 licence_unavailable` rather than admitting. The cache is keyed by the token hash, never by tenant.",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. The agent platform is consulted as the consumer, and the tenant admin is informed because the tool allowlist is the authorization surface.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the 2026-11-09 delivery was recorded only in the slice plan and the evidence file. The statement records what the tree and the suites prove today and names the deferred live step instead of implying it happened.",
        "scope": "In scope are `_meta.operation` on every registered tool, the licence introspection in `crates/auth/src/machine.rs` with its 60-second capped cache and fail-closed rule, `Principal.kind` across human, agent and service, `approvals.request` over `insert_approval_tx`, and the end-to-end `/mcp` session in `crates/api/tests/mcp_agent.rs`. Out of scope are the `fr` toolkit, MCP tasks (S06c), the copilot surface (S10) and any signed HMAC channel (S06a, superseded)."
      },
      "created_at": "2026-11-28T07:26:11.487462+00:00",
      "id": "01a2210c-1c22-792c-8f82-18dc219fba9c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06b Agent Interface (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:26:11.809378+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2210e-f7bb-70ff-8d4d-5d6f216ed585",
      "approved_at": "2026-11-28T07:29:18.922843+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is a scripted session: open a conversation, ask for a card move, receive a proposal and one pending approval with zero action invocations; approve as an authorized member and see exactly one invocation, the card moved and the turn completed citing that invocation; reconnect with `Last-Event-ID` and receive the turn once; see a low-scope principal's two surfaces omit the same tool; see an unavailable router fail the turn with zero invocations; and see the coverage gate fail an unmapped operation by name. The local proofs are `crates/api/tests/copilot_session.rs`, `copilot_proposal.rs`, `copilot_approve.rs`, `copilot_replay.rs`, `copilot_surface.rs`, `copilot_turn.rs` and `crates/actions/tests/copilot_coverage.rs`.",
        "assignment": "The architecture work assigns S10 the copilot's one action surface: project the S04 registry by principal, turn a destructive tool call into an S06 approval, run the conversation as two tables and an event stream, and keep model access outside the conveyor. This statement governs the retrospective record of work built in the S10 slice and re-read at the 2026-11-27 snapshot.",
        "constraints": "S10-D1 keeps the surface a query over `ActionSpec`, so no second registry or tool table exists. S10-D2 makes a proposal an approval created through S06's `insert_approval_tx`, with one pending proposal per turn enforced by the partial unique index on `(tenant_id, meta->>'turn_id')`. S10-D3 makes execution a job with a conditional claim, a 120-second lease, re-checked proposer scopes and the proposal idempotency key. S10-D4 makes only terminal turns durable, with the terminal meta, the proposal and the `conversation_turn.created` event committed together. S10-D5 makes `completions.create` fail closed: an outage, timeout or open breaker ends the turn `failed` and runs no tool. S10-D6 declares `copilot.chat` the only coverage exemption. S10-D7 keeps AG-UI as a serializer over the native stream.",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. S12 is consulted as the renderer of the declared screens, operations is consulted on model availability and key custody, and the tenant admin is consulted because the channel grants and the approval decision are the authorization surface.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the S10 row was created on 2026-11-12 and the build was recorded in the slice plan and the receipt. The statement records what the tree and the suites prove today, names the pre-pack receipt as red on lifecycle checks, and leaves the deployed UI and router proof to S12 and S14 instead of implying it happened.",
        "scope": "In scope are the `conversation` and `conversation_turn` tables with forced row-level security, the turn runner and its eight-round cap, the `agent_surface` projection over the action registry, proposals carried as `approval(kind='copilot.action')` with sealed inputs, the conditional execution claim and sweep, the SSE topic `conversation:{id}` with `Last-Event-ID` replay, `completions.create` as an external action with a 60-second timeout and breaker, the AG-UI serializer, and the screens declared as data for S12. Out of scope are inference-router (D14), the `/mcp` transport and scope filtering (S04), the approval state machine (S06), fr's 34 AG-UI action modules, the agent's principal and tool contract (S06b), and chat UI rendering (S12)."
      },
      "created_at": "2026-11-28T07:29:18.626402+00:00",
      "id": "01a2210e-f724-7ebf-8b08-74d6c0f24f4c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S10 Copilot / AG-UI (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:29:18.922843+00:00",
      "version": 2
    },
    {
      "approval_id": "01a22110-1c69-7458-a9be-c5550ac1ee8a",
      "approved_at": "2026-11-28T07:30:33.839885+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is author \u2192 publish \u2192 execute a process run: an SOP and a three-step process are created and published, a card starts the run, the second step parks on an approval, the approval resumes it, and the run finishes with three succeeded run_step rows, non-empty input and output, and one approval. The negative proofs are an unresolved template (422 refusal, never an empty string), an unknown action refused at publish, and a retry that appends a second attempt row. The local proof is the suite set in crates/actions/tests plus the domain unit tests; the plan's \u00a77 two-process rehearsal is the live gate and is recorded as not executed in this pack.",
        "assignment": "The architecture work assigns S11 the executable-documentation capability: define a versioned process as an ordered list of steps, give each step a registry action and a resolved input template, attach a versioned SOP to each step, validate the whole draft at publish, and execute a published process as a run with one immutable run_step per attempt. This statement governs the retrospective record of work built on 2026-11-09 and re-read at the 2026-11-27 snapshot.",
        "constraints": "The store forbids a second published or draft row per (tenant_id, key). Publish validates in one transaction against the live manifest and refuses an unknown action, a step naming process_runs.*, a forward step reference, a retry on a non-idempotent action, an unresolved gate kind and an SOP whose inspection is empty. The resolver is one pure function with the roots run, steps, card and procedure; an absent path refuses and a null resolves. The executor claims a queued step with a status-guarded update, persists the resolved input at claim, and re-invokes a replayed attempt under the same idempotency key derived from (run_id, position, attempt).",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the store, resolver, actions and executor evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. The process author is consulted because tenant authorship is the capability's consumer, and S13 and S12 are informed because they consume the run tree and the declared read models.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the 2026-11-09 delivery was recorded in the slice plan and in the test tree. The statement records what the tree and the suites prove today and names the deferred live gate, the zero seed and the S15 seam instead of implying they happened.",
        "scope": "In scope are migrations 0015 (procedure, procedure_step, run_step), 0016 (the draft index), 0017 (lock_version), 0021 (the run capability seam), 0022 (the kata_executor role) and 0035 (the rename to process/process_step, the SOP table and process_map); the process, procedure (SOP) and process_map actions; publish validation and the frozen revision; the resolver in crates/domain/src/template.rs; and the executor in crates/actions/src/process/step.rs dispatched as the process.step job. Out of scope are matrix legality (S03), approval transport (S06), sub-processes and the engine (S13), screens (S12), PDL (never) and the deferred default seed."
      },
      "created_at": "2026-11-28T07:30:33.564273+00:00",
      "id": "01a22110-1bde-74d0-8b3e-e693e6c7291b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S11 Processes, Steps and SOPs (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:30:33.839885+00:00",
      "version": 2
    },
    {
      "approval_id": "01a222a6-2960-71b2-8edc-797253376758",
      "approved_at": "2026-11-28T14:54:04.782673+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks, each with its own target: exactly one task-capable action read from the compiled registry; the project-scoped `inputRequests`; the answer written as `approvals.decide`; a terminal result that never says `failed` on the wire; one `-32602` for every ownership failure; and the parity suite proving the task surface and the HTTP action agree.",
        "assignment": "Deliver the architectural record for durable agent tasks: a client that declares the MCP task extension drives a long-running project \u2014 starting it, polling it, answering the approval it waits on and reading a terminal result \u2014 without a second store and without a service between the client and the conveyor. The assignment is retrospective: the product shipped on 2026-11-09 and this statement records the architecture it was built to.",
        "constraints": "No new table and no new service. The run state machine is the task lifecycle, so the reaper had to learn to skip project executions and the one-live-run property had to become a partial unique index because the existing index does not dedupe nulls. Every constraint in this project is a consequence of the reuse decision rather than an independent choice.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the agent interface; the MCP client author is the consumer whose conformance the contract is written against. The retrospective author is not the accepting role, so the separation of duties holds even though one session wrote the product record and this set.",
        "retrospective": "This statement is written after the fact and declares so. The slice plan's build record gives the shipped state; the gate's compiled-registry assertion is the one the plan had to correct, because its first form grepped a flag no serialised surface carries. That correction is recorded here rather than tidied away.",
        "scope": "In scope: the Phase A\u2013D artefact set, the task-to-run identity, the project-scoped `inputRequests` listing, the answer path through `approvals.decide`, and the two indexes migration 0023 adds. Out of scope: the agent's own task support against external MCP servers, decommissioning `mcp_server`, SSE or push delivery, a progress percentage, and any new table."
      },
      "created_at": "2026-11-28T14:54:04.488688+00:00",
      "id": "01a222a6-28cb-7ab4-b430-124b3ec82cb8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06c MCP Tasks (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:54:04.782673+00:00",
      "version": 2
    },
    {
      "approval_id": "01a222a7-1881-7d2e-a3b5-928c86cb2b76",
      "approved_at": "2026-11-28T14:55:06.005722+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks: the capability aggregate resolves with its executor and versioned binding; readiness refuses a run with `409 capability_not_ready` before the first step; a cited passage comes from a pinned strata pack; memory is written, consolidated into `strata_rag` and recalled by `source_id`; and a degraded source mid-run reports `knowledge_degraded`, not the pre-run refusal.",
        "assignment": "Deliver the architectural record for a capability aggregate: a capability is a persisted thing with an executor spec, a versioned knowledge binding and memory, which a card's run resolves and readiness-checks before its first step. The assignment is retrospective \u2014 steps 1\u20134 shipped on 2026-11-09 and step 5's actions remain open \u2014 so this statement records the architecture and the remaining work together.",
        "constraints": "No second loop and no second retrieval path. The conveyor owns the binding and strata owns retrieval, so a run that read knowledge by any route other than `knowledge.query` would make the pinned pack advisory. Every new table carries a tenant column with row-level security forced, and documents are immutable \u2014 ingest a new version and prune the old, never update in place.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the capability model; the capability author writes the pack and its readiness declaration; the retrieval owner operates the strata pack. The retrospective author is not the accepting role, so the separation of duties holds.",
        "retrospective": "This statement is written after the first four build steps and before the fifth, which makes it unusually literal: it records what exists and what does not rather than reconstructing a finished shape. The seven remaining actions, the unproved pack-bound retrieval and the unproved run-transactional memory write are named as open work.",
        "scope": "In scope: the Phase A\u2013D artefact set, the capability tables and their versioning, the pre-run readiness check, the pinned retrieval binding, and the memory write, consolidation and recall. Out of scope: the model loop (S13's `agent.run`), the interactive copilot surface (S10), embeddings and answer generation (the executor's), a tenant-authored prompt store, KAG, and S08's dropped `project_capability` ledger."
      },
      "created_at": "2026-11-28T14:55:05.705673+00:00",
      "id": "01a222a7-17ec-751f-bc1c-9e932f9b4763",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S15 Capability, Knowledge and Memory (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:55:06.005722+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Contract

The contract record, its version and its approval.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a222a7-1881-7d2e-a3b5-928c86cb2b76",
      "approved_at": "2026-11-28T14:55:06.005722+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks: the capability aggregate resolves with its executor and versioned binding; readiness refuses a run with `409 capability_not_ready` before the first step; a cited passage comes from a pinned strata pack; memory is written, consolidated into `strata_rag` and recalled by `source_id`; and a degraded source mid-run reports `knowledge_degraded`, not the pre-run refusal.",
        "assignment": "Deliver the architectural record for a capability aggregate: a capability is a persisted thing with an executor spec, a versioned knowledge binding and memory, which a card's run resolves and readiness-checks before its first step. The assignment is retrospective \u2014 steps 1\u20134 shipped on 2026-11-09 and step 5's actions remain open \u2014 so this statement records the architecture and the remaining work together.",
        "constraints": "No second loop and no second retrieval path. The conveyor owns the binding and strata owns retrieval, so a run that read knowledge by any route other than `knowledge.query` would make the pinned pack advisory. Every new table carries a tenant column with row-level security forced, and documents are immutable \u2014 ingest a new version and prune the old, never update in place.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the capability model; the capability author writes the pack and its readiness declaration; the retrieval owner operates the strata pack. The retrospective author is not the accepting role, so the separation of duties holds.",
        "retrospective": "This statement is written after the first four build steps and before the fifth, which makes it unusually literal: it records what exists and what does not rather than reconstructing a finished shape. The seven remaining actions, the unproved pack-bound retrieval and the unproved run-transactional memory write are named as open work.",
        "scope": "In scope: the Phase A\u2013D artefact set, the capability tables and their versioning, the pre-run readiness check, the pinned retrieval binding, and the memory write, consolidation and recall. Out of scope: the model loop (S13's `agent.run`), the interactive copilot surface (S10), embeddings and answer generation (the executor's), a tenant-authored prompt store, KAG, and S08's dropped `project_capability` ledger."
      },
      "created_at": "2026-11-28T14:55:05.705673+00:00",
      "id": "01a222a7-17ec-751f-bc1c-9e932f9b4763",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S15 Capability, Knowledge and Memory (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:55:06.005722+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

```json
{
  "category": "statement-of-architecture-work",
  "count": 38,
  "rows": [
    {
      "approval_id": null,
      "approved_at": "2026-11-19T07:50:28.603416+00:00",
      "approved_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "body": {
        "acceptance_criteria": [
          "D1 \u2014 the artefact standard and repository: a catalog's entries are rows, an approved artefact is immutable and a supersede writes version 2, and a reuse link is searchable across plateaus.",
          "D2 \u2014 the Preliminary capability: the tailored profile names each phase's mandatory set by plateau, the board is a governance body row, and a maturity assessment carries all nine dimensions and a skills assessment all seven categories.",
          "D3 \u2014 the architecture project and phase gates: an approved artefact set and a live contract advance a phase, a draft artefact refuses the advance naming the artefact, and a contract change re-opens the gate.",
          "D4 \u2014 delivery compliance: a design gate without a live contract refuses, a superseded contract refuses naming its status, and one compliance level per review and a dated waiver without an owner are refused.",
          "D5 \u2014 change management: a review's exactly-one outcome is a change row or a kata candidate, and a review with no verdict cannot close the phase.",
          "D6 \u2014 the console surface: the Architecture destination renders its tabs from the live aggregates and each interaction records state."
        ],
        "assignment": "Deliver the TOGAF ADM capability as rows and actions in the one workspace: the artefact standard and repository, the Preliminary capability, the architecture project and its phase gates, delivery compliance, change management, and its console surface.",
        "responsibilities": "The programme architect (the maintainer sessions under the owner's authority) delivers and evidences each deliverable; the Architecture Board reviews and approves; the programme owner accepts the capability.",
        "scope": "The eleven tables of migration 0054 and the 26 architecture.* actions that expose them; the four ADM contract kinds; the seeded ten-type artefact standard; the Architecture Board; the maturity and skills baselines; and the Architecture capability screen (D6). The design is docs/plans/T9-architecture-capability.md."
      },
      "created_at": "2026-11-19T07:42:46.140382+00:00",
      "id": "01a1f2c2-0e1a-787f-9011-46b424d1ec71",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d35-7bd1-945d-cea12c5dfd6d",
      "requested_by": null,
      "status": "superseded",
      "superseded_by_id": "01a2023e-4e59-75ad-b2ab-fd7d014511d2",
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 the TOGAF ADM capability (T9)",
      "updated_at": "2026-11-22T07:52:45.884024+00:00",
      "version": 3
    },
    {
      "approval_id": "01a1fdd2-74eb-7fe3-a424-cdf02d021931",
      "approved_at": "2026-11-21T11:16:38.828544+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/P0.toml is green against the live record at the published SHA",
        "assignment": "Rework the programme definition project P0 by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526).",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "manifest": "acceptance/programme/P0.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the 21 programme-definition outputs, the charter and scope baseline, the benefit and governance baselines, the tranche-0 record and the closure review"
      },
      "created_at": "2026-11-21T11:16:21.046667+00:00",
      "id": "01a1fdd2-5041-7f80-8032-3305751d498c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1dcb8-d24b-76de-b901-56fd56e001bc",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 P0 programme definition (retrospective, U5a)",
      "updated_at": "2026-11-21T11:16:38.828544+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff2f-c76b-7bfe-b71a-a39294a95644",
      "approved_at": "2026-11-21T17:38:09.721121+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S01.toml is green against the live record at the published SHA; the decision record marks the unmeasured values not recoverable.",
        "assignment": "Rework the UI-stack spike by the reconstituted lifecycle (U5a, #526); the decision itself was made 2026-11-08.",
        "manifest": "acceptance/programme/S01.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "ADR 0001 and its measured table: interaction TTI, bundle, first render, drag frames, SSE\u2192DOM, memory, cold build and auth."
      },
      "created_at": "2026-11-21T17:36:38.122067+00:00",
      "id": "01a1ff2e-7974-74cd-be6d-377e3c0071a4",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-724c-9bae-af96b2d926b1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S01 (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:38:09.721121+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff37-f693-7336-8c6d-fe98f6f5c7b7",
      "approved_at": "2026-11-21T17:47:05.975622+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S00.toml is green against the live record at the published SHA; the inventory's 244 dropped rows are the gate's evidence.",
        "assignment": "Rework the usage inventory project by the reconstituted lifecycle (U5a, #526); the inventory itself was delivered 2026-11-07/08.",
        "manifest": "acceptance/programme/S00.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The 1,300-row parity inventory: 218 used, 295 rare, 543 unused, 244 dropped, across fr (798), RM (314), Planka (96) and n8n (92)."
      },
      "created_at": "2026-11-21T17:46:54.771712+00:00",
      "id": "01a1ff37-e240-779b-aab5-74a3fa86ecad",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S00 (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:47:05.975622+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff3c-8f0f-7326-b8a7-2de4d2eb6597",
      "approved_at": "2026-11-21T17:52:06.626934+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "Four, each with its check: (1) the public site renders the programme from Postgres \u2014 `data/snapshot.js` carries the programme's rows and the public sidebar names it only when the snapshot does (#470, 7d4e0d27); (2) a published output appears at its stable URL \u2014 165 committed output pages, every one named by `docs/design/docs/outputs.js` (#516, 06ba0fe3); (3) signed out there is no API request and no write control \u2014 `node docs/design/scripts/check-console.cjs`; (4) the pack's claims are checked like code \u2014 `build-coverage.py --check`, `build-docs.py --check` and the surface checks in the `design-pack` job of `.github/workflows/ci-public.yml`. The project's own gate is `acceptance/programme/DESIGN-PUBLISH.toml`.",
        "assignment": "Deliver to the programme owner, the Architecture Board and every outside reader the published studio design pack that makes the SPEC_V2 operating model readable and challengeable from a browser, and keep it live from the application image. This is the Phase A architecture work of a programme-level project in tranche 0, held to `docs/design/STUDIO-SPEC.md`. Recorded retrospectively on 2026-11-21: the pack was built and published by units #32, #33, #36, #37, #89, #97, #103 and #149, all closed.",
        "manifest": "acceptance/programme/DESIGN-PUBLISH.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The Phase A artefact set at plateau psa \u2014 stakeholder map (matrix), driver/goal/objective catalog, requirements catalog and solution concept (diagram) \u2014 with the Phase B\u2013D catalogues: business capability map, data entity catalog and technology standards. The published surfaces: `studio.html` offline, `https://conveyor.nonagentic.ai/design/` from the app image, `/design/data/snapshot.js` read from the tenant's rows, and `/design/docs/output-<id>.html` for every published output. Out of scope: any V2 backend aggregate, any signed-out write path, and the retired homepage mirror."
      },
      "created_at": "2026-11-21T17:51:55.933191+00:00",
      "id": "01a1ff3c-7aad-7dda-820e-1c7d85fe70a8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 DESIGN-PUBLISH: the published studio design pack",
      "updated_at": "2026-11-21T17:52:06.626934+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff41-2c98-7646-b835-7c7a96e1ca6e",
      "approved_at": "2026-11-21T17:57:08.762631+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest `acceptance/programme/SPEC-V2.toml` is green: the closure review exists, every declared closure output (`evidence`, `closure-review`) is published, and the project re-closes through the one gate decided by an approver other than its requester. The Phase-G compliance review records exactly one standard level against this contract and cites the published standing delegation. The seven A\u2013D artefacts are approved rows with their entries; no outcome-level claim substitutes for them.",
        "assignment": "Produce and publish SPEC V2 as the programme's adopted operating model, and prove it through the programme's executable acceptance. The work is the document and its decision set \u2014 `docs/SPEC_V2.md`, decisions D28\u2013D63 \u2014 with the ownership, organisational, work, decision, tool and rigour models it defines. This Statement of Architecture Work is recorded retrospectively on 2026-11-21 for the tranche-0 rework (issue #526) under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the project's real, contemporaneous statement was never recorded and is not recoverable.",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The V2 document and its decisions, and the phase A\u2013D architecture artefacts this contract governs: the stakeholder map, the driver/goal/objective catalog, the requirements catalog and the solution concept (phase A); the business capability map (phase B); the data entity/data component catalog (phase C); the technology standards catalog (phase D). Not in scope: V1's S-slices (not re-opened), the V2 delivery slices T0\u2013T17 (their own projects), and `docs/SPEC_LEGACY_KAG.md` (D54)."
      },
      "created_at": "2026-11-21T17:56:58.859998+00:00",
      "id": "01a1ff41-19f9-77bb-aa4a-f416f0d758e3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 SPEC-V2, the organisational operating model (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:57:08.762631+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff45-c6f7-7a7d-ab9f-0c9c8293147e",
      "approved_at": "2026-11-21T18:02:10.160888+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T0.toml is green against the live record at the published SHA: a job is created, a release is published, a binding pins its hash, a run on a retired binding refuses 409 job_binding_stale, no concept has two stores, and a test-only third executor kind registers without editing the run path. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T0 (SPEC_V2 \u00a712, issue #34) by the reconstituted TOGAF/MSP lifecycle: produce and publish the Phase A\u2013D mandatory artefacts, the charter and the delivery contract, the Phase-G compliance review and the closure review, declare the closure outputs and re-close through the one gate.",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/T0.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the model and its invariants (SPEC_V2 \u00a73 P1\u2013P11, \u00a74.1\u2013\u00a74.6, \u00a710.1, \u00a710.3): the organisation spine \u2014 industry, ordered departments, job_catalogue, the immutable job_release and the tenant-scoped job_binding; the decision-ladder types; and the internal Executor contract. The action contracts are those the registry serves and served-spec.lock.json pins. Execution behaviour is T2; the plan spine is T1; the tool platform is T13."
      },
      "created_at": "2026-11-21T18:02:00.126911+00:00",
      "id": "01a1ff45-b2ce-7276-8a88-cf3d744e7976",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7734-98da-fb7f1e2386c7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T0 Model and contracts (retrospective, U5a)",
      "updated_at": "2026-11-21T18:02:10.160888+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff84-57c3-7f67-9229-94bf0188732d",
      "approved_at": "2026-11-21T19:10:30.370289+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S02.toml is green against the published snapshot at the recorded SHA, with the S2 gate proven: two tenants isolated including RLS, and login works. The long-job lease/recovery proof under two workers was issue #2's P2; it is proved by `apps/server/tests/jobs_lease_two_workers.rs` at d367a4a9, and issue #2 closes with the lane PR.",
        "assignment": "Rework the delivered S02 skeleton by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526, tranche 1), recording the A\u2013D artefacts, the delivery contract and the Phase-G compliance review, and closing the project once the long-job proof issue #2 named is landed in this batch.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The ten-member workspace and one `conveyor` binary delivered by S02: eleven platform tables in `migrations/0001_platform.sql`, forced RLS on the nine tenant-owned tables, OIDC and development authentication, the fail-closed `app_licence` gate, the fenced job runner, the replaying `/api/events` stream, and the declared `shell/login` surface for S12."
      },
      "created_at": "2026-11-21T19:10:20.569282+00:00",
      "id": "01a1ff84-4425-75d9-8772-08e09cf71069",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S02 Skeleton (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:10:30.370289+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff89-1794-79d0-8707-f7614987ee7c",
      "approved_at": "2026-11-21T19:15:42.114015+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T1 gate: one project with a baseline, a change request and a refused self-approval; WSJF recomputes from inputs; the portfolio view lists the in-flight set; a dependency cycle is refused; a benefit profile without baseline, target, method or owner is refused; a benefit-map edge names its five levels. The ten issue-#72 scenarios are proved by the named real-Postgres tests in crates/actions/tests/plan_spine.rs and crates/api/tests/approval_dual.rs; the rework's acceptance instrument is acceptance/programme/T1.toml. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T1 (SPEC_V2 \u00a712, issue #72) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the plan spine and programme-definition rows of SPEC_V2 \u00a712: wbs_node, project_baseline, project_register_item, project_dependency, programme_cadence, programme_tranche, benefit_profile, benefit_link, benefit_transition, stakeholder_profile, programme_baseline, and the widened governed_asset kinds; the spine action families; and the requester/executor exclusion in approvals.decide. Out of scope: the decision ladder (T2), the PM job (T3), cadence materialisation and PI planning and the benefits-realization procedure (T10), metrics (T11), and any migration of V1's outline tree."
      },
      "created_at": "2026-11-21T19:15:31.506201+00:00",
      "id": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T1 Plan spine (retrospective, U5a)",
      "updated_at": "2026-11-21T19:15:42.114015+00:00",
      "version": 2
    },
    {
      "approval_id": "01a1ff8d-b4cd-74f9-9f32-2b9f6cae33cd",
      "approved_at": "2026-11-21T19:20:43.741951+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T11.toml is green against the published snapshot at the re-close: every metric and every metering level recomputes from rows; the reconciliation with billing_service is exact at six decimals or opens an incident; a C0\u2013C5 level moves only when the measured evidence crosses its recorded threshold, naming that threshold version; a gate that never refused is flagged. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T11 (SPEC_V2 \u00a77.5, \u00a710.4, \u00a712; issues #84 and #458) by the reconstituted TOGAF/MSP lifecycle: produce and approve the Phase A\u2013D mandatory artefacts, this Statement of Architecture Work, the delivery contract, the Phase-G compliance review and the closure review; publish the declared outputs; and re-close the project through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The metrics, metering and maturity capability delivered by #84 (commit 183f5111, 2026-11-19) and the live Metering screen delivered by #458 (commit 0a8a6e95, 2026-11-20): migration 0059_metrics.sql, crates/domain/src/metrics.rs, crates/store/src/metrics.rs, crates/actions/src/metrics/mod.rs, the eight metrics actions, the billing.summary reconciliation input, the incident card type, the served-spec lock and the console screen. Charging, eval/benchmark-ledger quality metrics, knowledge metrics without ledgers and per-tool health are out of scope."
      },
      "created_at": "2026-11-21T19:20:34.079029+00:00",
      "id": "01a1ff8d-a0ad-7a91-b026-d9a2575daa3f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T11 Metrics, metering and maturity (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:20:43.741951+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2005a-8c6e-708a-84df-ab16d175072e",
      "approved_at": "2026-11-21T23:04:28.797665+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-21T23:04:18.681795+00:00",
      "id": "01a2005a-7886-7f2d-9b49-53d174544ee9",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:14.119814+00:00",
      "version": 3
    },
    {
      "approval_id": "01a20061-d19f-7269-9f5c-113f3aeb0001",
      "approved_at": "2026-11-21T23:12:26.539242+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC \u00a78 S3 gate: proptest on transitions and schema review. Property (a) \u2014 no transition leaves list.kind and archived_at contradictory, proved by board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error. Property (b) \u2014 claim not granted twice without release or expiry, proved as tested by claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic, with the lease-exclusivity half recorded as the P1 package v1-s03-1. Property (c) \u2014 one pending approval per (run, gate), proved by approval_race.rs::sixteen_racers_create_exactly_one_pending_approval. Property (d) \u2014 terminal run and dense revision, proved by transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense. Tenant isolation is proved by rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema review against data-arch \u00a72.3/\u00a72.4 admits the P2 package v1-s03-2. The rework's acceptance instrument is acceptance/programme/S03.toml; the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V1 slice S03 (SPEC \u00a76, \u00a78 S3) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate on the P0-only gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the baseline domain of SPEC \u00a75.3 and \u00a76: the fourteen tables of migrations/0002_domain.sql (programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state), the may_move matrix and the run/approval/asset state machines, the store `_tx` modules with TenantId, and the catalogue_entry.kind vocabulary. Out of scope: every route and action (S04\u2013S06), the board satellites (S05), channels and delivery (S06), outlines, quality and project_request (S07), models, views and portfolio registers (S08), documents and secrets (S09), conversations (S10), procedures (S11), workflow (S13) and the capability columns (S15)."
      },
      "created_at": "2026-11-21T23:12:15.079018+00:00",
      "id": "01a20061-bd73-7a89-b08c-bd8b867eb061",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S03 Domain and schema (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T23:12:26.539242+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20204-34e9-7eea-87d1-5b8265816c62",
      "approved_at": "2026-11-22T06:49:22.301988+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/PLATFORM.toml is green against the live record at the published SHA; the register's PLATFORM P0s are cleared or owned by a named executable package.",
        "assignment": "Hold the programme's definition, standards, gates and record; keep every project's acceptance manifest executable.",
        "manifest": "acceptance/programme/PLATFORM.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The programme capability: the one tenant, the programme definition set, the tranche plan, the artefact standard and the acceptance manifests; the programme record as reworked under #526."
      },
      "created_at": "2026-11-22T06:48:35.177183+00:00",
      "id": "01a20203-883b-7231-b0d3-dee948cf90e4",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work - PLATFORM (programme capability, retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T06:49:22.301988+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2023f-11ac-7fe6-bf6f-101ced8f4ba2",
      "approved_at": "2026-11-22T07:53:45.939115+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": [
          "D1 \u2014 the artefact standard and repository: a catalog's entries are rows, an approved artefact is immutable and a supersede writes version 2, and a reuse link is searchable across plateaus.",
          "D2 \u2014 the Preliminary capability: the tailored profile names each phase's mandatory set by plateau, the board is a governance body row, and a maturity assessment carries all nine dimensions and a skills assessment all seven categories.",
          "D3 \u2014 the architecture project and phase gates: an approved artefact set and a live contract advance a phase, a draft artefact refuses the advance naming the artefact, and a contract change re-opens the gate.",
          "D4 \u2014 delivery compliance: a design gate without a live contract refuses, a superseded contract refuses naming its status, and one compliance level per review and a dated waiver without an owner are refused.",
          "D5 \u2014 change management: a review's exactly-one outcome is a change row or a kata candidate, and a review with no verdict cannot close the phase.",
          "D6 \u2014 the console surface: the Architecture destination renders its tabs from the live aggregates and each interaction records state."
        ],
        "assignment": "Deliver the TOGAF ADM capability as rows and actions in the one workspace: the artefact standard and repository, the Preliminary capability, the architecture project and its phase gates, delivery compliance, change management, and its console surface.",
        "responsibilities": "The programme architect (the maintainer sessions under the owner's authority) delivers and evidences each deliverable; the Architecture Board reviews and approves; the programme owner accepts the capability.",
        "scope": "The eleven tables of migration 0054 and the 26 architecture.* actions that expose them; the four ADM contract kinds; the seeded ten-type artefact standard; the Architecture Board; the maturity and skills baselines; and the Architecture capability screen (D6). The design is docs/plans/T9-architecture-capability.md."
      },
      "created_at": "2026-11-22T07:52:45.884024+00:00",
      "id": "01a2023e-4e59-75ad-b2ab-fd7d014511d2",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d35-7bd1-945d-cea12c5dfd6d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": "01a1f2c2-0e1a-787f-9011-46b424d1ec71",
      "title": "Statement of Architecture Work - the TOGAF ADM capability (T9) - re-issued 2026-11-22",
      "updated_at": "2026-11-22T07:53:45.939115+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20256-80ac-72f0-a310-d114488f1d7c",
      "approved_at": "2026-11-22T08:19:17.038367+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": [
          {
            "criterion": "HTTP and MCP cannot diverge",
            "id": "a",
            "proof": "All 20 (16 internal + 4 fixtures.*) both ways; stripping request_id/duration_ms, http.body == mcp.structuredContent."
          },
          {
            "criterion": "One value, three projections",
            "id": "b",
            "proof": "spec_hash: manifest == resolved operation == MCP tool."
          },
          {
            "criterion": "Duplicates fail closed",
            "id": "c",
            "proof": "Exit 1 on a duplicate name, an MCP collision or an out-of-charset name."
          },
          {
            "criterion": "Schema-invalid input",
            "id": "d",
            "proof": "RFC 9457 with populated errors[], never a 500."
          },
          {
            "criterion": "Idempotent replay",
            "id": "e",
            "proof": "Two identical POSTs with one key: same status and body, one action_invocation."
          },
          {
            "criterion": "The lock catches drift; breaking ingest refused",
            "id": "f",
            "proof": "A renamed field makes --check-lock exit 1 naming the operation; the removed-field fixture exits 2 (422) while the approved version serves."
          }
        ],
        "assignment": {
          "depends_on": [
            "S02",
            "S03"
          ],
          "gate": "SPEC section 8 S4 - 20 actions identical over HTTP and MCP",
          "issue": "#13",
          "plan_status": "planned",
          "programme_project": "S04",
          "spec_sections": [
            "section 5.5",
            "section 7",
            "section 8 S4",
            "section 3 D11"
          ],
          "title": "Action registry - derive, manifest, OpenAPI, MCP, invocation, external ingest"
        },
        "manifest": {
          "adds_actions": [
            "actions.list",
            "actions.describe",
            "me.get",
            "tenants.get",
            "tenants.update_settings",
            "members.list",
            "members.set_role",
            "departments.list",
            "departments.create",
            "audit.list",
            "jobs.list",
            "jobs.retry",
            "schedules.list",
            "schedules.upsert",
            "events.list",
            "licences.get",
            "fixtures.echo",
            "fixtures.slow",
            "fixtures.fail",
            "fixtures.timeout"
          ],
          "adds_events": [],
          "adds_modules": [
            "actions"
          ],
          "adds_tables": [
            "action_registry"
          ],
          "deliverables": [
            "crates/actions-derive/src/lib.rs",
            "crates/actions/src/{spec,registry,invoke,mcp,openapi,lock,ingest}.rs",
            "crates/actions/tests/{registry,parity_http_mcp,idempotency,timeout,ingest_diff,external_actions}.rs",
            "crates/actions/tests/fixtures/external/openapi.json",
            "crates/{api,store}/src/actions.rs",
            "migrations/0003_action_registry.sql",
            "served-spec.lock.json"
          ],
          "source_pins": {
            "agent": "83b8de7e",
            "fr": "4a579e91",
            "fr_source_map": "6f7c5097",
            "pinned": "2026-11-07"
          }
        },
        "responsibilities": [
          {
            "responsibility": "Build crates/actions-derive and crates/actions, wire the boot assertion before TcpListener::bind, and add migrations/0003_action_registry.sql.",
            "role": "S04 delivery"
          },
          {
            "responsibility": "Supplies begin_scoped, audit_event, action_invocation, idempotency_key, event and /api/me.",
            "role": "S02"
          },
          {
            "responsibility": "Supplies the 14 domain aggregates and transitions used by the 16 internal actions.",
            "role": "S03"
          },
          {
            "responsibility": "Add domain actions and their own events in the same transaction; later slices add actions, never a surface.",
            "role": "S05-S11"
          },
          {
            "responsibility": "Provides the kind: workflow engine; until then workflow returns 501.",
            "role": "S13"
          },
          {
            "responsibility": "Supplies channel-mcp delivery rows and the signed mcp_server channel (S06-alpha).",
            "role": "S06"
          }
        ],
        "retrospective": "Planned, not yet delivered. Decisions closed here: S4-D1 derive crate is the 10th member; S4-D2 one literal route per action plus a spec_exempt fallback; S4-D3 enable/disable is approved_at; S4-D4 an external name colliding with any name is refused at ingest; S4-D5 idempotency scope is the action name; S4-D6 scope-filtered MCP tools with the refused tool absent; S4-D7 workflow returns 501 until S13. Traps to carry: four artifacts drifting is the fr lesson; Router::merge drops paths; MCP tool names are not action names; a timeout is the caller's; idempotency and the mutation must share a transaction; circular actions hit invocation_depth 8 and 409 action-recursion-limit.",
        "scope": "IN: ActionSpec plus #[action]; the compile-time registry and boot assertion; GET /api/actions; POST /api/actions/{name}/invocations with idempotency, timeout, audit, outbox; rmcp at /mcp; the OpenAPI fold and lock; openapiv3 ingest; migrations/0003_action_registry.sql; 16 internal plus 4 fixtures.* actions. OUT: domain actions (cards.move, ...) are S05-S11's; kind: workflow (S13); channel-mcp delivery rows (S06); the signed mcp_server channel (S06-alpha)."
      },
      "created_at": "2026-11-22T08:19:08.182233+00:00",
      "id": "01a20256-6ede-7d70-aae8-5dbfe681a1b7",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-7eb6-a62a-f420e742595c",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work - S04 Action registry (retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T08:19:17.038367+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20273-4f0f-77e6-8005-9744d1e3a574",
      "approved_at": "2026-11-22T08:50:49.577591+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": "# Statement of Architecture Work \u2014 T13 Tool platform\n\n## Context\nSPEC_V2 \u00a711 replaces ad-hoc backend calls with one governed path. The conveyor already owns actions, secrets, entitlements, audit and a durable queue, so a second catalogue/queue/runner (the predecessor MCP server) is refused (D44, D59). T13 is the slice that makes the one path real.\n\n## Scope\nThe broker `tools.call` and its seven steps; the four catalogue/ledger tables; effect-class gating; the closed-schema input check; credential aliases over sealed secrets; the direct MinIO storage adapter with the mandatory tenant prefix; the coding toolkit and one kag-era toolkit; V1 external-action migration; six gate proofs.\n\n## Approach\nImplement the broker and storage layers in the conveyor workspace, prove each refusal before dispatch at the wire (a counting stub), record one `tool_call` and one `tool.called` event per call, and record every artefact of this rework retrospectively from \u00a711 and the tests rather than from memory.\n\n## Deliverables\n`crates/actions/tests/tool_platform.rs` (the six proofs and the extra broker tests); `conveyor_actions::storage::{JobStorage, TENANT_PREFIX, sha256_hex}`; `conveyor_store::tool::{calls_for_run, ToolCallRow}`; `conveyor_actions::prompt::compile_for_audit`; the reused `conveyor_store::secret::create_secret_tx`; tables `tool`, `toolkit`, `tool_binding`, `tool_call`; action `tools.call`; event `tool.called`.\n\n## Constraints and standards\nRust workspace; PostgreSQL 18; MinIO (S3-compatible) as the platform store; a closed JSON-Schema subset (additionalProperties false, enums, patterns, bounds); OpenAPI 3.1 and streamable-HTTP MCP as the only two BYO forms; idempotency keys on writes and bounded polls.\n\n## Acceptance\nThe six \u00a712 clauses and their named tests, as written in `acceptance/programme/T13.toml`; the manifest is the gate, not this document.\n\n## Risks\nThe suite serialises because the destination URL is process-global; the storage test path is test-only until live MinIO wiring exists; the catalogue write path is deferred to T14; a production binding writer is still absent.\n\n## Decisions closed by this rework\nThe binding row is the broker's authority; refusals are proved on the wire; `tool_call` is not `action_invocation`; the storage adapter builds every key, so traversal and hash mismatches are refused.\n",
      "created_at": "2026-11-22T08:48:20.084131+00:00",
      "id": "01a20271-2a37-7329-b159-28031ddcb966",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T13 Tool platform (retrospective, 2026-11-22)",
      "updated_at": "2026-11-22T08:50:49.577591+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20287-813e-7abd-83c5-ddf3bb9e98b3",
      "approved_at": "2026-11-22T09:12:50.020166+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T10 gate: PI planning materialises cards from the backlog by WSJF within WIP; a deliverable without evidence cannot demo; retro produces lessons; an end-of-tranche review with no baselines, benefit status or risk evidence is refused; a transition that never measured its baseline cannot report the benefit realized. Clauses 2, 3 and 5 are proved by named tests; clause 1 is proved for a project backlog and not for the programme-wide backlog (v1-p82-5); clause 4 is unproved at HEAD because free-text placeholders satisfy the evidence check (v1-p83-1) and the baseline requirement is not gated (v1-p83-4). The rework's acceptance instrument is `acceptance/programme/T10.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T10 (SPEC_V2 sections 8.2 and 8.3 and section 12, issues #82 and #502) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the SAFe cadence and the MSP tranche machinery of SPEC_V2 sections 8.2 and 8.3: the cadence state machine, WSJF and WIP-bounded materialisation behind an approved PI plan, backlog refinement, the demo credit, the retro lesson requirement, the tranche review and its evidence, and the benefit transition with its baseline measurement. Out of scope: the decision ladder (T2), the plan-spine baseline contract (T1), the coach card (T4) except the retro card, metrics (T11) and the one-organisation-loop acceptance (T12)."
      },
      "created_at": "2026-11-22T09:12:38.058104+00:00",
      "id": "01a20287-696c-76a4-a2db-28c6092b5813",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-70c1-a5e8-f50956c5e3f9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T10 SAFe cadence and MSP tranches (retrospective, U5a)",
      "updated_at": "2026-11-22T09:12:50.020166+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2028b-f63c-7c27-b580-d4cba2c1e70f",
      "approved_at": "2026-11-22T09:17:42.179607+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T4 gate: an injected failure yields exactly one coach card and one remedy; a repeat signature escalates; no coach card about a coach card; a systemic class opens a kata. Clauses 2 and 3 are proved by named tests in `crates/actions/tests/coach.rs`; clause 1 is proved for a first interruption but refuted as a universal by v1-p6-4; clause 4 is proved only as a `kata.open` process start and v1-p6-2 shows no kata record, instrument or threshold is created. The rework's acceptance instrument is `acceptance/programme/T4.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T4 (SPEC_V2 sections 6.2, 6.4 and 12, issue #75) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the interruption taxonomy, the closed remedy set, per-signature uniqueness and the depth cap of SPEC_V2 section 6.2, plus the systemic-class path of section 6.4: the `coach_interruption` and `coach_remedy` tables, the `coach.interrupt` action, the seeded coach process and the real-Postgres suite `crates/actions/tests/coach.rs`. Out of scope: the board and pull path's `err.card` producer, the kata's own lifecycle and measurement (T5), the PM job (T3), operations (T7) and the tool platform (T13)."
      },
      "created_at": "2026-11-22T09:17:30.02765+00:00",
      "id": "01a2028b-dde6-7ed4-9e47-6e582f2341d8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T4 Kanban coach (retrospective, U5a)",
      "updated_at": "2026-11-22T09:17:42.179607+00:00",
      "version": 2
    },
    {
      "approval_id": "01a20ce0-644d-7340-a696-1af10fa8c61e",
      "approved_at": "2026-11-24T09:26:06.517885+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The six \u00a712 gate clauses plus D1\u2013D8; the live gate recorded in docs/plans/T19/evidence.md; the closure decision in docs/plans/T19/closure-review.md.",
        "assignment": "Build T19 \u2014 decision forums \u2014 to the \u00a712 gate: a meeting is a card whose agenda items resolve approvals, whose participants hold typed positions, and whose outcomes are derived arithmetic over those positions.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the SRO accepts the benefit B7 and the tranche evidence.",
        "scope": "The meeting aggregate, the approval_decision participant extension, the minutes governed-asset kind, the twelve meetings.* actions, the Meetings destination and the two consumer seams (governance_body, programme_cadence)."
      },
      "created_at": "2026-11-24T09:25:57.096759+00:00",
      "id": "01a20ce0-5183-776c-84bd-327f2301b19b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b9b8-7214-abc6-f2c4f75a1fc1",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T19 decision forums (2026-11-24)",
      "updated_at": "2026-11-24T09:26:06.517885+00:00",
      "version": 2
    },
    {
      "approval_id": "01a210bd-cdb2-7f2f-ac9a-c314fb1856af",
      "approved_at": "2026-11-25T03:26:48.7374+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The T19F plan's P1\u2013P5 scenarios, the live gate recorded in docs/plans/T19F/evidence.md, and the closure review in docs/plans/T19F/closure-review.md that defers project closure to the tranche-6 review.",
        "assignment": "Build T19F \u2014 the forum bindings \u2014 to the plan's P1\u2013P5: a meeting_type version is written only by the reviewed authoring route, a board subject is decided in a sitting with via='meeting' rows, and a cadence row materialises its own meeting exactly once.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the programme manager owns B7's measurement; the SRO accepts the tranche-6 evidence.",
        "scope": "Migration 0073; the authoring publish hook and meetings.type_list/type_get; architecture.board_sit_open/board_sit_decide; cadences.materialise; the mock-UI and real-app surfaces; the programme record and its evidence."
      },
      "created_at": "2026-11-25T03:26:38.984805+00:00",
      "id": "01a210bd-ba25-78c1-a363-3eea5aa71335",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T19F forum bindings (2026-11-25)",
      "updated_at": "2026-11-25T03:26:48.7374+00:00",
      "version": 2
    },
    {
      "approval_id": "01a217fb-a503-7db9-b47b-55a111c1c8de",
      "approved_at": "2026-11-26T13:11:37.614131+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate in SPEC_V2 \u00a712 is executable: a PM runs a request end to end; the PM cannot approve its own gate; a baseline change requires a change request. The close gate reads the project's declared outputs \u2014 project-brief and evidence \u2014 plus the closure-review output, a live board-approved delivery contract, a Phase-G review citing the standing delegation, the approved scope baseline, the seven mandatory A\u2013D artefacts with their registers, WBS inputs 8, 8, 5, 3, a demo credit per accepted deliverable and at least one tranche lesson. This content pack is checked offline by scripts/rework-content-check.py --project T3 before the factory reads it, and the factory then records the contract, review, artefacts and closure against the snapshot.",
        "assignment": "Rework T3 by the reconstituted TOGAF/MSP lifecycle: author the charter, this Statement of Architecture Work, the seven mandatory Phase A\u2013D artefacts, the delivery contract, the WBS deliverable and the Phase-G review, and re-publish the project-brief and evidence outputs. The architecture work is the PM job itself \u2014 one project run from charter through WBS, scope baseline, staffing, gate routing and closure, with the approver kept distinct from the requester and every baseline change carried by a change request \u2014 recorded retrospectively and dated 2026-11-26.",
        "contract": "This statement is authored retrospectively under the delegation and is approved through the same board gate it describes; its approval is recorded by the factory from this pack, not asserted here. It is complemented by the delivery contract whose commitments, compliance and approval fields this pack authors, on the approved scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa (kind scope, taken 2026-11-26, approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545), and by the Phase-G review the factory records from this pack. The tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9 decided continue on 2026-11-26 and authorised tranche 4; the reopen this pack answers is dated 2026-11-26T07:44:41Z and its reason is the missing contract, review and baseline record.",
        "responsibilities": "The programme architect authors the pack under the #526 rework delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14; the Architecture Board reviews the seven A\u2013D artefacts, which this pack records as approved. The programme owner is the approver of the delivery contract and of the closure: the scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa is approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545, and the closure gate recorded for T3 was decided by an approver distinct from its requester, which is the gate's own rule. The PM runs the request and may not approve a gate it raised.",
        "retrospective": true,
        "scope": "In scope: the charter and its approved gate; the WBS with the four typed WSJF inputs 8, 8, 5, 3; the scope baseline 01a216d0-9c9b-7c8c-a6ef-0d9365632efa taken on 2026-11-26 and approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545; the staffing plan; the gate router that refuses self-approval and records every decision; the change request that must precede a baseline change; the closure record with its closure-review output; the seven A\u2013D artefacts and the project registers. Out of scope: the T2 decision ladder's policy engine, the T4 coach, the T5 kata, the T13 tool platform, the T1 plan spine's implementation and every V1 slice; the rework completes the record and adds no scope."
      },
      "created_at": "2026-11-26T13:11:37.331166+00:00",
      "id": "01a217fb-a475-7824-b01c-bfcfcb4680d3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7915-8833-75f4dc31cc07",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T3 PM job (retrospective, 2026-11-26)",
      "updated_at": "2026-11-26T13:11:37.614131+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2180f-168b-79af-af7f-e4cc18948e7b",
      "approved_at": "2026-11-26T13:32:51.863706+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-26T13:32:51.58027+00:00",
      "id": "01a2180f-15fe-794d-aba4-395c2f06a32f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:51.863706+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d8-8c6d-715e-ad85-da38d5f3351c",
      "approved_at": "2026-11-26T21:52:31.994413+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S05.toml` and plan \u00a77: (a) replay fidelity \u2014 1,000 cards/10 lists, `admin diff` prints zero deltas; (b) one winner \u2014 two clients with the same `If-Match` get one 200 and one 412 carrying the current representation, `version` +1; (c) 1k load \u2014 `GET /api/boards/{id}/cards?limit=200` p95 \u2264 120 ms at 20 rps, 1,000 cards in five pages \u2264 500 ms; (d) webhook shape \u2014 `{event,data,prevData,user}` exactly, `event` the internal register type and `data.list_id`/`prevData.list_id` the lanes; (e) SSE \u2014 a second client on `board:{id}` gets `card.moved` \u2264 200 ms live and on replay; (f) no second store \u2014 one `comment`, one `board_membership`, no `card_membership`, no `attachment`. The instrument is `acceptance/programme/S05.toml`; the closure gate is decided by an approver other than its requester. The snapshot records no Phase-G review of these properties; this rework records that gap rather than claiming a pass.",
        "assignment": "Rework the V1 slice S05 (SPEC \u00a78 S5) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the acceptance instrument.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the board slice of the plan: the nine tables (board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription), the 39 actions, the read projections and cursors, the SSE topics `board:{id}` and the resolved `card:{id}`, the webhook emitter and its `webhook.deliver` job, and the key-referenced `board-export/v1` CLI. Out of scope: attachments and card-level memberships, claims/assignee/stopwatch writes (S06), webhook delivery rows as actions, and the UI (S12)."
      },
      "created_at": "2026-11-26T21:52:31.682863+00:00",
      "id": "01a219d8-8bc7-71a2-8dbe-33702699eac1",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S05 Board (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:52:31.994413+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d9-303c-7fdc-b217-ec349e82329b",
      "approved_at": "2026-11-26T21:53:13.933646+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "Acceptance criteria": "Phase A closes when this statement is approved, the seven A\u2013D artefacts are approved in standard order, and the seven artefacts each carry metadata, entries, links and a document section with authored prose. Delivery acceptance is the plan \u00a77 gate: (a) three revisions with max(seq)=3 and a compare showing the status change; (b) an idempotent seed returning {0,0,N}; (c) both GIN indexes used under enable_seqscan=off with rising idx_scan; (d) relevance where B meets \u00a75.6's recall@5 \u2265 0.75, MRR \u2265 0.60 and B \u2265 C; (e) traceability in both directions with zero cross-tenant rows; (f) quality scoring read-only; (g) no orphan over the 28 RM modules. The record-level acceptance is `acceptance/programme/S07.toml`, whose 17 checks are 15 P0 and two P1.",
        "Assignment": "The assignment is to give the conveyor one governed requirements surface: requirements, principles and decisions share a single `governed_asset` lifecycle carried by S03, every mutation writes one immutable `revision`, `requirements.compare` diffs two revisions in Rust, quality is scored adviserily on a revision, and search ranks assets, cards and catalogue entries through SQL. The work is asked for by the tranche-4 sponsoring group and accepted by the Architecture Board; this statement is authored retrospectively on 2026-11-24 under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 for project 01a1d039-0d2c-7322-b2f9-4dd041153894.",
        "Contract": "The contract record is the delivery contract of project 01a1d039-0d2c-7322-b2f9-4dd041153894, version 1, approved by the Architecture Board on 2026-11-24 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. Its compliance clause is plan \u00a77 and the acceptance instrument `acceptance/programme/S07.toml`; its manifest names that instrument; its acceptance date is the snapshot of 2026-11-24, when the project row is closed at version 7 and tranche 4 (01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce) is authorised by the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9. The close gate is decided by an approver other than the requester.",
        "Responsibilities": "The programme architect (conveyor-e2e) is responsible for the plan, the seven artefacts, this statement, the delivery contract and the retrospective pack. The Architecture Board (conveyor-architecture-board) reviews and approves the artefact set and the Phase-G review. The S03 and S04 owners are consulted because `governed_asset`, `revision`, `catalogue_entry`, `trace_link` and the action registry are their surfaces; S08 and S12 are informed as consumers. The approver of the closure gate is deliberately a different principal from the requester under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14.",
        "Scope": "The phase set is A to D: the stakeholder map, the driver/goal/objective catalog, the requirements catalog, the solution concept diagram, the business capability map, the data entity/data component catalog and the technology standards catalog. The boundary is the plan \u00a75 material: the five \u00a75.1 tables, the 32 actions, the `search` module, the catalogue seed, the quality rubric and job, the eval set and the traceability actions. The project does not build the TOC API, ArchiMate or UML renders, project CRUD, batch bulk operations, RM's promote/project-copy or any UI, and it does not recreate `outline`/`outline_node`, which migration 0011 dropped."
      },
      "created_at": "2026-11-26T21:53:13.637379+00:00",
      "id": "01a219d9-2fa8-703a-99f2-fb7e7572e467",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S07 Requirements and search (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:53:13.933646+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219d9-f717-73ba-9c86-18adeeecd4ef",
      "approved_at": "2026-11-26T21:54:04.836362+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The plan \u00a77 gate: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, then delete yields GET 404 and object_store::head NotFound, asserted in documents_roundtrip; (b) cap \u2014 10,485,760 bytes accepted and one byte more refused 413 application/problem+json with a stable type in documents_limits; (c) secret \u2014 rotate to version 2 and GET returns no plaintext, value key or ciphertext in secrets_never_revealed; (d) workers \u2014 the page renders live rows and a stopped service yields a banner with the breaker open after five failures; (e) reports \u2014 every number equals its SQL count on the same snapshot in reports_match_sql; (f) externals \u2014 GET /api/actions carries timeout_ms on every external and zero reqwest:: outside the executor in externals_declared; (g) billing \u2014 no write action and no metering event in the manifest. The same instrument also requires migration 0013 applied twice as a no-op, FORCE RLS on all four tables, and the nine declared event types. The snapshot records no run of these properties.",
        "assignment": "Rework the V1 slice S09 (SPEC \u00a78 S9) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, this charter, this Statement of Architecture Work and the delivery contract, take the scope baseline with one WBS deliverable, record a retrospective Phase-G review at level consistent, and publish the expected outputs project-brief and evidence. Because all nine linked issues are recorded closed, this rework re-closes the project at level consistent through the acceptance manifest, with the closure and closure review dated 2026-11-24.",
        "contract": "This Statement establishes the delivery contract against acceptance/programme/S09.toml, whose gate text names the seven properties above. Compliance is assessed clause by clause at the 2026-11-24 retrospective snapshot; the project's published evidence page is 01a1e687-7682-753d-b60e-1f00243b13a3 and its pre-rework audit pack is docs/evidence/rework/S09.json, which records verdict red on the missing baseline, charter, contract, Phase-G review and closure outputs. The rework supplies the lifecycle record and re-closes the project retrospectively on the P0 checks of the manifest; the remaining verification limits are carried in the closure review, the review and the evidence output rather than converted into a pass.",
        "responsibilities": "The programme architect (conveyor-e2e) owns the slice plan and this retrospective set; the Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract; the approver (conveyor-approver) decides the Phase-G level; the S03 team supplies tenancy, trace_link and revision, and the S04 team supplies the action registry with its OpenAPI and MCP projection. Operators own the KEK source in the pod environment and the MinIO bucket policy.",
        "scope": "the four tables document, repository, project_repository and secret in migrations/0013_documents.sql; the integrations module (object_store, external, breaker, ingest); documents.create/get/list/download/update/delete/report, 26 of the front matter's 27 actions in all; the nine event types; MinIO bucket conveyor-documents with key t/<tenant>/<document>/<version>; the KEK/HKDF/XChaCha20-Poly1305 secret envelope; repositories as references; the read-only workers, billing and observability displays; report queries over local tables; and the strata handoff. Out of scope: billing logic and metering writes, worker scheduling, artifact objects, code analysis, licence issuance, ingestion and embeddings as services, screens (S12) and project_request.repository_url (S07)."
      },
      "created_at": "2026-11-26T21:54:04.526424+00:00",
      "id": "01a219d9-f671-708a-b28f-063e7e79e6ec",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S09 Documents and integrations (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:54:04.836362+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219dd-4701-7342-9adf-f2c47331c4af",
      "approved_at": "2026-11-26T21:57:41.900644+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T14 gate, executed on the live estate through `acceptance/programme/T14.toml`: (a) each published toolkit has a probe and one live call; (b) `scientific_processor` is scaled up for its probe and one live call, then scaled back, with a receipt; (c) a zero-use report exists; (d) a backend outage unbinds new runs while in-flight runs finish. The snapshot carries no executed receipt for any of the four clauses, so this rework records them as the project's open verification item rather than a pass; the level written is consistent, never non_conformant without a waiver, and no waiver exists.",
        "assignment": "Rework the V2 slice T14 (SPEC_V2 \u00a712) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, record the Phase-G compliance review at the level the record supports, publish the expected outputs (`project-brief`, `evidence`) and leave the project active with its gate recorded as open verification rather than closing it.",
        "contract": "No live Statement of Architecture Work and no live delivery contract exist for T14 at the snapshot; this pair is supplied retrospectively by the 2026-11-24 rework and is the contract of record until a Board-approved one supersedes it. It binds the project to the four gate clauses above, to broker-only calls inherited from T13, to draft status for the named draft services, and to a receipt per claim; it does not authorise closure. The standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 is the authority under which the rework acts.",
        "responsibilities": "the conveyor-approver approves the Phase A\u2013D set and the delivery contract; the conveyor-architecture-board governs the standards; the programme architect (conveyor-e2e) owns T14 and this retrospective pack; the T13 tool-platform team owns the tool family, the broker, the effect classes and the credential profiles; the estate service owners grant the live-call window and the outage drill; the SRO is accountable for the tranche outcome.",
        "retrospective": "written by the 2026-11-24 tranche-4 rework; it reconstructs the record and does not attest contemporaneous intent.",
        "scope": "the estate-toolkit import wave on T13's tool platform: the `tool`, `toolkit`, `tool_binding` and `tool_call` rows for the coding fleet, strata, billing and licence reads, webscraper, media_discovery, scientific_processor, lean4_prover, video-compiler, the w2v orchestrator and the ops read tools; `evidence_review`, `discovery` :8807 and the planned AHTAPol services as `draft`; the probe and the one live call per published toolkit; the `scientific_processor` scale-up and scale-back receipt; the zero-use report; and the backend-outage unbind test. Out of scope: the broker itself (T13), tool authoring and BYO (T15), and the UI."
      },
      "created_at": "2026-11-26T21:57:41.6166+00:00",
      "id": "01a219dd-4673-70da-b701-3f43675b4fa6",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T14 Estate toolkits (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:57:41.900644+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219dd-edc0-7da7-a7fd-00eafabf0eae",
      "approved_at": "2026-11-26T21:58:24.588139+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The pack passes `python3 scripts/rework-content-check.py --project T16 --content docs/evidence/rework/tranche-4/rework-content-T16.json --snapshot /tmp/conv526-w1-snap/data --wsjf 10,8,7,8 --closes false` with exit 0; every cited identifier exists in the snapshot; the seven artefacts carry the existing titles, phase, plateau and kind in the required order; each artefact body carries Metadata, Entries, Links and Document as authored prose; the project brief and evidence carry their template sections with substance; and no closure key appears. The project's own gate remains acceptance/programme/T16.toml, and this review's verdict is consistent.",
        "assignment": "Under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, the retrospective completes T16's governance record against acceptance/programme/T16.toml for the procedure-ingestion slice of SPEC_V2 \u00a712. The assignment is record completion, not rework of the built capability: author the charter, the Phase A\u2013D artefact set, this Statement of Architecture Work, the delivery contract, the Phase-G review and the published project brief and evidence, and leave the project active for a separate close decision.",
        "contract": "The record is delivered under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 against scope baseline 01a216d0-f529-7bcb-81a7-d429d008d638 (approved 2026-11-24T16:58:21Z) and tranche 4 01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce. The delivery contract below records compliance, acceptance, approval, manifest and retrospective limits. Any change to scope or gate returns to the Architecture Board; no waiver is claimed or required, and the project stays active for the tranche-4 review to decide continue.",
        "responsibilities": "The programme architect authors the record under the delegation; the Architecture Board reviews contract and Phase-G conformance; procedure owners and publishers own the sources and the publication decision; delivery teams consume the ingestion and PDDL surface; tenants own their rows and sources; the programme owner owns the outcome and any close decision. scripts/rework-project.py performs the live writes, and scripts/rework-content-check.py refuses this pack before any write if it is structurally wrong or cites an identifier absent from the snapshot.",
        "scope": "In scope: SOP intake; RDF/OWL/Turtle as a strata knowledge pack bound by knowledge_pack_ref; BPMN, Visio and draw.io into PDDL procedure drafts; capability, agent and job design from an SOP; the source-review and human publication gates; re-ingest versioning; bound PDDL execution; and the governance records named in the assignment. Out of scope: any runtime RDF store, graph store or reasoner (decision D53, owner 2026-11-19), T17's tenant-wide generalisation, T19's meetings records, the /procedures destination work of issue #448, and new ingestion scope."
      },
      "created_at": "2026-11-26T21:58:24.307416+00:00",
      "id": "01a219dd-ed35-79eb-b898-3323854d5411",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d38-76fb-9394-b75934d16fcd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T16 SOP, RDF/OWL and process-document ingestion (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:58:24.588139+00:00",
      "version": 2
    },
    {
      "approval_id": "01a219de-7e7e-72db-b751-a17bdd9b013d",
      "approved_at": "2026-11-26T21:59:01.639909+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T18 gate, judged by the fifteen checks of acceptance/programme/T18.toml: every destination route exists with its spec, each spec asserts empty, error, happy and one refusal, the coverage record reports exists only from a real route and a present spec with a *Gate reported as extend naming its owner, no production screen renders a fixture, and a signed-in tenant drives portfolio \u2192 programme \u2192 project \u2192 work package, organisation \u2192 release \u2192 binding, procedures intake \u2192 publish, tools call \u2192 ledger, improve \u2192 kata \u2192 adoption, operations incident \u2192 postmortem and the ADM phase rail from the app, with the T12 loop as the final spec.",
        "assignment": "Rework the V2 delivery slice T18 (SPEC_V2 \u00a710.5 and \u00a712 T18) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter, the Statement of Architecture Work and the delivery contract, take the scope baseline with the one WBS deliverable, record the retrospective Phase-G compliance review, publish the expected outputs project-brief, inception-analysis and architecture-design plus the evidence output, and leave the project active because T18-4 and T18-5 are not delivered. The work is retrospective: it reconstructs the governance record from the project row, the plan, the route table and the issue registry, and it attests no contemporaneous intent.",
        "contract": "The delivery contract record is requested and approved live by scripts/rework-project.py under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 and is titled 'Delivery contract \u2014 T18 The real V2 UI (retrospective, 2026-11-24)'. It commits the lane to the \u00a712 gate and to the two open units as typed dependencies, and it records that this retrospective neither closes the project nor waives any check. The contract's approval is retrospective; the historical snapshot has no live contract, which is one of the failed checks in the prior acceptance record.",
        "responsibilities": "The T18 lane under the programme architect (conveyor-e2e) is responsible for the route table, the screen modules, the specs and the coverage instrument. The Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract. The sponsoring group is accountable for the tranche-4 authorisation and for keeping the project active. The owning slices T4, T5, T7, T9, T10, T11, T13, T16 and T17 are responsible for the actions each destination calls, and T12 owns the final organisation loop.",
        "retrospective": true,
        "scope": "the shell, route registry and navigation; the ten V2 destinations of SPEC_V2 \u00a710.5 and the execution room; the L1\u2013L4 delivery view; one Playwright spec per destination; the V2 coverage instrument measured from apps/web/src/app.rs and the named specs; and the T12 organisation loop as the final acceptance. Out of scope: new tables, actions, events or ports; SSR, a second router or frontend, or a mobile app; retiring or rebuilding the design pack; the V1 routes, which are linked rather than re-implemented; and any production fixture or live tenant write inside the retrospective."
      },
      "created_at": "2026-11-26T21:59:01.357241+00:00",
      "id": "01a219de-7def-7827-8a21-98428640f502",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T18 The real V2 UI (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:59:01.639909+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21a39-aae9-7d90-9069-b0576ed463d4",
      "approved_at": "2026-11-26T23:38:36.773008+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S06.toml` and plan \u00a77: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 `cards.move(done)` yields `run.succeeded`, `approval.approved` and one audit, invocation and event row each; (b) dual control \u2014 `required_decisions=2`, one approve stays pending, the same approver again is 409, a second approver closes; (c) retry \u2014 `kill -9` mid-run leads to `run.reap`, a crashed run, then `attempt=2` with two rows and never two live runs; (d) andon \u2014 engaged gives 409 and a pending `run.execute` for 60 s, released resumes within one poll; (e) delivery retry \u2014 a stub 500 gives `attempts=5`, `state=failed`, one row; (f) no timers \u2014 `rg 'setInterval' crates apps` is empty and every cadence is a `schedule` row; (g) human vs agent \u2014 `decided_by_kind` is `human` for one decision and `agent` for the other. The instrument is `acceptance/programme/S06.toml`; the close gate is decided by an approver other than its requester. The snapshot records no run of these properties and no Phase-G review; this rework records the level `non_conformant` rather than claiming a pass.",
        "assignment": "Rework the V1 slice S06 (SPEC \u00a78 S6) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure decision, and re-close through the acceptance instrument once its properties run.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the control-plane slice of the plan: the six tables of `02-DATA-ARCHITECTURE \u00a72.6` (`channel_binding`, `channel_preference`, `message`, `delivery`, `audience`, `audience_member` \u2014 the last two PENDING-S0), the 17 actions, the 17 events, the SSE topics `run:{id}`, `approval:{id}` and `tenant`, the Telegram in/out path with its sender-identity resolution, the andon gauge and poller pause, and the job kinds that replace 11 of fr's 14 timers (three deleted). Out of scope: the signed mcp_server channel (S06a), MCP tool surfacing (S10), `project_request.timeout` (S07), the UI (S12) and the audience screens (PENDING-S0)."
      },
      "created_at": "2026-11-26T23:38:36.518039+00:00",
      "id": "01a21a39-aa69-7167-8696-e615aa3b012f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06 Control Plane (retrospective, 2026-11-27)",
      "updated_at": "2026-11-26T23:38:36.773008+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c16-879a-765f-9e1b-c788a9c1ef4a",
      "approved_at": "2026-11-27T08:19:28.421371+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The S08 gate is the slice gate of SPEC \u00a78 S8: the Part A round-trip from principle to ADR to TOC to register to deliverable to capability to stakeholder, the TOC cycle refusal, the absence of a duplicate principles/decisions surface, three golden models in SVG/PNG/PDF, the profile's write-time 422, the XMI round-trip, the post-commit validation job, deterministic ids, the two retention producers and the render byte-route parity.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the S08 slice: the governance surface over S03's spine and the modelling surface over seven new tables, with a deterministic renderer and a profile-scoped XMI exchange. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the S08 slice plan.",
        "constraints": "The build must not introduce a second model store, must not use the registry as the validation authority, must not call load_system_fonts, must not query JSONB geometry in SQL, must not validate inside the mutation transaction, and must not trust an XMI document: size, depth, duplicate ids and a DOCTYPE are refused. These constraints are the plan's traps and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set, the contract and the profile; S03 supplies the spine tables and RLS; S04 supplies the registry and invocation route; S07 supplies the outline and the principles/decisions actions; S12 receives the declared trace/architecture surface; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the slice plan, the measured \u00a77.2 completion record and the record's project row. It records the assignment and the acceptance the build was measured against; it does not claim that a contemporaneous board approved this text on 2026-11-08, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are the 68 actions, the seven S08 tables, the two retention producers, the two byte routes and the seven Phase A\u2013D artefacts. Out of scope are the UI, model search, project CRUD, principle-conflict verdicts, architecture containers, change-set/review/publication splits, graph layout and OCL/Profile/SysML; each exclusion is named in the slice plan's \u00a73 so a later reader cannot mistake absence for omission."
      },
      "created_at": "2026-11-27T08:19:28.107129+00:00",
      "id": "01a21c16-86ee-7106-8eab-4155e8b4b141",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2c-7bad-8e6c-b5c5d1fd40e2",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S08 Governance and Modelling (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:19:28.421371+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c1b-b5de-788c-9918-d57c95016a17",
      "approved_at": "2026-11-27T08:25:07.941014+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The S17 gate is the slice gate of SPEC \u00a78 S17: the banked parity oracle 555/555, gold 50/50 and empty 0/50, the golden eval scripts 50/50, the exact McNemar vectors, RLS on all four tables, the fail-closed sandbox, deterministic Job names and the 1 MiB ConfigMap cap, no answers to producers, no benchmark action in AGENT_TOOLS, and the live sandbox lifecycle recorded on compute-1.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the S17 slice: one measurement loop over four tables, a hashed seed artifact, a pure grader and exact McNemar, a k8s Job sandbox and a single run driver. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the S17 slice plan.",
        "constraints": "The build must not import Python into the product or the image, must not let a producer see answers, must not widen the sandbox config to a default namespace, must not exceed the 1 MiB ConfigMap cap or the 256 KiB patch cap, must not let the conveyor arm start before S18, and must not repoint the grading node. These constraints are the plan's traps and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set, the contract and the measurement standard; S02 supplies the job/event/audit tables; S03 supplies the RLS spine and the document row; S04 supplies the registry and scopes; S09 supplies the object store the eval log is written to; S18 takes the conveyor arm; S12 receives the declared screens; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the slice plan, docs/evidence/S17-CORE.md and the record's project row. It records the assignment and the acceptance the build was measured against; it does not claim that a contemporaneous board approved this text on 2026-11-09, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are the four benchmark tables, the seed artifact and its image lock, the standalone seed generator, the grader and 12 parsers, the exact McNemar, the sandbox adapter, the gold/empty/baseline arms, the eight actions, two scopes, two events, and the declared S12 screens. Out of scope are the conveyor arm (S18), model hosting, HF calls, the old CLI, a second service and Python; each exclusion is named in the slice plan's \u00a73."
      },
      "created_at": "2026-11-27T08:25:07.66928+00:00",
      "id": "01a21c1b-b557-7b0a-ba9d-bc9a2de9247d",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d30-794f-bd53-c72986b18e60",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S17 Native Benchmark Harness (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:25:07.941014+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c1d-aa55-7091-ae4b-93328e410035",
      "approved_at": "2026-11-27T08:27:16.055256+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The T17 gate: a brief plus documents draft the catalogue; documents yield linked procedure and policy drafts; an unpublished dependency is refused; a reviewed catalogue publishes with its decisions; no asset publishes itself; a non-human decision cannot complete a gate; no asset widens a tool grant; and the real-app E2E creates a draft and sees the row render.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the T17 slice: the authoring lifecycle over two tables, a human review gate, a published-dependency check and a publish-through-the-target rule. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by the T17 inception set and SPEC_V2 \u00a75.7/\u00a710.4\u2013\u00a710.5.",
        "constraints": "The build must not write the target tables, must not let an asset publish itself, must not let a non-human decision complete a gate, must not widen a tool grant, must not duplicate the governed_asset lifecycle and must not refuse the dependency order later than the draft attempt. These constraints are the inception brief's risks and are carried into the contract.",
        "responsibilities": "The programme architect owns the slice and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set and the contract; T3 supplies the decision ladder and the approval primitives; T16 owns live ingestion and the SOP human gate; T13 owns tools and bindings; the web team owns the Authoring screen and the E2E; the tenant authors and reviewers take the human decisions; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the inception set, the evidence and closure-review documents and the record's project row. It records the assignment and the acceptance the delivery was measured against; it does not claim that a contemporaneous board approved this text, and it carries that limitation into the Phase-G review.",
        "scope": "In scope are intake, drafting, review and publication of organisation/department/job/procedure/policy assets, the two authoring tables, the state machine, the review gate, the published-dependency refusal and the Authoring screen. Out of scope are writing the target tables, the SOP/RDF/BPMN converter (T16), tools and bindings (T13), a governed_asset fork and a new template table except asset_template; each exclusion is named in the inception brief's Boundary."
      },
      "created_at": "2026-11-27T08:27:15.79094+00:00",
      "id": "01a21c1d-a9d1-70d8-8a6a-901c21e4dd7b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d38-7db1-b685-d76bd4cebf47",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T17 Tenant Asset Authoring (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:27:16.055256+00:00",
      "version": 2
    },
    {
      "approval_id": "01a21c26-afda-78c1-aea9-920275ed65c4",
      "approved_at": "2026-11-27T08:37:07.301597+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The \u00a712 T7 gate: an injected pod failure is triaged, mitigated by an approved write, resolved and postmortemed; a destructive action without the tenant's configured approvals is refused; restart, scale, rollback and flag-flip are callable. The executed suite maps each clause to a named test in T7-acceptance.md; the live cluster script is deploy-bound and is not claimed as run.",
        "assignment": "The programme architect (conveyor-e2e) is assigned the T7 record: the incident lifecycle over the existing card and approval primitives, the four gated cluster writes, the postmortem/kata loop and the /operations console. The assignment is recorded retrospectively for the 2026-11-27 rework and is bounded by SPEC_V2 \u00a76.5, \u00a79.2 and \u00a712 T7.",
        "constraints": "The build must not let an ops write reach the cluster without an approval, a recorded dry run and two distinct human approvers; must not let the mitigation wrapper bypass the gate; must not let an incident that needed no write stay unresolvable; must not widen a tool grant; and must not claim the live cluster proof before the deployed image carries the gate. These constraints are the finding acceptance and are carried into the contract.",
        "responsibilities": "The programme architect owns the record and the acceptance instrument; the Architecture Board approves the Phase A\u2013D set and the contract; T2 supplies the job/event/audit rows; T4 supplies the registry and the destructive effect; T13 owns tools; the web team owns the /operations screen; the on-call operators take the incident decisions; the SRO is accountable for the tranche outcome.",
        "retrospective": "This Statement of Architecture Work is authored by the 2026-11-27 rework from the SPEC row, the issue #78 delivery, the acceptance evidence and the two fixes at ca155fada. It records the assignment and the acceptance the delivery was measured against; it does not claim a contemporaneous board approval, and it carries the live-proof limitation into the Phase-G review.",
        "scope": "In scope are the five incident actions, the four write verbs and their gate, the k8s read family, the incident meta lifecycle and the /operations screen. Out of scope are tools and bindings (T13), the agent tool surface (the ops writes are not agent-reachable) and a second ops service; each exclusion is deliberate."
      },
      "created_at": "2026-11-27T08:37:06.989963+00:00",
      "id": "01a21c26-af30-7c3e-ad6d-39f7ae1937ea",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d34-7f89-a0be-f3de3c9f540e",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T7 Operations (retrospective, 2026-11-27)",
      "updated_at": "2026-11-27T08:37:07.301597+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2210c-1cd2-7eb0-a3d2-c90c96b7d6e7",
      "approved_at": "2026-11-28T07:26:11.809378+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is one real agent task over `/mcp`: create a project, read its progress, open and answer an approval, close the project, with `decided_by_kind='agent'` recorded, no `nonagenticai_mcp_server` component in the path, and no `X-MCP-Binding` header. The local proof is `crates/api/tests/mcp_agent.rs::one_licence_principal_task_creates_reads_decides_and_closes`; the deployed agent-app run is S14's gate and is recorded as deferred.",
        "assignment": "The architecture work assigns S06b the agent's direct interface to the conveyor: define the tool key every MCP tool carries, resolve a machine principal from a licence bearer, propagate that principal into attribution, and add the approval-request action an agent needs to escalate. This statement governs the retrospective record of work completed on 2026-11-09 and re-read at the 2026-11-27 snapshot.",
        "constraints": "The one-membership rule rejects zero or several memberships, so the tenant is taken from the licence. A JWT-shaped token that fails JWT verification never falls through to licence introspection. The licence service is on the request path, so its verdict is cached for at most 60 seconds, and an unreachable service refuses with `503 licence_unavailable` rather than admitting. The cache is keyed by the token hash, never by tenant.",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. The agent platform is consulted as the consumer, and the tenant admin is informed because the tool allowlist is the authorization surface.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the 2026-11-09 delivery was recorded only in the slice plan and the evidence file. The statement records what the tree and the suites prove today and names the deferred live step instead of implying it happened.",
        "scope": "In scope are `_meta.operation` on every registered tool, the licence introspection in `crates/auth/src/machine.rs` with its 60-second capped cache and fail-closed rule, `Principal.kind` across human, agent and service, `approvals.request` over `insert_approval_tx`, and the end-to-end `/mcp` session in `crates/api/tests/mcp_agent.rs`. Out of scope are the `fr` toolkit, MCP tasks (S06c), the copilot surface (S10) and any signed HMAC channel (S06a, superseded)."
      },
      "created_at": "2026-11-28T07:26:11.487462+00:00",
      "id": "01a2210c-1c22-792c-8f82-18dc219fba9c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06b Agent Interface (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:26:11.809378+00:00",
      "version": 2
    },
    {
      "approval_id": "01a2210e-f7bb-70ff-8d4d-5d6f216ed585",
      "approved_at": "2026-11-28T07:29:18.922843+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is a scripted session: open a conversation, ask for a card move, receive a proposal and one pending approval with zero action invocations; approve as an authorized member and see exactly one invocation, the card moved and the turn completed citing that invocation; reconnect with `Last-Event-ID` and receive the turn once; see a low-scope principal's two surfaces omit the same tool; see an unavailable router fail the turn with zero invocations; and see the coverage gate fail an unmapped operation by name. The local proofs are `crates/api/tests/copilot_session.rs`, `copilot_proposal.rs`, `copilot_approve.rs`, `copilot_replay.rs`, `copilot_surface.rs`, `copilot_turn.rs` and `crates/actions/tests/copilot_coverage.rs`.",
        "assignment": "The architecture work assigns S10 the copilot's one action surface: project the S04 registry by principal, turn a destructive tool call into an S06 approval, run the conversation as two tables and an event stream, and keep model access outside the conveyor. This statement governs the retrospective record of work built in the S10 slice and re-read at the 2026-11-27 snapshot.",
        "constraints": "S10-D1 keeps the surface a query over `ActionSpec`, so no second registry or tool table exists. S10-D2 makes a proposal an approval created through S06's `insert_approval_tx`, with one pending proposal per turn enforced by the partial unique index on `(tenant_id, meta->>'turn_id')`. S10-D3 makes execution a job with a conditional claim, a 120-second lease, re-checked proposer scopes and the proposal idempotency key. S10-D4 makes only terminal turns durable, with the terminal meta, the proposal and the `conversation_turn.created` event committed together. S10-D5 makes `completions.create` fail closed: an outage, timeout or open breaker ends the turn `failed` and runs no tool. S10-D6 declares `copilot.chat` the only coverage exemption. S10-D7 keeps AG-UI as a serializer over the native stream.",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. S12 is consulted as the renderer of the declared screens, operations is consulted on model availability and key custody, and the tenant admin is consulted because the channel grants and the approval decision are the authorization surface.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the S10 row was created on 2026-11-12 and the build was recorded in the slice plan and the receipt. The statement records what the tree and the suites prove today, names the pre-pack receipt as red on lifecycle checks, and leaves the deployed UI and router proof to S12 and S14 instead of implying it happened.",
        "scope": "In scope are the `conversation` and `conversation_turn` tables with forced row-level security, the turn runner and its eight-round cap, the `agent_surface` projection over the action registry, proposals carried as `approval(kind='copilot.action')` with sealed inputs, the conditional execution claim and sweep, the SSE topic `conversation:{id}` with `Last-Event-ID` replay, `completions.create` as an external action with a 60-second timeout and breaker, the AG-UI serializer, and the screens declared as data for S12. Out of scope are inference-router (D14), the `/mcp` transport and scope filtering (S04), the approval state machine (S06), fr's 34 AG-UI action modules, the agent's principal and tool contract (S06b), and chat UI rendering (S12)."
      },
      "created_at": "2026-11-28T07:29:18.626402+00:00",
      "id": "01a2210e-f724-7ebf-8b08-74d6c0f24f4c",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S10 Copilot / AG-UI (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:29:18.922843+00:00",
      "version": 2
    },
    {
      "approval_id": "01a22110-1c69-7458-a9be-c5550ac1ee8a",
      "approved_at": "2026-11-28T07:30:33.839885+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The gate is author \u2192 publish \u2192 execute a process run: an SOP and a three-step process are created and published, a card starts the run, the second step parks on an approval, the approval resumes it, and the run finishes with three succeeded run_step rows, non-empty input and output, and one approval. The negative proofs are an unresolved template (422 refusal, never an empty string), an unknown action refused at publish, and a retry that appends a second attempt row. The local proof is the suite set in crates/actions/tests plus the domain unit tests; the plan's \u00a77 two-process rehearsal is the live gate and is recorded as not executed in this pack.",
        "assignment": "The architecture work assigns S11 the executable-documentation capability: define a versioned process as an ordered list of steps, give each step a registry action and a resolved input template, attach a versioned SOP to each step, validate the whole draft at publish, and execute a published process as a run with one immutable run_step per attempt. This statement governs the retrospective record of work built on 2026-11-09 and re-read at the 2026-11-27 snapshot.",
        "constraints": "The store forbids a second published or draft row per (tenant_id, key). Publish validates in one transaction against the live manifest and refuses an unknown action, a step naming process_runs.*, a forward step reference, a retry on a non-idempotent action, an unresolved gate kind and an SOP whose inspection is empty. The resolver is one pure function with the roots run, steps, card and procedure; an absent path refuses and a null resolves. The executor claims a queued step with a status-guarded update, persists the resolved input at claim, and re-invokes a replayed attempt under the same idempotency key derived from (run_id, position, attempt).",
        "responsibilities": "The programme architect authors this retrospective statement; the delivery team (conveyor-e2e) is accountable for the store, resolver, actions and executor evidence; the Architecture Board reviews the artefact set; the SRO accepts the project's closure at the tranche-6 review. The process author is consulted because tenant authorship is the capability's consumer, and S13 and S12 are informed because they consume the run tree and the declared read models.",
        "retrospective": "This statement of architecture work is authored on 2026-11-27 for the tranche-6 rework. No contemporaneous SoAW exists; the 2026-11-09 delivery was recorded in the slice plan and in the test tree. The statement records what the tree and the suites prove today and names the deferred live gate, the zero seed and the S15 seam instead of implying they happened.",
        "scope": "In scope are migrations 0015 (procedure, procedure_step, run_step), 0016 (the draft index), 0017 (lock_version), 0021 (the run capability seam), 0022 (the kata_executor role) and 0035 (the rename to process/process_step, the SOP table and process_map); the process, procedure (SOP) and process_map actions; publish validation and the frozen revision; the resolver in crates/domain/src/template.rs; and the executor in crates/actions/src/process/step.rs dispatched as the process.step job. Out of scope are matrix legality (S03), approval transport (S06), sub-processes and the engine (S13), screens (S12), PDL (never) and the deferred default seed."
      },
      "created_at": "2026-11-28T07:30:33.564273+00:00",
      "id": "01a22110-1bde-74d0-8b3e-e693e6c7291b",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S11 Processes, Steps and SOPs (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:30:33.839885+00:00",
      "version": 2
    },
    {
      "approval_id": "01a222a6-2960-71b2-8edc-797253376758",
      "approved_at": "2026-11-28T14:54:04.782673+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks, each with its own target: exactly one task-capable action read from the compiled registry; the project-scoped `inputRequests`; the answer written as `approvals.decide`; a terminal result that never says `failed` on the wire; one `-32602` for every ownership failure; and the parity suite proving the task surface and the HTTP action agree.",
        "assignment": "Deliver the architectural record for durable agent tasks: a client that declares the MCP task extension drives a long-running project \u2014 starting it, polling it, answering the approval it waits on and reading a terminal result \u2014 without a second store and without a service between the client and the conveyor. The assignment is retrospective: the product shipped on 2026-11-09 and this statement records the architecture it was built to.",
        "constraints": "No new table and no new service. The run state machine is the task lifecycle, so the reaper had to learn to skip project executions and the one-live-run property had to become a partial unique index because the existing index does not dedupe nulls. Every constraint in this project is a consequence of the reuse decision rather than an independent choice.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the agent interface; the MCP client author is the consumer whose conformance the contract is written against. The retrospective author is not the accepting role, so the separation of duties holds even though one session wrote the product record and this set.",
        "retrospective": "This statement is written after the fact and declares so. The slice plan's build record gives the shipped state; the gate's compiled-registry assertion is the one the plan had to correct, because its first form grepped a flag no serialised surface carries. That correction is recorded here rather than tidied away.",
        "scope": "In scope: the Phase A\u2013D artefact set, the task-to-run identity, the project-scoped `inputRequests` listing, the answer path through `approvals.decide`, and the two indexes migration 0023 adds. Out of scope: the agent's own task support against external MCP servers, decommissioning `mcp_server`, SSE or push delivery, a progress percentage, and any new table."
      },
      "created_at": "2026-11-28T14:54:04.488688+00:00",
      "id": "01a222a6-28cb-7ab4-b430-124b3ec82cb8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06c MCP Tasks (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:54:04.782673+00:00",
      "version": 2
    },
    {
      "approval_id": "01a222a7-1881-7d2e-a3b5-928c86cb2b76",
      "approved_at": "2026-11-28T14:55:06.005722+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest's checks: the capability aggregate resolves with its executor and versioned binding; readiness refuses a run with `409 capability_not_ready` before the first step; a cited passage comes from a pinned strata pack; memory is written, consolidated into `strata_rag` and recalled by `source_id`; and a degraded source mid-run reports `knowledge_degraded`, not the pre-run refusal.",
        "assignment": "Deliver the architectural record for a capability aggregate: a capability is a persisted thing with an executor spec, a versioned knowledge binding and memory, which a card's run resolves and readiness-checks before its first step. The assignment is retrospective \u2014 steps 1\u20134 shipped on 2026-11-09 and step 5's actions remain open \u2014 so this statement records the architecture and the remaining work together.",
        "constraints": "No second loop and no second retrieval path. The conveyor owns the binding and strata owns retrieval, so a run that read knowledge by any route other than `knowledge.query` would make the pinned pack advisory. Every new table carries a tenant column with row-level security forced, and documents are immutable \u2014 ingest a new version and prune the old, never update in place.",
        "responsibilities": "The programme architect authors the set under the standing delegation; the Architecture Board reviews it; the SRO is accountable for the capability model; the capability author writes the pack and its readiness declaration; the retrieval owner operates the strata pack. The retrospective author is not the accepting role, so the separation of duties holds.",
        "retrospective": "This statement is written after the first four build steps and before the fifth, which makes it unusually literal: it records what exists and what does not rather than reconstructing a finished shape. The seven remaining actions, the unproved pack-bound retrieval and the unproved run-transactional memory write are named as open work.",
        "scope": "In scope: the Phase A\u2013D artefact set, the capability tables and their versioning, the pre-run readiness check, the pinned retrieval binding, and the memory write, consolidation and recall. Out of scope: the model loop (S13's `agent.run`), the interactive copilot surface (S10), embeddings and answer generation (the executor's), a tenant-authored prompt store, KAG, and S08's dropped `project_capability` ledger."
      },
      "created_at": "2026-11-28T14:55:05.705673+00:00",
      "id": "01a222a7-17ec-751f-bc1c-9e932f9b4763",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S15 Capability, Knowledge and Memory (retrospective, Phase A)",
      "updated_at": "2026-11-28T14:55:06.005722+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

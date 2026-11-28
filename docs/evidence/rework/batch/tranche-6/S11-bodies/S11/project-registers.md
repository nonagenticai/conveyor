# Project Registers — Processes, steps and SOPs — executable documentation

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The executor proofs run in the integration harness against Postgres, with the API and worker driven in-process; the plan's \u00a77 block starts a real server and worker under CONVEYOR_DEV_AUTH=1 and drives them with curl as dev:alice and dev:reviewer. The harness exercises the same execute_step path, but it is not the \u00a77 rehearsal, and no run id from a two-process server session is recorded at this snapshot.",
      "code": "S11-R1",
      "created_at": "2026-11-28T07:30:40.183769+00:00",
      "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S11-procedures.md#7"
        ]
      },
      "meta": {
        "owner": "the release manager (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": "Run the \u00a77 block against dev-a at the tranche-6 review, retain the run id and the three succeeded run_step rows, and re-read this risk against that capture.",
      "review_at": "2026-12-11",
      "severity": "medium",
      "status": "open",
      "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack",
      "updated_at": "2026-11-28T07:30:40.183769+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The plan records George's 2026-11-09 deferral: S11 closes with zero seeded procedures, and build step 11 expects a tenant-scoped count of 0 published rows. A tenant must author and publish a process and an SOP before process_runs.start can succeed on a card. The default 280-flow catalogue is vocabulary data plus a follow-up derivation, not delivered state.",
      "code": "S11-R2",
      "created_at": "2026-11-28T07:30:40.325091+00:00",
      "id": "01a22110-3648-7562-99ca-bcd43f99f380",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S11-procedures.md#5.6",
          "docs/plans/slices/S11-procedures.md#6"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "high",
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": "Derive one process per S0-evidenced cap.* flow from parity/evidence/executions.tsv plus a read-only census of the n8n workflow_entity nodes, record the census in parity/evidence/cap-flow-census.tsv, import seed/procedures.json through migrate --seed, and seed no zero-evidence flow.",
      "review_at": "2026-12-11",
      "severity": "medium",
      "status": "open",
      "title": "The procedure seed is zero by decision, so no default process or SOP exists",
      "updated_at": "2026-11-28T07:30:40.325091+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Migration 0021 adds capability_id and capability_agent_id to run without foreign keys, and process_runs.start calls one resolve_start_context plus ensure_start_ready pair before writing anything. Today the resolver returns StartContext::Unresolved because no capability schema exists, so readiness and allowed_actions are not enforced at start; the enum makes the skip explicit rather than silent.",
      "code": "S11-R3",
      "created_at": "2026-11-28T07:30:40.469182+00:00",
      "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S11-procedures.md#8"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": "S15 fills the Resolved body (readiness, 409 capability_not_ready with blocking[], allowed_actions passed into the invocation) and must not add a second start path; re-read this risk when S15 closes.",
      "review_at": "2026-12-11",
      "severity": "medium",
      "status": "open",
      "title": "The executor-scoping seam is unresolved until S15",
      "updated_at": "2026-11-28T07:30:40.469182+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Issues

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 4,
  "rows": [
    {
      "baseline_ref": null,
      "body": "Closed in an unauthorised tranche before the tranche review; accepted by the delegate as a retrospective exception under the standing delegation. The project is reworked in its batch and re-closed through the gates.",
      "code": "U3-EXC-S11",
      "created_at": "2026-11-21T01:17:45.768577+00:00",
      "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": "P0",
      "status": "accepted",
      "title": "Out-of-sequence closure accepted as a retrospective exception (S11)",
      "updated_at": "2026-11-21T01:17:45.768577+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "At the 2026-11-27 snapshot the project row is closed with meta.measured_status built and plan_status built, while the receipt docs/evidence/rework/S11.json is red on 14 of its 19 checks: baseline, charter, closure outputs, contract, review, closure gate, PI cadence, PI plan, PI objectives, WSJF, demo credits, lessons, artefact standard and rework. Only project, waivers and product pass. The rework packages are project_in_unauthorised_tranche, closed_without_closure_review, closure_outputs_undeclared, unpublished_declared_output and mandatory_artefacts. The capability and its suites exist; the lifecycle record did not.",
      "code": "S11-I1",
      "created_at": "2026-11-28T07:30:40.613629+00:00",
      "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": "This pack is the repair: it supplies the charter, the seven artefacts, five registers, the contract, the WBS, the Phase-G review, both declared outputs and the closure review; the factory re-runs the 17-check manifest in acceptance/programme/S11.toml after publishing.",
      "review_at": null,
      "severity": "high",
      "status": "open",
      "title": "The project was closed with no lifecycle set and its receipt is red",
      "updated_at": "2026-11-28T07:30:40.613629+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The plan's \u00a77 negative expectation names 422 procedure.template_unresolved and 422 procedure.action_unknown, while the tree's refusal codes are process.template_unresolved and process.action_unknown. The plan's frontmatter also names migration 0033 for the process/step/SOP split, but 0033 is the D23 eval caller and the rename lands in migrations/0035_process_tree.sql. Both are documentation drift, not runtime defects; the suites assert the tree's behaviour.",
      "code": "S11-I2",
      "created_at": "2026-11-28T07:30:40.756507+00:00",
      "id": "01a22110-37f7-759e-8a42-4da978f8648e",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": "Amend the plan's \u00a77 code names and its migration reference to match the tree; do not change running code to match a document.",
      "review_at": null,
      "severity": "low",
      "status": "open",
      "title": "Plan and tree disagree on two names a reader would run",
      "updated_at": "2026-11-28T07:30:40.756507+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The 2026-11-25 receipt carried a P0 package project_in_unauthorised_tranche:S11 because tranche 6 was authorised only by the tranche-5 review on 2026-11-27. The programme record now carries tranche 6 authorised, and this project is one of its members; the package is expected to clear on the pinned post-authorisation snapshot, and this pack's receipt on that snapshot is the proof, not the older register's text.",
      "code": "S11-I3",
      "created_at": "2026-11-28T07:30:40.894959+00:00",
      "id": "01a22110-3881-7d27-9b58-11fda9beb116",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": "Read the fresh receipt's rework_resolution check on the pinned snapshot; if the P0 package still appears there, the snapshot predates the authorisation and the receipt must be rebuilt.",
      "review_at": null,
      "severity": "medium",
      "status": "open",
      "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot",
      "updated_at": "2026-11-28T07:30:40.894959+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Assumptions

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 4,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The delivery contract judges acceptance against the record at the pinned rework snapshot; the content checker refuses any id that appears nowhere in it. Every judgement in this pack is read from that snapshot, so if it is incomplete or stale for S11 the pack's facts and any re-close move with it.",
      "code": "S11-A1",
      "created_at": "2026-11-28T07:30:41.039091+00:00",
      "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The 2026-11-27 rework snapshot is the authoritative record for S11",
      "updated_at": "2026-11-28T07:30:41.039091+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A gated step parks as an S06 approval with kind='procedure', gate=gate_kind, meta carrying position, attempt and procedure_step_id, quorum from the gate catalogue and expires_at when present; approval resumes the step and rejection or expiry applies on_result. The assumption is that no second gate mechanism is introduced in the executor; the gate suite proves the park, the approve, the reject paths and the expiry path.",
      "code": "S11-A2",
      "created_at": "2026-11-28T07:30:41.181373+00:00",
      "id": "01a22110-39a0-736e-8cce-7bd666941127",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The S06 approval state machine is the only gate machine for a process step",
      "updated_at": "2026-11-28T07:30:41.181373+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "process_runs.complete_step was dropped on 2026-11-09 because an external mark-complete would set a result without executing it; the executor claims a queued step with a status-guarded update and a terminal replay is a no-op. The assumption is that no consumer introduces a hand-write path; the suites assert that a terminal replay is a no-op and that a run cannot be finished by hand.",
      "code": "S11-A3",
      "created_at": "2026-11-28T07:30:41.323092+00:00",
      "id": "01a22110-3a2e-7157-a617-06e8947f525c",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "A run step is only ever written by the executor",
      "updated_at": "2026-11-28T07:30:41.323092+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A run starts only from a published process row, and a published step is immutable; an edit forks v+1 and publish re-reads nothing at step time because the resolver reads the run's frozen materialisation. The assumption is that no seed, migration or operator path writes a published process outside the action's publish transaction; the start suite refuses a draft and the store's partial unique indexes hold the one-published rule.",
      "code": "S11-A4",
      "created_at": "2026-11-28T07:30:41.468358+00:00",
      "id": "01a22110-3abf-7283-9ca3-ff135d816650",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "Publish is the only path that creates an executable version",
      "updated_at": "2026-11-28T07:30:41.468358+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Dependencies

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The project row records depends_on needs S03, needs S04, needs S06 and needs S07. S03 supplies run, approval, card_type, catalogue_entry and revision plus run::transition; S04 the action registry and JSON-Schema validation; S06 runs.*, approvals.decide|list, the job runner and SSE run:{id}; S07 owns migrate --seed and the card_type and gate catalogue the publish check reads. All four are green at the same snapshot.",
      "code": "S11-D1",
      "created_at": "2026-11-28T07:30:41.611615+00:00",
      "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "met",
      "title": "S11 depends on S03, S04, S06 and S07",
      "updated_at": "2026-11-28T07:30:41.611615+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "S12 consumes the declared processes, procedures (SOP), process_step and run_step read models and SSE run:{id}; S13 owns sub-processes, the run tree, the judge and the bus, and the plan caps call depth at 3 until those exist. Neither can be evidenced before the process actions and the executor exist, so both are carried as blocks rather than related work.",
      "code": "S11-D2",
      "created_at": "2026-11-28T07:30:41.752093+00:00",
      "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S11 blocks S12's read models and S13's sub-process engine",
      "updated_at": "2026-11-28T07:30:41.752093+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Migration 0021 adds the run capability columns and process_runs.start calls one resolve_start_context plus ensure_start_ready pair; S15 must fill the Resolved arm without adding a second start path. Decision 6 records that S16's prompt audit changes the built procedures.publish by adding an optional acknowledge_findings member and a 422 refusal, with the override written to the publish audit_event. Both are seams into built actions and are recorded as open because neither is evidenced in this pack.",
      "code": "S11-D3",
      "created_at": "2026-11-28T07:30:41.893116+00:00",
      "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S15 fills the executor-scoping arm and S16 changes the built publish action",
      "updated_at": "2026-11-28T07:30:41.893116+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Benefits

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 4,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The plan's starting premise is that the per-type flows are not 280 programs: matrix legality is catalogue data, card types and gates are catalogue rows, and the executable part is a process of registry-action steps. The benefit is a structural reduction in code and configuration; it is declared, not measured, and the seed that would prove the default catalogue is deferred.",
      "code": "S11-B1",
      "created_at": "2026-11-28T07:30:42.038476+00:00",
      "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The 280 card-type by list flows become vocabulary data plus a few processes",
      "updated_at": "2026-11-28T07:30:42.038476+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A run keeps one run_step per position and attempt, with the resolved input persisted at claim and the output recorded after the call; a retry appends rather than overwrites. The benefit is an audit property: the run's history is read from the database, not reconstructed from logs, and process_trace and process_run_output read the same rows.",
      "code": "S11-B2",
      "created_at": "2026-11-28T07:30:42.183192+00:00",
      "id": "01a22110-3d8a-743b-9028-eb9934e06816",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Every attempt is an immutable, inspectable row",
      "updated_at": "2026-11-28T07:30:42.183192+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The SOP carries definition of ready, definition of done, acceptance criteria, inspection and declared outputs, and its inspection is validated at creation; a step names the published SOP it is judged against. The benefit is that the standard travels with the process and can be inspected by a checker, a laya judgement or an eval rather than living in a document nobody executes.",
      "code": "S11-B3",
      "created_at": "2026-11-28T07:30:42.324794+00:00",
      "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A step is judged against a published SOP, not prose",
      "updated_at": "2026-11-28T07:30:42.324794+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A process is a version family per tenant with draft, published and retired states; a second draft is refused with 409 process.draft_exists and a re-publish returns the same version. The benefit is that a tenant can change how its cards move through work without a code release, while publish remains the single validation gate.",
      "code": "S11-B4",
      "created_at": "2026-11-28T07:30:42.467426+00:00",
      "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A tenant authors its own process without a deploy",
      "updated_at": "2026-11-28T07:30:42.467426+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Changes

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 1,
  "rows": [
    {
      "baseline_ref": "01a22110-3f41-731b-9f29-5e8f5908f0d6",
      "body": "The retrospective MSP/TOGAF/SAFe rework changed this project's record: its artefacts were re-versioned with documents and links, its register was written, and its schedule and cost baselines were taken. This change records that rework against the frozen scope baseline.",
      "code": "CH-S11-055179",
      "created_at": "2026-11-28T07:30:55.434034+00:00",
      "id": "01a22110-7150-70df-8542-b7e849055179",
      "impact": null,
      "kind": "change",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {
        "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
        "impact": {
          "scope": "the retrospective rework's record acts"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "The retrospective rework of S11",
      "updated_at": "2026-11-28T07:30:55.815549+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```

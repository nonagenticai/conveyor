# Project Registers — Agent direct interface — the conveyor's own MCP surface

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The local proof runs the whole task over the real `/mcp` HTTP surface as a licence principal, but it does not run the `agent` app's own importer or a deployed conveyor. The plan records both as S14's gate. Until that run exists the end-to-end consumer contract is evidenced on the loopback path only, and no retrospective pack may claim more than the suite proves.",
      "code": "S06b-R1",
      "created_at": "2026-11-28T07:26:18.652187+00:00",
      "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S06b-agent-direct-interface.md#7"
        ]
      },
      "meta": {
        "owner": "the release manager (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": "Run the agent app against the deployed conveyor at S14, record the importer's tool keys and the task's project, approval and decision ids, and re-read this risk against that receipt.",
      "review_at": "2026-12-11",
      "severity": "medium",
      "status": "open",
      "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only",
      "updated_at": "2026-11-28T07:26:18.652187+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The fail-closed rule is deliberate: an unreachable licences service refuses with 503 licence_unavailable and admits no principal. The cost is availability, not confidentiality \u2014 a licences outage stops every agent call. The 60-second cache bounds the blast radius but does not remove it, and the cache is keyed by token hash so a revoked token cannot ride another tenant's entry.",
      "code": "S06b-R2",
      "created_at": "2026-11-28T07:26:18.801087+00:00",
      "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S06b-agent-direct-interface.md#5"
        ]
      },
      "meta": {
        "owner": "the operations lead (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "low",
      "programme_id": null,
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": "Track the licences service's availability with the estate's external probe; if an outage is measured during a delivery window, record the incident and re-open the cache TTL decision rather than weakening the fail-closed rule.",
      "review_at": "2026-12-11",
      "severity": "medium",
      "status": "open",
      "title": "Licence introspection is on the request path, so a licences outage refuses every machine call",
      "updated_at": "2026-11-28T07:26:18.801087+00:00",
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
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "Closed in an unauthorised tranche before the tranche review; accepted by the delegate as a retrospective exception under the standing delegation. The project is reworked in its batch and re-closed through the gates.",
      "code": "U3-EXC-S06b",
      "created_at": "2026-11-21T01:17:29.429007+00:00",
      "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": null,
      "review_at": null,
      "severity": "P0",
      "status": "accepted",
      "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)",
      "updated_at": "2026-11-21T01:17:29.429007+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "At the 2026-11-27 snapshot the project row records measured_status built while plan_status is planned, and the snapshot carries no charter, statement of architecture work, delivery contract, Phase A\u2013D set or Phase-G review for S06b. The capability and its suites exist; the lifecycle record did not. This pack supplies the missing set retrospectively and records the deferred live step explicitly.",
      "code": "S06b-I1",
      "created_at": "2026-11-28T07:26:18.948033+00:00",
      "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": "This pack is the repair: the lifecycle set, the receipt and the Phase-G review are authored and verified at the pinned snapshot; the deferred agent-app run stays S14's gate.",
      "review_at": null,
      "severity": "medium",
      "status": "open",
      "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review",
      "updated_at": "2026-11-28T07:26:18.948033+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The plan's property (b) \u2014 the agent's `mcp-import` yields `projects.create`-style keys rather than `tool.*` \u2014 is exercised on the conveyor side by the tool key and the parity suite, but the agent's importer runs in the agent repository. The S06b evidence file records this as S14 \u00a77's check, and the retrospective pack keeps it open rather than claiming the consumer.",
      "code": "S06b-I2",
      "created_at": "2026-11-28T07:26:19.093323+00:00",
      "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": "At S14, capture the importer's output for this conveyor's tool list and attach it to the cutover evidence; close this issue row against that capture.",
      "review_at": null,
      "severity": "low",
      "status": "open",
      "title": "The agent app's own importer is not executed in this repository",
      "updated_at": "2026-11-28T07:26:19.093323+00:00",
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
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The delivery contract judges acceptance against the record at the pinned rework snapshot; the content checker refuses any id that appears nowhere in it. Every judgement in this pack is read from that snapshot, so if it is incomplete or stale for S06b the pack's facts and any re-close move with it.",
      "code": "S06b-A1",
      "created_at": "2026-11-28T07:26:19.23849+00:00",
      "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b",
      "updated_at": "2026-11-28T07:26:19.23849+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The machine authenticator calls the licences service for a non-JWT bearer and trusts its verdict for at most 60 seconds. The assumption is that the deployed service answers the introspection contract the S02 delivery settled; if it does not, every agent call refuses and the failure is loud rather than silent.",
      "code": "S06b-A2",
      "created_at": "2026-11-28T07:26:19.392795+00:00",
      "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The licences service's introspection endpoint behaves as the S02 contract declares",
      "updated_at": "2026-11-28T07:26:19.392795+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "S06d made the MCP channel's tool list a tenant policy whose compiled default is MCP_DEFAULT_ACTIONS, and the machine floor still refuses eval sign-off, held-out reads and administrative actions. The assumption is that no stored grant has widened the list beyond the floor; the suite asserts the agent's view and the floor's refusals.",
      "code": "S06b-A3",
      "created_at": "2026-11-28T07:26:19.579155+00:00",
      "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS",
      "updated_at": "2026-11-28T07:26:19.579155+00:00",
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
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The project row records depends_on needs S04, needs S05 and needs S06. S04 supplies the action registry and the `/mcp` transport, S05 the project and card actions, and S06 the approval state machine, the events and the SSE the agent reads instead of building a second surface. All three are green at the same snapshot.",
      "code": "S06b-D1",
      "created_at": "2026-11-28T07:26:19.726057+00:00",
      "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "met",
      "title": "S06b depends on S04, S05 and S06",
      "updated_at": "2026-11-28T07:26:19.726057+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "S10's copilot is a projection of the same registry S06b exposes over `/mcp`, and S14's cutover runs the agent app's task against the deployed conveyor. Neither can be evidenced before the agent principal and the approval-request action exist, so both dependencies are carried as blocks rather than related work.",
      "code": "S06b-D2",
      "created_at": "2026-11-28T07:26:19.915827+00:00",
      "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S06b blocks S10's copilot surface and S14's deployed task",
      "updated_at": "2026-11-28T07:26:19.915827+00:00",
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
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "D19 makes the conveyor's own `/mcp` the agent interface, so the estate carries one wire, one principal model and one action registry instead of an HMAC channel with its own grammar and shared secret. The benefit is declared in the charter and carried by the parity suite and the retired-S06a decision; it is a structural reduction, not a measured saving.",
      "code": "S06b-B1",
      "created_at": "2026-11-28T07:26:20.059721+00:00",
      "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "One agent interface replaces a second signed channel",
      "updated_at": "2026-11-28T07:26:20.059721+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The three actor_kind sites read the principal, so an agent's approval decision records decided_by_kind='agent' with via naming the licence principal. The benefit is an audit property: the decision can be traced to a machine identity without reading the transport logs, and the mcp_agent suite asserts the field.",
      "code": "S06b-B2",
      "created_at": "2026-11-28T07:26:20.202128+00:00",
      "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Machine decisions become attributable",
      "updated_at": "2026-11-28T07:26:20.202128+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "approvals.request uses insert_approval_tx and accepts a NULL run_id, so an agent can raise a decision while no run is open and the approval stays decidable. The benefit is that the escalation path exists outside the run lifecycle, which the plan's property (f) records and the suite proves.",
      "code": "S06b-B3",
      "created_at": "2026-11-28T07:26:20.34688+00:00",
      "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Escalation no longer needs a running run",
      "updated_at": "2026-11-28T07:26:20.34688+00:00",
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
      "baseline_ref": "01a2210c-3f5e-7916-ad59-706b10c177ef",
      "body": "The retrospective MSP/TOGAF/SAFe rework changed this project's record: its artefacts were re-versioned with documents and links, its register was written, and its schedule and cost baselines were taken. This change records that rework against the frozen scope baseline.",
      "code": "CH-S06b-871458",
      "created_at": "2026-11-28T07:26:33.527213+00:00",
      "id": "01a2210c-723d-7d93-a47b-c58477871458",
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
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "The retrospective rework of S06b",
      "updated_at": "2026-11-28T07:26:33.915507+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```

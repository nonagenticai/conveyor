# Project Baseline — Processes, steps and SOPs — executable documentation

## Scope

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-baseline",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:42.614858+00:00",
      "id": "01a22110-3f41-731b-9f29-5e8f5908f0d6",
      "kind": "scope",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:42.614858+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

```json
{
  "category": "project-baseline",
  "count": 3,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:42.614858+00:00",
      "id": "01a22110-3f41-731b-9f29-5e8f5908f0d6",
      "kind": "scope",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:42.614858+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:42.772078+00:00",
      "id": "01a22110-3fdc-745d-9378-a1f12536c8e6",
      "kind": "schedule",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:42.772078+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:43.078808+00:00",
      "id": "01a22110-410e-740c-b2e0-6cd9cc59b111",
      "kind": "cost",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:43.078808+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

## Schedule

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-baseline",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:42.772078+00:00",
      "id": "01a22110-3fdc-745d-9378-a1f12536c8e6",
      "kind": "schedule",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:42.772078+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

```json
{
  "category": "project-baseline",
  "count": 3,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:42.614858+00:00",
      "id": "01a22110-3f41-731b-9f29-5e8f5908f0d6",
      "kind": "scope",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:42.614858+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:42.772078+00:00",
      "id": "01a22110-3fdc-745d-9378-a1f12536c8e6",
      "kind": "schedule",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:42.772078+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:43.078808+00:00",
      "id": "01a22110-410e-740c-b2e0-6cd9cc59b111",
      "kind": "cost",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:43.078808+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

## Cost

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-baseline",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:43.078808+00:00",
      "id": "01a22110-410e-740c-b2e0-6cd9cc59b111",
      "kind": "cost",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:43.078808+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

```json
{
  "category": "project-baseline",
  "count": 3,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:42.614858+00:00",
      "id": "01a22110-3f41-731b-9f29-5e8f5908f0d6",
      "kind": "scope",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:42.614858+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:42.772078+00:00",
      "id": "01a22110-3fdc-745d-9378-a1f12536c8e6",
      "kind": "schedule",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:42.772078+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:43.078808+00:00",
      "id": "01a22110-410e-740c-b2e0-6cd9cc59b111",
      "kind": "cost",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:43.078808+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

## Approval

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-baseline",
  "count": 3,
  "rows": [
    {
      "card_id": null,
      "created_at": "2026-11-28T07:30:42.614858+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a22110-3f43-7a3a-9b6a-fa72f99ca314",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a22110-3f41-731b-9f29-5e8f5908f0d6",
        "description": "Approve the scope baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:30:55.097362+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T07:30:42.772078+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a22110-3fde-7985-9a31-091002c88144",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a22110-3fdc-745d-9378-a1f12536c8e6",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:30:42.920635+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T07:30:43.078808+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a22110-4110-7b50-b057-a21db27502b6",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a22110-410e-740c-b2e0-6cd9cc59b111",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:30:43.26296+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

```json
{
  "category": "project-baseline",
  "count": 3,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:42.614858+00:00",
      "id": "01a22110-3f41-731b-9f29-5e8f5908f0d6",
      "kind": "scope",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:42.614858+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:42.772078+00:00",
      "id": "01a22110-3fdc-745d-9378-a1f12536c8e6",
      "kind": "schedule",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:42.772078+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:30:43.078808+00:00",
      "id": "01a22110-410e-740c-b2e0-6cd9cc59b111",
      "kind": "cost",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
        "register": [
          {
            "code": "S11-B4",
            "id": "01a22110-3ea6-76f3-921f-a2de4d14e743",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A tenant authors its own process without a deploy"
          },
          {
            "code": "S11-B3",
            "id": "01a22110-3e17-7c8e-8f59-2d323c7a5455",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A step is judged against a published SOP, not prose"
          },
          {
            "code": "S11-B2",
            "id": "01a22110-3d8a-743b-9028-eb9934e06816",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every attempt is an immutable, inspectable row"
          },
          {
            "code": "S11-B1",
            "id": "01a22110-3cf9-7658-a985-9c3e7472add6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 card-type by list flows become vocabulary data plus a few processes"
          },
          {
            "code": "S11-D3",
            "id": "01a22110-3c68-73e1-abd5-98f3e36c6428",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S15 fills the executor-scoping arm and S16 changes the built publish action"
          },
          {
            "code": "S11-D2",
            "id": "01a22110-3bdb-7198-a7ca-19d09777e857",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S11 blocks S12's read models and S13's sub-process engine"
          },
          {
            "code": "S11-D1",
            "id": "01a22110-3b4e-7dd4-abfb-66a87c482885",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S11 depends on S03, S04, S06 and S07"
          },
          {
            "code": "S11-A4",
            "id": "01a22110-3abf-7283-9ca3-ff135d816650",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Publish is the only path that creates an executable version"
          },
          {
            "code": "S11-A3",
            "id": "01a22110-3a2e-7157-a617-06e8947f525c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "A run step is only ever written by the executor"
          },
          {
            "code": "S11-A2",
            "id": "01a22110-39a0-736e-8cce-7bd666941127",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S06 approval state machine is the only gate machine for a process step"
          },
          {
            "code": "S11-A1",
            "id": "01a22110-3911-7d65-8d79-cd5b88269c8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S11"
          },
          {
            "code": "S11-I3",
            "id": "01a22110-3881-7d27-9b58-11fda9beb116",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The pre-rework receipt carried a P0 unauthorised-tranche package from the pre-authorisation snapshot"
          },
          {
            "code": "S11-I2",
            "id": "01a22110-37f7-759e-8a42-4da978f8648e",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Plan and tree disagree on two names a reader would run"
          },
          {
            "code": "S11-I1",
            "id": "01a22110-3768-7793-9c2f-191d2fd88a95",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The project was closed with no lifecycle set and its receipt is red"
          },
          {
            "code": "S11-R3",
            "id": "01a22110-36d8-7bf2-affb-6d5c2fabc200",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The executor-scoping seam is unresolved until S15"
          },
          {
            "code": "S11-R2",
            "id": "01a22110-3648-7562-99ca-bcd43f99f380",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The procedure seed is zero by decision, so no default process or SOP exists"
          },
          {
            "code": "S11-R1",
            "id": "01a22110-35ba-7c8d-ae6b-b4d5efff3f21",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan's \u00a77 two-process gate is the live acceptance and is not executed by this pack"
          },
          {
            "code": "U3-EXC-S11",
            "id": "01a1fbae-4a7e-71d1-991e-7828e73163cb",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S11)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a22110-3f3c-7adb-8ea6-c6418a362def",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 7,
            "status": "open",
            "time_criticality": 7,
            "title": "Executable documentation \u2014 a published process of steps, a versioned SOP per step, and an immutable run trace"
          }
        ]
      },
      "taken_at": "2026-11-28T07:30:43.078808+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

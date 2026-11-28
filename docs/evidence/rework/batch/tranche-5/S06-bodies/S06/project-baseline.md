# Project Baseline — Control plane — claims, runs, approvals, andon, comms and notifications

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
      "created_at": "2026-11-26T23:38:44.189998+00:00",
      "id": "01a21a39-c867-720b-b440-0b8a9b04d907",
      "kind": "scope",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
        "register": [
          {
            "code": "S06-B3",
            "id": "01a21a39-c7dd-72a1-b379-e957945b595b",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The close decision is decidable from the acceptance instrument's property set"
          },
          {
            "code": "S06-B2",
            "id": "01a21a39-c75a-75a3-8dfa-be8d744db31c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Eleven fr timers become one idempotent job runner and three timers are deleted"
          },
          {
            "code": "S06-B1",
            "id": "01a21a39-c6d2-732e-a2e2-aefde6a45a70",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One claim statement and a monotonic fence replace competing claim paths"
          },
          {
            "code": "S06-D2",
            "id": "01a21a39-c649-7cff-ab85-8f75ea1f39b4",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06 blocks S06a, S07, S10 and S12; the signed channel and the UI are not built here"
          },
          {
            "code": "S06-D1",
            "id": "01a21a39-c5c8-752c-af9a-773719cfa590",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06 is blocked by S03, S04 and S05"
          },
          {
            "code": "S06-A3",
            "id": "01a21a39-c546-79f6-9853-2fadf7cc1bf1",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Telegram is the first channel and the signed mcp transport belongs to S06a"
          },
          {
            "code": "S06-A2",
            "id": "01a21a39-c4c6-7b67-8028-c76f9192fd74",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S03, S04, S05 and S02 primitives exist as the plan declares them"
          },
          {
            "code": "S06-A1",
            "id": "01a21a39-c448-7bcc-ad5e-d020cba27b31",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06"
          },
          {
            "code": "S06-I2",
            "id": "01a21a39-c3c8-7a99-a93e-c908bf73de75",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Issue #604 closed with the multicast approval test timing out at 10 s under full-suite load"
          },
          {
            "code": "S06-I1",
            "id": "01a21a39-c343-7e12-b433-390fef09a386",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The record claims built but carries no Phase-G compliance review or acceptance run"
          },
          {
            "code": "S06-R2",
            "id": "01a21a39-c2c4-7bbe-ad04-bdbd456d30fe",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Issue #4 leaves a shared decision boundary and the Telegram sender identity unresolved"
          },
          {
            "code": "S06-R1",
            "id": "01a21a39-c245-77f6-985e-a01e62ed0051",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The seven gate properties have no recorded run, so the built claim is unproven"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a21a39-c863-789a-9322-30bb467b8693",
            "job_size": 6,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The control plane \u2014 claims, runs, approvals, andon, comms and notifications"
          }
        ]
      },
      "taken_at": "2026-11-26T23:38:44.189998+00:00",
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
      "created_at": "2026-11-26T23:38:44.326349+00:00",
      "id": "01a21a39-c8ed-7455-b982-a879a290814f",
      "kind": "schedule",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
        "register": [
          {
            "code": "S06-B3",
            "id": "01a21a39-c7dd-72a1-b379-e957945b595b",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The close decision is decidable from the acceptance instrument's property set"
          },
          {
            "code": "S06-B2",
            "id": "01a21a39-c75a-75a3-8dfa-be8d744db31c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Eleven fr timers become one idempotent job runner and three timers are deleted"
          },
          {
            "code": "S06-B1",
            "id": "01a21a39-c6d2-732e-a2e2-aefde6a45a70",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One claim statement and a monotonic fence replace competing claim paths"
          },
          {
            "code": "S06-D2",
            "id": "01a21a39-c649-7cff-ab85-8f75ea1f39b4",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06 blocks S06a, S07, S10 and S12; the signed channel and the UI are not built here"
          },
          {
            "code": "S06-D1",
            "id": "01a21a39-c5c8-752c-af9a-773719cfa590",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06 is blocked by S03, S04 and S05"
          },
          {
            "code": "S06-A3",
            "id": "01a21a39-c546-79f6-9853-2fadf7cc1bf1",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Telegram is the first channel and the signed mcp transport belongs to S06a"
          },
          {
            "code": "S06-A2",
            "id": "01a21a39-c4c6-7b67-8028-c76f9192fd74",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S03, S04, S05 and S02 primitives exist as the plan declares them"
          },
          {
            "code": "S06-A1",
            "id": "01a21a39-c448-7bcc-ad5e-d020cba27b31",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06"
          },
          {
            "code": "S06-I2",
            "id": "01a21a39-c3c8-7a99-a93e-c908bf73de75",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Issue #604 closed with the multicast approval test timing out at 10 s under full-suite load"
          },
          {
            "code": "S06-I1",
            "id": "01a21a39-c343-7e12-b433-390fef09a386",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The record claims built but carries no Phase-G compliance review or acceptance run"
          },
          {
            "code": "S06-R2",
            "id": "01a21a39-c2c4-7bbe-ad04-bdbd456d30fe",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Issue #4 leaves a shared decision boundary and the Telegram sender identity unresolved"
          },
          {
            "code": "S06-R1",
            "id": "01a21a39-c245-77f6-985e-a01e62ed0051",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The seven gate properties have no recorded run, so the built claim is unproven"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a21a39-c863-789a-9322-30bb467b8693",
            "job_size": 6,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The control plane \u2014 claims, runs, approvals, andon, comms and notifications"
          }
        ]
      },
      "taken_at": "2026-11-26T23:38:44.326349+00:00",
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
      "created_at": "2026-11-26T23:38:44.598868+00:00",
      "id": "01a21a39-c9ff-7f2b-9d19-7d58660d9f96",
      "kind": "cost",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
        "register": [
          {
            "code": "S06-B3",
            "id": "01a21a39-c7dd-72a1-b379-e957945b595b",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The close decision is decidable from the acceptance instrument's property set"
          },
          {
            "code": "S06-B2",
            "id": "01a21a39-c75a-75a3-8dfa-be8d744db31c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Eleven fr timers become one idempotent job runner and three timers are deleted"
          },
          {
            "code": "S06-B1",
            "id": "01a21a39-c6d2-732e-a2e2-aefde6a45a70",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One claim statement and a monotonic fence replace competing claim paths"
          },
          {
            "code": "S06-D2",
            "id": "01a21a39-c649-7cff-ab85-8f75ea1f39b4",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06 blocks S06a, S07, S10 and S12; the signed channel and the UI are not built here"
          },
          {
            "code": "S06-D1",
            "id": "01a21a39-c5c8-752c-af9a-773719cfa590",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06 is blocked by S03, S04 and S05"
          },
          {
            "code": "S06-A3",
            "id": "01a21a39-c546-79f6-9853-2fadf7cc1bf1",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Telegram is the first channel and the signed mcp transport belongs to S06a"
          },
          {
            "code": "S06-A2",
            "id": "01a21a39-c4c6-7b67-8028-c76f9192fd74",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The S03, S04, S05 and S02 primitives exist as the plan declares them"
          },
          {
            "code": "S06-A1",
            "id": "01a21a39-c448-7bcc-ad5e-d020cba27b31",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06"
          },
          {
            "code": "S06-I2",
            "id": "01a21a39-c3c8-7a99-a93e-c908bf73de75",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Issue #604 closed with the multicast approval test timing out at 10 s under full-suite load"
          },
          {
            "code": "S06-I1",
            "id": "01a21a39-c343-7e12-b433-390fef09a386",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The record claims built but carries no Phase-G compliance review or acceptance run"
          },
          {
            "code": "S06-R2",
            "id": "01a21a39-c2c4-7bbe-ad04-bdbd456d30fe",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Issue #4 leaves a shared decision boundary and the Telegram sender identity unresolved"
          },
          {
            "code": "S06-R1",
            "id": "01a21a39-c245-77f6-985e-a01e62ed0051",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The seven gate properties have no recorded run, so the built claim is unproven"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a21a39-c863-789a-9322-30bb467b8693",
            "job_size": 6,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The control plane \u2014 claims, runs, approvals, andon, comms and notifications"
          }
        ]
      },
      "taken_at": "2026-11-26T23:38:44.598868+00:00",
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
      "created_at": "2026-11-26T23:38:44.189998+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a21a39-c869-7273-a16b-ea0d8926856c",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a21a39-c867-720b-b440-0b8a9b04d907",
        "description": "Approve the scope baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T23:38:52.564884+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T23:38:44.326349+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a21a39-c8ef-74f3-bc9e-f754dc4bc9ec",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a21a39-c8ed-7455-b982-a879a290814f",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T23:38:44.458251+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T23:38:44.598868+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a21a39-ca02-7658-b2ea-2aa9e1951246",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a21a39-c9ff-7f2b-9d19-7d58660d9f96",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T23:38:44.733048+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

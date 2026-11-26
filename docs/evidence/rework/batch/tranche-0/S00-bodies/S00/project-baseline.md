# Project Baseline — Usage inventory → parity/parity.yaml

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
      "created_at": "2026-11-21T17:48:43.761438+00:00",
      "id": "01a1ff39-8d6e-7270-b2ba-9d235eb28b83",
      "kind": "scope",
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
        "register": [],
        "wbs": [
          {
            "business_value": 5,
            "id": "01a1ff39-8c90-73f2-b53c-6f51d8a8ebbb",
            "job_size": 12,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 3,
            "status": "open",
            "time_criticality": 5,
            "title": "The usage inventory and its evidence record"
          }
        ]
      },
      "taken_at": "2026-11-21T17:48:43.761438+00:00",
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
      "created_at": "2026-11-26T09:41:09.494877+00:00",
      "id": "01a2173a-f4fd-7849-9a78-7d06d0d4d38b",
      "kind": "schedule",
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
        "register": [
          {
            "code": "",
            "id": "01a2173a-f46f-7a4e-a0ae-f02711dae796",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every planned feature enters the rebuild with a stable id and a named acceptance"
          },
          {
            "code": "",
            "id": "01a2173a-f3ea-7276-a449-e61e97db2d40",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Zero-usage surface is dropped rather than ported"
          },
          {
            "code": "",
            "id": "01a2173a-f365-765f-9e04-0cf4ce88c891",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "The measurement depends on read-only access to master Postgres and Loki"
          },
          {
            "code": "",
            "id": "01a2173a-f2df-79bd-8cb2-fdcc84e4e795",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "Every later slice depends on parity_ids, and S14 gates on 100 % of used"
          },
          {
            "code": "",
            "id": "01a2173a-f25a-73d3-966f-de2b64a2dc2b",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The two adjacent 24-hour windows represent ordinary traffic"
          },
          {
            "code": "",
            "id": "01a2173a-f1d3-74ae-b81d-47e4db0ac17c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "No active caller reaches a decision-dropped surface, so the sign-off carried no contradiction"
          },
          {
            "code": "",
            "id": "01a2173a-f14e-7f22-bd84-077baf35eb9f",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The execution census covers three days, so it cannot answer quarterly use"
          },
          {
            "code": "",
            "id": "01a2173a-f0ca-7c32-95f4-f6d31fb7a56a",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Planka's database cannot evidence usage: four boards and five test cards"
          },
          {
            "code": "",
            "id": "01a2173a-f027-7d13-96bb-32307194cd0c",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "A surface dropped by decision may still receive live traffic after sign-off"
          },
          {
            "code": "",
            "id": "01a2173a-ef9f-7c91-9069-6bab957780b9",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "A used id that no later slice builds would break S14's 100 % gate"
          },
          {
            "code": "",
            "id": "01a2173a-ef1a-7668-8696-42364dccb4d5",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The dated samples cannot be re-measured, so a later reader cannot reproduce the counts"
          },
          {
            "code": "U5-CHG-S00",
            "id": "01a20200-7fca-7da3-8f84-be7af377c451",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "accepted",
            "title": "Retrospective MSP/TOGAF/SAFe rework of S00 under the #526 scope change"
          }
        ],
        "wbs": [
          {
            "business_value": 5,
            "id": "01a1ff39-8c90-73f2-b53c-6f51d8a8ebbb",
            "job_size": 12,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 3,
            "status": "accepted",
            "time_criticality": 5,
            "title": "The usage inventory and its evidence record"
          }
        ]
      },
      "taken_at": "2026-11-26T09:41:09.494877+00:00",
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
      "created_at": "2026-11-26T09:41:09.796946+00:00",
      "id": "01a2173a-f632-7217-bd82-d77eca5e7366",
      "kind": "cost",
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
        "register": [
          {
            "code": "",
            "id": "01a2173a-f46f-7a4e-a0ae-f02711dae796",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every planned feature enters the rebuild with a stable id and a named acceptance"
          },
          {
            "code": "",
            "id": "01a2173a-f3ea-7276-a449-e61e97db2d40",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Zero-usage surface is dropped rather than ported"
          },
          {
            "code": "",
            "id": "01a2173a-f365-765f-9e04-0cf4ce88c891",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "The measurement depends on read-only access to master Postgres and Loki"
          },
          {
            "code": "",
            "id": "01a2173a-f2df-79bd-8cb2-fdcc84e4e795",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "Every later slice depends on parity_ids, and S14 gates on 100 % of used"
          },
          {
            "code": "",
            "id": "01a2173a-f25a-73d3-966f-de2b64a2dc2b",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The two adjacent 24-hour windows represent ordinary traffic"
          },
          {
            "code": "",
            "id": "01a2173a-f1d3-74ae-b81d-47e4db0ac17c",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "No active caller reaches a decision-dropped surface, so the sign-off carried no contradiction"
          },
          {
            "code": "",
            "id": "01a2173a-f14e-7f22-bd84-077baf35eb9f",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The execution census covers three days, so it cannot answer quarterly use"
          },
          {
            "code": "",
            "id": "01a2173a-f0ca-7c32-95f4-f6d31fb7a56a",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Planka's database cannot evidence usage: four boards and five test cards"
          },
          {
            "code": "",
            "id": "01a2173a-f027-7d13-96bb-32307194cd0c",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "A surface dropped by decision may still receive live traffic after sign-off"
          },
          {
            "code": "",
            "id": "01a2173a-ef9f-7c91-9069-6bab957780b9",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "A used id that no later slice builds would break S14's 100 % gate"
          },
          {
            "code": "",
            "id": "01a2173a-ef1a-7668-8696-42364dccb4d5",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The dated samples cannot be re-measured, so a later reader cannot reproduce the counts"
          },
          {
            "code": "U5-CHG-S00",
            "id": "01a20200-7fca-7da3-8f84-be7af377c451",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "accepted",
            "title": "Retrospective MSP/TOGAF/SAFe rework of S00 under the #526 scope change"
          }
        ],
        "wbs": [
          {
            "business_value": 5,
            "id": "01a1ff39-8c90-73f2-b53c-6f51d8a8ebbb",
            "job_size": 12,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 3,
            "status": "accepted",
            "time_criticality": 5,
            "title": "The usage inventory and its evidence record"
          }
        ]
      },
      "taken_at": "2026-11-26T09:41:09.796946+00:00",
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
      "created_at": "2026-11-21T17:48:43.761438+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a1ff39-8dc8-78db-bb58-2d18f93d0b35",
      "kind": "pm",
      "meta": {
        "baseline_id": "01a1ff39-8d6e-7270-b2ba-9d235eb28b83",
        "description": "Approve the scope baseline"
      },
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-21T17:48:48.880013+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T09:41:09.494877+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a2173a-f4ff-787f-877a-892fede0305d",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a2173a-f4fd-7849-9a78-7d06d0d4d38b",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T09:41:09.628978+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T09:41:09.796946+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a2173a-f634-7cc4-944b-ad6137378923",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a2173a-f632-7217-bd82-d77eca5e7366",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T09:41:09.942652+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

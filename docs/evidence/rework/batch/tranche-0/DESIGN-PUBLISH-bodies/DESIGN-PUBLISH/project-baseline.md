# Project Baseline — Publish the SPEC_V2 studio design pack, live

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
      "created_at": "2026-11-21T17:53:56.128695+00:00",
      "id": "01a1ff3e-51c4-75b1-9db1-a2f729dd10b8",
      "kind": "scope",
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
        "register": [],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a1ff3e-50d7-74e4-bf2c-5e866b75cc86",
            "job_size": 3,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 5,
            "title": "Publish the studio design pack and its retrospective closure record"
          }
        ]
      },
      "taken_at": "2026-11-21T17:53:56.128695+00:00",
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

_Explicit absence — the project schedule baseline: a project_baseline row of kind schedule._

## Cost

The record the section derives from, or an explicit absence.

_Explicit absence — the project cost baseline: a project_baseline row of kind cost._

## Approval

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-baseline",
  "count": 1,
  "rows": [
    {
      "card_id": null,
      "created_at": "2026-11-21T17:53:56.128695+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a1ff3e-5250-70a7-bdd4-c16e15c3a99a",
      "kind": "pm",
      "meta": {
        "baseline_id": "01a1ff3e-51c4-75b1-9db1-a2f729dd10b8",
        "description": "Approve the scope baseline"
      },
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-21T17:54:02.472603+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

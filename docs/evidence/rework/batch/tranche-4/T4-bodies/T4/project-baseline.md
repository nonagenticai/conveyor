# Project Baseline — Kanban coach

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
      "created_at": "2026-11-22T09:19:40.844998+00:00",
      "id": "01a2028d-e17b-793d-9ea2-fe4f7f8e2b72",
      "kind": "scope",
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
        "register": [
          {
            "code": "U3-EXC-T4",
            "id": "01a1fbad-ac1e-709a-8410-9e8553098661",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (T4)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a2028d-dec8-7b26-9bde-e4cd578eaa62",
            "job_size": 5,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 7,
            "title": "The coach loop \u2014 one card, one triage, one remedy, per-signature escalation and the depth cap"
          }
        ]
      },
      "taken_at": "2026-11-22T09:19:40.844998+00:00",
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
      "created_at": "2026-11-22T09:19:40.844998+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a2028d-e2ad-71fb-b3a9-656be39ce8ba",
      "kind": "pm",
      "meta": {
        "baseline_id": "01a2028d-e17b-793d-9ea2-fe4f7f8e2b72",
        "description": "Approve the scope baseline"
      },
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-22T09:45:22.454329+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

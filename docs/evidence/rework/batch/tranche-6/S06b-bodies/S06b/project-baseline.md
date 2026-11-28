# Project Baseline — Agent direct interface — the conveyor's own MCP surface

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
      "created_at": "2026-11-28T07:26:20.500292+00:00",
      "id": "01a2210c-3f5e-7916-ad59-706b10c177ef",
      "kind": "scope",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.500292+00:00",
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
      "created_at": "2026-11-28T07:26:20.500292+00:00",
      "id": "01a2210c-3f5e-7916-ad59-706b10c177ef",
      "kind": "scope",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.500292+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:26:20.655826+00:00",
      "id": "01a2210c-3ff7-7c53-a63c-612e2956fa73",
      "kind": "schedule",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.655826+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:26:20.967715+00:00",
      "id": "01a2210c-4136-760d-8179-a1a65ba53215",
      "kind": "cost",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.967715+00:00",
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
      "created_at": "2026-11-28T07:26:20.655826+00:00",
      "id": "01a2210c-3ff7-7c53-a63c-612e2956fa73",
      "kind": "schedule",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.655826+00:00",
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
      "created_at": "2026-11-28T07:26:20.500292+00:00",
      "id": "01a2210c-3f5e-7916-ad59-706b10c177ef",
      "kind": "scope",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.500292+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:26:20.655826+00:00",
      "id": "01a2210c-3ff7-7c53-a63c-612e2956fa73",
      "kind": "schedule",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.655826+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:26:20.967715+00:00",
      "id": "01a2210c-4136-760d-8179-a1a65ba53215",
      "kind": "cost",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.967715+00:00",
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
      "created_at": "2026-11-28T07:26:20.967715+00:00",
      "id": "01a2210c-4136-760d-8179-a1a65ba53215",
      "kind": "cost",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.967715+00:00",
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
      "created_at": "2026-11-28T07:26:20.500292+00:00",
      "id": "01a2210c-3f5e-7916-ad59-706b10c177ef",
      "kind": "scope",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.500292+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:26:20.655826+00:00",
      "id": "01a2210c-3ff7-7c53-a63c-612e2956fa73",
      "kind": "schedule",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.655826+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:26:20.967715+00:00",
      "id": "01a2210c-4136-760d-8179-a1a65ba53215",
      "kind": "cost",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.967715+00:00",
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
      "created_at": "2026-11-28T07:26:20.500292+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a2210c-3f60-7af0-b925-87429f1d4f6e",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a2210c-3f5e-7916-ad59-706b10c177ef",
        "description": "Approve the scope baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:26:33.187397+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T07:26:20.655826+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a2210c-3ffa-7299-bcab-3bdc70925c3d",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a2210c-3ff7-7c53-a63c-612e2956fa73",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:26:20.806344+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T07:26:20.967715+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a2210c-4138-7d7b-868e-a6eaa40df128",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a2210c-4136-760d-8179-a1a65ba53215",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:26:21.126925+00:00",
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
      "created_at": "2026-11-28T07:26:20.500292+00:00",
      "id": "01a2210c-3f5e-7916-ad59-706b10c177ef",
      "kind": "scope",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.500292+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:26:20.655826+00:00",
      "id": "01a2210c-3ff7-7c53-a63c-612e2956fa73",
      "kind": "schedule",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.655826+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:26:20.967715+00:00",
      "id": "01a2210c-4136-760d-8179-a1a65ba53215",
      "kind": "cost",
      "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2a-7ea9-b809-929452f221fd",
        "register": [
          {
            "code": "S06b-B3",
            "id": "01a2210c-3ebe-74e0-bfa8-e40dc93bc355",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Escalation no longer needs a running run"
          },
          {
            "code": "S06b-B2",
            "id": "01a2210c-3e2d-76b8-9ccd-5081c751e6ce",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Machine decisions become attributable"
          },
          {
            "code": "S06b-B1",
            "id": "01a2210c-3d9e-79ea-81e8-2d0e0b9aafae",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One agent interface replaces a second signed channel"
          },
          {
            "code": "S06b-D2",
            "id": "01a2210c-3d0e-7cfd-8ac0-40ff281ed56d",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b blocks S10's copilot surface and S14's deployed task"
          },
          {
            "code": "S06b-D1",
            "id": "01a2210c-3c50-7c58-8d40-16dbbf8ae204",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S06b depends on S04, S05 and S06"
          },
          {
            "code": "S06b-A3",
            "id": "01a2210c-3bbd-7e33-95c0-f90feacc3e91",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The agent principal's tool list is the compiled MCP_DEFAULT_ACTIONS"
          },
          {
            "code": "S06b-A2",
            "id": "01a2210c-3b07-7392-ab3f-3fd92fbd6785",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The licences service's introspection endpoint behaves as the S02 contract declares"
          },
          {
            "code": "S06b-A1",
            "id": "01a2210c-3a6a-7a1c-bd8e-b2c229c8e625",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S06b"
          },
          {
            "code": "S06b-I2",
            "id": "01a2210c-39d8-71c3-943a-e501fdf25e32",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent app's own importer is not executed in this repository"
          },
          {
            "code": "S06b-I1",
            "id": "01a2210c-3947-75b1-b3ea-3837624599cc",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record claimed built but carried no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06b-R2",
            "id": "01a2210c-38b4-74c6-baa3-ded7ed00904f",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Licence introspection is on the request path, so a licences outage refuses every machine call"
          },
          {
            "code": "S06b-R1",
            "id": "01a2210c-3820-767a-8cdb-cf85ca69a217",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The deployed agent-app task is S14's gate, so the consumer path is proven locally only"
          },
          {
            "code": "U3-EXC-S06b",
            "id": "01a1fbae-0aa8-7b68-b46e-fbc98b445bc2",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06b)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a2210c-3f5a-7d08-a6d6-802ee41d66fd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 13,
            "title": "The conveyor's own /mcp as the agent interface \u2014 one wire, one principal, attributable decisions"
          }
        ]
      },
      "taken_at": "2026-11-28T07:26:20.967715+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

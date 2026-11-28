# Project Baseline — Requirements, catalogues, revisions, quality scoring, search

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
      "created_at": "2026-11-26T21:53:22.486358+00:00",
      "id": "01a219d9-5241-7d98-8c54-550959dc4138",
      "kind": "scope",
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
        "register": [
          {
            "code": "S07-B3",
            "id": "01a219d9-51a2-7a77-ba70-e09fb54bde15",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "planned",
            "title": "A measured relevance baseline over 40 human-labelled queries"
          },
          {
            "code": "S07-B2",
            "id": "01a219d9-5110-7b35-aac4-b56d80ef5d59",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "claimed",
            "title": "Every mutation leaves an immutable revision with a Rust field-level compare"
          },
          {
            "code": "S07-B1",
            "id": "01a219d9-507d-75f8-9bf9-73f4dda099e7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "claimed",
            "title": "One governed_asset lifecycle replaces RM's 57 domain tables"
          },
          {
            "code": "S07-D3",
            "id": "01a219d9-4fe6-7681-a1e9-5d7a9bbfc6fb",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "PostgreSQL 18 carries pg_trgm and the LEAKPROOF operators"
          },
          {
            "code": "S07-D2",
            "id": "01a219d9-4f53-7191-b52f-8becbdc4977b",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "S04 projects every S07 action through the registry"
          },
          {
            "code": "S07-D1",
            "id": "01a219d9-4ec1-72e1-a6de-b6538a702c09",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "S03 keeps governed_asset, revision, catalogue_entry and trace_link"
          },
          {
            "code": "S07-A2",
            "id": "01a219d9-4e09-747a-a808-68ae58bca75f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "S03 and S04 keep the surfaces S07 reuses"
          },
          {
            "code": "S07-A1",
            "id": "01a219d9-4d74-78e7-a237-ef8f79d94942",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-24 snapshot is the authoritative record for S07"
          },
          {
            "code": "S07-I2",
            "id": "01a219d9-4ce6-7807-b901-ad527ab2c609",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The plan \u00a710.3 open question still assumes outline and outline_node exist"
          },
          {
            "code": "S07-I1",
            "id": "01a219d9-4c58-755f-98a5-dd6fad64f344",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The snapshot carries no linked issue and no baseline row"
          },
          {
            "code": "S07-R3",
            "id": "01a219d9-4bc9-7feb-a6f2-082b7e349ca2",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "Both GIN indexes are dead if the trigram operators are not LEAKPROOF"
          },
          {
            "code": "S07-R2",
            "id": "01a219d9-4b3a-73e3-9f52-6ce5536c9de7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The \u00a75.6 relevance targets have no measured eval numbers"
          },
          {
            "code": "S07-R1",
            "id": "01a219d9-4aaa-7180-8433-4eafaf4a3f49",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The plan \u00a77 functional gate has no recorded run"
          },
          {
            "code": "U3-EXC-S07",
            "id": "01a1fbad-2d6e-7c68-9968-af101d0acb0c",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S07)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a219d9-523d-76df-8f8e-ca68a7fcc4f6",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 8,
            "title": "The requirements and search slice \u2014 one governed_asset lifecycle, one revision per mutation, the advisory quality rubric, FTS plus trigram search and its eval set, the catalogue seed and the project-request intake"
          }
        ]
      },
      "taken_at": "2026-11-26T21:53:22.486358+00:00",
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
      "created_at": "2026-11-26T21:53:22.661+00:00",
      "id": "01a219d9-530c-7706-ba33-28f4bfc62d8e",
      "kind": "schedule",
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
        "register": [
          {
            "code": "S07-B3",
            "id": "01a219d9-51a2-7a77-ba70-e09fb54bde15",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "planned",
            "title": "A measured relevance baseline over 40 human-labelled queries"
          },
          {
            "code": "S07-B2",
            "id": "01a219d9-5110-7b35-aac4-b56d80ef5d59",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "claimed",
            "title": "Every mutation leaves an immutable revision with a Rust field-level compare"
          },
          {
            "code": "S07-B1",
            "id": "01a219d9-507d-75f8-9bf9-73f4dda099e7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "claimed",
            "title": "One governed_asset lifecycle replaces RM's 57 domain tables"
          },
          {
            "code": "S07-D3",
            "id": "01a219d9-4fe6-7681-a1e9-5d7a9bbfc6fb",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "PostgreSQL 18 carries pg_trgm and the LEAKPROOF operators"
          },
          {
            "code": "S07-D2",
            "id": "01a219d9-4f53-7191-b52f-8becbdc4977b",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "S04 projects every S07 action through the registry"
          },
          {
            "code": "S07-D1",
            "id": "01a219d9-4ec1-72e1-a6de-b6538a702c09",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "S03 keeps governed_asset, revision, catalogue_entry and trace_link"
          },
          {
            "code": "S07-A2",
            "id": "01a219d9-4e09-747a-a808-68ae58bca75f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "S03 and S04 keep the surfaces S07 reuses"
          },
          {
            "code": "S07-A1",
            "id": "01a219d9-4d74-78e7-a237-ef8f79d94942",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-24 snapshot is the authoritative record for S07"
          },
          {
            "code": "S07-I2",
            "id": "01a219d9-4ce6-7807-b901-ad527ab2c609",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The plan \u00a710.3 open question still assumes outline and outline_node exist"
          },
          {
            "code": "S07-I1",
            "id": "01a219d9-4c58-755f-98a5-dd6fad64f344",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The snapshot carries no linked issue and no baseline row"
          },
          {
            "code": "S07-R3",
            "id": "01a219d9-4bc9-7feb-a6f2-082b7e349ca2",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "Both GIN indexes are dead if the trigram operators are not LEAKPROOF"
          },
          {
            "code": "S07-R2",
            "id": "01a219d9-4b3a-73e3-9f52-6ce5536c9de7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The \u00a75.6 relevance targets have no measured eval numbers"
          },
          {
            "code": "S07-R1",
            "id": "01a219d9-4aaa-7180-8433-4eafaf4a3f49",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The plan \u00a77 functional gate has no recorded run"
          },
          {
            "code": "U3-EXC-S07",
            "id": "01a1fbad-2d6e-7c68-9968-af101d0acb0c",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S07)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a219d9-523d-76df-8f8e-ca68a7fcc4f6",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 8,
            "title": "The requirements and search slice \u2014 one governed_asset lifecycle, one revision per mutation, the advisory quality rubric, FTS plus trigram search and its eval set, the catalogue seed and the project-request intake"
          }
        ]
      },
      "taken_at": "2026-11-26T21:53:22.661+00:00",
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
      "created_at": "2026-11-26T21:53:23.003305+00:00",
      "id": "01a219d9-5443-7080-9cf2-0a7e914564ba",
      "kind": "cost",
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
        "register": [
          {
            "code": "S07-B3",
            "id": "01a219d9-51a2-7a77-ba70-e09fb54bde15",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "planned",
            "title": "A measured relevance baseline over 40 human-labelled queries"
          },
          {
            "code": "S07-B2",
            "id": "01a219d9-5110-7b35-aac4-b56d80ef5d59",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "claimed",
            "title": "Every mutation leaves an immutable revision with a Rust field-level compare"
          },
          {
            "code": "S07-B1",
            "id": "01a219d9-507d-75f8-9bf9-73f4dda099e7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "claimed",
            "title": "One governed_asset lifecycle replaces RM's 57 domain tables"
          },
          {
            "code": "S07-D3",
            "id": "01a219d9-4fe6-7681-a1e9-5d7a9bbfc6fb",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "PostgreSQL 18 carries pg_trgm and the LEAKPROOF operators"
          },
          {
            "code": "S07-D2",
            "id": "01a219d9-4f53-7191-b52f-8becbdc4977b",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "S04 projects every S07 action through the registry"
          },
          {
            "code": "S07-D1",
            "id": "01a219d9-4ec1-72e1-a6de-b6538a702c09",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "S03 keeps governed_asset, revision, catalogue_entry and trace_link"
          },
          {
            "code": "S07-A2",
            "id": "01a219d9-4e09-747a-a808-68ae58bca75f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "S03 and S04 keep the surfaces S07 reuses"
          },
          {
            "code": "S07-A1",
            "id": "01a219d9-4d74-78e7-a237-ef8f79d94942",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-24 snapshot is the authoritative record for S07"
          },
          {
            "code": "S07-I2",
            "id": "01a219d9-4ce6-7807-b901-ad527ab2c609",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The plan \u00a710.3 open question still assumes outline and outline_node exist"
          },
          {
            "code": "S07-I1",
            "id": "01a219d9-4c58-755f-98a5-dd6fad64f344",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The snapshot carries no linked issue and no baseline row"
          },
          {
            "code": "S07-R3",
            "id": "01a219d9-4bc9-7feb-a6f2-082b7e349ca2",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "Both GIN indexes are dead if the trigram operators are not LEAKPROOF"
          },
          {
            "code": "S07-R2",
            "id": "01a219d9-4b3a-73e3-9f52-6ce5536c9de7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The \u00a75.6 relevance targets have no measured eval numbers"
          },
          {
            "code": "S07-R1",
            "id": "01a219d9-4aaa-7180-8433-4eafaf4a3f49",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The plan \u00a77 functional gate has no recorded run"
          },
          {
            "code": "U3-EXC-S07",
            "id": "01a1fbad-2d6e-7c68-9968-af101d0acb0c",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S07)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a219d9-523d-76df-8f8e-ca68a7fcc4f6",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 8,
            "title": "The requirements and search slice \u2014 one governed_asset lifecycle, one revision per mutation, the advisory quality rubric, FTS plus trigram search and its eval set, the catalogue seed and the project-request intake"
          }
        ]
      },
      "taken_at": "2026-11-26T21:53:23.003305+00:00",
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
      "created_at": "2026-11-26T21:53:22.486358+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a219d9-5245-707e-981e-103433ec240f",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a219d9-5241-7d98-8c54-550959dc4138",
        "description": "Approve the scope baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T21:53:31.199573+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T21:53:22.661+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a219d9-5310-7959-8bd3-32d561e55205",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a219d9-530c-7706-ba33-28f4bfc62d8e",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T21:53:22.845631+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T21:53:23.003305+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a219d9-5445-7c98-9b45-251cac05140f",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a219d9-5443-7080-9cf2-0a7e914564ba",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T21:53:23.154974+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

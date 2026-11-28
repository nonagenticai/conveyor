# Project Baseline — Estate toolkits

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
      "created_at": "2026-11-26T21:57:50.376905+00:00",
      "id": "01a219dd-68b3-7467-b150-49c5b261bfb9",
      "kind": "scope",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
        "register": [
          {
            "code": "T14-B3",
            "id": "01a219dd-6816-734b-b2fb-f68d0e57423c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "baselined",
            "title": "An unused toolkit has a recorded exit path through the zero-use report and the provenance and retirement gate"
          },
          {
            "code": "T14-B2",
            "id": "01a219dd-6785-7e7d-9796-71f25cf5194d",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "baselined",
            "title": "Publication requires a probe and one live call, so an unproven toolkit cannot be counted as published"
          },
          {
            "code": "T14-B1",
            "id": "01a219dd-66ef-7544-b165-f234cd5a34ad",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "baselined",
            "title": "Every estate tool is called through the broker and leaves one tool_call row, so no call escapes the governed path"
          },
          {
            "code": "T14-D2",
            "id": "01a219dd-665c-7aac-9f4e-83b616436b1b",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "P0 supplies the standing delegation and the accepted exception under which this retrospective runs"
          },
          {
            "code": "T14-D1",
            "id": "01a219dd-65af-755f-a766-75e18690dfba",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "T13 supplies the tool family, the broker, the effect classes and the credential profiles"
          },
          {
            "code": "T14-A2",
            "id": "01a219dd-6521-7496-9c02-a66f1eec4925",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The level consistent stands without a waiver because the record shows no refused gate"
          },
          {
            "code": "T14-A1",
            "id": "01a219dd-6493-7b54-af39-8e34bb1cf412",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-24 facts snapshot is the authoritative record for T14"
          },
          {
            "code": "T14-I3",
            "id": "01a219dd-6405-759d-a762-cf1081e4330c",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The plan fragment docs/SPEC_V2.md#12 is status draft and there is no standalone T14 slice plan"
          },
          {
            "code": "T14-I2",
            "id": "01a219dd-6373-71a9-aa63-1937b2d39a2c",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Issue 561 (research and Lean toolkits, first wave) remains open while the record claims the slice built"
          },
          {
            "code": "T14-I1",
            "id": "01a219dd-62e6-734c-9e4f-8936ed8890ef",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "No probe or live-call receipt exists for any published toolkit at the 2026-11-24 snapshot"
          },
          {
            "code": "T14-R2",
            "id": "01a219dd-6256-7a17-92c6-5947b65035ff",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "scientific_processor could be left scaled up or scaled without a receipt, consuming scarce host compute"
          },
          {
            "code": "T14-R1",
            "id": "01a219dd-61ca-7176-adf9-57ef0693e4b2",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The four gate clauses have no executed receipt, so the project's built status is unproven"
          },
          {
            "code": "U3-EXC-T14",
            "id": "01a1fbad-8c5f-7086-b569-28e8f0cf6fac",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (T14)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a219dd-68af-713f-9fef-79edd53e8d58",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 9,
            "title": "The estate-toolkit import wave \u2014 live services as governed toolkits, with probe, live-call, scale, zero-use and outage receipts"
          }
        ]
      },
      "taken_at": "2026-11-26T21:57:50.376905+00:00",
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
      "created_at": "2026-11-26T21:57:50.532242+00:00",
      "id": "01a219dd-694c-76b3-8e11-65a71309058c",
      "kind": "schedule",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
        "register": [
          {
            "code": "T14-B3",
            "id": "01a219dd-6816-734b-b2fb-f68d0e57423c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "baselined",
            "title": "An unused toolkit has a recorded exit path through the zero-use report and the provenance and retirement gate"
          },
          {
            "code": "T14-B2",
            "id": "01a219dd-6785-7e7d-9796-71f25cf5194d",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "baselined",
            "title": "Publication requires a probe and one live call, so an unproven toolkit cannot be counted as published"
          },
          {
            "code": "T14-B1",
            "id": "01a219dd-66ef-7544-b165-f234cd5a34ad",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "baselined",
            "title": "Every estate tool is called through the broker and leaves one tool_call row, so no call escapes the governed path"
          },
          {
            "code": "T14-D2",
            "id": "01a219dd-665c-7aac-9f4e-83b616436b1b",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "P0 supplies the standing delegation and the accepted exception under which this retrospective runs"
          },
          {
            "code": "T14-D1",
            "id": "01a219dd-65af-755f-a766-75e18690dfba",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "T13 supplies the tool family, the broker, the effect classes and the credential profiles"
          },
          {
            "code": "T14-A2",
            "id": "01a219dd-6521-7496-9c02-a66f1eec4925",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The level consistent stands without a waiver because the record shows no refused gate"
          },
          {
            "code": "T14-A1",
            "id": "01a219dd-6493-7b54-af39-8e34bb1cf412",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-24 facts snapshot is the authoritative record for T14"
          },
          {
            "code": "T14-I3",
            "id": "01a219dd-6405-759d-a762-cf1081e4330c",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The plan fragment docs/SPEC_V2.md#12 is status draft and there is no standalone T14 slice plan"
          },
          {
            "code": "T14-I2",
            "id": "01a219dd-6373-71a9-aa63-1937b2d39a2c",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Issue 561 (research and Lean toolkits, first wave) remains open while the record claims the slice built"
          },
          {
            "code": "T14-I1",
            "id": "01a219dd-62e6-734c-9e4f-8936ed8890ef",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "No probe or live-call receipt exists for any published toolkit at the 2026-11-24 snapshot"
          },
          {
            "code": "T14-R2",
            "id": "01a219dd-6256-7a17-92c6-5947b65035ff",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "scientific_processor could be left scaled up or scaled without a receipt, consuming scarce host compute"
          },
          {
            "code": "T14-R1",
            "id": "01a219dd-61ca-7176-adf9-57ef0693e4b2",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The four gate clauses have no executed receipt, so the project's built status is unproven"
          },
          {
            "code": "U3-EXC-T14",
            "id": "01a1fbad-8c5f-7086-b569-28e8f0cf6fac",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (T14)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a219dd-68af-713f-9fef-79edd53e8d58",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 9,
            "title": "The estate-toolkit import wave \u2014 live services as governed toolkits, with probe, live-call, scale, zero-use and outage receipts"
          }
        ]
      },
      "taken_at": "2026-11-26T21:57:50.532242+00:00",
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
      "created_at": "2026-11-26T21:57:50.838322+00:00",
      "id": "01a219dd-6a7e-7b5e-95e3-9dfb7a10a56e",
      "kind": "cost",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
        "register": [
          {
            "code": "T14-B3",
            "id": "01a219dd-6816-734b-b2fb-f68d0e57423c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "baselined",
            "title": "An unused toolkit has a recorded exit path through the zero-use report and the provenance and retirement gate"
          },
          {
            "code": "T14-B2",
            "id": "01a219dd-6785-7e7d-9796-71f25cf5194d",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "baselined",
            "title": "Publication requires a probe and one live call, so an unproven toolkit cannot be counted as published"
          },
          {
            "code": "T14-B1",
            "id": "01a219dd-66ef-7544-b165-f234cd5a34ad",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "baselined",
            "title": "Every estate tool is called through the broker and leaves one tool_call row, so no call escapes the governed path"
          },
          {
            "code": "T14-D2",
            "id": "01a219dd-665c-7aac-9f4e-83b616436b1b",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "P0 supplies the standing delegation and the accepted exception under which this retrospective runs"
          },
          {
            "code": "T14-D1",
            "id": "01a219dd-65af-755f-a766-75e18690dfba",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "T13 supplies the tool family, the broker, the effect classes and the credential profiles"
          },
          {
            "code": "T14-A2",
            "id": "01a219dd-6521-7496-9c02-a66f1eec4925",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The level consistent stands without a waiver because the record shows no refused gate"
          },
          {
            "code": "T14-A1",
            "id": "01a219dd-6493-7b54-af39-8e34bb1cf412",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-24 facts snapshot is the authoritative record for T14"
          },
          {
            "code": "T14-I3",
            "id": "01a219dd-6405-759d-a762-cf1081e4330c",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The plan fragment docs/SPEC_V2.md#12 is status draft and there is no standalone T14 slice plan"
          },
          {
            "code": "T14-I2",
            "id": "01a219dd-6373-71a9-aa63-1937b2d39a2c",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Issue 561 (research and Lean toolkits, first wave) remains open while the record claims the slice built"
          },
          {
            "code": "T14-I1",
            "id": "01a219dd-62e6-734c-9e4f-8936ed8890ef",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "No probe or live-call receipt exists for any published toolkit at the 2026-11-24 snapshot"
          },
          {
            "code": "T14-R2",
            "id": "01a219dd-6256-7a17-92c6-5947b65035ff",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "scientific_processor could be left scaled up or scaled without a receipt, consuming scarce host compute"
          },
          {
            "code": "T14-R1",
            "id": "01a219dd-61ca-7176-adf9-57ef0693e4b2",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The four gate clauses have no executed receipt, so the project's built status is unproven"
          },
          {
            "code": "U3-EXC-T14",
            "id": "01a1fbad-8c5f-7086-b569-28e8f0cf6fac",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (T14)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a219dd-68af-713f-9fef-79edd53e8d58",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 9,
            "title": "The estate-toolkit import wave \u2014 live services as governed toolkits, with probe, live-call, scale, zero-use and outage receipts"
          }
        ]
      },
      "taken_at": "2026-11-26T21:57:50.838322+00:00",
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
      "created_at": "2026-11-26T21:57:50.376905+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a219dd-68b5-7506-bd1d-58e341bfd05b",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a219dd-68b3-7467-b150-49c5b261bfb9",
        "description": "Approve the scope baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T21:57:58.938018+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T21:57:50.532242+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a219dd-694e-7d12-a710-8ae4643e482c",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a219dd-694c-76b3-8e11-65a71309058c",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T21:57:50.679424+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T21:57:50.838322+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a219dd-6a81-70ad-b859-4070ae2e8c48",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a219dd-6a7e-7b5e-95e3-9dfb7a10a56e",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T21:57:50.988618+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

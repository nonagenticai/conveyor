# Project Baseline — CI, runners and platform

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
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "id": "01a1fdd8-cff3-7cd9-9535-26fed3d5836b",
      "kind": "scope",
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
        "register": [],
        "wbs": [
          {
            "business_value": 3,
            "id": "01a1fdd8-cdf3-7108-aac5-0cec790e988d",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 2,
            "status": "open",
            "time_criticality": 3,
            "title": "CI and runners available"
          },
          {
            "business_value": 3,
            "id": "01a1fdd8-ce5c-70c8-87be-2dd12bf61cb0",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 2.0,
            "risk_reduction": 2,
            "status": "open",
            "time_criticality": 2,
            "title": "Registry and ingress available"
          },
          {
            "business_value": 3,
            "id": "01a1fdd8-cedd-74c2-bad5-a22aa9918c87",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 3.0,
            "risk_reduction": 3,
            "status": "open",
            "time_criticality": 3,
            "title": "Postgres primary, backups and restore"
          },
          {
            "business_value": 2,
            "id": "01a1fdd8-cf24-737d-a364-b0bdc88eac00",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 4.0,
            "risk_reduction": 2,
            "status": "open",
            "time_criticality": 2,
            "title": "Shared workflows and archetypes"
          }
        ]
      },
      "taken_at": "2026-11-21T11:23:26.290608+00:00",
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
      "created_at": "2026-11-26T10:28:46.75224+00:00",
      "id": "01a21766-8e28-7441-883d-47701f8fc01f",
      "kind": "schedule",
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
        "register": [
          {
            "code": "",
            "id": "01a21764-dfbc-7e43-bd06-e3ea06f5592c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "B1 Auditable decisions: every choice has a decision row"
          },
          {
            "code": "",
            "id": "01a21764-df3c-730c-a1ea-f74efd65c83c",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "Every delivery tranche depends on the platform's CI, registry and Postgres paths"
          },
          {
            "code": "",
            "id": "01a21764-debb-7616-b8a7-145addbe913d",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The replay record is the programme's working record until the rework is promoted"
          },
          {
            "code": "",
            "id": "01a21764-de38-7a57-8cbb-94215151316b",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The live public window lags the committed pack until the next publish"
          },
          {
            "code": "",
            "id": "01a21764-ddb4-70a5-b295-53fe1cb1473c",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The standby has not been promoted in a drill, so the recovery path is documented but unproven"
          },
          {
            "code": "",
            "id": "01a21764-dd32-713c-93d5-5578b5192040",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The platform's dependency on a single CI node is a single point of failure for every tranche"
          },
          {
            "code": "R17",
            "id": "01a216d0-d926-70b3-9d0f-ec5345a14424",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Tranches 5 and 10-13 cannot deliver on proposed projects"
          },
          {
            "code": "R16",
            "id": "01a216d0-d8b7-7717-bee8-092bfa73ea7f",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The six benefit baselines are placeholders"
          },
          {
            "code": "R15",
            "id": "01a216d0-d847-7633-adf4-719516677bd0",
            "impact": 5,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "V2 acceptance (#88/T12) remains open"
          },
          {
            "code": "U5-ACCEPTANCE-STANDARD",
            "id": "01a2029f-8e2e-78de-bc99-119da42fdfbf",
            "impact": null,
            "kind": "change",
            "severity": "P1",
            "status": "open",
            "title": "Change: the enhanced acceptance standard (gates executed, no deferrals)"
          },
          {
            "code": "U5-NO-DEFERRALS",
            "id": "01a2029f-5a5f-70aa-be13-029435893a40",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "No deferrals \u2014 every finding executes by owner decision (2026-11-22)"
          },
          {
            "code": "U5-CHG-PLATFORM",
            "id": "01a20200-562e-7a53-9fcd-e136048a00fb",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "accepted",
            "title": "Retrospective MSP/TOGAF/SAFe rework of PLATFORM under the #526 scope change"
          },
          {
            "code": "",
            "id": "01a1ffa9-3907-7c8d-a6c0-819281d6c050",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Tranches 5 and 10-13 cannot deliver on proposed projects"
          },
          {
            "code": "",
            "id": "01a1ffa9-1b48-7f8b-974a-c8d8e5366922",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The six benefit baselines are placeholders"
          },
          {
            "code": "",
            "id": "01a1ffa9-032a-7792-98c3-a85aa9bad193",
            "impact": 5,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "V2 acceptance (#88/T12) remains open"
          }
        ],
        "wbs": [
          {
            "business_value": 3,
            "id": "01a1fdd8-cdf3-7108-aac5-0cec790e988d",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 2,
            "status": "open",
            "time_criticality": 3,
            "title": "CI and runners available"
          },
          {
            "business_value": 3,
            "id": "01a1fdd8-ce5c-70c8-87be-2dd12bf61cb0",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 2.0,
            "risk_reduction": 2,
            "status": "open",
            "time_criticality": 2,
            "title": "Registry and ingress available"
          },
          {
            "business_value": 3,
            "id": "01a1fdd8-cedd-74c2-bad5-a22aa9918c87",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 3.0,
            "risk_reduction": 3,
            "status": "open",
            "time_criticality": 3,
            "title": "Postgres primary, backups and restore"
          },
          {
            "business_value": 2,
            "id": "01a1fdd8-cf24-737d-a364-b0bdc88eac00",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 4.0,
            "risk_reduction": 2,
            "status": "open",
            "time_criticality": 2,
            "title": "Shared workflows and archetypes"
          },
          {
            "business_value": 5,
            "id": "01a1ff24-a6c6-7504-a811-0fecfbc460f6",
            "job_size": 6,
            "kind": "deliverable",
            "parent_id": null,
            "position": 5.0,
            "risk_reduction": 3,
            "status": "accepted",
            "time_criticality": 5,
            "title": "The standing platform record \u2014 charter, baseline and the estate verification"
          }
        ]
      },
      "taken_at": "2026-11-26T10:28:46.75224+00:00",
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
      "created_at": "2026-11-26T10:28:47.029924+00:00",
      "id": "01a21766-8f3d-71be-9403-ceb7d8a7f58e",
      "kind": "cost",
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
        "register": [
          {
            "code": "",
            "id": "01a21764-dfbc-7e43-bd06-e3ea06f5592c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "B1 Auditable decisions: every choice has a decision row"
          },
          {
            "code": "",
            "id": "01a21764-df3c-730c-a1ea-f74efd65c83c",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "Every delivery tranche depends on the platform's CI, registry and Postgres paths"
          },
          {
            "code": "",
            "id": "01a21764-debb-7616-b8a7-145addbe913d",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The replay record is the programme's working record until the rework is promoted"
          },
          {
            "code": "",
            "id": "01a21764-de38-7a57-8cbb-94215151316b",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The live public window lags the committed pack until the next publish"
          },
          {
            "code": "",
            "id": "01a21764-ddb4-70a5-b295-53fe1cb1473c",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The standby has not been promoted in a drill, so the recovery path is documented but unproven"
          },
          {
            "code": "",
            "id": "01a21764-dd32-713c-93d5-5578b5192040",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The platform's dependency on a single CI node is a single point of failure for every tranche"
          },
          {
            "code": "R17",
            "id": "01a216d0-d926-70b3-9d0f-ec5345a14424",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Tranches 5 and 10-13 cannot deliver on proposed projects"
          },
          {
            "code": "R16",
            "id": "01a216d0-d8b7-7717-bee8-092bfa73ea7f",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The six benefit baselines are placeholders"
          },
          {
            "code": "R15",
            "id": "01a216d0-d847-7633-adf4-719516677bd0",
            "impact": 5,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "V2 acceptance (#88/T12) remains open"
          },
          {
            "code": "U5-ACCEPTANCE-STANDARD",
            "id": "01a2029f-8e2e-78de-bc99-119da42fdfbf",
            "impact": null,
            "kind": "change",
            "severity": "P1",
            "status": "open",
            "title": "Change: the enhanced acceptance standard (gates executed, no deferrals)"
          },
          {
            "code": "U5-NO-DEFERRALS",
            "id": "01a2029f-5a5f-70aa-be13-029435893a40",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "No deferrals \u2014 every finding executes by owner decision (2026-11-22)"
          },
          {
            "code": "U5-CHG-PLATFORM",
            "id": "01a20200-562e-7a53-9fcd-e136048a00fb",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "accepted",
            "title": "Retrospective MSP/TOGAF/SAFe rework of PLATFORM under the #526 scope change"
          },
          {
            "code": "",
            "id": "01a1ffa9-3907-7c8d-a6c0-819281d6c050",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Tranches 5 and 10-13 cannot deliver on proposed projects"
          },
          {
            "code": "",
            "id": "01a1ffa9-1b48-7f8b-974a-c8d8e5366922",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The six benefit baselines are placeholders"
          },
          {
            "code": "",
            "id": "01a1ffa9-032a-7792-98c3-a85aa9bad193",
            "impact": 5,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "V2 acceptance (#88/T12) remains open"
          }
        ],
        "wbs": [
          {
            "business_value": 3,
            "id": "01a1fdd8-cdf3-7108-aac5-0cec790e988d",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 2,
            "status": "open",
            "time_criticality": 3,
            "title": "CI and runners available"
          },
          {
            "business_value": 3,
            "id": "01a1fdd8-ce5c-70c8-87be-2dd12bf61cb0",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 2.0,
            "risk_reduction": 2,
            "status": "open",
            "time_criticality": 2,
            "title": "Registry and ingress available"
          },
          {
            "business_value": 3,
            "id": "01a1fdd8-cedd-74c2-bad5-a22aa9918c87",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 3.0,
            "risk_reduction": 3,
            "status": "open",
            "time_criticality": 3,
            "title": "Postgres primary, backups and restore"
          },
          {
            "business_value": 2,
            "id": "01a1fdd8-cf24-737d-a364-b0bdc88eac00",
            "job_size": 1,
            "kind": "work_package",
            "parent_id": null,
            "position": 4.0,
            "risk_reduction": 2,
            "status": "open",
            "time_criticality": 2,
            "title": "Shared workflows and archetypes"
          },
          {
            "business_value": 5,
            "id": "01a1ff24-a6c6-7504-a811-0fecfbc460f6",
            "job_size": 6,
            "kind": "deliverable",
            "parent_id": null,
            "position": 5.0,
            "risk_reduction": 3,
            "status": "accepted",
            "time_criticality": 5,
            "title": "The standing platform record \u2014 charter, baseline and the estate verification"
          }
        ]
      },
      "taken_at": "2026-11-26T10:28:47.029924+00:00",
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
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a1fdd8-d052-7532-8e66-944a4db595ab",
      "kind": "pm",
      "meta": {
        "baseline_id": "01a1fdd8-cff3-7cd9-9535-26fed3d5836b",
        "description": "Approve the scope baseline"
      },
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-21T11:23:38.379267+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T10:28:46.75224+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a21766-8e2b-7496-a660-1687574c0f66",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a21766-8e28-7441-883d-47701f8fc01f",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T10:28:46.888226+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T10:28:47.029924+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a21766-8f3f-7420-b8dd-71bd1b42e52d",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a21766-8f3d-71be-9403-ceb7d8a7f58e",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T10:28:47.163352+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

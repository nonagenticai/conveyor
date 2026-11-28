# Project Baseline — Metrics, metering and maturity

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
      "created_at": "2026-11-21T19:22:22.038135+00:00",
      "id": "01a1ff8f-47cb-78cf-9db8-d98d863e7dd0",
      "kind": "scope",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
        "register": [],
        "wbs": [
          {
            "business_value": 5,
            "id": "01a1ff8f-46f3-7d8c-bac2-ee1ec2d1ac20",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 5,
            "status": "open",
            "time_criticality": 3,
            "title": "The T11 read models \u2014 eight metrics actions, eight metering levels, C0\u2013C5 threshold configuration and the billing reconciliation"
          }
        ]
      },
      "taken_at": "2026-11-21T19:22:22.038135+00:00",
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
      "created_at": "2026-11-26T12:52:08.37085+00:00",
      "id": "01a217e9-ce3a-74c6-af36-fa0fb868d684",
      "kind": "schedule",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
        "register": [
          {
            "code": "",
            "id": "01a217e9-cda0-77f7-b9c4-4d937f0d2a4f",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Billing divergence surfaces as an incident rather than a silent adjustment"
          },
          {
            "code": "",
            "id": "01a217e9-cd15-7aa5-8cce-513ceb3e69ac",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every metric is computed from rows, so a figure cannot drift from the ledger it summarises"
          },
          {
            "code": "",
            "id": "01a217e9-cc83-7881-b3a5-e9459d02a508",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "The console depends on the published snapshot, not on a live metrics call"
          },
          {
            "code": "",
            "id": "01a217e9-cbf8-7a71-91b1-f6e64836fcc6",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "Every T11 figure depends on the ledgers it reads"
          },
          {
            "code": "",
            "id": "01a217e9-cb6e-7303-9650-4b3b150401cc",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The published snapshot's measure.inference window is representative enough to display"
          },
          {
            "code": "",
            "id": "01a217e9-cadf-73bf-a054-f37a9d015a8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "billing.summary's attested total is the charging system's authoritative figure for the period"
          },
          {
            "code": "",
            "id": "01a217e9-ca4f-7313-96a9-87166518a584",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Eval- and benchmark-backed quality metrics sit outside T11"
          },
          {
            "code": "",
            "id": "01a217e9-c9b9-7361-917e-88138eaca2e5",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The original T11 governance record was never authored"
          },
          {
            "code": "",
            "id": "01a217e9-c929-78b6-9ae6-4ccf1b233930",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A tenant with few approvals can reach C1 on a very small sample"
          },
          {
            "code": "",
            "id": "01a217e9-c89b-73ab-98f6-7f90af5b032f",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A stale billing total could match the derived cost by accident"
          },
          {
            "code": "",
            "id": "01a217e9-c80c-7b75-adb6-9b9a4cd157eb",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "A metrics read that starts writing a roll-up row would turn derived figures into a second store"
          },
          {
            "code": "U5-CHG-T11",
            "id": "01a20201-5f8c-7e50-aae8-1f3e8544bd72",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "accepted",
            "title": "Retrospective MSP/TOGAF/SAFe rework of T11 under the #526 scope change"
          }
        ],
        "wbs": [
          {
            "business_value": 5,
            "id": "01a1ff8f-46f3-7d8c-bac2-ee1ec2d1ac20",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 5,
            "status": "accepted",
            "time_criticality": 3,
            "title": "The T11 read models \u2014 eight metrics actions, eight metering levels, C0\u2013C5 threshold configuration and the billing reconciliation"
          }
        ]
      },
      "taken_at": "2026-11-26T12:52:08.37085+00:00",
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
      "created_at": "2026-11-26T12:52:08.676511+00:00",
      "id": "01a217e9-cf6b-7ee1-a4f6-2ba717d1531f",
      "kind": "cost",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
        "register": [
          {
            "code": "",
            "id": "01a217e9-cda0-77f7-b9c4-4d937f0d2a4f",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Billing divergence surfaces as an incident rather than a silent adjustment"
          },
          {
            "code": "",
            "id": "01a217e9-cd15-7aa5-8cce-513ceb3e69ac",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Every metric is computed from rows, so a figure cannot drift from the ledger it summarises"
          },
          {
            "code": "",
            "id": "01a217e9-cc83-7881-b3a5-e9459d02a508",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "The console depends on the published snapshot, not on a live metrics call"
          },
          {
            "code": "",
            "id": "01a217e9-cbf8-7a71-91b1-f6e64836fcc6",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "Every T11 figure depends on the ledgers it reads"
          },
          {
            "code": "",
            "id": "01a217e9-cb6e-7303-9650-4b3b150401cc",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The published snapshot's measure.inference window is representative enough to display"
          },
          {
            "code": "",
            "id": "01a217e9-cadf-73bf-a054-f37a9d015a8e",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "billing.summary's attested total is the charging system's authoritative figure for the period"
          },
          {
            "code": "",
            "id": "01a217e9-ca4f-7313-96a9-87166518a584",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Eval- and benchmark-backed quality metrics sit outside T11"
          },
          {
            "code": "",
            "id": "01a217e9-c9b9-7361-917e-88138eaca2e5",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The original T11 governance record was never authored"
          },
          {
            "code": "",
            "id": "01a217e9-c929-78b6-9ae6-4ccf1b233930",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A tenant with few approvals can reach C1 on a very small sample"
          },
          {
            "code": "",
            "id": "01a217e9-c89b-73ab-98f6-7f90af5b032f",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A stale billing total could match the derived cost by accident"
          },
          {
            "code": "",
            "id": "01a217e9-c80c-7b75-adb6-9b9a4cd157eb",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "A metrics read that starts writing a roll-up row would turn derived figures into a second store"
          },
          {
            "code": "U5-CHG-T11",
            "id": "01a20201-5f8c-7e50-aae8-1f3e8544bd72",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "accepted",
            "title": "Retrospective MSP/TOGAF/SAFe rework of T11 under the #526 scope change"
          }
        ],
        "wbs": [
          {
            "business_value": 5,
            "id": "01a1ff8f-46f3-7d8c-bac2-ee1ec2d1ac20",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 5,
            "status": "accepted",
            "time_criticality": 3,
            "title": "The T11 read models \u2014 eight metrics actions, eight metering levels, C0\u2013C5 threshold configuration and the billing reconciliation"
          }
        ]
      },
      "taken_at": "2026-11-26T12:52:08.676511+00:00",
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
      "created_at": "2026-11-21T19:22:22.038135+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a1ff8f-482a-746a-bd48-67924668daf2",
      "kind": "pm",
      "meta": {
        "baseline_id": "01a1ff8f-47cb-78cf-9db8-d98d863e7dd0",
        "description": "Approve the scope baseline"
      },
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-21T19:22:27.339913+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T12:52:08.37085+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a217e9-ce3c-7b1b-91e3-1dd900366c12",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a217e9-ce3a-74c6-af36-fa0fb868d684",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T12:52:08.518277+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T12:52:08.676511+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a217e9-cf6e-7389-8606-b96a53a7c334",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a217e9-cf6b-7ee1-a4f6-2ba717d1531f",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T12:52:08.821658+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

# Project Baseline — Board — projects, boards, lists, cards, labels, comments, tasks, custom fields, webhooks, SSE

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
      "created_at": "2026-11-26T21:52:40.238769+00:00",
      "id": "01a219d8-ad39-77c1-83f6-da901324441d",
      "kind": "scope",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
        "register": [
          {
            "code": "S05-B3",
            "id": "01a219d8-ac9d-73cf-9758-7d99d8411538",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "realized",
            "title": "The re-close is decidable from the acceptance instrument's P0 checks"
          },
          {
            "code": "S05-B2",
            "id": "01a219d8-ac0a-778b-8e66-145431e0206b",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One comment store and one membership model replace the Planka and fr duplication"
          },
          {
            "code": "S05-B1",
            "id": "01a219d8-ab5b-7c62-9da1-3ae8c9b952cc",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 board\u2194project lookup flows become one foreign-key read through the project_type matrix"
          },
          {
            "code": "S05-D2",
            "id": "01a219d8-aab0-72e7-98cd-00e76378760e",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 and S06 consume the board surface; S05 never writes claims, assignee or stopwatch"
          },
          {
            "code": "S05-D1",
            "id": "01a219d8-aa21-7bb6-b1c6-8f00bfeb1a87",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "S05 is blocked by S03, S04 and S02"
          },
          {
            "code": "S05-A2",
            "id": "01a219d8-a992-7cf2-8c55-cd7cf1d52f73",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The live Planka database is test data and never gate evidence"
          },
          {
            "code": "S05-A1",
            "id": "01a219d8-a8ff-7400-8cee-ec579f0b00e6",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-24 rework snapshot is the authoritative record for S05"
          },
          {
            "code": "S05-I2",
            "id": "01a219d8-a847-711a-a1cc-e070482e5879",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The Planka fork is absent, so the per-event envelope and List.type transitions stay unverified"
          },
          {
            "code": "S05-I1",
            "id": "01a219d8-a7b2-71bb-ad4e-ec84c334b0bd",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The close preceded tranche 4's authorisation by three days and rests only on an accepted P0 exception"
          },
          {
            "code": "S05-R2",
            "id": "01a219d8-a714-72fe-b1d9-6076461df613",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan frontmatter says planned while the project meta says built"
          },
          {
            "code": "S05-R1",
            "id": "01a219d8-a67c-75d1-a9a5-17a0c715b121",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The six gate properties have no recorded run, so the built claim is unproven"
          },
          {
            "code": "U3-EXC-S05",
            "id": "01a1fbad-0be3-7557-9402-cfb405a3147c",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S05)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a219d8-ad35-7c0b-9ab6-e69d1ff20407",
            "job_size": 5,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 8,
            "title": "The board slice \u2014 projects, boards, lists, cards, labels, comments, tasks, custom fields, webhooks and SSE"
          }
        ]
      },
      "taken_at": "2026-11-26T21:52:40.238769+00:00",
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
      "created_at": "2026-11-26T21:52:40.399838+00:00",
      "id": "01a219d8-add8-7fad-8923-5836c04b16b0",
      "kind": "schedule",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
        "register": [
          {
            "code": "S05-B3",
            "id": "01a219d8-ac9d-73cf-9758-7d99d8411538",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "realized",
            "title": "The re-close is decidable from the acceptance instrument's P0 checks"
          },
          {
            "code": "S05-B2",
            "id": "01a219d8-ac0a-778b-8e66-145431e0206b",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One comment store and one membership model replace the Planka and fr duplication"
          },
          {
            "code": "S05-B1",
            "id": "01a219d8-ab5b-7c62-9da1-3ae8c9b952cc",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 board\u2194project lookup flows become one foreign-key read through the project_type matrix"
          },
          {
            "code": "S05-D2",
            "id": "01a219d8-aab0-72e7-98cd-00e76378760e",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 and S06 consume the board surface; S05 never writes claims, assignee or stopwatch"
          },
          {
            "code": "S05-D1",
            "id": "01a219d8-aa21-7bb6-b1c6-8f00bfeb1a87",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "S05 is blocked by S03, S04 and S02"
          },
          {
            "code": "S05-A2",
            "id": "01a219d8-a992-7cf2-8c55-cd7cf1d52f73",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The live Planka database is test data and never gate evidence"
          },
          {
            "code": "S05-A1",
            "id": "01a219d8-a8ff-7400-8cee-ec579f0b00e6",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-24 rework snapshot is the authoritative record for S05"
          },
          {
            "code": "S05-I2",
            "id": "01a219d8-a847-711a-a1cc-e070482e5879",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The Planka fork is absent, so the per-event envelope and List.type transitions stay unverified"
          },
          {
            "code": "S05-I1",
            "id": "01a219d8-a7b2-71bb-ad4e-ec84c334b0bd",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The close preceded tranche 4's authorisation by three days and rests only on an accepted P0 exception"
          },
          {
            "code": "S05-R2",
            "id": "01a219d8-a714-72fe-b1d9-6076461df613",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan frontmatter says planned while the project meta says built"
          },
          {
            "code": "S05-R1",
            "id": "01a219d8-a67c-75d1-a9a5-17a0c715b121",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The six gate properties have no recorded run, so the built claim is unproven"
          },
          {
            "code": "U3-EXC-S05",
            "id": "01a1fbad-0be3-7557-9402-cfb405a3147c",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S05)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a219d8-ad35-7c0b-9ab6-e69d1ff20407",
            "job_size": 5,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 8,
            "title": "The board slice \u2014 projects, boards, lists, cards, labels, comments, tasks, custom fields, webhooks and SSE"
          }
        ]
      },
      "taken_at": "2026-11-26T21:52:40.399838+00:00",
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
      "created_at": "2026-11-26T21:52:40.79966+00:00",
      "id": "01a219d8-af66-7cd1-96a9-35fba3636e76",
      "kind": "cost",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
        "register": [
          {
            "code": "S05-B3",
            "id": "01a219d8-ac9d-73cf-9758-7d99d8411538",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "realized",
            "title": "The re-close is decidable from the acceptance instrument's P0 checks"
          },
          {
            "code": "S05-B2",
            "id": "01a219d8-ac0a-778b-8e66-145431e0206b",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One comment store and one membership model replace the Planka and fr duplication"
          },
          {
            "code": "S05-B1",
            "id": "01a219d8-ab5b-7c62-9da1-3ae8c9b952cc",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The 280 board\u2194project lookup flows become one foreign-key read through the project_type matrix"
          },
          {
            "code": "S05-D2",
            "id": "01a219d8-aab0-72e7-98cd-00e76378760e",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 and S06 consume the board surface; S05 never writes claims, assignee or stopwatch"
          },
          {
            "code": "S05-D1",
            "id": "01a219d8-aa21-7bb6-b1c6-8f00bfeb1a87",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "satisfied",
            "title": "S05 is blocked by S03, S04 and S02"
          },
          {
            "code": "S05-A2",
            "id": "01a219d8-a992-7cf2-8c55-cd7cf1d52f73",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The live Planka database is test data and never gate evidence"
          },
          {
            "code": "S05-A1",
            "id": "01a219d8-a8ff-7400-8cee-ec579f0b00e6",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-24 rework snapshot is the authoritative record for S05"
          },
          {
            "code": "S05-I2",
            "id": "01a219d8-a847-711a-a1cc-e070482e5879",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The Planka fork is absent, so the per-event envelope and List.type transitions stay unverified"
          },
          {
            "code": "S05-I1",
            "id": "01a219d8-a7b2-71bb-ad4e-ec84c334b0bd",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "The close preceded tranche 4's authorisation by three days and rests only on an accepted P0 exception"
          },
          {
            "code": "S05-R2",
            "id": "01a219d8-a714-72fe-b1d9-6076461df613",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The plan frontmatter says planned while the project meta says built"
          },
          {
            "code": "S05-R1",
            "id": "01a219d8-a67c-75d1-a9a5-17a0c715b121",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The six gate properties have no recorded run, so the built claim is unproven"
          },
          {
            "code": "U3-EXC-S05",
            "id": "01a1fbad-0be3-7557-9402-cfb405a3147c",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S05)"
          }
        ],
        "wbs": [
          {
            "business_value": 10,
            "id": "01a219d8-ad35-7c0b-9ab6-e69d1ff20407",
            "job_size": 5,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 8,
            "title": "The board slice \u2014 projects, boards, lists, cards, labels, comments, tasks, custom fields, webhooks and SSE"
          }
        ]
      },
      "taken_at": "2026-11-26T21:52:40.79966+00:00",
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
      "created_at": "2026-11-26T21:52:40.238769+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a219d8-ad3b-7cfd-b383-c7c185b1f818",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a219d8-ad39-77c1-83f6-da901324441d",
        "description": "Approve the scope baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T21:52:48.843614+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T21:52:40.399838+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a219d8-addc-7015-aaea-c4c6fbab8ad9",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a219d8-add8-7fad-8923-5836c04b16b0",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T21:52:40.571838+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T21:52:40.79966+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a219d8-af69-71ea-a6cd-3780e97ba875",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a219d8-af66-7cd1-96a9-35fba3636e76",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T21:52:40.944421+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

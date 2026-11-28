# Project Baseline — Domain and schema — aggregates, state machines, baseline domain migration

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
      "created_at": "2026-11-21T23:14:08.772733+00:00",
      "id": "01a20063-7af8-727b-b092-24de3821d204",
      "kind": "scope",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
        "register": [
          {
            "code": "U3-EXC-S03",
            "id": "01a1fbac-8ec9-717c-be79-6d1b97980743",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S03)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a20063-7a23-7454-b5b6-0fedd41baf93",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 5,
            "title": "The domain and schema \u2014 aggregates, state machines, leases and the baseline domain migration"
          }
        ]
      },
      "taken_at": "2026-11-21T23:14:08.772733+00:00",
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
      "created_at": "2026-11-26T13:08:24.958191+00:00",
      "id": "01a217f8-b505-769d-ab07-a6b8e6c59fe7",
      "kind": "schedule",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
        "register": [
          {
            "code": "B-S03-02",
            "id": "01a217f8-b46f-7f1f-92bd-76adb49fad33",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The database and Rust refuse the same transition"
          },
          {
            "code": "B-S03-01",
            "id": "01a217f8-b3e3-77af-b2f5-1eaa52ce07ee",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One stored domain behind every later slice"
          },
          {
            "code": "D-S03-02",
            "id": "01a217f8-b357-7b38-a35b-ebd811104fe6",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "Later slices add tables and never re-invent one"
          },
          {
            "code": "D-S03-01",
            "id": "01a217f8-b2cc-7b1d-995c-0723649d6202",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S02 platform migration and tenancy primitives land first"
          },
          {
            "code": "A-S03-02",
            "id": "01a217f8-b245-759f-b8dd-c5f4a83eb2ac",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Postgres 18 enforces FORCE RLS for the owner and scopes the runtime role"
          },
          {
            "code": "A-S03-01",
            "id": "01a217f8-b1bc-7f6a-9a3d-6c65bc101922",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "S02's platform primitives stay stable under the domain"
          },
          {
            "code": "I-S03-02",
            "id": "01a217f8-b133-7bd4-aaed-e078c1fd9c03",
            "impact": null,
            "kind": "issue",
            "severity": null,
            "status": "closed",
            "title": "No contemporaneous schema review or Phase A-D artefact set existed at the snapshot"
          },
          {
            "code": "I-S03-01",
            "id": "01a217f8-b0ad-7317-987c-f82f016bf405",
            "impact": null,
            "kind": "issue",
            "severity": null,
            "status": "accepted",
            "title": "S03 closed out of sequence before the tranche-2 review (U3-EXC-S03)"
          },
          {
            "code": "R-S03-02",
            "id": "01a217f8-b016-733d-9a75-14afc9fd14ff",
            "impact": 2,
            "kind": "risk",
            "severity": null,
            "status": "admitted",
            "title": "The archive-exclusion claim covers only the two board-scan indexes (v1-s03-2)"
          },
          {
            "code": "R-S03-01",
            "id": "01a217f8-af89-72bc-b545-722a33f7f0de",
            "impact": 3,
            "kind": "risk",
            "severity": null,
            "status": "admitted",
            "title": "The lease-exclusivity claim is not exercised by the 32-racer test (v1-s03-1)"
          },
          {
            "code": "U5-CHG-S03",
            "id": "01a2022d-b82e-723c-ac03-07afb43890dc",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "accepted",
            "title": "Retrospective MSP/TOGAF/SAFe rework of S03 under the #526 scope change"
          },
          {
            "code": "U3-EXC-S03",
            "id": "01a1fbac-8ec9-717c-be79-6d1b97980743",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S03)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a20063-7a23-7454-b5b6-0fedd41baf93",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "accepted",
            "time_criticality": 5,
            "title": "The domain and schema \u2014 aggregates, state machines, leases and the baseline domain migration"
          }
        ]
      },
      "taken_at": "2026-11-26T13:08:24.958191+00:00",
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
      "created_at": "2026-11-26T13:08:25.25186+00:00",
      "id": "01a217f8-b62b-7106-afae-f19a5506ea54",
      "kind": "cost",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
        "register": [
          {
            "code": "B-S03-02",
            "id": "01a217f8-b46f-7f1f-92bd-76adb49fad33",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The database and Rust refuse the same transition"
          },
          {
            "code": "B-S03-01",
            "id": "01a217f8-b3e3-77af-b2f5-1eaa52ce07ee",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One stored domain behind every later slice"
          },
          {
            "code": "D-S03-02",
            "id": "01a217f8-b357-7b38-a35b-ebd811104fe6",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "Later slices add tables and never re-invent one"
          },
          {
            "code": "D-S03-01",
            "id": "01a217f8-b2cc-7b1d-995c-0723649d6202",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S02 platform migration and tenancy primitives land first"
          },
          {
            "code": "A-S03-02",
            "id": "01a217f8-b245-759f-b8dd-c5f4a83eb2ac",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Postgres 18 enforces FORCE RLS for the owner and scopes the runtime role"
          },
          {
            "code": "A-S03-01",
            "id": "01a217f8-b1bc-7f6a-9a3d-6c65bc101922",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "S02's platform primitives stay stable under the domain"
          },
          {
            "code": "I-S03-02",
            "id": "01a217f8-b133-7bd4-aaed-e078c1fd9c03",
            "impact": null,
            "kind": "issue",
            "severity": null,
            "status": "closed",
            "title": "No contemporaneous schema review or Phase A-D artefact set existed at the snapshot"
          },
          {
            "code": "I-S03-01",
            "id": "01a217f8-b0ad-7317-987c-f82f016bf405",
            "impact": null,
            "kind": "issue",
            "severity": null,
            "status": "accepted",
            "title": "S03 closed out of sequence before the tranche-2 review (U3-EXC-S03)"
          },
          {
            "code": "R-S03-02",
            "id": "01a217f8-b016-733d-9a75-14afc9fd14ff",
            "impact": 2,
            "kind": "risk",
            "severity": null,
            "status": "admitted",
            "title": "The archive-exclusion claim covers only the two board-scan indexes (v1-s03-2)"
          },
          {
            "code": "R-S03-01",
            "id": "01a217f8-af89-72bc-b545-722a33f7f0de",
            "impact": 3,
            "kind": "risk",
            "severity": null,
            "status": "admitted",
            "title": "The lease-exclusivity claim is not exercised by the 32-racer test (v1-s03-1)"
          },
          {
            "code": "U5-CHG-S03",
            "id": "01a2022d-b82e-723c-ac03-07afb43890dc",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "accepted",
            "title": "Retrospective MSP/TOGAF/SAFe rework of S03 under the #526 scope change"
          },
          {
            "code": "U3-EXC-S03",
            "id": "01a1fbac-8ec9-717c-be79-6d1b97980743",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S03)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a20063-7a23-7454-b5b6-0fedd41baf93",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "accepted",
            "time_criticality": 5,
            "title": "The domain and schema \u2014 aggregates, state machines, leases and the baseline domain migration"
          }
        ]
      },
      "taken_at": "2026-11-26T13:08:25.25186+00:00",
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
      "created_at": "2026-11-21T23:14:08.772733+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a20063-7b58-7840-b544-cdfc5b38232f",
      "kind": "pm",
      "meta": {
        "baseline_id": "01a20063-7af8-727b-b092-24de3821d204",
        "description": "Approve the scope baseline"
      },
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-21T23:14:14.070483+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T13:08:24.958191+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a217f8-b507-7c8a-badc-04459d9410c3",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a217f8-b505-769d-ab07-a6b8e6c59fe7",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T13:08:25.100744+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T13:08:25.25186+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a217f8-b62d-7666-878e-80daaa5ad9cc",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a217f8-b62b-7106-afae-f19a5506ea54",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T13:08:25.391147+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

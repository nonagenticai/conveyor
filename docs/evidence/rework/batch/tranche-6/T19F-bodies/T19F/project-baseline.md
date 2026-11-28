# Project Baseline — Decision-forum governance (follow-on)

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
      "created_at": "2026-11-28T07:33:57.474144+00:00",
      "id": "01a22113-386c-72ea-a05a-e4d06902e55f",
      "kind": "scope",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.474144+00:00",
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
      "created_at": "2026-11-28T07:33:57.474144+00:00",
      "id": "01a22113-386c-72ea-a05a-e4d06902e55f",
      "kind": "scope",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.474144+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:33:57.626782+00:00",
      "id": "01a22113-3902-718d-8d03-7975a8ed2968",
      "kind": "schedule",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.626782+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:33:57.932105+00:00",
      "id": "01a22113-3a3a-7d1a-b53b-7edf821a0ddd",
      "kind": "cost",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.932105+00:00",
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
      "created_at": "2026-11-28T07:33:57.626782+00:00",
      "id": "01a22113-3902-718d-8d03-7975a8ed2968",
      "kind": "schedule",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.626782+00:00",
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
      "created_at": "2026-11-28T07:33:57.474144+00:00",
      "id": "01a22113-386c-72ea-a05a-e4d06902e55f",
      "kind": "scope",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.474144+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:33:57.626782+00:00",
      "id": "01a22113-3902-718d-8d03-7975a8ed2968",
      "kind": "schedule",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.626782+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:33:57.932105+00:00",
      "id": "01a22113-3a3a-7d1a-b53b-7edf821a0ddd",
      "kind": "cost",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.932105+00:00",
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
      "created_at": "2026-11-28T07:33:57.932105+00:00",
      "id": "01a22113-3a3a-7d1a-b53b-7edf821a0ddd",
      "kind": "cost",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.932105+00:00",
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
      "created_at": "2026-11-28T07:33:57.474144+00:00",
      "id": "01a22113-386c-72ea-a05a-e4d06902e55f",
      "kind": "scope",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.474144+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:33:57.626782+00:00",
      "id": "01a22113-3902-718d-8d03-7975a8ed2968",
      "kind": "schedule",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.626782+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:33:57.932105+00:00",
      "id": "01a22113-3a3a-7d1a-b53b-7edf821a0ddd",
      "kind": "cost",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.932105+00:00",
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
      "created_at": "2026-11-28T07:33:57.474144+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a22113-386e-7b42-8eac-648e158cada0",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a22113-386c-72ea-a05a-e4d06902e55f",
        "description": "Approve the scope baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:34:10.087862+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T07:33:57.626782+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a22113-3904-7852-9802-4de73df7432c",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a22113-3902-718d-8d03-7975a8ed2968",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:33:57.773299+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T07:33:57.932105+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a22113-3a42-7325-9c2a-4e3d96447220",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a22113-3a3a-7d1a-b53b-7edf821a0ddd",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:33:58.120457+00:00",
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
      "created_at": "2026-11-28T07:33:57.474144+00:00",
      "id": "01a22113-386c-72ea-a05a-e4d06902e55f",
      "kind": "scope",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.474144+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:33:57.626782+00:00",
      "id": "01a22113-3902-718d-8d03-7975a8ed2968",
      "kind": "schedule",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.626782+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:33:57.932105+00:00",
      "id": "01a22113-3a3a-7d1a-b53b-7edf821a0ddd",
      "kind": "cost",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "snapshot": {
        "frozen": true,
        "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
        "register": [
          {
            "code": "T19F-B3",
            "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One meeting per cadence slot, without a stronger scheduling contract"
          },
          {
            "code": "T19F-B2",
            "id": "01a22113-3742-7547-868f-2c85efe8c905",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Board decisions are bound to the forum that took them"
          },
          {
            "code": "T19F-B1",
            "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Forum policy changes without mutating history"
          },
          {
            "code": "T19F-D2",
            "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T19F is the last delivery member of tranche 6"
          },
          {
            "code": "T19F-D1",
            "id": "01a22113-359c-760f-9485-5160dc341238",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "T19F depends on T19 and T17 and on the tranche-5 authority"
          },
          {
            "code": "T19F-A3",
            "id": "01a22113-350d-784f-9360-311fda17b38a",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "T19's uq_meeting_live remains the slot rule"
          },
          {
            "code": "T19F-A2",
            "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The direct architecture.board_decide path stays unchanged"
          },
          {
            "code": "T19F-A1",
            "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F"
          },
          {
            "code": "T19F-I2",
            "id": "01a22113-336a-7405-81b0-6da2e730db16",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710"
          },
          {
            "code": "T19F-I1",
            "id": "01a22113-32de-7be4-a634-330c9c038859",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively"
          },
          {
            "code": "T19F-R2",
            "id": "01a22113-3251-74b7-88d5-83f13973c52b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture"
          },
          {
            "code": "T19F-R1",
            "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a22113-3868-7a96-b19a-3fe8133fa057",
            "job_size": 14,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 10,
            "status": "open",
            "time_criticality": 6,
            "title": "Decision-forum governance \u2014 versioned policy, bound board sittings and cadence materialisation"
          }
        ]
      },
      "taken_at": "2026-11-28T07:33:57.932105+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

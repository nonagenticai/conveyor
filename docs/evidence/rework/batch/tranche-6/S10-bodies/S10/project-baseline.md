# Project Baseline — Copilot / AG-UI — one action surface, proposals, conversations

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
      "created_at": "2026-11-28T07:29:28.491005+00:00",
      "id": "01a2210f-1db4-7dd2-b55b-ac22df9e2110",
      "kind": "scope",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:28.491005+00:00",
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
      "created_at": "2026-11-28T07:29:28.491005+00:00",
      "id": "01a2210f-1db4-7dd2-b55b-ac22df9e2110",
      "kind": "scope",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:28.491005+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:29:28.653794+00:00",
      "id": "01a2210f-1e57-782b-8313-25bda10e853a",
      "kind": "schedule",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:28.653794+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:29:29.007415+00:00",
      "id": "01a2210f-1fb7-707b-bb8e-a61a5a23eacb",
      "kind": "cost",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:29.007415+00:00",
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
      "created_at": "2026-11-28T07:29:28.653794+00:00",
      "id": "01a2210f-1e57-782b-8313-25bda10e853a",
      "kind": "schedule",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:28.653794+00:00",
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
      "created_at": "2026-11-28T07:29:28.491005+00:00",
      "id": "01a2210f-1db4-7dd2-b55b-ac22df9e2110",
      "kind": "scope",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:28.491005+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:29:28.653794+00:00",
      "id": "01a2210f-1e57-782b-8313-25bda10e853a",
      "kind": "schedule",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:28.653794+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:29:29.007415+00:00",
      "id": "01a2210f-1fb7-707b-bb8e-a61a5a23eacb",
      "kind": "cost",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:29.007415+00:00",
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
      "created_at": "2026-11-28T07:29:29.007415+00:00",
      "id": "01a2210f-1fb7-707b-bb8e-a61a5a23eacb",
      "kind": "cost",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:29.007415+00:00",
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
      "created_at": "2026-11-28T07:29:28.491005+00:00",
      "id": "01a2210f-1db4-7dd2-b55b-ac22df9e2110",
      "kind": "scope",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:28.491005+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:29:28.653794+00:00",
      "id": "01a2210f-1e57-782b-8313-25bda10e853a",
      "kind": "schedule",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:28.653794+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:29:29.007415+00:00",
      "id": "01a2210f-1fb7-707b-bb8e-a61a5a23eacb",
      "kind": "cost",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:29.007415+00:00",
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
      "created_at": "2026-11-28T07:29:28.491005+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a2210f-1dba-7639-8ea0-2ddff2d3c4cc",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a2210f-1db4-7dd2-b55b-ac22df9e2110",
        "description": "Approve the scope baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:29:41.154327+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T07:29:28.653794+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a2210f-1e5a-7735-a9ff-cd52fde06447",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a2210f-1e57-782b-8313-25bda10e853a",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:29:28.814099+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T07:29:29.007415+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a2210f-1fb9-7bef-bdf3-6a066dd622c6",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a2210f-1fb7-707b-bb8e-a61a5a23eacb",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T07:29:29.176991+00:00",
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
      "created_at": "2026-11-28T07:29:28.491005+00:00",
      "id": "01a2210f-1db4-7dd2-b55b-ac22df9e2110",
      "kind": "scope",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:28.491005+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:29:28.653794+00:00",
      "id": "01a2210f-1e57-782b-8313-25bda10e853a",
      "kind": "schedule",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:28.653794+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T07:29:29.007415+00:00",
      "id": "01a2210f-1fb7-707b-bb8e-a61a5a23eacb",
      "kind": "cost",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
        "register": [
          {
            "code": "S10-B5",
            "id": "01a2210f-1d15-7a05-951c-c34751a705ba",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A proposal does not expose a secret-bearing input to approvers"
          },
          {
            "code": "S10-B4",
            "id": "01a2210f-1c81-784d-bb66-fa2b85abf1de",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "The coverage gate fails by name rather than widening an exemption"
          },
          {
            "code": "S10-B3",
            "id": "01a2210f-1bef-7919-95c1-42e7605d5d90",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Conversations are durable and replayable without a second execution log"
          },
          {
            "code": "S10-B2",
            "id": "01a2210f-1b5c-7fed-a85e-6b6bd427ebde",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A destructive tool call becomes a decidable approval"
          },
          {
            "code": "S10-B1",
            "id": "01a2210f-1acc-7f1b-9bb0-6c58f908f082",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One action surface replaces a second copilot registry"
          },
          {
            "code": "S10-D3",
            "id": "01a2210f-1a3b-706c-a873-a07acb9e4aaf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b's agent principal is the machine consumer of the same projection"
          },
          {
            "code": "S10-D2",
            "id": "01a2210f-19ab-70c3-bc8d-d21b21333a37",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 renders the declared screens and S14 carries the deployed proof"
          },
          {
            "code": "S10-D1",
            "id": "01a2210f-191b-76e5-86a2-a13313feba58",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "met",
            "title": "S10 depends on S02, S03, S04 and S06"
          },
          {
            "code": "S10-A4",
            "id": "01a2210f-1887-7ccf-b949-f37af9ad5719",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The tenant key used to seal a proposal input is available to the executing process"
          },
          {
            "code": "S10-A3",
            "id": "01a2210f-17f6-786c-9093-77f11f0b8e27",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "Approvals never expire is the owner ruling the proposal rows follow"
          },
          {
            "code": "S10-A2",
            "id": "01a2210f-1765-7a85-b7fc-c186f6b22af4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The suites are the proof boundary; no deployed copilot run is claimed"
          },
          {
            "code": "S10-A1",
            "id": "01a2210f-16d5-75bc-9a54-a3da934b68c0",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "held",
            "title": "The 2026-11-27 rework snapshot is the authoritative record for S10"
          },
          {
            "code": "S10-I4",
            "id": "01a2210f-1645-7bb0-bb4d-5f3a360d7124",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The suite set has grown beyond the plan's eight named API suites"
          },
          {
            "code": "S10-I3",
            "id": "01a2210f-15b6-75c3-a3a3-64e53d5e4ed7",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The agent-surface module header contradicts the suite on the omitted-call refusal"
          },
          {
            "code": "S10-I2",
            "id": "01a2210f-1527-702b-8b88-337666445b83",
            "impact": null,
            "kind": "issue",
            "severity": "low",
            "status": "open",
            "title": "The slice plan's deliverable paths have drifted from the tree"
          },
          {
            "code": "S10-I1",
            "id": "01a2210f-1497-76d4-823f-5c4f7cc8e918",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "The record was closed with no charter, baseline, contract, artefact set or Phase-G review"
          },
          {
            "code": "S10-R4",
            "id": "01a2210f-1406-7169-828a-1a450bc3ba96",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "The AG-UI adapter's live consumer is not proven by the golden-frame suite"
          },
          {
            "code": "S10-R3",
            "id": "01a2210f-1373-7e85-9d4b-cf14fe0cebe1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "A sealed proposal input depends on the tenant key being available at execution"
          },
          {
            "code": "S10-R2",
            "id": "01a2210f-12e0-7fd6-b737-b123e23e702b",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Exactly-once execution depends on the claim lease and the idempotency key together"
          },
          {
            "code": "S10-R1",
            "id": "01a2210f-124f-7b7d-bd76-9020a9e064f7",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The model boundary is external, so a router outage stops every turn"
          },
          {
            "code": "U3-EXC-S10",
            "id": "01a1fbae-2b08-70fc-9704-ad41ef5c3be8",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S10)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a2210f-1db0-705f-a73a-3fb5f98a8703",
            "job_size": 9,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 8,
            "title": "The copilot as one action surface \u2014 proposals, durable conversations and replay"
          }
        ]
      },
      "taken_at": "2026-11-28T07:29:29.007415+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

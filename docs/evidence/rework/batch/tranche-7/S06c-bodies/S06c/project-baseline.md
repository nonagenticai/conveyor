# Project Baseline — MCP tasks — a long-running project asks for input

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
      "created_at": "2026-11-28T14:54:13.964101+00:00",
      "id": "01a222a6-4dd7-71e2-b652-66d5f1b7a071",
      "kind": "scope",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:13.964101+00:00",
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
      "created_at": "2026-11-28T14:54:13.964101+00:00",
      "id": "01a222a6-4dd7-71e2-b652-66d5f1b7a071",
      "kind": "scope",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:13.964101+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:54:14.120514+00:00",
      "id": "01a222a6-4e70-7636-9768-16321e7e4dea",
      "kind": "schedule",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:14.120514+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:54:14.420114+00:00",
      "id": "01a222a6-4f9b-73e6-b70e-da35e203d286",
      "kind": "cost",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:14.420114+00:00",
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
      "created_at": "2026-11-28T14:54:14.120514+00:00",
      "id": "01a222a6-4e70-7636-9768-16321e7e4dea",
      "kind": "schedule",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:14.120514+00:00",
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
      "created_at": "2026-11-28T14:54:13.964101+00:00",
      "id": "01a222a6-4dd7-71e2-b652-66d5f1b7a071",
      "kind": "scope",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:13.964101+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:54:14.120514+00:00",
      "id": "01a222a6-4e70-7636-9768-16321e7e4dea",
      "kind": "schedule",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:14.120514+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:54:14.420114+00:00",
      "id": "01a222a6-4f9b-73e6-b70e-da35e203d286",
      "kind": "cost",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:14.420114+00:00",
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
      "created_at": "2026-11-28T14:54:14.420114+00:00",
      "id": "01a222a6-4f9b-73e6-b70e-da35e203d286",
      "kind": "cost",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:14.420114+00:00",
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
      "created_at": "2026-11-28T14:54:13.964101+00:00",
      "id": "01a222a6-4dd7-71e2-b652-66d5f1b7a071",
      "kind": "scope",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:13.964101+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:54:14.120514+00:00",
      "id": "01a222a6-4e70-7636-9768-16321e7e4dea",
      "kind": "schedule",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:14.120514+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:54:14.420114+00:00",
      "id": "01a222a6-4f9b-73e6-b70e-da35e203d286",
      "kind": "cost",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:14.420114+00:00",
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
      "created_at": "2026-11-28T14:54:13.964101+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a222a6-4dd9-7f47-aad2-aff8f28dfb2e",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a222a6-4dd7-71e2-b652-66d5f1b7a071",
        "description": "Approve the scope baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T14:54:26.796992+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T14:54:14.120514+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a222a6-4e72-7803-b78d-2acd82e0389a",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a222a6-4e70-7636-9768-16321e7e4dea",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T14:54:14.264756+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T14:54:14.420114+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a222a6-4f9f-7296-80ca-58bfc1b02a62",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a222a6-4f9b-73e6-b70e-da35e203d286",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T14:54:14.565949+00:00",
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
      "created_at": "2026-11-28T14:54:13.964101+00:00",
      "id": "01a222a6-4dd7-71e2-b652-66d5f1b7a071",
      "kind": "scope",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:13.964101+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:54:14.120514+00:00",
      "id": "01a222a6-4e70-7636-9768-16321e7e4dea",
      "kind": "schedule",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:14.120514+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:54:14.420114+00:00",
      "id": "01a222a6-4f9b-73e6-b70e-da35e203d286",
      "kind": "cost",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
        "register": [
          {
            "code": "S06c-B4",
            "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Ownership failures are indistinguishable"
          },
          {
            "code": "S06c-B3",
            "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One task-capable action keeps the surface small"
          },
          {
            "code": "S06c-B2",
            "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A blocked project is answerable through the protocol that started it"
          },
          {
            "code": "S06c-B1",
            "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A long-running project becomes a durable task with no second store"
          },
          {
            "code": "S06c-D3",
            "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan"
          },
          {
            "code": "S06c-D2",
            "id": "01a222a6-4a61-710f-be32-91a0f176862a",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06b supplies the licence principal the task surface authenticates"
          },
          {
            "code": "S06c-D1",
            "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S06c depends on S04, S05 and S06"
          },
          {
            "code": "S06c-A4",
            "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The approval is a sufficient input request"
          },
          {
            "code": "S06c-A3",
            "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "One live execution per project is the right cardinality"
          },
          {
            "code": "S06c-A2",
            "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A conforming MCP client accepts polling as the only delivery mechanism"
          },
          {
            "code": "S06c-A1",
            "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c"
          },
          {
            "code": "S06c-I3",
            "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A progress percentage and push delivery are absent by scope, not by omission"
          },
          {
            "code": "S06c-I2",
            "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "The gate's first form could never match, and its zero-match run exited green"
          },
          {
            "code": "S06c-I1",
            "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S06c-R3",
            "id": "01a222a6-4562-7421-8a21-73815103546c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing"
          },
          {
            "code": "S06c-R2",
            "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
            "impact": 4,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The reaper must skip project executions, and its correctness depends on that predicate"
          },
          {
            "code": "S06c-R1",
            "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls"
          },
          {
            "code": "U3-EXC-S06c",
            "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a222a6-4dd2-7dd6-8ac3-1fcced4e47e9",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 6,
            "status": "open",
            "time_criticality": 6,
            "title": "A durable agent task \u2014 one task-capable action whose id is the run id, a project-scoped inputRequests listing and a terminal result with isError on failure"
          }
        ]
      },
      "taken_at": "2026-11-28T14:54:14.420114+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

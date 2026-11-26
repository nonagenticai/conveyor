# Project Baseline — Tool platform

## Scope

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-baseline",
  "count": 2,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-22T08:55:18.027529+00:00",
      "id": "01a20277-9131-7ee2-acad-7e40ffa8f947",
      "kind": "scope",
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
        "register": [
          {
            "code": "U3-EXC-T13",
            "id": "01a1fbac-cd38-7aad-8cfa-30e76cf46cf0",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (T13)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a20277-8d10-702f-a975-4dd6469e7b1d",
            "job_size": 5,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 5,
            "title": "The tool platform \u2014 one governed call path, the call ledger, the schema and effect gates, and tenant-prefixed storage"
          }
        ]
      },
      "taken_at": "2026-11-22T08:55:18.027529+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-22T08:57:26.477641+00:00",
      "id": "01a20279-854d-79f6-a751-a34d51be7589",
      "kind": "scope",
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
        "register": [
          {
            "code": "U3-EXC-T13",
            "id": "01a1fbac-cd38-7aad-8cfa-30e76cf46cf0",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (T13)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a20277-8d10-702f-a975-4dd6469e7b1d",
            "job_size": 5,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 5,
            "title": "The tool platform \u2014 one governed call path, the call ledger, the schema and effect gates, and tenant-prefixed storage"
          },
          {
            "business_value": 8,
            "id": "01a20279-82b3-77d7-abf4-19c99e62330b",
            "job_size": 5,
            "kind": "deliverable",
            "parent_id": null,
            "position": 2.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 5,
            "title": "The tool platform \u2014 one governed call path, the call ledger, the schema and effect gates, and tenant-prefixed storage"
          }
        ]
      },
      "taken_at": "2026-11-22T08:57:26.477641+00:00",
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
      "created_at": "2026-11-26T21:49:23.172413+00:00",
      "id": "01a219d5-ab6e-7716-ac08-3234740e1ccd",
      "kind": "schedule",
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
        "register": [
          {
            "code": "B-T13-02",
            "id": "01a219d5-aabf-7c87-8992-4b890c3b9053",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Job bytes are tenant-separated without a compatibility shim"
          },
          {
            "code": "B-T13-01",
            "id": "01a219d5-a9fa-759e-ac83-e768a8e5348d",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A job reaches a backend only through one governed call path"
          },
          {
            "code": "D-T13-02",
            "id": "01a219d5-a963-7f60-8d96-85c73c6a5db8",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "Calls depend on a reviewed destination alias, never a caller-supplied URL"
          },
          {
            "code": "D-T13-01",
            "id": "01a219d5-a8c8-70b3-a43c-363679a3b261",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T13 needs T2's run and queue, and T14 needs T13"
          },
          {
            "code": "A-T13-03",
            "id": "01a219d5-a82f-7a9c-851c-7dca886c863d",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Directly written test rows exercise the same resolution the broker performs in production"
          },
          {
            "code": "A-T13-02",
            "id": "01a219d5-a796-79fc-a305-f2e3a626ed47",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "No destructive call is bound before its tenant configures approvals"
          },
          {
            "code": "A-T13-01",
            "id": "01a219d5-a6f9-74da-a21a-98988bb08d02",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The six \u00a712 proofs are sufficient evidence of the gate"
          },
          {
            "code": "I-T13-02",
            "id": "01a219d5-a65f-7c44-8f40-1776ff4e2548",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Catalogue actions and the estate import are deferred to T14"
          },
          {
            "code": "I-T13-01",
            "id": "01a219d5-a5c3-78fb-a348-6e9523021edd",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "No production tool_binding writer exists \u2014 bindings are written directly in the suite"
          },
          {
            "code": "R-T13-03",
            "id": "01a219d5-a52a-729f-a19f-aa82ceffd5c7",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The destination URL is process-global, so a parallel runner would produce false refusals"
          },
          {
            "code": "R-T13-02",
            "id": "01a219d5-a492-7a36-be72-4aca1c9b1fa5",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The storage round trip falls back to the in-memory store, so live MinIO stays unproved"
          },
          {
            "code": "R-T13-01",
            "id": "01a219d5-a3fd-7094-a075-1f1422bc9cec",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The approved destructive execution path is not proved \u2014 only its refusal is"
          },
          {
            "code": "",
            "id": "01a216d0-89c1-7fd6-b9fb-16ebb6009ef5",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "approved",
            "title": "Record the B2 benefit baseline"
          },
          {
            "code": "",
            "id": "01a202c2-e964-7130-8d6e-cb922e2667cb",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "approved",
            "title": "Accept the retrospective WBS deliverable for T13"
          },
          {
            "code": "U3-EXC-T13",
            "id": "01a1fbac-cd38-7aad-8cfa-30e76cf46cf0",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (T13)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a20277-8d10-702f-a975-4dd6469e7b1d",
            "job_size": 5,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "accepted",
            "time_criticality": 5,
            "title": "The tool platform \u2014 one governed call path, the call ledger, the schema and effect gates, and tenant-prefixed storage"
          },
          {
            "business_value": 8,
            "id": "01a20279-82b3-77d7-abf4-19c99e62330b",
            "job_size": 5,
            "kind": "deliverable",
            "parent_id": null,
            "position": 2.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 5,
            "title": "The tool platform \u2014 one governed call path, the call ledger, the schema and effect gates, and tenant-prefixed storage"
          }
        ]
      },
      "taken_at": "2026-11-26T21:49:23.172413+00:00",
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
      "created_at": "2026-11-26T21:49:23.499719+00:00",
      "id": "01a219d5-acb5-7027-bb83-35ac27872d14",
      "kind": "cost",
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
        "register": [
          {
            "code": "B-T13-02",
            "id": "01a219d5-aabf-7c87-8992-4b890c3b9053",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Job bytes are tenant-separated without a compatibility shim"
          },
          {
            "code": "B-T13-01",
            "id": "01a219d5-a9fa-759e-ac83-e768a8e5348d",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A job reaches a backend only through one governed call path"
          },
          {
            "code": "D-T13-02",
            "id": "01a219d5-a963-7f60-8d96-85c73c6a5db8",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "Calls depend on a reviewed destination alias, never a caller-supplied URL"
          },
          {
            "code": "D-T13-01",
            "id": "01a219d5-a8c8-70b3-a43c-363679a3b261",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "T13 needs T2's run and queue, and T14 needs T13"
          },
          {
            "code": "A-T13-03",
            "id": "01a219d5-a82f-7a9c-851c-7dca886c863d",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Directly written test rows exercise the same resolution the broker performs in production"
          },
          {
            "code": "A-T13-02",
            "id": "01a219d5-a796-79fc-a305-f2e3a626ed47",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "No destructive call is bound before its tenant configures approvals"
          },
          {
            "code": "A-T13-01",
            "id": "01a219d5-a6f9-74da-a21a-98988bb08d02",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The six \u00a712 proofs are sufficient evidence of the gate"
          },
          {
            "code": "I-T13-02",
            "id": "01a219d5-a65f-7c44-8f40-1776ff4e2548",
            "impact": null,
            "kind": "issue",
            "severity": "medium",
            "status": "open",
            "title": "Catalogue actions and the estate import are deferred to T14"
          },
          {
            "code": "I-T13-01",
            "id": "01a219d5-a5c3-78fb-a348-6e9523021edd",
            "impact": null,
            "kind": "issue",
            "severity": "high",
            "status": "open",
            "title": "No production tool_binding writer exists \u2014 bindings are written directly in the suite"
          },
          {
            "code": "R-T13-03",
            "id": "01a219d5-a52a-729f-a19f-aa82ceffd5c7",
            "impact": 2,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The destination URL is process-global, so a parallel runner would produce false refusals"
          },
          {
            "code": "R-T13-02",
            "id": "01a219d5-a492-7a36-be72-4aca1c9b1fa5",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The storage round trip falls back to the in-memory store, so live MinIO stays unproved"
          },
          {
            "code": "R-T13-01",
            "id": "01a219d5-a3fd-7094-a075-1f1422bc9cec",
            "impact": 4,
            "kind": "risk",
            "severity": "high",
            "status": "open",
            "title": "The approved destructive execution path is not proved \u2014 only its refusal is"
          },
          {
            "code": "",
            "id": "01a216d0-89c1-7fd6-b9fb-16ebb6009ef5",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "approved",
            "title": "Record the B2 benefit baseline"
          },
          {
            "code": "",
            "id": "01a202c2-e964-7130-8d6e-cb922e2667cb",
            "impact": null,
            "kind": "change",
            "severity": null,
            "status": "approved",
            "title": "Accept the retrospective WBS deliverable for T13"
          },
          {
            "code": "U3-EXC-T13",
            "id": "01a1fbac-cd38-7aad-8cfa-30e76cf46cf0",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (T13)"
          }
        ],
        "wbs": [
          {
            "business_value": 8,
            "id": "01a20277-8d10-702f-a975-4dd6469e7b1d",
            "job_size": 5,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 8,
            "status": "accepted",
            "time_criticality": 5,
            "title": "The tool platform \u2014 one governed call path, the call ledger, the schema and effect gates, and tenant-prefixed storage"
          },
          {
            "business_value": 8,
            "id": "01a20279-82b3-77d7-abf4-19c99e62330b",
            "job_size": 5,
            "kind": "deliverable",
            "parent_id": null,
            "position": 2.0,
            "risk_reduction": 8,
            "status": "open",
            "time_criticality": 5,
            "title": "The tool platform \u2014 one governed call path, the call ledger, the schema and effect gates, and tenant-prefixed storage"
          }
        ]
      },
      "taken_at": "2026-11-26T21:49:23.499719+00:00",
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
  "count": 4,
  "rows": [
    {
      "card_id": null,
      "created_at": "2026-11-22T08:55:18.027529+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a20277-92f1-7e49-9675-5bb2717081e8",
      "kind": "pm",
      "meta": {
        "baseline_id": "01a20277-9131-7ee2-acad-7e40ffa8f947",
        "description": "Approve the scope baseline"
      },
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-22T08:55:26.310036+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-22T08:57:26.477641+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a20279-8672-70c6-86b4-975cad387b8c",
      "kind": "pm",
      "meta": {
        "baseline_id": "01a20279-854d-79f6-a751-a34d51be7589",
        "description": "Approve the scope baseline"
      },
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-22T08:57:33.781641+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T21:49:23.172413+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a219d5-ab72-7656-b973-36f856877e30",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a219d5-ab6e-7716-ac08-3234740e1ccd",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T21:49:23.3334+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-26T21:49:23.499719+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a219d5-acb7-7daa-afd7-9077fb298da7",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a219d5-acb5-7027-bb83-35ac27872d14",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d37-730f-8f8a-2f50e5a0933d",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-26T21:49:23.653205+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

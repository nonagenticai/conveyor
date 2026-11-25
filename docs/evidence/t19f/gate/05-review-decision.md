# T19F live gate — the review approval

Action `approvals.decide` for approval 01a210c3-9007-75f4-927c-d1dc32f54c6b, by the distinct `conveyor-approver` seat.


```
== write: approvals.decide ==
{
  "approvals": 1,
  "card_id": null,
  "created_at": "2026-11-25T03:33:01.322357Z",
  "decisions": [],
  "description": "Review the T19F live-gate policy asset 01a210c3-7c34-7338-89f2-6c7a6e40bea7",
  "expires_at": null,
  "gate": "authoring.review",
  "id": "01a210c3-9007-75f4-927c-d1dc32f54c6b",
  "kind": "gate",
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "requested_by_label": "FR Test",
  "required_decisions": 1,
  "run_id": null,
  "status": "approved",
  "target": "01a210c3-7c34-7338-89f2-6c7a6e40bea7",
  "version": 2
}
== read-back: approvals.list ==
{
  "items": [
    {
      "approvals": 1,
      "card_id": null,
      "created_at": "2026-11-25T03:33:01.322357Z",
      "decisions": [
        {
          "approver": "conveyor-approver",
          "approver_id": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
          "decided_at": "2026-11-25T03:33:06.447890Z",
          "decided_by_kind": "service",
          "decision": "approve",
          "note": "Reviewed against docs/plans/T19F/architecture-design.md",
          "via": {
            "channel": "internal"
          }
        }
      ],
      "description": "Review the T19F live-gate policy asset 01a210c3-7c34-7338-89f2-6c7a6e40bea7",
      "expires_at": null,
      "gate": "authoring.review",
      "id": "01a210c3-9007-75f4-927c-d1dc32f54c6b",
      "kind": "gate",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "requested_by_label": "FR Test",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "target": "01a210c3-7c34-7338-89f2-6c7a6e40bea7",
      "version": 2
    }
  ]
}
```

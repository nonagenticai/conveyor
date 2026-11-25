# T19F live gate — the review gate

Action `approvals.request`, gate `authoring.review`.


```
== write: approvals.request ==
{
  "approvals": 0,
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
  "status": "pending",
  "target": "01a210c3-7c34-7338-89f2-6c7a6e40bea7",
  "version": 1
}
== read-back: approvals.list ==
{
  "items": [
    {
      "approvals": 0,
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
      "status": "pending",
      "target": "01a210c3-7c34-7338-89f2-6c7a6e40bea7",
      "version": 1
    }
  ]
}
```

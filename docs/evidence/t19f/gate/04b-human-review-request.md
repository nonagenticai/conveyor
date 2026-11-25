# T19F live gate — the human review gate (fresh)

The existing `authoring.review` gate `01a210c3-9007-75f4-927c-d1dc32f54c6b` for policy asset
`01a210c3-7c34-7338-89f2-6c7a6e40bea7` was approved at its quorum by the `conveyor-approver` seat
through the admin CLI, which records `decided_by_kind='service'`. `authoring.review` accepts only an
approval whose latest decision is `human`, and an approved gate refuses a second decision, so this
gate records a fresh review gate requested by the distinct `conveyor-approver` seat for FR Test to
decide in the signed-in console.

```
== write: approvals.request ==
{
  "approvals": 0,
  "card_id": null,
  "created_at": "2026-11-25T08:34:57.958362Z",
  "decisions": [],
  "description": "Human review of the T19F live-gate policy asset (fresh gate; the prior gate 01a210c3-9007 was service-decided)",
  "expires_at": null,
  "gate": "authoring.review",
  "id": "01a211d8-001f-7296-bbc1-ccb13835a3ef",
  "kind": "gate",
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "requested_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
  "requested_by_label": "conveyor-approver",
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
      "created_at": "2026-11-25T08:34:57.958362Z",
      "decisions": [],
      "description": "Human review of the T19F live-gate policy asset (fresh gate; the prior gate 01a210c3-9007 was service-decided)",
      "expires_at": null,
      "gate": "authoring.review",
      "id": "01a211d8-001f-7296-bbc1-ccb13835a3ef",
      "kind": "gate",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "requested_by_label": "conveyor-approver",
      "required_decisions": 1,
      "run_id": null,
      "status": "pending",
      "target": "01a210c3-7c34-7338-89f2-6c7a6e40bea7",
      "version": 1
    },
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

# T19F delivery contract — approval

Action `architecture.contracts_approve` for contract 01a210bd-f408-778a-b67e-9b6cf36ca62a, If-Match 1, approval 01a210be-08ae-77ff-8656-fd9da2828264, delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3. The approver is `conveyor-approver`, distinct from the requester.


```
== write: architecture.contracts_approve ==
{
  "approval_id": "01a210be-08ae-77ff-8656-fd9da2828264",
  "approved_at": "2026-11-25T03:27:03.799814Z",
  "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
  "body": {
    "approval": "The Architecture Board approves this contract under the standing delegation; the board decision is cited in the approval field of the contract row.",
    "commitments": [
      "Migration 0073 is live: the authoring output link, the body and cadence meeting-type keys, and no new index on meeting.",
      "A reviewed kind=policy asset publishes the next immutable meeting_type version; an in-flight meeting keeps its pinned version.",
      "A board sitting decides a real subject through T19's close_item_tx with per-seat via='meeting' rows; the direct board path is unchanged.",
      "cadences.materialise schedules the cadence's meeting once, serialised on the cadence row; a second call returns the first.",
      "The slice's acceptance is the live gate recorded in docs/plans/T19F/evidence.md."
    ],
    "compliance_method": "The Architecture Board's Phase-G compliance reviews read the delivered rows against the Statement of Architecture Work; each deviation is a change request or a recorded waiver.",
    "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3"
  },
  "id": "01a210bd-f408-778a-b67e-9b6cf36ca62a",
  "kind": "delivery_contract",
  "phase_code": "f",
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "requester_unknown": false,
  "status": "live",
  "superseded_by_id": null,
  "supersedes_id": null,
  "title": "Delivery contract \u2014 T19F forum bindings (2026-11-25)",
  "version": 2
}
== read-back: architecture.contracts_get ==
{
  "approval_id": "01a210be-08ae-77ff-8656-fd9da2828264",
  "approved_at": "2026-11-25T03:27:03.799814Z",
  "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
  "body": {
    "approval": "The Architecture Board approves this contract under the standing delegation; the board decision is cited in the approval field of the contract row.",
    "commitments": [
      "Migration 0073 is live: the authoring output link, the body and cadence meeting-type keys, and no new index on meeting.",
      "A reviewed kind=policy asset publishes the next immutable meeting_type version; an in-flight meeting keeps its pinned version.",
      "A board sitting decides a real subject through T19's close_item_tx with per-seat via='meeting' rows; the direct board path is unchanged.",
      "cadences.materialise schedules the cadence's meeting once, serialised on the cadence row; a second call returns the first.",
      "The slice's acceptance is the live gate recorded in docs/plans/T19F/evidence.md."
    ],
    "compliance_method": "The Architecture Board's Phase-G compliance reviews read the delivered rows against the Statement of Architecture Work; each deviation is a change request or a recorded waiver.",
    "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3"
  },
  "id": "01a210bd-f408-778a-b67e-9b6cf36ca62a",
  "kind": "delivery_contract",
  "phase_code": "f",
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "requester_unknown": false,
  "status": "live",
  "superseded_by_id": null,
  "supersedes_id": null,
  "title": "Delivery contract \u2014 T19F forum bindings (2026-11-25)",
  "version": 2
}
```

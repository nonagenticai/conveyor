# T19F architecture contract — approval

Action `architecture.contracts_approve` for contract 01a210bd-ba25-78c1-a363-3eea5aa71335, If-Match 1, approval 01a210bd-cdb2-7f2f-ac9a-c314fb1856af, delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3. The approver is `conveyor-approver`, distinct from the requester.


```
== write: architecture.contracts_approve ==
{
  "approval_id": "01a210bd-cdb2-7f2f-ac9a-c314fb1856af",
  "approved_at": "2026-11-25T03:26:48.737400Z",
  "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
  "body": {
    "acceptance_criteria": "The T19F plan's P1\u2013P5 scenarios, the live gate recorded in docs/plans/T19F/evidence.md, and the closure review in docs/plans/T19F/closure-review.md that defers project closure to the tranche-6 review.",
    "assignment": "Build T19F \u2014 the forum bindings \u2014 to the plan's P1\u2013P5: a meeting_type version is written only by the reviewed authoring route, a board subject is decided in a sitting with via='meeting' rows, and a cadence row materialises its own meeting exactly once.",
    "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
    "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the programme manager owns B7's measurement; the SRO accepts the tranche-6 evidence.",
    "scope": "Migration 0073; the authoring publish hook and meetings.type_list/type_get; architecture.board_sit_open/board_sit_decide; cadences.materialise; the mock-UI and real-app surfaces; the programme record and its evidence."
  },
  "id": "01a210bd-ba25-78c1-a363-3eea5aa71335",
  "kind": "statement_of_architecture_work",
  "phase_code": "a",
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "requester_unknown": false,
  "status": "live",
  "superseded_by_id": null,
  "supersedes_id": null,
  "title": "Statement of Architecture Work \u2014 T19F forum bindings (2026-11-25)",
  "version": 2
}
== read-back: architecture.contracts_get ==
{
  "approval_id": "01a210bd-cdb2-7f2f-ac9a-c314fb1856af",
  "approved_at": "2026-11-25T03:26:48.737400Z",
  "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
  "body": {
    "acceptance_criteria": "The T19F plan's P1\u2013P5 scenarios, the live gate recorded in docs/plans/T19F/evidence.md, and the closure review in docs/plans/T19F/closure-review.md that defers project closure to the tranche-6 review.",
    "assignment": "Build T19F \u2014 the forum bindings \u2014 to the plan's P1\u2013P5: a meeting_type version is written only by the reviewed authoring route, a board subject is decided in a sitting with via='meeting' rows, and a cadence row materialises its own meeting exactly once.",
    "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
    "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the programme manager owns B7's measurement; the SRO accepts the tranche-6 evidence.",
    "scope": "Migration 0073; the authoring publish hook and meetings.type_list/type_get; architecture.board_sit_open/board_sit_decide; cadences.materialise; the mock-UI and real-app surfaces; the programme record and its evidence."
  },
  "id": "01a210bd-ba25-78c1-a363-3eea5aa71335",
  "kind": "statement_of_architecture_work",
  "phase_code": "a",
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "requester_unknown": false,
  "status": "live",
  "superseded_by_id": null,
  "supersedes_id": null,
  "title": "Statement of Architecture Work \u2014 T19F forum bindings (2026-11-25)",
  "version": 2
}
```

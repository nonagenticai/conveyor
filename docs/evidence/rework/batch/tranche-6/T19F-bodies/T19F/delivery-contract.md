# Delivery Contract — Decision-forum governance (follow-on)

## Commitments

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 3,
  "rows": [
    {
      "approval_id": "01a210be-08ae-77ff-8656-fd9da2828264",
      "approved_at": "2026-11-25T03:27:03.799814+00:00",
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
      "created_at": "2026-11-25T03:26:53.805668+00:00",
      "id": "01a210bd-f408-778a-b67e-9b6cf36ca62a",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery contract \u2014 T19F forum bindings (2026-11-25)",
      "updated_at": "2026-11-28T07:33:55.258024+00:00",
      "version": 3
    },
    {
      "approval_id": "01a22113-304e-7972-95b7-cfb7297df018",
      "approved_at": "2026-11-28T07:33:55.541724+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": null,
      "created_at": "2026-11-25T03:38:24.653635+00:00",
      "id": "01a210c8-7ea8-7dc9-8104-700ecfe2905b",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "T19F live gate \u2014 the sitting subject",
      "updated_at": "2026-11-28T07:33:55.541724+00:00",
      "version": 2
    },
    {
      "approval_id": null,
      "approved_at": null,
      "approved_by": null,
      "body": null,
      "created_at": "2026-11-25T08:38:21.802439+00:00",
      "id": "01a211db-1c05-78c9-8607-fea6634193e8",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "T19F live gate \u2014 the sitting subject",
      "updated_at": "2026-11-25T08:41:16.158173+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Compliance method

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 3,
  "rows": [
    {
      "approval_id": "01a210be-08ae-77ff-8656-fd9da2828264",
      "approved_at": "2026-11-25T03:27:03.799814+00:00",
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
      "created_at": "2026-11-25T03:26:53.805668+00:00",
      "id": "01a210bd-f408-778a-b67e-9b6cf36ca62a",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery contract \u2014 T19F forum bindings (2026-11-25)",
      "updated_at": "2026-11-28T07:33:55.258024+00:00",
      "version": 3
    },
    {
      "approval_id": "01a22113-304e-7972-95b7-cfb7297df018",
      "approved_at": "2026-11-28T07:33:55.541724+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": null,
      "created_at": "2026-11-25T03:38:24.653635+00:00",
      "id": "01a210c8-7ea8-7dc9-8104-700ecfe2905b",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "T19F live gate \u2014 the sitting subject",
      "updated_at": "2026-11-28T07:33:55.541724+00:00",
      "version": 2
    },
    {
      "approval_id": null,
      "approved_at": null,
      "approved_by": null,
      "body": null,
      "created_at": "2026-11-25T08:38:21.802439+00:00",
      "id": "01a211db-1c05-78c9-8607-fea6634193e8",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "T19F live gate \u2014 the sitting subject",
      "updated_at": "2026-11-25T08:41:16.158173+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Approval

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_at": "2026-11-28T07:33:55.541724+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a22113-304e-7972-95b7-cfb7297df018",
      "kind": "contract_approval",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "approved",
      "title": "approval 01a22113-304e-7972-95b7-cfb7297df018 \u2014 approved"
    }
  ],
  "source": "computed:contract_approval"
}
```

```json
{
  "category": "delivery-contract",
  "count": 3,
  "rows": [
    {
      "approval_id": "01a210be-08ae-77ff-8656-fd9da2828264",
      "approved_at": "2026-11-25T03:27:03.799814+00:00",
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
      "created_at": "2026-11-25T03:26:53.805668+00:00",
      "id": "01a210bd-f408-778a-b67e-9b6cf36ca62a",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery contract \u2014 T19F forum bindings (2026-11-25)",
      "updated_at": "2026-11-28T07:33:55.258024+00:00",
      "version": 3
    },
    {
      "approval_id": "01a22113-304e-7972-95b7-cfb7297df018",
      "approved_at": "2026-11-28T07:33:55.541724+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": null,
      "created_at": "2026-11-25T03:38:24.653635+00:00",
      "id": "01a210c8-7ea8-7dc9-8104-700ecfe2905b",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "T19F live gate \u2014 the sitting subject",
      "updated_at": "2026-11-28T07:33:55.541724+00:00",
      "version": 2
    },
    {
      "approval_id": null,
      "approved_at": null,
      "approved_by": null,
      "body": null,
      "created_at": "2026-11-25T08:38:21.802439+00:00",
      "id": "01a211db-1c05-78c9-8607-fea6634193e8",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "T19F live gate \u2014 the sitting subject",
      "updated_at": "2026-11-25T08:41:16.158173+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

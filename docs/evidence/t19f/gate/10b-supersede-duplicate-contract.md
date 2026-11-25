# T19F live gate — superseding the duplicate sitting subject

The first resumed `--apply` (2026-11-25, after the human review) re-ran the sitting's
`architecture.contracts_request` before the resumed-sitting guard existed and created a second
draft delivery contract with the same title (`01a211db-1c05-78c9-8607-fea6634193e8`). The original
sitting subject (`01a210c8-7ea8-7dc9-8104-700ecfe2905b`) and the two live governance contracts are
untouched. This action supersedes the accidental duplicate so the record is clean; it is a
delivery-contract draft, never a sitting subject and never approved.

```
== write: architecture.contracts_supersede ==
{
  "approval_id": null,
  "approved_at": null,
  "approved_by": null,
  "body": null,
  "id": "01a211db-1c05-78c9-8607-fea6634193e8",
  "kind": "delivery_contract",
  "phase_code": "f",
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "requester_unknown": false,
  "status": "superseded",
  "superseded_by_id": null,
  "supersedes_id": null,
  "title": "T19F live gate \u2014 the sitting subject",
  "version": 2
}
== read-back: architecture.contracts_get ==
{
  "approval_id": null,
  "approved_at": null,
  "approved_by": null,
  "body": null,
  "id": "01a211db-1c05-78c9-8607-fea6634193e8",
  "kind": "delivery_contract",
  "phase_code": "f",
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "requester_unknown": false,
  "status": "superseded",
  "superseded_by_id": null,
  "supersedes_id": null,
  "title": "T19F live gate \u2014 the sitting subject",
  "version": 2
}
```

# Delivery Contract — MCP tasks — a long-running project asks for input

## Commitments

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a222a6-4228-7504-a579-4dc4aca55a62",
      "field": "commitments",
      "kind": "contract_field",
      "title": "commitments: The delivery commits to the five requirements the Requirements Catalog states, each with its owning check in `acceptance/programme/S06c.toml`, and to the boundary the Statement of Architecture Work draws: no new table, no new service, no push channel and no progress percentage. The retrospective nature of the contract is declared rather than concealed.",
      "value": "The delivery commits to the five requirements the Requirements Catalog states, each with its owning check in `acceptance/programme/S06c.toml`, and to the boundary the Statement of Architecture Work draws: no new table, no new service, no push channel and no progress percentage. The retrospective nature of the contract is declared rather than concealed."
    }
  ],
  "source": "computed:contract_field"
}
```

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a222a6-42b7-75cb-b5eb-a77ab8641532",
      "approved_at": "2026-11-28T14:54:11.26996+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_at": "2026-11-28",
        "approval": "The contract is approved under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The approving role is the Architecture Board and the accepting role is the SRO; the session that authored this pack is neither, and the record carries the approver's own decision rather than an inference from its silence.",
        "commitments": "The delivery commits to the five requirements the Requirements Catalog states, each with its owning check in `acceptance/programme/S06c.toml`, and to the boundary the Statement of Architecture Work draws: no new table, no new service, no push channel and no progress percentage. The retrospective nature of the contract is declared rather than concealed.",
        "compliance": "Compliance is demonstrated by running the manifest at the pinned snapshot and reading the receipt, including the parity suite that proves the task surface and the HTTP action agree. A check that cannot run is recorded as deferred; a check whose filter selects zero tests is treated as red, because a zero-match libtest exits 0.",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "`acceptance/programme/S06c.toml` is the manifest of record. Its gate checks name the baseline, the charter, the closure outputs, the contract, the review, the closure gate, the waivers, the PI artefacts, the WBS deliverable, the demo credits, the lessons row, the product check, the artefact standard and the rework package count.",
        "retrospective": "This contract is written after delivery. It records the terms the project was built to, reconstructed from the slice plan, the shipped tests and the snapshot, and it does not claim those terms were agreed before the build began."
      },
      "created_at": "2026-11-28T14:54:10.982363+00:00",
      "id": "01a222a6-4228-7504-a579-4dc4aca55a62",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery Contract \u2014 S06c MCP Tasks (Phase F, retrospective)",
      "updated_at": "2026-11-28T14:54:11.26996+00:00",
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
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a222a6-4228-7504-a579-4dc4aca55a62",
      "field": "compliance",
      "kind": "contract_field",
      "title": "compliance: Compliance is demonstrated by running the manifest at the pinned snapshot and reading the receipt, including the parity suite that proves the task surface and the HTTP action agree. A check that cannot run is recorded as deferred; a check whose filter selects zero tests is treated as red, because a zero-match libtest exits 0.",
      "value": "Compliance is demonstrated by running the manifest at the pinned snapshot and reading the receipt, including the parity suite that proves the task surface and the HTTP action agree. A check that cannot run is recorded as deferred; a check whose filter selects zero tests is treated as red, because a zero-match libtest exits 0."
    }
  ],
  "source": "computed:contract_field"
}
```

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a222a6-42b7-75cb-b5eb-a77ab8641532",
      "approved_at": "2026-11-28T14:54:11.26996+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_at": "2026-11-28",
        "approval": "The contract is approved under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The approving role is the Architecture Board and the accepting role is the SRO; the session that authored this pack is neither, and the record carries the approver's own decision rather than an inference from its silence.",
        "commitments": "The delivery commits to the five requirements the Requirements Catalog states, each with its owning check in `acceptance/programme/S06c.toml`, and to the boundary the Statement of Architecture Work draws: no new table, no new service, no push channel and no progress percentage. The retrospective nature of the contract is declared rather than concealed.",
        "compliance": "Compliance is demonstrated by running the manifest at the pinned snapshot and reading the receipt, including the parity suite that proves the task surface and the HTTP action agree. A check that cannot run is recorded as deferred; a check whose filter selects zero tests is treated as red, because a zero-match libtest exits 0.",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "`acceptance/programme/S06c.toml` is the manifest of record. Its gate checks name the baseline, the charter, the closure outputs, the contract, the review, the closure gate, the waivers, the PI artefacts, the WBS deliverable, the demo credits, the lessons row, the product check, the artefact standard and the rework package count.",
        "retrospective": "This contract is written after delivery. It records the terms the project was built to, reconstructed from the slice plan, the shipped tests and the snapshot, and it does not claim those terms were agreed before the build began."
      },
      "created_at": "2026-11-28T14:54:10.982363+00:00",
      "id": "01a222a6-4228-7504-a579-4dc4aca55a62",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery Contract \u2014 S06c MCP Tasks (Phase F, retrospective)",
      "updated_at": "2026-11-28T14:54:11.26996+00:00",
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
      "approved_at": "2026-11-28T14:54:11.26996+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a222a6-42b7-75cb-b5eb-a77ab8641532",
      "kind": "contract_approval",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "approved",
      "title": "approval 01a222a6-42b7-75cb-b5eb-a77ab8641532 \u2014 approved"
    }
  ],
  "source": "computed:contract_approval"
}
```

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a222a6-42b7-75cb-b5eb-a77ab8641532",
      "approved_at": "2026-11-28T14:54:11.26996+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_at": "2026-11-28",
        "approval": "The contract is approved under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The approving role is the Architecture Board and the accepting role is the SRO; the session that authored this pack is neither, and the record carries the approver's own decision rather than an inference from its silence.",
        "commitments": "The delivery commits to the five requirements the Requirements Catalog states, each with its owning check in `acceptance/programme/S06c.toml`, and to the boundary the Statement of Architecture Work draws: no new table, no new service, no push channel and no progress percentage. The retrospective nature of the contract is declared rather than concealed.",
        "compliance": "Compliance is demonstrated by running the manifest at the pinned snapshot and reading the receipt, including the parity suite that proves the task surface and the HTTP action agree. A check that cannot run is recorded as deferred; a check whose filter selects zero tests is treated as red, because a zero-match libtest exits 0.",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "`acceptance/programme/S06c.toml` is the manifest of record. Its gate checks name the baseline, the charter, the closure outputs, the contract, the review, the closure gate, the waivers, the PI artefacts, the WBS deliverable, the demo credits, the lessons row, the product check, the artefact standard and the rework package count.",
        "retrospective": "This contract is written after delivery. It records the terms the project was built to, reconstructed from the slice plan, the shipped tests and the snapshot, and it does not claim those terms were agreed before the build began."
      },
      "created_at": "2026-11-28T14:54:10.982363+00:00",
      "id": "01a222a6-4228-7504-a579-4dc4aca55a62",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery Contract \u2014 S06c MCP Tasks (Phase F, retrospective)",
      "updated_at": "2026-11-28T14:54:11.26996+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

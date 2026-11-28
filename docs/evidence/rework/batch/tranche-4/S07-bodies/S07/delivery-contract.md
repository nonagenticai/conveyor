# Delivery Contract — Requirements, catalogues, revisions, quality scoring, search

## Commitments

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a219d9-4891-7eb2-ad11-095d4ddd9051",
      "field": "commitments",
      "kind": "contract_field",
      "title": "commitments: S07 commits to carrying RM's requirement surface onto S07's governed_asset, revision, catalogue_entry and trace_link: one lifecycle for requirements, principles and ADRs instead of RM's 57 domain tables, one revision per mutation with a working Rust compare, read-only quality scoring on immutable revisions, FTS plus trigram search measured against an agreed baseline, and traceability in both directions with zero cross-tenant rows.",
      "value": "S07 commits to carrying RM's requirement surface onto S07's governed_asset, revision, catalogue_entry and trace_link: one lifecycle for requirements, principles and ADRs instead of RM's 57 domain tables, one revision per mutation with a working Rust compare, read-only quality scoring on immutable revisions, FTS plus trigram search measured against an agreed baseline, and traceability in both directions with zero cross-tenant rows."
    }
  ],
  "source": "computed:contract_field"
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
      "contract_id": "01a219d9-4891-7eb2-ad11-095d4ddd9051",
      "field": "compliance",
      "kind": "contract_field",
      "title": "compliance: S07 is assessed clause by clause against SPEC \u00a78 S7 and plan \u00a77. The seven properties are: (a) three revisions with max(seq)=3 and a compare that shows the status change; (b) an idempotent seed returning {0,0,N}; (c) both GIN indexes used under enable_seqscan=off with idx_scan rising on a 50k-row load; (d) relevance where B meets \u00a75.6's recall@5 \u2265 0.75, MRR \u2265 0.60 and B \u2265 C, with pgvector shipped only when the loss exceeds 0.05 or C is unreachable and B fails A + 0.20; (e) traceability in both directions with zero cross-tenant rows; (f) quality scoring read-only with version and status untouched; (g) no orphan over RM's 28 modules. Each property is carried by a named target (search_index, s07_assets, s07_requests, s07_store, s07_seed, s07_governance, search_eval) or the EXPLAIN block. The snapshot records no Phase-G review and no run of these properties; this rework supplies the seven artefacts, this contract, the Phase-G review and the closure review, records the level consistent, and re-closes on the 15 P0 checks of acceptance/programme/S07.toml. The close gate is decided by an approver other than its requester under the standing delegation.",
      "value": "S07 is assessed clause by clause against SPEC \u00a78 S7 and plan \u00a77. The seven properties are: (a) three revisions with max(seq)=3 and a compare that shows the status change; (b) an idempotent seed returning {0,0,N}; (c) both GIN indexes used under enable_seqscan=off with idx_scan rising on a 50k-row load; (d) relevance where B meets \u00a75.6's recall@5 \u2265 0.75, MRR \u2265 0.60 and B \u2265 C, with pgvector shipped only when the loss exceeds 0.05 or C is unreachable and B fails A + 0.20; (e) traceability in both directions with zero cross-tenant rows; (f) quality scoring read-only with version and status untouched; (g) no orphan over RM's 28 modules. Each property is carried by a named target (search_index, s07_assets, s07_requests, s07_store, s07_seed, s07_governance, search_eval) or the EXPLAIN block. The snapshot records no Phase-G review and no run of these properties; this rework supplies the seven artefacts, this contract, the Phase-G review and the closure review, records the level consistent, and re-closes on the 15 P0 checks of acceptance/programme/S07.toml. The close gate is decided by an approver other than its requester under the standing delegation."
    }
  ],
  "source": "computed:contract_field"
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
      "approved_at": "2026-11-26T21:53:20.310358+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a219d9-4924-7a6c-bc55-997d9e42ee70",
      "kind": "contract_approval",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "approved",
      "title": "approval 01a219d9-4924-7a6c-bc55-997d9e42ee70 \u2014 approved"
    }
  ],
  "source": "computed:contract_approval"
}
```

# Compliance Review — Requirements, catalogues, revisions, quality scoring, search

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "contract_id": "01a219d9-4891-7eb2-ad11-095d4ddd9051",
      "created_at": "2026-11-26T21:53:27.547242+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S07.toml",
        "content_check": "scripts/rework-content-check.py --project S07 --closes true",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "evidence_page": "01a1e687-4d95-70a0-8cf9-a624a30dcb36",
        "manifest": "acceptance/programme/S07.toml",
        "plan": "docs/plans/slices/S07-requirements-search.md",
        "sha": "60b290a3c661b22572603c4f007906561ee7f44b",
        "snapshot": "/tmp/conv526-w1-snap/data"
      },
      "findings": "This retrospective Phase-G review records the level consistent for S07 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The project row 01a1d039-0d2c-7322-b2f9-4dd041153894 is closed at version 7 with the row last updated 2026-11-21T01:06:16Z, inside tranche 4 (01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce), authorised 2026-11-24T17:04:34Z by the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9. At the snapshot the project has no charter, Statement of Architecture Work, delivery contract, Phase A\u2013D artefact set, Phase-G review, closure review, baseline row or linked issue; its only publication is the evidence page 01a1e687-4d95-70a0-8cf9-a624a30dcb36, published 2026-11-16T22:43:11Z. The project meta records measured_status built, plan_status planned, expected_outputs project-brief/evidence, closure_outputs empty and no issues. The plan \u00a77 functional gate has no recorded run and \u00a75.6's relevance targets have no recorded numbers. This content supplies the charter, the Statement of Architecture Work, the seven A\u2013D artefacts, the delivery contract, this review, the project brief, the evidence statement and the closure review; the level is consistent because the record set now matches the snapshot and nothing beyond the record is claimed. A level of compliant, conformant or fully_conformant would require a recorded gate and eval run, which the snapshot does not carry; non_conformant would require a waiver the programme forbids. The plan's traps remain constraints: no Rust-side filtering, SQL predicates with per-arm limits, the generated tsvector with a literal english config, trigram and full-text as distinct indexes, no quality scoring inside the mutation transaction, and the unversioned action path. The gate run and the relevance eval are carried as open verification items in this rework.",
      "id": "01a219d9-6600-7a4f-bd43-a6d8d4a8fd7b",
      "level": "consistent",
      "outcome": null,
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "reviewed_at": "2026-11-26T21:53:27.547242+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T21:53:27.547242+00:00",
      "version": 1
    }
  ],
  "source": "architecture.json#/compliance_reviews"
}
```

## Findings

The evidence for each finding, against the contract.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "contract_id": "01a219d9-4891-7eb2-ad11-095d4ddd9051",
      "created_at": "2026-11-26T21:53:27.547242+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S07.toml",
        "content_check": "scripts/rework-content-check.py --project S07 --closes true",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "evidence_page": "01a1e687-4d95-70a0-8cf9-a624a30dcb36",
        "manifest": "acceptance/programme/S07.toml",
        "plan": "docs/plans/slices/S07-requirements-search.md",
        "sha": "60b290a3c661b22572603c4f007906561ee7f44b",
        "snapshot": "/tmp/conv526-w1-snap/data"
      },
      "findings": "This retrospective Phase-G review records the level consistent for S07 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The project row 01a1d039-0d2c-7322-b2f9-4dd041153894 is closed at version 7 with the row last updated 2026-11-21T01:06:16Z, inside tranche 4 (01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce), authorised 2026-11-24T17:04:34Z by the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9. At the snapshot the project has no charter, Statement of Architecture Work, delivery contract, Phase A\u2013D artefact set, Phase-G review, closure review, baseline row or linked issue; its only publication is the evidence page 01a1e687-4d95-70a0-8cf9-a624a30dcb36, published 2026-11-16T22:43:11Z. The project meta records measured_status built, plan_status planned, expected_outputs project-brief/evidence, closure_outputs empty and no issues. The plan \u00a77 functional gate has no recorded run and \u00a75.6's relevance targets have no recorded numbers. This content supplies the charter, the Statement of Architecture Work, the seven A\u2013D artefacts, the delivery contract, this review, the project brief, the evidence statement and the closure review; the level is consistent because the record set now matches the snapshot and nothing beyond the record is claimed. A level of compliant, conformant or fully_conformant would require a recorded gate and eval run, which the snapshot does not carry; non_conformant would require a waiver the programme forbids. The plan's traps remain constraints: no Rust-side filtering, SQL predicates with per-arm limits, the generated tsvector with a literal english config, trigram and full-text as distinct indexes, no quality scoring inside the mutation transaction, and the unversioned action path. The gate run and the relevance eval are carried as open verification items in this rework.",
      "id": "01a219d9-6600-7a4f-bd43-a6d8d4a8fd7b",
      "level": "consistent",
      "outcome": null,
      "project_id": "01a1d039-0d2c-7322-b2f9-4dd041153894",
      "reviewed_at": "2026-11-26T21:53:27.547242+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T21:53:27.547242+00:00",
      "version": 1
    }
  ],
  "source": "architecture.json#/compliance_reviews"
}
```

## Waivers

Each accepted deviation with its owner, reason, cost, risk and reassessment date.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "kind": "waiver_status",
      "title": "None recorded \u2014 0 waivers for this project",
      "waivers": 0
    }
  ],
  "source": "computed:waiver_status"
}
```

## Decision

The outcome and the conditions that attach to it.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "conditions": "none recorded",
      "decision": "consistent",
      "findings": "This retrospective Phase-G review records the level consistent for S07 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The project row 01a1d039-0d2c-7322-b2f9-4dd041153894 is closed at version 7 with the row last updated 2026-11-21T01:06:16Z, inside tranche 4 (01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce), authorised 2026-11-24T17:04:34Z by the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9. At the snapshot the project has no charter, Statement of Architecture Work, delivery contract, Phase A\u2013D artefact set, Phase-G review, closure review, baseline row or linked issue; its only publication is the evidence page 01a1e687-4d95-70a0-8cf9-a624a30dcb36, published 2026-11-16T22:43:11Z. The project meta records measured_status built, plan_status planned, expected_outputs project-brief/evidence, closure_outputs empty and no issues. The plan \u00a77 functional gate has no recorded run and \u00a75.6's relevance targets have no recorded numbers. This content supplies the charter, the Statement of Architecture Work, the seven A\u2013D artefacts, the delivery contract, this review, the project brief, the evidence statement and the closure review; the level is consistent because the record set now matches the snapshot and nothing beyond the record is claimed. A level of compliant, conformant or fully_conformant would require a recorded gate and eval run, which the snapshot does not carry; non_conformant would require a waiver the programme forbids. The plan's traps remain constraints: no Rust-side filtering, SQL predicates with per-arm limits, the generated tsvector with a literal english config, trigram and full-text as distinct indexes, no quality scoring inside the mutation transaction, and the unversioned action path. The gate run and the relevance eval are carried as open verification items in this rework.",
      "id": "01a219d9-6600-7a4f-bd43-a6d8d4a8fd7b",
      "kind": "governing_review",
      "level": "consistent",
      "phase_h_outcome": null,
      "reviewed_at": "2026-11-26T21:53:27.547242+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a219d9-6600-7a4f-bd43-a6d8d4a8fd7b: consistent governs; phase-H outcome not recorded (a phase-G decision is its level)"
    }
  ],
  "source": "computed:governing_review"
}
```

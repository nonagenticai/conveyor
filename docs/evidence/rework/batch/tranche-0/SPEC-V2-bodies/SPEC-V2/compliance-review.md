# Compliance Review — SPEC V2 — the organisational operating model

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 2,
  "rows": [
    {
      "contract_id": "01a1ff42-7bd0-7d9e-883a-1059368cd356",
      "created_at": "2026-11-21T17:59:44.513906+00:00",
      "evidence": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "result": "green",
        "sha": "d7293f96082dbf2f99f53bb8fd765834dddf6858"
      },
      "findings": [
        {
          "code": "closed_without_closure_review",
          "note": "The project was closed 2026-11-17 with no closure review and no Phase-G compliance review. This review, recorded against the live delivery contract, and the closure review published 2026-11-21 close that gap; the close gate that follows is decided by an approver other than its requester."
        },
        {
          "code": "closure_outputs_undeclared",
          "note": "The project declared no closure outputs. It now declares `evidence` and `closure-review`, and both are published before the re-close; the manifest's closure_outputs check reads the declaration, not the intent."
        },
        {
          "code": "amendment_after_review",
          "note": "D53 was amended on 2026-11-19 \u2014 the PKO store was removed and RDF/OWL/Turtle became a strata knowledge-pack import \u2014 after the spec's 2026-11-11 reviews. The amendment is recorded in \u00a75.6, \u00a715.1 D53 and \u00a715.2 #13, and re-scopes T16; it is a recorded change, not a silent one."
        },
        {
          "code": "proposed_decisions_open",
          "note": "D61 (executor seam; external runtime deferred), D62 (MSP programme definition and benefits realization) and D63 (TOGAF ADM as an artefact-typed capability) are proposed, not accepted; \u00a715.2 #21 stays open until D61 is accepted. Compliance is assessed against the document as written, including those statuses."
        }
      ],
      "id": "01a1ff43-a1bc-758d-89dc-c97d0fb0265a",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "reviewed_at": "2026-11-21T17:59:44.513906+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-21T17:59:44.513906+00:00",
      "version": 1
    },
    {
      "contract_id": "01a217a9-dc13-7ff5-8a9d-aa8bbdf0ac23",
      "created_at": "2026-11-26T11:42:23.129786+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/SPEC-V2.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "delivery_contract": "01a1ff42-7bd0-7d9e-883a-1059368cd356",
        "live_review": "01a1ff43-a1bc-758d-89dc-c97d0fb0265a",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "rework_note": "docs/evidence/rework/SPEC-V2.md",
        "scope_baseline": "01a1ff42-ba64-7670-b694-99bb4b1354e5",
        "sha": "0131780f275f59e19aa9b08e30deb4a5cab8b2b5"
      },
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live review `01a1ff43-a1bc-758d-89dc-c97d0fb0265a` is compliant against the delivery contract `01a1ff42-7bd0-7d9e-883a-1059368cd356` and cites the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`, and no active waiver exists. The scope baseline `01a1ff42-ba64-7670-b694-99bb4b1354e5` is approved by `01a1e8a7-8b23-76a9-9114-aafb95f7a545` and taken by `01a1e607-17d9-77d3-98d3-a43a87c4633d`; the seven A\u2013D artefacts are approved rows; and `python3 scripts/verify-project.py --project SPEC-V2 --snapshot <fresh>` is green with 11 checks and 0 failures at `d7293f96`, with no P0/P1 entry on the strict audit. The finding is that the definition set is published and its governance record is sound, while two honest limits remain: D61, D62 and D63 are proposed rather than accepted, and D64 to D66 were ruled on 2026-11-22, after the 2026-11-21 record this review rests on. The review therefore assesses the plan as it stands on 2026-11-26 and does not back-date the later rulings. It also records that who closed the project on 2026-11-17 and why is not recoverable, and that the contemporaneous WSJF inputs and B1's baseline value are not recoverable; neither gap is repaired by assertion.",
      "id": "01a217a9-f19f-73e3-be6e-8becabd123ee",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "reviewed_at": "2026-11-26T11:42:23.129786+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T11:42:23.129786+00:00",
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
  "count": 2,
  "rows": [
    {
      "contract_id": "01a1ff42-7bd0-7d9e-883a-1059368cd356",
      "created_at": "2026-11-21T17:59:44.513906+00:00",
      "evidence": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "result": "green",
        "sha": "d7293f96082dbf2f99f53bb8fd765834dddf6858"
      },
      "findings": [
        {
          "code": "closed_without_closure_review",
          "note": "The project was closed 2026-11-17 with no closure review and no Phase-G compliance review. This review, recorded against the live delivery contract, and the closure review published 2026-11-21 close that gap; the close gate that follows is decided by an approver other than its requester."
        },
        {
          "code": "closure_outputs_undeclared",
          "note": "The project declared no closure outputs. It now declares `evidence` and `closure-review`, and both are published before the re-close; the manifest's closure_outputs check reads the declaration, not the intent."
        },
        {
          "code": "amendment_after_review",
          "note": "D53 was amended on 2026-11-19 \u2014 the PKO store was removed and RDF/OWL/Turtle became a strata knowledge-pack import \u2014 after the spec's 2026-11-11 reviews. The amendment is recorded in \u00a75.6, \u00a715.1 D53 and \u00a715.2 #13, and re-scopes T16; it is a recorded change, not a silent one."
        },
        {
          "code": "proposed_decisions_open",
          "note": "D61 (executor seam; external runtime deferred), D62 (MSP programme definition and benefits realization) and D63 (TOGAF ADM as an artefact-typed capability) are proposed, not accepted; \u00a715.2 #21 stays open until D61 is accepted. Compliance is assessed against the document as written, including those statuses."
        }
      ],
      "id": "01a1ff43-a1bc-758d-89dc-c97d0fb0265a",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "reviewed_at": "2026-11-21T17:59:44.513906+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-21T17:59:44.513906+00:00",
      "version": 1
    },
    {
      "contract_id": "01a217a9-dc13-7ff5-8a9d-aa8bbdf0ac23",
      "created_at": "2026-11-26T11:42:23.129786+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/SPEC-V2.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "delivery_contract": "01a1ff42-7bd0-7d9e-883a-1059368cd356",
        "live_review": "01a1ff43-a1bc-758d-89dc-c97d0fb0265a",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "rework_note": "docs/evidence/rework/SPEC-V2.md",
        "scope_baseline": "01a1ff42-ba64-7670-b694-99bb4b1354e5",
        "sha": "0131780f275f59e19aa9b08e30deb4a5cab8b2b5"
      },
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live review `01a1ff43-a1bc-758d-89dc-c97d0fb0265a` is compliant against the delivery contract `01a1ff42-7bd0-7d9e-883a-1059368cd356` and cites the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`, and no active waiver exists. The scope baseline `01a1ff42-ba64-7670-b694-99bb4b1354e5` is approved by `01a1e8a7-8b23-76a9-9114-aafb95f7a545` and taken by `01a1e607-17d9-77d3-98d3-a43a87c4633d`; the seven A\u2013D artefacts are approved rows; and `python3 scripts/verify-project.py --project SPEC-V2 --snapshot <fresh>` is green with 11 checks and 0 failures at `d7293f96`, with no P0/P1 entry on the strict audit. The finding is that the definition set is published and its governance record is sound, while two honest limits remain: D61, D62 and D63 are proposed rather than accepted, and D64 to D66 were ruled on 2026-11-22, after the 2026-11-21 record this review rests on. The review therefore assesses the plan as it stands on 2026-11-26 and does not back-date the later rulings. It also records that who closed the project on 2026-11-17 and why is not recoverable, and that the contemporaneous WSJF inputs and B1's baseline value are not recoverable; neither gap is repaired by assertion.",
      "id": "01a217a9-f19f-73e3-be6e-8becabd123ee",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "reviewed_at": "2026-11-26T11:42:23.129786+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T11:42:23.129786+00:00",
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
      "conditions": null,
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live review `01a1ff43-a1bc-758d-89dc-c97d0fb0265a` is compliant against the delivery contract `01a1ff42-7bd0-7d9e-883a-1059368cd356` and cites the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`, and no active waiver exists. The scope baseline `01a1ff42-ba64-7670-b694-99bb4b1354e5` is approved by `01a1e8a7-8b23-76a9-9114-aafb95f7a545` and taken by `01a1e607-17d9-77d3-98d3-a43a87c4633d`; the seven A\u2013D artefacts are approved rows; and `python3 scripts/verify-project.py --project SPEC-V2 --snapshot <fresh>` is green with 11 checks and 0 failures at `d7293f96`, with no P0/P1 entry on the strict audit. The finding is that the definition set is published and its governance record is sound, while two honest limits remain: D61, D62 and D63 are proposed rather than accepted, and D64 to D66 were ruled on 2026-11-22, after the 2026-11-21 record this review rests on. The review therefore assesses the plan as it stands on 2026-11-26 and does not back-date the later rulings. It also records that who closed the project on 2026-11-17 and why is not recoverable, and that the contemporaneous WSJF inputs and B1's baseline value are not recoverable; neither gap is repaired by assertion.",
      "id": "01a217a9-f19f-73e3-be6e-8becabd123ee",
      "kind": "governing_review",
      "level": "compliant",
      "outcome": null,
      "reviewed_at": "2026-11-26T11:42:23.129786+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a217a9-f19f-73e3-be6e-8becabd123ee: compliant / no outcome"
    }
  ],
  "source": "computed:governing_review"
}
```

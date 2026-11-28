# Compliance Review — Estate toolkits

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "contract_id": "01a219dd-5fc8-7e02-8e87-af833eb030b4",
      "created_at": "2026-11-26T21:57:55.006653+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T14.toml",
        "authorising_review": "01a216d0-d634-738e-985a-00a85cfd48d9",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/T14.toml",
        "pi_cadence": "01a202a1-dbb9-7e49-b64a-cf97c3fab095",
        "project": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
        "refinement_cadence": "01a202a2-2705-7392-a2d1-03ca2b3a174e",
        "register_item": "01a1fbad-8c5f-7086-b569-28e8f0cf6fac",
        "sha": "6f2d9e2e5a774131b045ab151f2d9b56b8c3f3e3",
        "tranche": "01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce"
      },
      "findings": "This retrospective Phase-G review records the level consistent for T14 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, and it is a new verdict written by the 2026-11-24 rework, not a copy of a prior one. The old review set is empty: the facts return no review row for the project and the snapshot's architecture set carries no T14 compliance review; the only prior governance judgement is the accepted P0 register item U3-EXC-T14 01a1fbad-8c5f-7086-b569-28e8f0cf6fac, which records a close in an unauthorised tranche before the tranche review as a retrospective exception, after which the project row stands active at version 11, updated 2026-11-24T07:41:22Z. The record now supports consistent: this pack supplies the seven A\u2013D artefacts, the Statement of Architecture Work and the delivery contract; the WSJF inputs 10/9/10/8 are the ones the tranche-4 refinement cadence 01a202a2-2705-7392-a2d1-03ca2b3a174e records for objective O1+O2; the tranche-4 PI cadence 01a202a1-dbb9-7e49-b64a-cf97c3fab095 lists T14 in its capacity; and tranche 4 01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce was authorised at 2026-11-24T17:04:34Z by review 01a216d0-d634-738e-985a-00a85cfd48d9. It does not support compliant or a stronger level: the snapshot carries no probe receipt and no live-call receipt for any toolkit, no scientific_processor scale-up and scale-back receipt, no zero-use report and no backend-outage unbind run; the plan fragment docs/SPEC_V2.md#12 is status draft and there is no standalone T14 slice plan; the project holds no scope baseline, no live contract before this rework and no published outputs; organisation.json carries no toolkit rows, so publication cannot be counted from the record; issue 561 (research and Lean toolkits, first wave) remains open while 558 and 560 are closed without receipts; and the only delivery evidence in the neighbourhood is T13's, whose compliance review records the tool platform passing at lane head 582e2dc6 against acceptance/programme/T13.toml. It equally does not support non_conformant: nothing in the record shows a refused gate, and the missing receipts are recorded as the project's open verification item rather than an unmet clause that would need a waiver the programme forbids. The four gate clauses therefore stay open and must execute on the live estate before any closure; this pack does not close the project.",
      "id": "01a219dd-7ac5-7777-a3ed-94afa7c3a5fe",
      "level": "consistent",
      "outcome": null,
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "reviewed_at": "2026-11-26T21:57:55.006653+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T21:57:55.006653+00:00",
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
      "contract_id": "01a219dd-5fc8-7e02-8e87-af833eb030b4",
      "created_at": "2026-11-26T21:57:55.006653+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T14.toml",
        "authorising_review": "01a216d0-d634-738e-985a-00a85cfd48d9",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/T14.toml",
        "pi_cadence": "01a202a1-dbb9-7e49-b64a-cf97c3fab095",
        "project": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
        "refinement_cadence": "01a202a2-2705-7392-a2d1-03ca2b3a174e",
        "register_item": "01a1fbad-8c5f-7086-b569-28e8f0cf6fac",
        "sha": "6f2d9e2e5a774131b045ab151f2d9b56b8c3f3e3",
        "tranche": "01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce"
      },
      "findings": "This retrospective Phase-G review records the level consistent for T14 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, and it is a new verdict written by the 2026-11-24 rework, not a copy of a prior one. The old review set is empty: the facts return no review row for the project and the snapshot's architecture set carries no T14 compliance review; the only prior governance judgement is the accepted P0 register item U3-EXC-T14 01a1fbad-8c5f-7086-b569-28e8f0cf6fac, which records a close in an unauthorised tranche before the tranche review as a retrospective exception, after which the project row stands active at version 11, updated 2026-11-24T07:41:22Z. The record now supports consistent: this pack supplies the seven A\u2013D artefacts, the Statement of Architecture Work and the delivery contract; the WSJF inputs 10/9/10/8 are the ones the tranche-4 refinement cadence 01a202a2-2705-7392-a2d1-03ca2b3a174e records for objective O1+O2; the tranche-4 PI cadence 01a202a1-dbb9-7e49-b64a-cf97c3fab095 lists T14 in its capacity; and tranche 4 01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce was authorised at 2026-11-24T17:04:34Z by review 01a216d0-d634-738e-985a-00a85cfd48d9. It does not support compliant or a stronger level: the snapshot carries no probe receipt and no live-call receipt for any toolkit, no scientific_processor scale-up and scale-back receipt, no zero-use report and no backend-outage unbind run; the plan fragment docs/SPEC_V2.md#12 is status draft and there is no standalone T14 slice plan; the project holds no scope baseline, no live contract before this rework and no published outputs; organisation.json carries no toolkit rows, so publication cannot be counted from the record; issue 561 (research and Lean toolkits, first wave) remains open while 558 and 560 are closed without receipts; and the only delivery evidence in the neighbourhood is T13's, whose compliance review records the tool platform passing at lane head 582e2dc6 against acceptance/programme/T13.toml. It equally does not support non_conformant: nothing in the record shows a refused gate, and the missing receipts are recorded as the project's open verification item rather than an unmet clause that would need a waiver the programme forbids. The four gate clauses therefore stay open and must execute on the live estate before any closure; this pack does not close the project.",
      "id": "01a219dd-7ac5-7777-a3ed-94afa7c3a5fe",
      "level": "consistent",
      "outcome": null,
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "reviewed_at": "2026-11-26T21:57:55.006653+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T21:57:55.006653+00:00",
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
      "findings": "This retrospective Phase-G review records the level consistent for T14 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, and it is a new verdict written by the 2026-11-24 rework, not a copy of a prior one. The old review set is empty: the facts return no review row for the project and the snapshot's architecture set carries no T14 compliance review; the only prior governance judgement is the accepted P0 register item U3-EXC-T14 01a1fbad-8c5f-7086-b569-28e8f0cf6fac, which records a close in an unauthorised tranche before the tranche review as a retrospective exception, after which the project row stands active at version 11, updated 2026-11-24T07:41:22Z. The record now supports consistent: this pack supplies the seven A\u2013D artefacts, the Statement of Architecture Work and the delivery contract; the WSJF inputs 10/9/10/8 are the ones the tranche-4 refinement cadence 01a202a2-2705-7392-a2d1-03ca2b3a174e records for objective O1+O2; the tranche-4 PI cadence 01a202a1-dbb9-7e49-b64a-cf97c3fab095 lists T14 in its capacity; and tranche 4 01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce was authorised at 2026-11-24T17:04:34Z by review 01a216d0-d634-738e-985a-00a85cfd48d9. It does not support compliant or a stronger level: the snapshot carries no probe receipt and no live-call receipt for any toolkit, no scientific_processor scale-up and scale-back receipt, no zero-use report and no backend-outage unbind run; the plan fragment docs/SPEC_V2.md#12 is status draft and there is no standalone T14 slice plan; the project holds no scope baseline, no live contract before this rework and no published outputs; organisation.json carries no toolkit rows, so publication cannot be counted from the record; issue 561 (research and Lean toolkits, first wave) remains open while 558 and 560 are closed without receipts; and the only delivery evidence in the neighbourhood is T13's, whose compliance review records the tool platform passing at lane head 582e2dc6 against acceptance/programme/T13.toml. It equally does not support non_conformant: nothing in the record shows a refused gate, and the missing receipts are recorded as the project's open verification item rather than an unmet clause that would need a waiver the programme forbids. The four gate clauses therefore stay open and must execute on the live estate before any closure; this pack does not close the project.",
      "id": "01a219dd-7ac5-7777-a3ed-94afa7c3a5fe",
      "kind": "governing_review",
      "level": "consistent",
      "phase_h_outcome": null,
      "reviewed_at": "2026-11-26T21:57:55.006653+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a219dd-7ac5-7777-a3ed-94afa7c3a5fe: consistent governs; phase-H outcome not recorded (a phase-G decision is its level)"
    }
  ],
  "source": "computed:governing_review"
}
```

# Compliance Review — SOP, RDF/OWL and process-document ingestion

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "contract_id": "01a219de-0a42-7ed6-88d1-fabfcb82775a",
      "created_at": "2026-11-26T21:58:38.78043+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T16.toml",
        "artefacts": [
          "01a216d1-00c2-7697-a69d-95f17c13ded9",
          "01a216d1-022c-798f-9c70-d80ae5fc1372",
          "01a216d1-03a4-75d7-b179-f0779eceb977",
          "01a216d1-0508-7333-9b49-bbdec7931317",
          "01a216d1-0667-7d27-b2b2-33c668b77857",
          "01a216d1-07ca-7336-9400-5de4096994c6",
          "01a216d1-092e-79a1-bdd6-66d886074986"
        ],
        "authorisation_review": "01a216d0-d634-738e-985a-00a85cfd48d9",
        "baseline": "01a216d0-f529-7bcb-81a7-d429d008d638",
        "baseline_approver": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "documents": [
          "01a216d0-7bc9-7449-9670-cc77c6bba3e5",
          "01a216d0-fb60-7265-b693-5ef6dfa1bc09",
          "01a216d0-fc70-7260-8ded-e230423d633e",
          "01a216d0-fd86-77d7-9477-8a543eef105e",
          "01a216d1-0a60-715a-b496-30b112f7ef87",
          "01a216d1-0b79-7709-a256-6330f794660b",
          "01a216d1-0c8e-727e-9843-6a6e243c16b2",
          "01a216d1-0d97-7707-8896-a2841281d410",
          "01a216d1-0ea0-7520-83b4-7008836231cf",
          "01a216d1-0fb4-7327-811c-4897bd3d1c1d",
          "01a216d1-10ce-738a-8987-8ca9327054a1",
          "01a216d1-1709-73fb-94fd-9090f58dee60",
          "01a216d1-1810-70b8-842b-cb0ebe8ade64"
        ],
        "issues": [
          85,
          448
        ],
        "manifest": "acceptance/programme/T16.toml",
        "pi_cadence": "01a202a1-dbb9-7e49-b64a-cf97c3fab095",
        "programme": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
        "project": "01a1d039-0d38-76fb-9394-b75934d16fcd",
        "refinement_cadence": "01a202a2-2705-7392-a2d1-03ca2b3a174e",
        "reopen_decision": "document/01a1fb9f-f19e-749b-9218-a4428b2e3dac",
        "sha": "df4a511718af8c52f04ab30f7f4a66c5ff4b1b06",
        "snapshot": "/tmp/conv526-w1-snap/data",
        "tranche": "01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce",
        "wsjf": {
          "business_value": 10,
          "job_size": 8,
          "risk_reduction": 7,
          "time_criticality": 8
        }
      },
      "findings": "This retrospective Phase-G review records the level consistent for T16 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. There is no earlier Phase-G review row: the project's review set is empty, and the old review is the published closure review 01a216d0-fd86-77d7-9477-8a543eef105e ('T16 \u2014 Closure review (record completed, 2026-11-23)'), which completed the record at 2026-11-24T16:58:23Z, before tranche 4 was authorised at 17:04:34Z by the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9. The reopen decision document/01a1fb9f-f19e-749b-9218-a4428b2e3dac records that the project had closed without a charter, baseline, delivery contract, Phase-G review or published closure review, and this pack supplies the missing charter, SoAW and contract retrospectively. The seven approved Phase A\u2013D artefacts, the scope baseline 01a216d0-f529-7bcb-81a7-d429d008d638, the published project brief 01a216d0-7bc9-7449-9670-cc77c6bba3e5, the evidence 01a216d0-fc70-7260-8ded-e230423d633e and eleven other published documents support a coherent record, which is why the level is consistent rather than irrelevant or non_conformant; it is not compliant because the record was reconstructed after an out-of-sequence close and no contemporaneous review, contract or in-sequence acceptance exists. Remaining limits: the T16 gate in acceptance/programme/T16.toml is not re-executed by this retrospective; the build level rests on meta.measured_status: built and the approved artefacts; the earlier close predates the tranche authorisation and needs the tranche-4 review to decide continue; and the project remains active, with no closure or closure review authored here.",
      "id": "01a219de-25c1-7eb7-adc2-fac9877f505f",
      "level": "consistent",
      "outcome": null,
      "project_id": "01a1d039-0d38-76fb-9394-b75934d16fcd",
      "reviewed_at": "2026-11-26T21:58:38.78043+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T21:58:38.78043+00:00",
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
      "contract_id": "01a219de-0a42-7ed6-88d1-fabfcb82775a",
      "created_at": "2026-11-26T21:58:38.78043+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T16.toml",
        "artefacts": [
          "01a216d1-00c2-7697-a69d-95f17c13ded9",
          "01a216d1-022c-798f-9c70-d80ae5fc1372",
          "01a216d1-03a4-75d7-b179-f0779eceb977",
          "01a216d1-0508-7333-9b49-bbdec7931317",
          "01a216d1-0667-7d27-b2b2-33c668b77857",
          "01a216d1-07ca-7336-9400-5de4096994c6",
          "01a216d1-092e-79a1-bdd6-66d886074986"
        ],
        "authorisation_review": "01a216d0-d634-738e-985a-00a85cfd48d9",
        "baseline": "01a216d0-f529-7bcb-81a7-d429d008d638",
        "baseline_approver": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "documents": [
          "01a216d0-7bc9-7449-9670-cc77c6bba3e5",
          "01a216d0-fb60-7265-b693-5ef6dfa1bc09",
          "01a216d0-fc70-7260-8ded-e230423d633e",
          "01a216d0-fd86-77d7-9477-8a543eef105e",
          "01a216d1-0a60-715a-b496-30b112f7ef87",
          "01a216d1-0b79-7709-a256-6330f794660b",
          "01a216d1-0c8e-727e-9843-6a6e243c16b2",
          "01a216d1-0d97-7707-8896-a2841281d410",
          "01a216d1-0ea0-7520-83b4-7008836231cf",
          "01a216d1-0fb4-7327-811c-4897bd3d1c1d",
          "01a216d1-10ce-738a-8987-8ca9327054a1",
          "01a216d1-1709-73fb-94fd-9090f58dee60",
          "01a216d1-1810-70b8-842b-cb0ebe8ade64"
        ],
        "issues": [
          85,
          448
        ],
        "manifest": "acceptance/programme/T16.toml",
        "pi_cadence": "01a202a1-dbb9-7e49-b64a-cf97c3fab095",
        "programme": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
        "project": "01a1d039-0d38-76fb-9394-b75934d16fcd",
        "refinement_cadence": "01a202a2-2705-7392-a2d1-03ca2b3a174e",
        "reopen_decision": "document/01a1fb9f-f19e-749b-9218-a4428b2e3dac",
        "sha": "df4a511718af8c52f04ab30f7f4a66c5ff4b1b06",
        "snapshot": "/tmp/conv526-w1-snap/data",
        "tranche": "01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce",
        "wsjf": {
          "business_value": 10,
          "job_size": 8,
          "risk_reduction": 7,
          "time_criticality": 8
        }
      },
      "findings": "This retrospective Phase-G review records the level consistent for T16 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. There is no earlier Phase-G review row: the project's review set is empty, and the old review is the published closure review 01a216d0-fd86-77d7-9477-8a543eef105e ('T16 \u2014 Closure review (record completed, 2026-11-23)'), which completed the record at 2026-11-24T16:58:23Z, before tranche 4 was authorised at 17:04:34Z by the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9. The reopen decision document/01a1fb9f-f19e-749b-9218-a4428b2e3dac records that the project had closed without a charter, baseline, delivery contract, Phase-G review or published closure review, and this pack supplies the missing charter, SoAW and contract retrospectively. The seven approved Phase A\u2013D artefacts, the scope baseline 01a216d0-f529-7bcb-81a7-d429d008d638, the published project brief 01a216d0-7bc9-7449-9670-cc77c6bba3e5, the evidence 01a216d0-fc70-7260-8ded-e230423d633e and eleven other published documents support a coherent record, which is why the level is consistent rather than irrelevant or non_conformant; it is not compliant because the record was reconstructed after an out-of-sequence close and no contemporaneous review, contract or in-sequence acceptance exists. Remaining limits: the T16 gate in acceptance/programme/T16.toml is not re-executed by this retrospective; the build level rests on meta.measured_status: built and the approved artefacts; the earlier close predates the tranche authorisation and needs the tranche-4 review to decide continue; and the project remains active, with no closure or closure review authored here.",
      "id": "01a219de-25c1-7eb7-adc2-fac9877f505f",
      "level": "consistent",
      "outcome": null,
      "project_id": "01a1d039-0d38-76fb-9394-b75934d16fcd",
      "reviewed_at": "2026-11-26T21:58:38.78043+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T21:58:38.78043+00:00",
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
      "findings": "This retrospective Phase-G review records the level consistent for T16 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. There is no earlier Phase-G review row: the project's review set is empty, and the old review is the published closure review 01a216d0-fd86-77d7-9477-8a543eef105e ('T16 \u2014 Closure review (record completed, 2026-11-23)'), which completed the record at 2026-11-24T16:58:23Z, before tranche 4 was authorised at 17:04:34Z by the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9. The reopen decision document/01a1fb9f-f19e-749b-9218-a4428b2e3dac records that the project had closed without a charter, baseline, delivery contract, Phase-G review or published closure review, and this pack supplies the missing charter, SoAW and contract retrospectively. The seven approved Phase A\u2013D artefacts, the scope baseline 01a216d0-f529-7bcb-81a7-d429d008d638, the published project brief 01a216d0-7bc9-7449-9670-cc77c6bba3e5, the evidence 01a216d0-fc70-7260-8ded-e230423d633e and eleven other published documents support a coherent record, which is why the level is consistent rather than irrelevant or non_conformant; it is not compliant because the record was reconstructed after an out-of-sequence close and no contemporaneous review, contract or in-sequence acceptance exists. Remaining limits: the T16 gate in acceptance/programme/T16.toml is not re-executed by this retrospective; the build level rests on meta.measured_status: built and the approved artefacts; the earlier close predates the tranche authorisation and needs the tranche-4 review to decide continue; and the project remains active, with no closure or closure review authored here.",
      "id": "01a219de-25c1-7eb7-adc2-fac9877f505f",
      "kind": "governing_review",
      "level": "consistent",
      "phase_h_outcome": null,
      "reviewed_at": "2026-11-26T21:58:38.78043+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a219de-25c1-7eb7-adc2-fac9877f505f: consistent governs; phase-H outcome not recorded (a phase-G decision is its level)"
    }
  ],
  "source": "computed:governing_review"
}
```

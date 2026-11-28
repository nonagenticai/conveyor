# Compliance Review — Documents, repositories, secrets, workers UI, reports/metrics, billing display

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "contract_id": "01a219da-0ec0-706f-9949-eb2bc03f2fdf",
      "created_at": "2026-11-26T21:54:19.206993+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S09.toml",
        "audit_pack": "docs/evidence/rework/S09.json",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/S09.toml",
        "published_evidence": "01a1e687-7682-753d-b60e-1f00243b13a3",
        "sha": "60b290a3c661b22572603c4f007906561ee7f44b"
      },
      "findings": "This retrospective Phase-G review records level consistent for S09 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The snapshot carries no earlier review rows for S09 at all: reviews is empty, so there is no prior Phase-G or compliance review to supersede, and the only governance review touching the slice is the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9 that authorised tranche 4 at 2026-11-24T17:04:34Z. The pre-rework audit pack docs/evidence/rework/S09.json records verdict red: no project baseline, no charter, no live board-approved delivery contract, no Phase-G review, no closure outputs and no closure gate, while the project is closed (version 10, updated 2026-11-24T07:41:22Z) with meta.measured_status built and plan_status planned. This rework supplies the missing lifecycle record \u2014 charter, Statement of Architecture Work, delivery contract, the seven Phase A\u2013D artefacts, this review, the project brief and the evidence output \u2014 and re-closes the project retrospectively at 2026-11-24 with the closure and closure review in this pack. Remaining limits are explicit: the snapshot records no run of the plan section 7 properties a to g; the deployed MinIO credentials and bucket policy, and the live strata_ingest path, are unproven; documents.ingest is the one unregistered action of 27 and is a follow-up with a live target; the plan frontmatter still reads planned while the measured status is built; the nine linked issues are recorded closed but their acceptance is not re-verified here; and the strata_rag path contract (/query versus /api/query) remains a stated risk. The level is consistent rather than compliant because the governance record is now complete but the functional gate is not evidenced; a non-conformant verdict would require a waiver the programme forbids.",
      "id": "01a219da-2fcd-78b3-9d0b-4159efee9baa",
      "level": "consistent",
      "outcome": null,
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "reviewed_at": "2026-11-26T21:54:19.206993+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T21:54:19.206993+00:00",
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
      "contract_id": "01a219da-0ec0-706f-9949-eb2bc03f2fdf",
      "created_at": "2026-11-26T21:54:19.206993+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S09.toml",
        "audit_pack": "docs/evidence/rework/S09.json",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/S09.toml",
        "published_evidence": "01a1e687-7682-753d-b60e-1f00243b13a3",
        "sha": "60b290a3c661b22572603c4f007906561ee7f44b"
      },
      "findings": "This retrospective Phase-G review records level consistent for S09 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The snapshot carries no earlier review rows for S09 at all: reviews is empty, so there is no prior Phase-G or compliance review to supersede, and the only governance review touching the slice is the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9 that authorised tranche 4 at 2026-11-24T17:04:34Z. The pre-rework audit pack docs/evidence/rework/S09.json records verdict red: no project baseline, no charter, no live board-approved delivery contract, no Phase-G review, no closure outputs and no closure gate, while the project is closed (version 10, updated 2026-11-24T07:41:22Z) with meta.measured_status built and plan_status planned. This rework supplies the missing lifecycle record \u2014 charter, Statement of Architecture Work, delivery contract, the seven Phase A\u2013D artefacts, this review, the project brief and the evidence output \u2014 and re-closes the project retrospectively at 2026-11-24 with the closure and closure review in this pack. Remaining limits are explicit: the snapshot records no run of the plan section 7 properties a to g; the deployed MinIO credentials and bucket policy, and the live strata_ingest path, are unproven; documents.ingest is the one unregistered action of 27 and is a follow-up with a live target; the plan frontmatter still reads planned while the measured status is built; the nine linked issues are recorded closed but their acceptance is not re-verified here; and the strata_rag path contract (/query versus /api/query) remains a stated risk. The level is consistent rather than compliant because the governance record is now complete but the functional gate is not evidenced; a non-conformant verdict would require a waiver the programme forbids.",
      "id": "01a219da-2fcd-78b3-9d0b-4159efee9baa",
      "level": "consistent",
      "outcome": null,
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "reviewed_at": "2026-11-26T21:54:19.206993+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T21:54:19.206993+00:00",
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
      "findings": "This retrospective Phase-G review records level consistent for S09 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The snapshot carries no earlier review rows for S09 at all: reviews is empty, so there is no prior Phase-G or compliance review to supersede, and the only governance review touching the slice is the tranche-3 review 01a216d0-d634-738e-985a-00a85cfd48d9 that authorised tranche 4 at 2026-11-24T17:04:34Z. The pre-rework audit pack docs/evidence/rework/S09.json records verdict red: no project baseline, no charter, no live board-approved delivery contract, no Phase-G review, no closure outputs and no closure gate, while the project is closed (version 10, updated 2026-11-24T07:41:22Z) with meta.measured_status built and plan_status planned. This rework supplies the missing lifecycle record \u2014 charter, Statement of Architecture Work, delivery contract, the seven Phase A\u2013D artefacts, this review, the project brief and the evidence output \u2014 and re-closes the project retrospectively at 2026-11-24 with the closure and closure review in this pack. Remaining limits are explicit: the snapshot records no run of the plan section 7 properties a to g; the deployed MinIO credentials and bucket policy, and the live strata_ingest path, are unproven; documents.ingest is the one unregistered action of 27 and is a follow-up with a live target; the plan frontmatter still reads planned while the measured status is built; the nine linked issues are recorded closed but their acceptance is not re-verified here; and the strata_rag path contract (/query versus /api/query) remains a stated risk. The level is consistent rather than compliant because the governance record is now complete but the functional gate is not evidenced; a non-conformant verdict would require a waiver the programme forbids.",
      "id": "01a219da-2fcd-78b3-9d0b-4159efee9baa",
      "kind": "governing_review",
      "level": "consistent",
      "phase_h_outcome": null,
      "reviewed_at": "2026-11-26T21:54:19.206993+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a219da-2fcd-78b3-9d0b-4159efee9baa: consistent governs; phase-H outcome not recorded (a phase-G decision is its level)"
    }
  ],
  "source": "computed:governing_review"
}
```

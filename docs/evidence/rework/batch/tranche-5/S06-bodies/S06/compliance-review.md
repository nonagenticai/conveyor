# Compliance Review — Control plane — claims, runs, approvals, andon, comms and notifications

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "contract_id": "01a21a39-c074-7cac-a6d0-ca18a59d30b7",
      "created_at": "2026-11-26T23:38:48.874985+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S06.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/S06.toml",
        "sha": "7e3883b05e167fca6c10cedb351b2bac90a0b75f"
      },
      "findings": "This retrospective Phase-G review records the level non_conformant for S06 under the standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3, because the acceptance is not evidenced. At the 2026-11-27 snapshot the project row is status active at version 10 (track v1, ADM phase G, delivery plateau, tranche 5) with meta.measured_status built, meta.plan_status planned, meta.viability rework, expected_outputs project-brief/evidence and closure_outputs empty; issues #4 (open) and #604 (closed) are linked. The project has no charter asset, no Statement of Architecture Work, no delivery contract, no Phase A\u2013D artefact set, no Phase-G compliance review and no gate receipt; its only document is the published evidence page 01a1e687-0674-7371-902f-58c7376b113f (2026-11-16T22:42:53Z). The seven plan \u00a77 properties (golden path, dual control, retry, andon, delivery retry, no timers, human versus agent) have no recorded run, and acceptance/programme/S06.toml is not evidenced. The findings are named honestly rather than rounded up: issue #4 records that the shared decision boundary and the Telegram sender identity were still open, and issue #604 closed with the multicast approval test timing out at 10 s under full-suite load, which is a closed issue and not a green re-run. This content supplies the seven A\u2013D artefacts, the charter, the Statement of Architecture Work, the contract, this review and the expected outputs; the level stays non_conformant until the gate runs and their receipts are read, and a conformant level would require evidence the snapshot does not contain.",
      "id": "01a21a39-dab0-733c-a041-9cf36da4703b",
      "level": "non_conformant",
      "outcome": null,
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "reviewed_at": "2026-11-26T23:38:48.874985+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T23:38:48.874985+00:00",
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
      "contract_id": "01a21a39-c074-7cac-a6d0-ca18a59d30b7",
      "created_at": "2026-11-26T23:38:48.874985+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S06.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/S06.toml",
        "sha": "7e3883b05e167fca6c10cedb351b2bac90a0b75f"
      },
      "findings": "This retrospective Phase-G review records the level non_conformant for S06 under the standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3, because the acceptance is not evidenced. At the 2026-11-27 snapshot the project row is status active at version 10 (track v1, ADM phase G, delivery plateau, tranche 5) with meta.measured_status built, meta.plan_status planned, meta.viability rework, expected_outputs project-brief/evidence and closure_outputs empty; issues #4 (open) and #604 (closed) are linked. The project has no charter asset, no Statement of Architecture Work, no delivery contract, no Phase A\u2013D artefact set, no Phase-G compliance review and no gate receipt; its only document is the published evidence page 01a1e687-0674-7371-902f-58c7376b113f (2026-11-16T22:42:53Z). The seven plan \u00a77 properties (golden path, dual control, retry, andon, delivery retry, no timers, human versus agent) have no recorded run, and acceptance/programme/S06.toml is not evidenced. The findings are named honestly rather than rounded up: issue #4 records that the shared decision boundary and the Telegram sender identity were still open, and issue #604 closed with the multicast approval test timing out at 10 s under full-suite load, which is a closed issue and not a green re-run. This content supplies the seven A\u2013D artefacts, the charter, the Statement of Architecture Work, the contract, this review and the expected outputs; the level stays non_conformant until the gate runs and their receipts are read, and a conformant level would require evidence the snapshot does not contain.",
      "id": "01a21a39-dab0-733c-a041-9cf36da4703b",
      "level": "non_conformant",
      "outcome": null,
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "reviewed_at": "2026-11-26T23:38:48.874985+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T23:38:48.874985+00:00",
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
      "decision": "non_conformant",
      "findings": "This retrospective Phase-G review records the level non_conformant for S06 under the standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3, because the acceptance is not evidenced. At the 2026-11-27 snapshot the project row is status active at version 10 (track v1, ADM phase G, delivery plateau, tranche 5) with meta.measured_status built, meta.plan_status planned, meta.viability rework, expected_outputs project-brief/evidence and closure_outputs empty; issues #4 (open) and #604 (closed) are linked. The project has no charter asset, no Statement of Architecture Work, no delivery contract, no Phase A\u2013D artefact set, no Phase-G compliance review and no gate receipt; its only document is the published evidence page 01a1e687-0674-7371-902f-58c7376b113f (2026-11-16T22:42:53Z). The seven plan \u00a77 properties (golden path, dual control, retry, andon, delivery retry, no timers, human versus agent) have no recorded run, and acceptance/programme/S06.toml is not evidenced. The findings are named honestly rather than rounded up: issue #4 records that the shared decision boundary and the Telegram sender identity were still open, and issue #604 closed with the multicast approval test timing out at 10 s under full-suite load, which is a closed issue and not a green re-run. This content supplies the seven A\u2013D artefacts, the charter, the Statement of Architecture Work, the contract, this review and the expected outputs; the level stays non_conformant until the gate runs and their receipts are read, and a conformant level would require evidence the snapshot does not contain.",
      "id": "01a21a39-dab0-733c-a041-9cf36da4703b",
      "kind": "governing_review",
      "level": "non_conformant",
      "phase_h_outcome": null,
      "reviewed_at": "2026-11-26T23:38:48.874985+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a21a39-dab0-733c-a041-9cf36da4703b: non_conformant governs; phase-H outcome not recorded (a phase-G decision is its level)"
    }
  ],
  "source": "computed:governing_review"
}
```

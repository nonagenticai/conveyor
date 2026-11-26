# Change Control — Publish the SPEC_V2 studio design pack, live

## Change requests

The record the section derives from, or an explicit absence.

```json
{
  "category": "change-control",
  "count": 2,
  "rows": [
    {
      "baseline_ref": "01a1ff3e-51c4-75b1-9db1-a2f729dd10b8",
      "body": "Change request: rebuild DESIGN-PUBLISH's record under the owner's 2026-11-21 scope change (#526). The missing MSP, TOGAF and SAFe artefacts are provided retroactively, the record is taken through the full lifecycle, and the project is re-closed through the gates.\n\nImpact: scope - the project's record and declared outputs are rebuilt and republished. Schedule - no change to the delivery tranche; the work runs in its rework batch. Cost - no separate cost baseline; the rework runs inside the existing programme tranche. Quality - every owned finding is executed or waived by a named owner, and the acceptance manifest is re-run at the closing head.\n\nDecision: accepted by the programme owner's standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, recorded in #526.",
      "code": "U5-CHG-DESIGN-PUBLISH",
      "created_at": "2026-11-22T06:44:53.339123+00:00",
      "id": "01a20200-26e0-71ea-b81a-e5da0e806aea",
      "impact": null,
      "kind": "change",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          {
            "kind": "issue",
            "number": 526
          },
          {
            "id": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
            "kind": "delegation"
          }
        ]
      },
      "meta": {
        "change_request": "the retrospective rework of DESIGN-PUBLISH under #526",
        "decision": {
          "decided_at": "2026-11-21",
          "decided_by": "the programme owner's standing delegation",
          "decision": "accepted",
          "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
          "record": "#526"
        },
        "impact": {
          "cost": "no separate cost baseline; the rework runs inside the existing programme tranche",
          "quality": "every owned finding executed or waived by a named owner; manifest re-run at the closing head",
          "schedule": "no change to the delivery tranche; the work runs in its rework batch",
          "scope": "the record and declared outputs are rebuilt and republished"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "accepted",
      "title": "Retrospective MSP/TOGAF/SAFe rework of DESIGN-PUBLISH under the #526 scope change",
      "updated_at": "2026-11-22T06:44:53.339123+00:00",
      "version": 1
    },
    {
      "baseline_ref": "01a1ff3e-51c4-75b1-9db1-a2f729dd10b8",
      "body": "Replayed from docs/evidence/record/journal.jsonl (natural key change:Record the B4 benefit baseline); the recorded request carried no rationale. Impact: {\"scope\": \"B4 public collaboration baseline and groups\"}",
      "code": "",
      "created_at": "2026-11-26T07:44:56.171049+00:00",
      "id": "01a216d0-8d71-7621-9022-4568316d2001",
      "impact": null,
      "kind": "change",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          {
            "id": "01a1e9df-1592-79a5-839e-037d8fe4251b",
            "kind": "benefit"
          }
        ]
      },
      "meta": {
        "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
        "impact": {
          "scope": "B4 public collaboration baseline and groups"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "Record the B4 benefit baseline",
      "updated_at": "2026-11-26T07:44:56.567518+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Impact

The record the section derives from, or an explicit absence.

```json
{
  "category": "change-control",
  "count": 2,
  "rows": [
    {
      "baseline_ref": "01a1ff3e-51c4-75b1-9db1-a2f729dd10b8",
      "body": "Change request: rebuild DESIGN-PUBLISH's record under the owner's 2026-11-21 scope change (#526). The missing MSP, TOGAF and SAFe artefacts are provided retroactively, the record is taken through the full lifecycle, and the project is re-closed through the gates.\n\nImpact: scope - the project's record and declared outputs are rebuilt and republished. Schedule - no change to the delivery tranche; the work runs in its rework batch. Cost - no separate cost baseline; the rework runs inside the existing programme tranche. Quality - every owned finding is executed or waived by a named owner, and the acceptance manifest is re-run at the closing head.\n\nDecision: accepted by the programme owner's standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, recorded in #526.",
      "code": "U5-CHG-DESIGN-PUBLISH",
      "created_at": "2026-11-22T06:44:53.339123+00:00",
      "id": "01a20200-26e0-71ea-b81a-e5da0e806aea",
      "impact": {
        "cost": "no separate cost baseline; the rework runs inside the existing programme tranche",
        "quality": "every owned finding executed or waived by a named owner; manifest re-run at the closing head",
        "schedule": "no change to the delivery tranche; the work runs in its rework batch",
        "scope": "the record and declared outputs are rebuilt and republished"
      },
      "kind": "change",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          {
            "kind": "issue",
            "number": 526
          },
          {
            "id": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
            "kind": "delegation"
          }
        ]
      },
      "meta": {
        "change_request": "the retrospective rework of DESIGN-PUBLISH under #526",
        "decision": {
          "decided_at": "2026-11-21",
          "decided_by": "the programme owner's standing delegation",
          "decision": "accepted",
          "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
          "record": "#526"
        },
        "impact": {
          "cost": "no separate cost baseline; the rework runs inside the existing programme tranche",
          "quality": "every owned finding executed or waived by a named owner; manifest re-run at the closing head",
          "schedule": "no change to the delivery tranche; the work runs in its rework batch",
          "scope": "the record and declared outputs are rebuilt and republished"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "accepted",
      "title": "Retrospective MSP/TOGAF/SAFe rework of DESIGN-PUBLISH under the #526 scope change",
      "updated_at": "2026-11-22T06:44:53.339123+00:00",
      "version": 1
    },
    {
      "baseline_ref": "01a1ff3e-51c4-75b1-9db1-a2f729dd10b8",
      "body": "Replayed from docs/evidence/record/journal.jsonl (natural key change:Record the B4 benefit baseline); the recorded request carried no rationale. Impact: {\"scope\": \"B4 public collaboration baseline and groups\"}",
      "code": "",
      "created_at": "2026-11-26T07:44:56.171049+00:00",
      "id": "01a216d0-8d71-7621-9022-4568316d2001",
      "impact": {
        "scope": "B4 public collaboration baseline and groups"
      },
      "kind": "change",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          {
            "id": "01a1e9df-1592-79a5-839e-037d8fe4251b",
            "kind": "benefit"
          }
        ]
      },
      "meta": {
        "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
        "impact": {
          "scope": "B4 public collaboration baseline and groups"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "Record the B4 benefit baseline",
      "updated_at": "2026-11-26T07:44:56.567518+00:00",
      "version": 2
    }
  ],
  "source": "computed:change_impact"
}
```

## Decisions

The record the section derives from, or an explicit absence.

```json
{
  "category": "change-control",
  "count": 2,
  "rows": [
    {
      "baseline_ref": "01a1ff3e-51c4-75b1-9db1-a2f729dd10b8",
      "body": "Change request: rebuild DESIGN-PUBLISH's record under the owner's 2026-11-21 scope change (#526). The missing MSP, TOGAF and SAFe artefacts are provided retroactively, the record is taken through the full lifecycle, and the project is re-closed through the gates.\n\nImpact: scope - the project's record and declared outputs are rebuilt and republished. Schedule - no change to the delivery tranche; the work runs in its rework batch. Cost - no separate cost baseline; the rework runs inside the existing programme tranche. Quality - every owned finding is executed or waived by a named owner, and the acceptance manifest is re-run at the closing head.\n\nDecision: accepted by the programme owner's standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, recorded in #526.",
      "code": "U5-CHG-DESIGN-PUBLISH",
      "created_at": "2026-11-22T06:44:53.339123+00:00",
      "id": "01a20200-26e0-71ea-b81a-e5da0e806aea",
      "impact": null,
      "kind": "change",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          {
            "kind": "issue",
            "number": 526
          },
          {
            "id": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
            "kind": "delegation"
          }
        ]
      },
      "meta": {
        "change_request": "the retrospective rework of DESIGN-PUBLISH under #526",
        "decision": {
          "decided_at": "2026-11-21",
          "decided_by": "the programme owner's standing delegation",
          "decision": "accepted",
          "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
          "record": "#526"
        },
        "impact": {
          "cost": "no separate cost baseline; the rework runs inside the existing programme tranche",
          "quality": "every owned finding executed or waived by a named owner; manifest re-run at the closing head",
          "schedule": "no change to the delivery tranche; the work runs in its rework batch",
          "scope": "the record and declared outputs are rebuilt and republished"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "accepted",
      "title": "Retrospective MSP/TOGAF/SAFe rework of DESIGN-PUBLISH under the #526 scope change",
      "updated_at": "2026-11-22T06:44:53.339123+00:00",
      "version": 1
    },
    {
      "baseline_ref": "01a1ff3e-51c4-75b1-9db1-a2f729dd10b8",
      "body": "Replayed from docs/evidence/record/journal.jsonl (natural key change:Record the B4 benefit baseline); the recorded request carried no rationale. Impact: {\"scope\": \"B4 public collaboration baseline and groups\"}",
      "code": "",
      "created_at": "2026-11-26T07:44:56.171049+00:00",
      "id": "01a216d0-8d71-7621-9022-4568316d2001",
      "impact": null,
      "kind": "change",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          {
            "id": "01a1e9df-1592-79a5-839e-037d8fe4251b",
            "kind": "benefit"
          }
        ]
      },
      "meta": {
        "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
        "impact": {
          "scope": "B4 public collaboration baseline and groups"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "Record the B4 benefit baseline",
      "updated_at": "2026-11-26T07:44:56.567518+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```

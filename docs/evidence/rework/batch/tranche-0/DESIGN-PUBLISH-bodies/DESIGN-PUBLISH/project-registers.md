# Project Registers — Publish the SPEC_V2 studio design pack, live

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The rework note records 102 outputs live against 165 in the committed pack at 2026-11-21, and the project-brief URL returns 404 live until the republish from live rows runs. This retrospective row, dated 2026-11-26, records the measured gap and assigns no fix to this project; U7 owns the republish.",
      "code": "",
      "created_at": "2026-11-26T08:07:37.807095+00:00",
      "id": "01a216e5-5452-7ab8-83ec-182ff757e13f",
      "impact": 4,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/evidence/rework/DESIGN-PUBLISH.md \u2014 the live window residual"
        ]
      },
      "meta": {
        "owner": "the programme architect under the #526 rework delegation"
      },
      "owner_job_id": null,
      "probability": "high",
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": "2026-12-15",
      "severity": "high",
      "status": "open",
      "title": "The live public window is narrower than the committed pack",
      "updated_at": "2026-11-26T08:07:37.807095+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The acceptance record generated 2026-11-25T10:37:50Z at 37f99108 reports 12 mandatory artefact packages open and the project closed on a red manifest. This retrospective row records the state before the batch runs; the content pack authored 2026-11-26 supplies the missing E\u2013H artefact bodies and does not itself clear the red.",
      "code": "",
      "created_at": "2026-11-26T08:07:37.939264+00:00",
      "id": "01a216e5-54d6-78f9-aea4-c8a66f35fbf9",
      "impact": 4,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/evidence/rework/DESIGN-PUBLISH.json \u2014 the 2026-11-25 acceptance record"
        ]
      },
      "meta": {
        "owner": "the programme architect under the #526 rework delegation"
      },
      "owner_job_id": null,
      "probability": "high",
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": "2026-12-15",
      "severity": "high",
      "status": "open",
      "title": "The 2026-11-25 acceptance record is red on the mandatory artefact standard",
      "updated_at": "2026-11-26T08:07:37.939264+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Issues

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 1,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The acceptance record names std-togaf-benefits-diagram-design-publish, std-togaf-candidate-solutions-design-publish, std-togaf-phase-h-design-publish-h and std-togaf-project-context-diagram-design-publish as the unresolved P0/P1 packages, part of the 12 mandatory packages open. Recorded retrospectively on 2026-11-26.",
      "code": "",
      "created_at": "2026-11-26T08:07:38.071651+00:00",
      "id": "01a216e5-555a-7634-afbe-8846bc307f5d",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/evidence/rework/DESIGN-PUBLISH.json \u2014 the failures array"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": null,
      "severity": "high",
      "status": "open",
      "title": "Four mandatory packages remain unresolved P0/P1 at the 2026-11-25 record",
      "updated_at": "2026-11-26T08:07:38.071651+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Assumptions

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 1,
  "rows": [
    {
      "baseline_ref": null,
      "body": "This retrospective assumption, recorded 2026-11-26, is that U7's republish from live rows closes the 102-to-165 gap and makes the project-brief URL resolve. If it does not, the benefit profile Public collaboration stays unrealised on the live estate and the risk above stands.",
      "code": "",
      "created_at": "2026-11-26T08:07:38.199212+00:00",
      "id": "01a216e5-55da-700b-ab5f-47210dafb09c",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The republish from live rows restores the public window",
      "updated_at": "2026-11-26T08:07:38.199212+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Dependencies

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 1,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The public window depends on U7, which the rework note names as the owner of the republish from live rows. This project does not attempt the republish; its closure record and its benefit claim both depend on U7 completing it. Recorded retrospectively on 2026-11-26.",
      "code": "",
      "created_at": "2026-11-26T08:07:38.329413+00:00",
      "id": "01a216e5-565c-7097-81d9-13206ae09f89",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The live window depends on U7's republish",
      "updated_at": "2026-11-26T08:07:38.329413+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Benefits

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 1,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The benefit profile 01a1e9df-1592-79a5-839e-037d8fe4251b records Public collaboration. The pack publishes the studio design as documents and pages so it can be read outside the repository; this retrospective row, dated 2026-11-26, links the benefit to the published pack without claiming realisation while the window is 102 against 165.",
      "code": "",
      "created_at": "2026-11-26T08:07:38.456889+00:00",
      "id": "01a216e5-56db-7ace-b28d-ae53a0c98044",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "01a1e9df-1592-79a5-839e-037d8fe4251b"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Public collaboration: the studio design is publicly readable",
      "updated_at": "2026-11-26T08:07:38.456889+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Changes

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
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

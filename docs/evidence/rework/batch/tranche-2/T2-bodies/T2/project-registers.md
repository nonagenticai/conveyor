# Project Registers — Decision ladder

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "`documents.compose` accepts `card_id` and `meta.generated = true` and attaches the document to the card with no gate, so a generated L2 draft can reach a card on the create path although `ladder.apply_draft` and `documents.update` refuse it (v1-t2-1, P0). Until the create path runs the same human `draft.apply` gate, the fourth clause of the \u00a712 T2 gate holds only on the update path.",
      "code": "R-T2-01",
      "created_at": "2026-11-26T13:07:42.588778+00:00",
      "id": "01a217f8-0f7f-7a54-bbd3-874d788e3a53",
      "impact": 5,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R5 P4.1 (create path)",
          "R4 P4.1/P4.2"
        ]
      },
      "meta": {
        "owner": "the head of platform in the T2 sponsoring group"
      },
      "owner_job_id": null,
      "probability": "high",
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": "mitigate: route `documents.compose` through `human_approved_gate_tx` before any generated document is attached to a card",
      "review_at": "2026-12-10",
      "severity": "P0",
      "status": "open",
      "title": "The create path attaches a generated draft without the gate",
      "updated_at": "2026-11-26T13:07:42.588778+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "`ladder.record` accepts any approved approval for the human rung instead of the approval its own gate produced (v1-t2-6). The requester and the approver are distinct identities under the standing delegation, but without a gate-identity check a requester can attach a third party's approval, or one borrowed from another case, and the self-approval refusal rests on convention rather than on the record.",
      "code": "R-T2-02",
      "created_at": "2026-11-26T13:07:42.71969+00:00",
      "id": "01a217f8-1002-7ea0-b3fc-702a9aaf0d2a",
      "impact": 4,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R6 P1.1 (human)",
          "R7 P1.1 (gate identity)"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": "mitigate: require the decision's `approval_id` to equal the gate approval for the same case and card",
      "review_at": "2026-12-10",
      "severity": "P1",
      "status": "open",
      "title": "A human decision cites an approval that did not gate it",
      "updated_at": "2026-11-26T13:07:42.71969+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The L1 rung is recorded but never calls laya (v1-t2-2), so a middle-rung decision can be written without the laya answer and latency the audit shape pins. The three rungs still share one row shape, but an L1 row that skipped laya cannot be compared with a rule or human row on the evidence the gate names.",
      "code": "R-T2-03",
      "created_at": "2026-11-26T13:07:42.852105+00:00",
      "id": "01a217f8-1086-7f7e-9f7e-2686762c7d1e",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R11 \u00a77.2 (laya)",
          "R2 P2.1"
        ]
      },
      "meta": {
        "owner": "the laya owner on the Architecture Board"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": "mitigate: call the laya contract from `ladder.evaluate` on the L1 rung and refuse a decision whose pin set is incomplete",
      "review_at": "2026-12-10",
      "severity": "P2",
      "status": "open",
      "title": "L1 records a decision without asking laya",
      "updated_at": "2026-11-26T13:07:42.852105+00:00",
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
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "Closed in an unauthorised tranche before the tranche review; accepted by the delegate as a retrospective exception under the standing delegation. The project is reworked in its batch and re-closed through the gates.",
      "code": "U3-EXC-T2",
      "created_at": "2026-11-21T01:16:00.361572+00:00",
      "id": "01a1fbac-aeb9-7dea-a079-bf159140344a",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": null,
      "review_at": null,
      "severity": "P0",
      "status": "accepted",
      "title": "Out-of-sequence closure accepted as a retrospective exception (T2)",
      "updated_at": "2026-11-21T01:16:00.361572+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The publish action accepts a refinement that changes an approval threshold with no gated approval recorded (v1-t2-3). A tenant can therefore move a threshold the ladder's policy pins without the human decision the gate requires, and the audit shape records the new version as if it had been approved.",
      "code": "I-T2-01",
      "created_at": "2026-11-26T13:09:09.790589+00:00",
      "id": "01a217f9-6422-7517-834f-afc4b037ea52",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R3 P3.1",
          "R10 \u00a77.2 (gated refinement)"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": null,
      "review_at": null,
      "severity": "P1",
      "status": "open",
      "title": "`refinements.publish` changes a threshold without the gate",
      "updated_at": "2026-11-26T13:09:09.790589+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Options and questions are carried as free text inside a prompt rather than as authored `decision_option` rows (v1-t2-4), so an option-set version cannot be pinned and a decision cannot be reproduced after the prompt or its options change.",
      "code": "I-T2-02",
      "created_at": "2026-11-26T13:09:09.936312+00:00",
      "id": "01a217f9-64b3-759c-b8f0-688ceecc3243",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R8 \u00a77.2 (options)",
          "R9 \u00a77.2 (questions)"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": null,
      "review_at": null,
      "severity": "P2",
      "status": "open",
      "title": "Options and questions are inline free text",
      "updated_at": "2026-11-26T13:09:09.936312+00:00",
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
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The retrospective rework acts under the delegation published on 2026-11-22, and the requester `conveyor-e2e` and the approver `conveyor-approver` stay distinct identities for every approval the ladder records. If the delegation lapses, the A-D artefacts and the delivery contract lose the authority they were approved under.",
      "code": "A-T2-01",
      "created_at": "2026-11-26T13:09:10.079958+00:00",
      "id": "01a217f9-6542-7fbb-b565-c542de8721b1",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R7 P1.1 (gate identity)"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The standing delegation remains the authority for this rework",
      "updated_at": "2026-11-26T13:09:10.079958+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The L0 rule, the L1 laya choice and the L3 human decision stay separate rungs with separate actors, and no rung is collapsed into another. The audit-shape test compares the three rows on one column set, and that comparison only means something while the rung and actor CHECKs keep the rows distinct.",
      "code": "A-T2-02",
      "created_at": "2026-11-26T13:09:10.223041+00:00",
      "id": "01a217f9-65d2-70b4-895a-874d20b6e945",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R1 P1.1"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The ladder keeps three distinct rungs and actors",
      "updated_at": "2026-11-26T13:09:10.223041+00:00",
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
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "T2 carries a declared `needs` dependency on T1 and reuses the five decision tables and the approval primitive T0 shipped as contracts in `migrations/0040_organisation.sql`. A schema break upstream would strand the rung, answer and version-pin columns that 0046 adds to `decision`.",
      "code": "D-T2-01",
      "created_at": "2026-11-26T13:09:10.364272+00:00",
      "id": "01a217f9-665f-7e6e-b20f-c0c84deb3918",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R1 P1.1",
          "R4 P4.1/P4.2"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "T1 and the T0 decision-table contracts hold",
      "updated_at": "2026-11-26T13:09:10.364272+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The tenant-scoped decision rows rely on the FORCE ROW LEVEL SECURITY and the forward-only migration discipline that `migrations/0040_organisation.sql` and `0046_decision_ladder.sql` establish. A store that does not enforce the same policies would break the tenant isolation the decision tables and their audit event depend on.",
      "code": "D-T2-02",
      "created_at": "2026-11-26T13:09:10.507434+00:00",
      "id": "01a217f9-66ee-7bc6-a669-5de9160c3498",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R13"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Postgres 18 with FORCE RLS stays the only store",
      "updated_at": "2026-11-26T13:09:10.507434+00:00",
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
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "A rule, a laya and a human decision are recorded through one `decision` row with the policy, refinement and laya-config versions pinned, so an auditor compares the three rungs on the same columns instead of translating three record families.",
      "code": "B-T2-01",
      "created_at": "2026-11-26T13:09:10.645867+00:00",
      "id": "01a217f9-6778-7aad-a3de-83a6f402d0aa",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R1 P1.1"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Every rung reads as one audit shape",
      "updated_at": "2026-11-26T13:09:10.645867+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A published refinement changes a decision threshold for one tenant without a code change or a deploy, because `ladder.evaluate` reads the pinned refinement version rather than a compiled constant. The same row records which refinement version answered, so the change is reproducible.",
      "code": "B-T2-02",
      "created_at": "2026-11-26T13:09:10.788242+00:00",
      "id": "01a217f9-6807-7d1c-8e32-9608422940bc",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R3 P3.1",
          "R10 \u00a77.2 (gated refinement)"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A tenant threshold changes without a build",
      "updated_at": "2026-11-26T13:09:10.788242+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Once the gate-identity check lands, the human rung requires the approval its own gate produced, and the requester is never the approver under the standing delegation. A self-approval, or an approval borrowed from another case, is refused, so the ladder's escalation path becomes an audit fact rather than a convention.",
      "code": "B-T2-03",
      "created_at": "2026-11-26T13:09:10.930885+00:00",
      "id": "01a217f9-6896-7192-aee3-2652da40773d",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R6 P1.1 (human)",
          "R7 P1.1 (gate identity)"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "No decision passes without its own gate approval",
      "updated_at": "2026-11-26T13:09:10.930885+00:00",
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
  "count": 1,
  "rows": [
    {
      "baseline_ref": "01a2005c-5a4f-7eb5-a004-ccb5545eb6fb",
      "body": "Change request: rebuild T2's record under the owner's 2026-11-21 scope change (#526). The missing MSP, TOGAF and SAFe artefacts are provided retroactively, the record is taken through the full lifecycle, and the project is re-closed through the gates.\n\nImpact: scope - the project's record and declared outputs are rebuilt and republished. Schedule - no change to the delivery tranche. Cost - no separate cost baseline; the rework runs inside the existing programme tranche. Quality - every owned finding is executed or waived by a named owner, and the acceptance manifest is re-run at the closing head.\n\nDecision: accepted by the programme owner's standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, recorded in #526.",
      "code": "U5-CHG-T2",
      "created_at": "2026-11-22T07:34:55.786488+00:00",
      "id": "01a2022d-f9a0-785a-be03-30a658a59468",
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
        "change_request": "the retrospective rework of T2 under #526",
        "decision": {
          "decided_at": "2026-11-21",
          "decided_by": "the programme owner's standing delegation",
          "decision": "accepted",
          "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
          "record": "#526"
        },
        "impact": {
          "cost": "no separate cost baseline; inside the existing tranche",
          "quality": "every owned finding executed or waived; manifest re-run at the closing head",
          "schedule": "no change to the delivery tranche",
          "scope": "the record and declared outputs are rebuilt and republished"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "accepted",
      "title": "Retrospective MSP/TOGAF/SAFe rework of T2 under the #526 scope change",
      "updated_at": "2026-11-22T07:34:55.786488+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

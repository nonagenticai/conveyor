# Project Registers — Domain and schema — aggregates, state machines, baseline domain migration

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "claim_card_tx lets the same assignee re-claim an unexpired lease through its third arm (OR assignee_id = $3), and the 32-racer test shares seed.user and Version::INITIAL, so the version predicate decides the winner and the test would pass with the lease arm deleted. Property (b) is therefore overstated for the lease half.",
      "code": "R-S03-01",
      "created_at": "2026-11-26T13:08:23.557945+00:00",
      "id": "01a217f8-af89-72bc-b545-722a33f7f0de",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [
          "S3-D5"
        ],
        "requirements": [
          "R2"
        ]
      },
      "meta": {
        "owner": "programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "high",
      "programme_id": null,
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": "2026-12-05",
      "severity": null,
      "status": "admitted",
      "title": "The lease-exclusivity claim is not exercised by the 32-racer test (v1-s03-1)",
      "updated_at": "2026-11-26T13:08:23.557945+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Only card_board_list_position_idx and card_board_position_idx carry WHERE archived_at IS NULL; card_assignee_lease_idx and card_tsv_idx include stamped cards, and the later card_name_trgm_idx and card_wbs_node_idx add no predicate. A later slice inheriting the schema could read the claim as absolute.",
      "code": "R-S03-02",
      "created_at": "2026-11-26T13:08:23.699193+00:00",
      "id": "01a217f8-b016-733d-9a75-14afc9fd14ff",
      "impact": 2,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [
          "S3-D1"
        ],
        "requirements": [
          "R10"
        ]
      },
      "meta": {
        "owner": "programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": "2026-12-05",
      "severity": null,
      "status": "admitted",
      "title": "The archive-exclusion claim covers only the two board-scan indexes (v1-s03-2)",
      "updated_at": "2026-11-26T13:08:23.699193+00:00",
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
      "code": "U3-EXC-S03",
      "created_at": "2026-11-21T01:15:52.1791+00:00",
      "id": "01a1fbac-8ec9-717c-be79-6d1b97980743",
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
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": null,
      "severity": "P0",
      "status": "accepted",
      "title": "Out-of-sequence closure accepted as a retrospective exception (S03)",
      "updated_at": "2026-11-21T01:15:52.1791+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The project row reached closed at 2026-11-21T01:05:32Z in an unauthorised tranche before the tranche review; the delegate accepted it as a retrospective exception under the standing delegation, and this rework re-closes the project through the one gate.",
      "code": "I-S03-01",
      "created_at": "2026-11-26T13:08:23.850033+00:00",
      "id": "01a217f8-b0ad-7317-987c-f82f016bf405",
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
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "accepted",
      "title": "S03 closed out of sequence before the tranche-2 review (U3-EXC-S03)",
      "updated_at": "2026-11-26T13:08:23.850033+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The plan requires a second reader over data-arch \u00a72.3/\u00a72.4 and the mandatory artefacts; none existed at the 2026-11-21T19:24:46Z snapshot. This retrospective batch authors the seven artefacts and re-performs the review, admitting two packages.",
      "code": "I-S03-02",
      "created_at": "2026-11-26T13:08:23.984758+00:00",
      "id": "01a217f8-b133-7bd4-aaed-e078c1fd9c03",
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
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "closed",
      "title": "No contemporaneous schema review or Phase A-D artefact set existed at the snapshot",
      "updated_at": "2026-11-26T13:08:23.984758+00:00",
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
      "body": "0001_platform.sql, begin_scoped, the conveyor_runtime role and domain::{TenantId, Id, Version} are the base the fourteen tables and the tenant_isolation policy stand on; a change to the runtime role or to app_current_tenant() would invalidate the RLS proof.",
      "code": "A-S03-01",
      "created_at": "2026-11-26T13:08:24.122096+00:00",
      "id": "01a217f8-b1bc-7f6a-9a3d-6c65bc101922",
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
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S02's platform primitives stay stable under the domain",
      "updated_at": "2026-11-26T13:08:24.122096+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The RLS proof assumes the runtime role is scoped to one tenant per transaction and that ENABLE plus FORCE leaves the table owner with zero rows under the app_current_tenant() policy; rls_domain.rs checks runtime, owner and superuser.",
      "code": "A-S03-02",
      "created_at": "2026-11-26T13:08:24.258277+00:00",
      "id": "01a217f8-b245-759f-b8dd-c5f4a83eb2ac",
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
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Postgres 18 enforces FORCE RLS for the owner and scopes the runtime role",
      "updated_at": "2026-11-26T13:08:24.258277+00:00",
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
      "body": "S03 is blocked by S02: migrations/0002_domain.sql builds on 0001_platform.sql, begin_scoped, the conveyor_runtime role and TenantId, Id and Version.",
      "code": "D-S03-01",
      "created_at": "2026-11-26T13:08:24.393809+00:00",
      "id": "01a217f8-b2cc-7b1d-995c-0723649d6202",
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
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S02 platform migration and tenancy primitives land first",
      "updated_at": "2026-11-26T13:08:24.393809+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "S04\u2013S13 register actions, board satellites, channels, outlines and workflow over these fourteen tables and may add tables, but the baseline domain, the may_move matrix and the catalogue vocabulary are inherited, not replaced.",
      "code": "D-S03-02",
      "created_at": "2026-11-26T13:08:24.531897+00:00",
      "id": "01a217f8-b357-7b38-a35b-ebd811104fe6",
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
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Later slices add tables and never re-invent one",
      "updated_at": "2026-11-26T13:08:24.531897+00:00",
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
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "Fourteen tenant-isolated tables in one baseline migration mean a later slice reaches for an existing table instead of inventing one, and the anti-accretion rule (R10) keeps fr's 224-migration history out of the baseline.",
      "code": "B-S03-01",
      "created_at": "2026-11-26T13:08:24.672301+00:00",
      "id": "01a217f8-b3e3-77af-b2f5-1eaa52ce07ee",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "One stored domain behind every later slice",
      "updated_at": "2026-11-26T13:08:24.672301+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The may_move matrix, the run, approval and asset state machines and the partial uniques give one refusal vocabulary: a terminal run rejects every transition, one pending approval exists per (run, gate), and a claim is granted once per version.",
      "code": "B-S03-02",
      "created_at": "2026-11-26T13:08:24.812741+00:00",
      "id": "01a217f8-b46f-7f1f-92bd-76adb49fad33",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The database and Rust refuse the same transition",
      "updated_at": "2026-11-26T13:08:24.812741+00:00",
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
      "baseline_ref": "01a20063-7af8-727b-b092-24de3821d204",
      "body": "Change request: rebuild S03's record under the owner's 2026-11-21 scope change (#526). The missing MSP, TOGAF and SAFe artefacts are provided retroactively, the record is taken through the full lifecycle, and the project is re-closed through the gates.\n\nImpact: scope - the project's record and declared outputs are rebuilt and republished. Schedule - no change to the delivery tranche. Cost - no separate cost baseline; the rework runs inside the existing programme tranche. Quality - every owned finding is executed or waived by a named owner, and the acceptance manifest is re-run at the closing head.\n\nDecision: accepted by the programme owner's standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14, recorded in #526.",
      "code": "U5-CHG-S03",
      "created_at": "2026-11-22T07:34:36.878165+00:00",
      "id": "01a2022d-b82e-723c-ac03-07afb43890dc",
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
        "change_request": "the retrospective rework of S03 under #526",
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
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "accepted",
      "title": "Retrospective MSP/TOGAF/SAFe rework of S03 under the #526 scope change",
      "updated_at": "2026-11-22T07:34:36.878165+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

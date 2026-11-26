# Project Registers — Board — projects, boards, lists, cards, labels, comments, tasks, custom fields, webhooks, SSE

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The Phase-G review, the delivery contract and the evidence Gap all record that at the 2026-11-24 snapshot no review records a run of the six plan section 7 properties: replay fidelity, one winner, 1k load, webhook shape, SSE and no second store. The project row is closed with meta.measured_status built, but built is a record claim, not a gate pass, and acceptance/programme/S05.toml has 17 checks whose closed-on-red state is the reason this rework exists. Until the properties run at a rework head, the re-close rests on the record set and the accepted P0 exception that carries the out-of-sequence close.",
      "code": "S05-R1",
      "created_at": "2026-11-26T21:52:38.518266+00:00",
      "id": "01a219d8-a67c-75d1-a9a5-17a0c715b121",
      "impact": 4,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R1 Property (a) replay fidelity",
          "R2 Property (b) one winner",
          "R3 Property (c) 1k load",
          "R4 Property (d) webhook shape",
          "R5 Property (e) SSE",
          "R6 Property (f) no second store"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "high",
      "programme_id": null,
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": "Run acceptance/programme/S05.toml and the four gate test binaries plus tests/load/board.js on a live stack, keep one receipt per property, and re-read the closure review on that evidence rather than inferring a pass from the built status.",
      "review_at": "2026-12-15",
      "severity": "high",
      "status": "open",
      "title": "The six gate properties have no recorded run, so the built claim is unproven",
      "updated_at": "2026-11-26T21:52:38.518266+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The project brief's Plan section and the evidence Gap both record that the slice plan frontmatter still reads status: planned and the project meta echoes meta.plan_status: planned although meta.measured_status is built. The Phase-G review judges the record set and does not reconcile the two, so a reader can take an unaccepted plan for an accepted one; the re-close rests on the acceptance record, not on the plan.",
      "code": "S05-R2",
      "created_at": "2026-11-26T21:52:38.672575+00:00",
      "id": "01a219d8-a714-72fe-b1d9-6076461df613",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "acceptance/programme/S05.toml"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": "Reconcile the plan frontmatter with meta.measured_status at the tranche-4 review and record the decision in the plan of record.",
      "review_at": "2026-12-15",
      "severity": "medium",
      "status": "open",
      "title": "The plan frontmatter says planned while the project meta says built",
      "updated_at": "2026-11-26T21:52:38.672575+00:00",
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
      "code": "U3-EXC-S05",
      "created_at": "2026-11-21T01:16:24.209747+00:00",
      "id": "01a1fbad-0be3-7557-9402-cfb405a3147c",
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
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": null,
      "review_at": null,
      "severity": "P0",
      "status": "accepted",
      "title": "Out-of-sequence closure accepted as a retrospective exception (S05)",
      "updated_at": "2026-11-21T01:16:24.209747+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The project closed at 2026-11-21T01:05:43Z (version 7) and tranche 4 was authorised at 2026-11-24T17:04:34Z by the tranche-3 review; the accepted P0 issue records the close in an unauthorised tranche before the tranche review as a retrospective exception under the standing delegation. The charter, the Statement of Architecture Work and the evidence Gap carry the same order, so this rework's governance acts follow the close rather than precede it.",
      "code": "S05-I1",
      "created_at": "2026-11-26T21:52:38.830357+00:00",
      "id": "01a219d8-a7b2-71bb-ad4e-ec84c334b0bd",
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
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": "Have the tranche-4 review ratify the retrospective exception and this re-close in one decision, or return S05 to the board.",
      "review_at": null,
      "severity": "high",
      "status": "open",
      "title": "The close preceded tranche 4's authorisation by three days and rests only on an accepted P0 exception",
      "updated_at": "2026-11-26T21:52:38.830357+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The Technology Standards Catalog's source-discipline row and the evidence Gap record that the Planka fork is not present and that the per-event data envelope and the List.type transitions stay UNVERIFIED until measured. The plan's trap 1 forbids guessing them, so the board slice's reuse of Planka field shapes rests on the openapi and schema inventory, not on a runnable source.",
      "code": "S05-I2",
      "created_at": "2026-11-26T21:52:38.979664+00:00",
      "id": "01a219d8-a847-711a-a1cc-e070482e5879",
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
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": "Measure the per-event envelope and the List.type transitions when the fork is restored, and keep the S14 cutover adapter deferred until then.",
      "review_at": null,
      "severity": "medium",
      "status": "open",
      "title": "The Planka fork is absent, so the per-event envelope and List.type transitions stay unverified",
      "updated_at": "2026-11-26T21:52:38.979664+00:00",
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
      "body": "The delivery contract judges acceptance against the record at the 2026-11-24 rework snapshot, project closed 2026-11-21T01:05:43Z at version 7, and the checker refuses any id that appears nowhere in that snapshot. Every judgement in this pack is read from the snapshot, so if the snapshot is incomplete or stale for S05 the pack's facts and this re-close move with it.",
      "code": "S05-A1",
      "created_at": "2026-11-26T21:52:39.1641+00:00",
      "id": "01a219d8-a8ff-7400-8cee-ec579f0b00e6",
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
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The 2026-11-24 rework snapshot is the authoritative record for S05",
      "updated_at": "2026-11-26T21:52:39.1641+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The project brief's reuse sources and the evidence Gap record that the live Planka database holds five cards, three webhooks and zero tasks, and plan traps 1 and 8 make it a test-data warning rather than evidence. The gate therefore rests on the two committed fixtures, v1-recording.json and v1-1000.json, so the live database can neither prove nor refute the six properties.",
      "code": "S05-A2",
      "created_at": "2026-11-26T21:52:39.311576+00:00",
      "id": "01a219d8-a992-7cf2-8c55-cd7cf1d52f73",
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
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The live Planka database is test data and never gate evidence",
      "updated_at": "2026-11-26T21:52:39.311576+00:00",
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
      "body": "The project brief makes S05 blocked-by S03 (project, board, list, card, card_type, catalogue_entry, may_move and card.version), S04 (the ActionSpec derive, the registry and POST /api/actions/{name}/invocations) and S02 (event, job, audit_event and the /api/events SSE with Last-Event-ID replay). S05 adds no card state column and no second write path, so its build and its gate stand on those three slices.",
      "code": "S05-D1",
      "created_at": "2026-11-26T21:52:39.454823+00:00",
      "id": "01a219d8-aa21-7bb6-b1c6-8f00bfeb1a87",
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
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "satisfied",
      "title": "S05 is blocked by S03, S04 and S02",
      "updated_at": "2026-11-26T21:52:39.454823+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The project brief makes S05 block S12's declared board and card-cockpit surface and S06's control-plane writes over the board (claims, assignee and stopwatch), and the charter puts those writes out of scope. The dependency stays open until S12 and S06 deliver against the declared surface; S05's re-close neither supplies nor verifies them.",
      "code": "S05-D2",
      "created_at": "2026-11-26T21:52:39.59715+00:00",
      "id": "01a219d8-aab0-72e7-98cd-00e76378760e",
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
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S12 and S06 consume the board surface; S05 never writes claims, assignee or stopwatch",
      "updated_at": "2026-11-26T21:52:39.59715+00:00",
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
      "body": "The charter's mandate and the Driver/Goal/Objective Catalog's D1/G1 record the benefit: cards.move loads the card, both lists and project.project_type_code, then reads the matrix from catalogue_entry once and caches it per (tenant, project_type_code), instead of duplicating 280 board\u2194project lookup flows across board surfaces. The benefit is declared in the record and carried by the R1-R6 gate; it is not yet evidenced by a recorded run.",
      "code": "S05-B1",
      "created_at": "2026-11-26T21:52:39.768564+00:00",
      "id": "01a219d8-ab5b-7c62-9da1-3ae8c9b952cc",
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
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The 280 board\u2194project lookup flows become one foreign-key read through the project_type matrix",
      "updated_at": "2026-11-26T21:52:39.768564+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The charter's mandate and the Driver/Goal/Objective Catalog's D2/G2 record one comment table and one board_membership row (owner, admin, member or viewer with can_comment and watching, and project_manager and board_subscription folded in) as the replacement for the duplicated Planka and fr stores, with gate property f (R6) as its check. As with B1 the benefit is declared in the record and unevidenced until the property runs.",
      "code": "S05-B2",
      "created_at": "2026-11-26T21:52:39.943073+00:00",
      "id": "01a219d8-ac0a-778b-8e66-145431e0206b",
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
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "One comment store and one membership model replace the Planka and fr duplication",
      "updated_at": "2026-11-26T21:52:39.943073+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The closure states that S05 re-closes on the P0 checks of acceptance/programme/S05.toml, 17 checks with only lessons and artefact-standard at P1, so the close decision is repeatable by an independent reader rather than resting on prose. The Phase-G review and the closure review both judge the record set against that manifest.",
      "code": "S05-B3",
      "created_at": "2026-11-26T21:52:40.090106+00:00",
      "id": "01a219d8-ac9d-73cf-9758-7d99d8411538",
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
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "realized",
      "title": "The re-close is decidable from the acceptance instrument's P0 checks",
      "updated_at": "2026-11-26T21:52:40.090106+00:00",
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
      "baseline_ref": "01a219d8-ad39-77c1-83f6-da901324441d",
      "body": "The retrospective MSP/TOGAF/SAFe rework changed this project's record: its artefacts were re-versioned with documents and links, its register was written, and its schedule and cost baselines were taken. This change records that rework against the frozen scope baseline.",
      "code": "CH-S05-5528da",
      "created_at": "2026-11-26T21:52:49.204056+00:00",
      "id": "01a219d8-d03a-7479-b67b-7822365528da",
      "impact": null,
      "kind": "change",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {
        "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
        "impact": {
          "scope": "the retrospective rework's record acts"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "The retrospective rework of S05",
      "updated_at": "2026-11-26T22:48:43.246893+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```

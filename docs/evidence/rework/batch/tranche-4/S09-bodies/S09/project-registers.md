# Project Registers — Documents, repositories, secrets, workers UI, reports/metrics, billing display

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The Phase-G review findings, the delivery contract compliance statement, the closure review's Outcome vs acceptance and the evidence Gap all record that at the 2026-11-24 snapshot no run of the plan section 7 properties exists: round-trip, cap, secret, workers, reports, externals and billing are each named with a test target, but no receipt is recorded. The project row is closed with meta.measured_status built, but built is a record claim, not a gate pass.",
      "code": "S09-R1",
      "created_at": "2026-11-26T21:54:11.284016+00:00",
      "id": "01a219da-10d6-7be2-b485-ce64cd134677",
      "impact": 4,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R1 Property (a) round-trip",
          "R2 Property (b) cap",
          "R3 Property (c) secret",
          "R4 Property (d) workers",
          "R5 Property (e) reports",
          "R6 Property (f) externals",
          "R7 Property (g) billing"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "high",
      "programme_id": null,
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": "Run acceptance/programme/S09.toml and the named targets documents_roundtrip, documents_limits, secrets_never_revealed, externals_declared and reports_match_sql plus the workers and billing checks on a live stack, keep one receipt per property, and re-read the closure review on that evidence rather than inferring a pass from the built status.",
      "review_at": "2026-12-15",
      "severity": "high",
      "status": "open",
      "title": "The seven gate properties a to g have no recorded run, so the built claim is unproven",
      "updated_at": "2026-11-26T21:54:11.284016+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The Solution Concept Diagram and the Business Capability Map record the deployed MinIO credentials and bucket policy as unproven, with scripts/prove-s3.sh proving only the S3 wire; strata_ingest is not reachable from a local gate, and documents.ingest is the one declared action of 27 that is not registered. The round-trip property may pass against an ephemeral bucket while the store the product will use stays unestablished.",
      "code": "S09-R2",
      "created_at": "2026-11-26T21:54:11.427063+00:00",
      "id": "01a219da-1166-714a-95e2-f34853019804",
      "impact": 4,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R1 Property (a) round-trip",
          "R9 27 declared actions, 26 registered"
        ]
      },
      "meta": {
        "owner": "operators"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": "Have operators confirm the MinIO endpoint, credentials and bucket policy, run the byte route against the deployed bucket, and register documents.ingest once a live strata_ingest target and a sealed per-tenant token exist.",
      "review_at": "2026-12-15",
      "severity": "high",
      "status": "open",
      "title": "The deployed MinIO leg and the live strata_ingest path behind documents.ingest are unproven",
      "updated_at": "2026-11-26T21:54:11.427063+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The Solution Concept Diagram's Links section and the Phase-G review findings both record that the strata_rag base-URL contract and the /query versus /api/query discrepancy remain a stated risk rather than a verified contract. Until the path is measured against a live target, any external action that reaches strata_rag rests on an unconfirmed interface.",
      "code": "S09-R3",
      "created_at": "2026-11-26T21:54:11.571091+00:00",
      "id": "01a219da-11f6-7260-9afc-f1db04bc5116",
      "impact": 2,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R6 Property (f) externals"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": "Measure the live strata_rag path against its published contract, record which base URL and route the registered external uses, and close the discrepancy in the plan of record.",
      "review_at": "2026-12-15",
      "severity": "medium",
      "status": "open",
      "title": "The strata_rag path contract is unresolved: /query versus /api/query",
      "updated_at": "2026-11-26T21:54:11.571091+00:00",
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
  "count": 4,
  "rows": [
    {
      "baseline_ref": null,
      "body": "Closed in an unauthorised tranche before the tranche review; accepted by the delegate as a retrospective exception under the standing delegation. The project is reworked in its batch and re-closed through the gates.",
      "code": "U3-EXC-S09",
      "created_at": "2026-11-21T01:16:41.056796+00:00",
      "id": "01a1fbad-4daf-72dd-a94c-32adec780aee",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": null,
      "review_at": null,
      "severity": "P0",
      "status": "accepted",
      "title": "Out-of-sequence closure accepted as a retrospective exception (S09)",
      "updated_at": "2026-11-21T01:16:41.056796+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The Requirements Catalog's R9 row, the Business Capability Map's strata ingest handoff and the evidence Gap record that 26 of the front matter's 27 actions are registered and documents.ingest is the single gap, because it needs a live strata_ingest target and a sealed per-tenant token. The action is recorded as a follow-up with a live target rather than as proven code.",
      "code": "S09-I1",
      "created_at": "2026-11-26T21:54:11.71108+00:00",
      "id": "01a219da-1281-7e9c-8740-ce93ff1dee40",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": "Register documents.ingest as kind internal once a reachable strata_ingest target and a sealed strata_ingest.token exist, then exercise the POST /api/ingest/jobs path and store jobs[0].job_id in document.external_ref.",
      "review_at": null,
      "severity": "medium",
      "status": "open",
      "title": "documents.ingest is the one declared action of 27 not registered",
      "updated_at": "2026-11-26T21:54:11.71108+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The project brief's Plan section, the Phase-G review findings and the evidence Gap all record that docs/plans/slices/S09-documents-integrations.md still carries status planned while the project row has meta.measured_status built and meta.plan_status planned. The two records are not reconciled, so a reader can take an unaccepted plan for an accepted one.",
      "code": "S09-I2",
      "created_at": "2026-11-26T21:54:11.852914+00:00",
      "id": "01a219da-130f-7b04-a105-e682983da0e1",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": "Reconcile the plan frontmatter with the project meta at the tranche-4 review and record the decision in the plan of record.",
      "review_at": null,
      "severity": "medium",
      "status": "open",
      "title": "The plan frontmatter reads planned while the project meta says measured_status built",
      "updated_at": "2026-11-26T21:54:11.852914+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The closure review's Open items and the evidence Gap record that issues 156, 168, 183, 195, 231, 245, 571, 577 and 579 are recorded closed in the snapshot, and that their acceptance is not re-verified by this retrospective. The re-close therefore rests on the closed record, not on a fresh acceptance check of each issue.",
      "code": "S09-I3",
      "created_at": "2026-11-26T21:54:11.992971+00:00",
      "id": "01a219da-139b-7e21-9d19-68b4daeb0020",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": "Re-verify each linked issue's acceptance at the tranche-4 review or carry the closed record as the accepted basis for the retrospective close; do not infer a fresh pass.",
      "review_at": null,
      "severity": "medium",
      "status": "open",
      "title": "The nine linked issues are recorded closed but their acceptance is not re-verified",
      "updated_at": "2026-11-26T21:54:11.992971+00:00",
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
      "body": "The delivery contract judges acceptance against the record at the 2026-11-24 rework snapshot, project 01a1d039-0d2d-71f1-a403-c9abe52c1888 status closed at version 10, and the content checker refuses any id that appears nowhere in that snapshot. Every judgement in this pack is read from that snapshot; if it is incomplete or stale for S09, the pack's facts and the re-close move with it.",
      "code": "S09-A1",
      "created_at": "2026-11-26T21:54:12.138927+00:00",
      "id": "01a219da-142d-7b80-a053-cf239641ba4c",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The 2026-11-24 rework snapshot is the authoritative record for S09",
      "updated_at": "2026-11-26T21:54:12.138927+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The project brief's Dependencies section makes S09 blocked-by S03 for tenant, trace_link, revision and the tenancy convention of the four new tables, and the Data Entity/Data Component Catalog's ArtifactRef row treats the trace_link projection as S03-supplied. The four tables' FORCE RLS and the ArtifactRef value rest on that contract holding without change.",
      "code": "S09-A2",
      "created_at": "2026-11-26T21:54:12.283485+00:00",
      "id": "01a219da-14be-771c-84b6-e55430fd6a9c",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "S03 supplies tenant, trace_link and revision exactly as the project brief records",
      "updated_at": "2026-11-26T21:54:12.283485+00:00",
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
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The project brief makes S09 blocked-by S03 (tenant, trace_link, revision and the tenancy convention for the four new tables), S04 (the ActionSpec derive, action_registry, POST /api/actions/{name}/invocations and the OpenAPI and MCP projection) and S02 (event, job, audit_event and the outbox that carries the nine S09 event types). S09 adds no second write path, so its build and its gate stand on those three slices.",
      "code": "S09-D1",
      "created_at": "2026-11-26T21:54:12.438145+00:00",
      "id": "01a219da-1569-7e7d-be4d-e6c4fc5ab6f4",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "satisfied",
      "title": "S09 is blocked by S03, S04 and S02",
      "updated_at": "2026-11-26T21:54:12.438145+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The Statement of Architecture Work assigns the KEK source and the MinIO bucket policy to operators, the Business Capability Map records the deployed MinIO credentials and bucket policy as unproven, and the closure review's Open items name operators as the owner of the deployed MinIO endpoint, credentials and bucket policy. The byte route cannot be proven end to end until that leg is confirmed.",
      "code": "S09-D2",
      "created_at": "2026-11-26T21:54:12.610171+00:00",
      "id": "01a219da-1605-7778-b1b7-5484ffb5035e",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The deployed leg depends on the operators' MinIO endpoint, credentials and bucket policy",
      "updated_at": "2026-11-26T21:54:12.610171+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The Business Capability Map, the Requirements Catalog's R9 row and the closure review's Open items record that documents.ingest needs a reachable strata_ingest target and a sealed strata_ingest.token before it can be registered and exercised; without them a tenant is refused 422 ingest_not_configured before a byte is read.",
      "code": "S09-D3",
      "created_at": "2026-11-26T21:54:12.754928+00:00",
      "id": "01a219da-1695-7c73-94b5-e018635e1628",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "documents.ingest depends on a live strata_ingest target and a sealed per-tenant token",
      "updated_at": "2026-11-26T21:54:12.754928+00:00",
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
      "body": "The Driver/Goal/Objective Catalog's D1/G1 records the benefit: bytes move out of Postgres TOAST into bucket conveyor-documents at key t/<tenant>/<document>/<version>, every byte streams through /api/documents/{id}/content authenticated against tenant_id and membership, and no presigned URL exists. Rows and dumps stop carrying document content, and there is one authorizer, not two.",
      "code": "S09-B1",
      "created_at": "2026-11-26T21:54:12.942035+00:00",
      "id": "01a219da-1751-7226-87f1-836b5415ad58",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Document bytes leave Postgres: content lives in MinIO behind one authorized stream",
      "updated_at": "2026-11-26T21:54:12.942035+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The Driver/Goal/Objective Catalog's D2/G2 and the Requirements Catalog's R3 record the benefit: create and rotate take value marked x-secret, get and list return id, name, scope, key_id, version and created_at only, and the served OpenAPI and MCP projection publish no default or example for the marked field. The whole-leak failure of a one-time reveal is removed by design.",
      "code": "S09-B2",
      "created_at": "2026-11-26T21:54:13.09076+00:00",
      "id": "01a219da-17e5-7c58-9e51-70dd68e52bd0",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Secrets are write-only: no response returns a plaintext, value key or ciphertext",
      "updated_at": "2026-11-26T21:54:13.09076+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The Driver/Goal/Objective Catalog's D3/G3 and the Technology Standards Catalog's external-calls standard record the benefit: every adjacent service is a declared action with a 5 s read or 30 s write timeout and a process-local breaker opening after five failures in 30 s, and reqwest:: appears only inside the executor. The conveyor does not absorb a worker manager, billing engine or KAG.",
      "code": "S09-B3",
      "created_at": "2026-11-26T21:54:13.238793+00:00",
      "id": "01a219da-1879-7e81-b32a-506e11d0aa45",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Adjacent services stay external: a call absent from action_registry does not exist",
      "updated_at": "2026-11-26T21:54:13.238793+00:00",
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
      "baseline_ref": "01a219da-1919-75c5-97c0-6c9e766306df",
      "body": "The retrospective MSP/TOGAF/SAFe rework changed this project's record: its artefacts were re-versioned with documents and links, its register was written, and its schedule and cost baselines were taken. This change records that rework against the frozen scope baseline.",
      "code": "CH-S09-4220a4",
      "created_at": "2026-11-26T21:54:23.46468+00:00",
      "id": "01a219da-406f-7ecb-a58f-d50f964220a4",
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
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "The retrospective rework of S09",
      "updated_at": "2026-11-26T22:03:08.648927+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```

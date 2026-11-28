# Statement Of Architecture Work — Documents, repositories, secrets, workers UI, reports/metrics, billing display

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a219d9-f717-73ba-9c86-18adeeecd4ef",
      "approved_at": "2026-11-26T21:54:04.836362+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The plan \u00a77 gate: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, then delete yields GET 404 and object_store::head NotFound, asserted in documents_roundtrip; (b) cap \u2014 10,485,760 bytes accepted and one byte more refused 413 application/problem+json with a stable type in documents_limits; (c) secret \u2014 rotate to version 2 and GET returns no plaintext, value key or ciphertext in secrets_never_revealed; (d) workers \u2014 the page renders live rows and a stopped service yields a banner with the breaker open after five failures; (e) reports \u2014 every number equals its SQL count on the same snapshot in reports_match_sql; (f) externals \u2014 GET /api/actions carries timeout_ms on every external and zero reqwest:: outside the executor in externals_declared; (g) billing \u2014 no write action and no metering event in the manifest. The same instrument also requires migration 0013 applied twice as a no-op, FORCE RLS on all four tables, and the nine declared event types. The snapshot records no run of these properties.",
        "assignment": "Rework the V1 slice S09 (SPEC \u00a78 S9) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, this charter, this Statement of Architecture Work and the delivery contract, take the scope baseline with one WBS deliverable, record a retrospective Phase-G review at level consistent, and publish the expected outputs project-brief and evidence. Because all nine linked issues are recorded closed, this rework re-closes the project at level consistent through the acceptance manifest, with the closure and closure review dated 2026-11-24.",
        "contract": "This Statement establishes the delivery contract against acceptance/programme/S09.toml, whose gate text names the seven properties above. Compliance is assessed clause by clause at the 2026-11-24 retrospective snapshot; the project's published evidence page is 01a1e687-7682-753d-b60e-1f00243b13a3 and its pre-rework audit pack is docs/evidence/rework/S09.json, which records verdict red on the missing baseline, charter, contract, Phase-G review and closure outputs. The rework supplies the lifecycle record and re-closes the project retrospectively on the P0 checks of the manifest; the remaining verification limits are carried in the closure review, the review and the evidence output rather than converted into a pass.",
        "responsibilities": "The programme architect (conveyor-e2e) owns the slice plan and this retrospective set; the Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract; the approver (conveyor-approver) decides the Phase-G level; the S03 team supplies tenancy, trace_link and revision, and the S04 team supplies the action registry with its OpenAPI and MCP projection. Operators own the KEK source in the pod environment and the MinIO bucket policy.",
        "scope": "the four tables document, repository, project_repository and secret in migrations/0013_documents.sql; the integrations module (object_store, external, breaker, ingest); documents.create/get/list/download/update/delete/report, 26 of the front matter's 27 actions in all; the nine event types; MinIO bucket conveyor-documents with key t/<tenant>/<document>/<version>; the KEK/HKDF/XChaCha20-Poly1305 secret envelope; repositories as references; the read-only workers, billing and observability displays; report queries over local tables; and the strata handoff. Out of scope: billing logic and metering writes, worker scheduling, artifact objects, code analysis, licence issuance, ingestion and embeddings as services, screens (S12) and project_request.repository_url (S07)."
      },
      "created_at": "2026-11-26T21:54:04.526424+00:00",
      "id": "01a219d9-f671-708a-b28f-063e7e79e6ec",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S09 Documents and integrations (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:54:04.836362+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Scope

The boundaries, the artefacts expected and the phase set.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a219d9-f717-73ba-9c86-18adeeecd4ef",
      "approved_at": "2026-11-26T21:54:04.836362+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The plan \u00a77 gate: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, then delete yields GET 404 and object_store::head NotFound, asserted in documents_roundtrip; (b) cap \u2014 10,485,760 bytes accepted and one byte more refused 413 application/problem+json with a stable type in documents_limits; (c) secret \u2014 rotate to version 2 and GET returns no plaintext, value key or ciphertext in secrets_never_revealed; (d) workers \u2014 the page renders live rows and a stopped service yields a banner with the breaker open after five failures; (e) reports \u2014 every number equals its SQL count on the same snapshot in reports_match_sql; (f) externals \u2014 GET /api/actions carries timeout_ms on every external and zero reqwest:: outside the executor in externals_declared; (g) billing \u2014 no write action and no metering event in the manifest. The same instrument also requires migration 0013 applied twice as a no-op, FORCE RLS on all four tables, and the nine declared event types. The snapshot records no run of these properties.",
        "assignment": "Rework the V1 slice S09 (SPEC \u00a78 S9) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, this charter, this Statement of Architecture Work and the delivery contract, take the scope baseline with one WBS deliverable, record a retrospective Phase-G review at level consistent, and publish the expected outputs project-brief and evidence. Because all nine linked issues are recorded closed, this rework re-closes the project at level consistent through the acceptance manifest, with the closure and closure review dated 2026-11-24.",
        "contract": "This Statement establishes the delivery contract against acceptance/programme/S09.toml, whose gate text names the seven properties above. Compliance is assessed clause by clause at the 2026-11-24 retrospective snapshot; the project's published evidence page is 01a1e687-7682-753d-b60e-1f00243b13a3 and its pre-rework audit pack is docs/evidence/rework/S09.json, which records verdict red on the missing baseline, charter, contract, Phase-G review and closure outputs. The rework supplies the lifecycle record and re-closes the project retrospectively on the P0 checks of the manifest; the remaining verification limits are carried in the closure review, the review and the evidence output rather than converted into a pass.",
        "responsibilities": "The programme architect (conveyor-e2e) owns the slice plan and this retrospective set; the Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract; the approver (conveyor-approver) decides the Phase-G level; the S03 team supplies tenancy, trace_link and revision, and the S04 team supplies the action registry with its OpenAPI and MCP projection. Operators own the KEK source in the pod environment and the MinIO bucket policy.",
        "scope": "the four tables document, repository, project_repository and secret in migrations/0013_documents.sql; the integrations module (object_store, external, breaker, ingest); documents.create/get/list/download/update/delete/report, 26 of the front matter's 27 actions in all; the nine event types; MinIO bucket conveyor-documents with key t/<tenant>/<document>/<version>; the KEK/HKDF/XChaCha20-Poly1305 secret envelope; repositories as references; the read-only workers, billing and observability displays; report queries over local tables; and the strata handoff. Out of scope: billing logic and metering writes, worker scheduling, artifact objects, code analysis, licence issuance, ingestion and embeddings as services, screens (S12) and project_request.repository_url (S07)."
      },
      "created_at": "2026-11-26T21:54:04.526424+00:00",
      "id": "01a219d9-f671-708a-b28f-063e7e79e6ec",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S09 Documents and integrations (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:54:04.836362+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Responsibilities

Who does the work and who accepts it.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a219d9-f717-73ba-9c86-18adeeecd4ef",
      "approved_at": "2026-11-26T21:54:04.836362+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The plan \u00a77 gate: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, then delete yields GET 404 and object_store::head NotFound, asserted in documents_roundtrip; (b) cap \u2014 10,485,760 bytes accepted and one byte more refused 413 application/problem+json with a stable type in documents_limits; (c) secret \u2014 rotate to version 2 and GET returns no plaintext, value key or ciphertext in secrets_never_revealed; (d) workers \u2014 the page renders live rows and a stopped service yields a banner with the breaker open after five failures; (e) reports \u2014 every number equals its SQL count on the same snapshot in reports_match_sql; (f) externals \u2014 GET /api/actions carries timeout_ms on every external and zero reqwest:: outside the executor in externals_declared; (g) billing \u2014 no write action and no metering event in the manifest. The same instrument also requires migration 0013 applied twice as a no-op, FORCE RLS on all four tables, and the nine declared event types. The snapshot records no run of these properties.",
        "assignment": "Rework the V1 slice S09 (SPEC \u00a78 S9) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, this charter, this Statement of Architecture Work and the delivery contract, take the scope baseline with one WBS deliverable, record a retrospective Phase-G review at level consistent, and publish the expected outputs project-brief and evidence. Because all nine linked issues are recorded closed, this rework re-closes the project at level consistent through the acceptance manifest, with the closure and closure review dated 2026-11-24.",
        "contract": "This Statement establishes the delivery contract against acceptance/programme/S09.toml, whose gate text names the seven properties above. Compliance is assessed clause by clause at the 2026-11-24 retrospective snapshot; the project's published evidence page is 01a1e687-7682-753d-b60e-1f00243b13a3 and its pre-rework audit pack is docs/evidence/rework/S09.json, which records verdict red on the missing baseline, charter, contract, Phase-G review and closure outputs. The rework supplies the lifecycle record and re-closes the project retrospectively on the P0 checks of the manifest; the remaining verification limits are carried in the closure review, the review and the evidence output rather than converted into a pass.",
        "responsibilities": "The programme architect (conveyor-e2e) owns the slice plan and this retrospective set; the Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract; the approver (conveyor-approver) decides the Phase-G level; the S03 team supplies tenancy, trace_link and revision, and the S04 team supplies the action registry with its OpenAPI and MCP projection. Operators own the KEK source in the pod environment and the MinIO bucket policy.",
        "scope": "the four tables document, repository, project_repository and secret in migrations/0013_documents.sql; the integrations module (object_store, external, breaker, ingest); documents.create/get/list/download/update/delete/report, 26 of the front matter's 27 actions in all; the nine event types; MinIO bucket conveyor-documents with key t/<tenant>/<document>/<version>; the KEK/HKDF/XChaCha20-Poly1305 secret envelope; repositories as references; the read-only workers, billing and observability displays; report queries over local tables; and the strata handoff. Out of scope: billing logic and metering writes, worker scheduling, artifact objects, code analysis, licence issuance, ingestion and embeddings as services, screens (S12) and project_request.repository_url (S07)."
      },
      "created_at": "2026-11-26T21:54:04.526424+00:00",
      "id": "01a219d9-f671-708a-b28f-063e7e79e6ec",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S09 Documents and integrations (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:54:04.836362+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Acceptance criteria

The conditions the architecture must meet to close Phase A.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a219d9-f717-73ba-9c86-18adeeecd4ef",
      "approved_at": "2026-11-26T21:54:04.836362+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The plan \u00a77 gate: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, then delete yields GET 404 and object_store::head NotFound, asserted in documents_roundtrip; (b) cap \u2014 10,485,760 bytes accepted and one byte more refused 413 application/problem+json with a stable type in documents_limits; (c) secret \u2014 rotate to version 2 and GET returns no plaintext, value key or ciphertext in secrets_never_revealed; (d) workers \u2014 the page renders live rows and a stopped service yields a banner with the breaker open after five failures; (e) reports \u2014 every number equals its SQL count on the same snapshot in reports_match_sql; (f) externals \u2014 GET /api/actions carries timeout_ms on every external and zero reqwest:: outside the executor in externals_declared; (g) billing \u2014 no write action and no metering event in the manifest. The same instrument also requires migration 0013 applied twice as a no-op, FORCE RLS on all four tables, and the nine declared event types. The snapshot records no run of these properties.",
        "assignment": "Rework the V1 slice S09 (SPEC \u00a78 S9) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, this charter, this Statement of Architecture Work and the delivery contract, take the scope baseline with one WBS deliverable, record a retrospective Phase-G review at level consistent, and publish the expected outputs project-brief and evidence. Because all nine linked issues are recorded closed, this rework re-closes the project at level consistent through the acceptance manifest, with the closure and closure review dated 2026-11-24.",
        "contract": "This Statement establishes the delivery contract against acceptance/programme/S09.toml, whose gate text names the seven properties above. Compliance is assessed clause by clause at the 2026-11-24 retrospective snapshot; the project's published evidence page is 01a1e687-7682-753d-b60e-1f00243b13a3 and its pre-rework audit pack is docs/evidence/rework/S09.json, which records verdict red on the missing baseline, charter, contract, Phase-G review and closure outputs. The rework supplies the lifecycle record and re-closes the project retrospectively on the P0 checks of the manifest; the remaining verification limits are carried in the closure review, the review and the evidence output rather than converted into a pass.",
        "responsibilities": "The programme architect (conveyor-e2e) owns the slice plan and this retrospective set; the Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract; the approver (conveyor-approver) decides the Phase-G level; the S03 team supplies tenancy, trace_link and revision, and the S04 team supplies the action registry with its OpenAPI and MCP projection. Operators own the KEK source in the pod environment and the MinIO bucket policy.",
        "scope": "the four tables document, repository, project_repository and secret in migrations/0013_documents.sql; the integrations module (object_store, external, breaker, ingest); documents.create/get/list/download/update/delete/report, 26 of the front matter's 27 actions in all; the nine event types; MinIO bucket conveyor-documents with key t/<tenant>/<document>/<version>; the KEK/HKDF/XChaCha20-Poly1305 secret envelope; repositories as references; the read-only workers, billing and observability displays; report queries over local tables; and the strata handoff. Out of scope: billing logic and metering writes, worker scheduling, artifact objects, code analysis, licence issuance, ingestion and embeddings as services, screens (S12) and project_request.repository_url (S07)."
      },
      "created_at": "2026-11-26T21:54:04.526424+00:00",
      "id": "01a219d9-f671-708a-b28f-063e7e79e6ec",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S09 Documents and integrations (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:54:04.836362+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Contract

The contract record, its version and its approval.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a219d9-f717-73ba-9c86-18adeeecd4ef",
      "approved_at": "2026-11-26T21:54:04.836362+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The plan \u00a77 gate: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, then delete yields GET 404 and object_store::head NotFound, asserted in documents_roundtrip; (b) cap \u2014 10,485,760 bytes accepted and one byte more refused 413 application/problem+json with a stable type in documents_limits; (c) secret \u2014 rotate to version 2 and GET returns no plaintext, value key or ciphertext in secrets_never_revealed; (d) workers \u2014 the page renders live rows and a stopped service yields a banner with the breaker open after five failures; (e) reports \u2014 every number equals its SQL count on the same snapshot in reports_match_sql; (f) externals \u2014 GET /api/actions carries timeout_ms on every external and zero reqwest:: outside the executor in externals_declared; (g) billing \u2014 no write action and no metering event in the manifest. The same instrument also requires migration 0013 applied twice as a no-op, FORCE RLS on all four tables, and the nine declared event types. The snapshot records no run of these properties.",
        "assignment": "Rework the V1 slice S09 (SPEC \u00a78 S9) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, this charter, this Statement of Architecture Work and the delivery contract, take the scope baseline with one WBS deliverable, record a retrospective Phase-G review at level consistent, and publish the expected outputs project-brief and evidence. Because all nine linked issues are recorded closed, this rework re-closes the project at level consistent through the acceptance manifest, with the closure and closure review dated 2026-11-24.",
        "contract": "This Statement establishes the delivery contract against acceptance/programme/S09.toml, whose gate text names the seven properties above. Compliance is assessed clause by clause at the 2026-11-24 retrospective snapshot; the project's published evidence page is 01a1e687-7682-753d-b60e-1f00243b13a3 and its pre-rework audit pack is docs/evidence/rework/S09.json, which records verdict red on the missing baseline, charter, contract, Phase-G review and closure outputs. The rework supplies the lifecycle record and re-closes the project retrospectively on the P0 checks of the manifest; the remaining verification limits are carried in the closure review, the review and the evidence output rather than converted into a pass.",
        "responsibilities": "The programme architect (conveyor-e2e) owns the slice plan and this retrospective set; the Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract; the approver (conveyor-approver) decides the Phase-G level; the S03 team supplies tenancy, trace_link and revision, and the S04 team supplies the action registry with its OpenAPI and MCP projection. Operators own the KEK source in the pod environment and the MinIO bucket policy.",
        "scope": "the four tables document, repository, project_repository and secret in migrations/0013_documents.sql; the integrations module (object_store, external, breaker, ingest); documents.create/get/list/download/update/delete/report, 26 of the front matter's 27 actions in all; the nine event types; MinIO bucket conveyor-documents with key t/<tenant>/<document>/<version>; the KEK/HKDF/XChaCha20-Poly1305 secret envelope; repositories as references; the read-only workers, billing and observability displays; report queries over local tables; and the strata handoff. Out of scope: billing logic and metering writes, worker scheduling, artifact objects, code analysis, licence issuance, ingestion and embeddings as services, screens (S12) and project_request.repository_url (S07)."
      },
      "created_at": "2026-11-26T21:54:04.526424+00:00",
      "id": "01a219d9-f671-708a-b28f-063e7e79e6ec",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2d-71f1-a403-c9abe52c1888",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S09 Documents and integrations (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:54:04.836362+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

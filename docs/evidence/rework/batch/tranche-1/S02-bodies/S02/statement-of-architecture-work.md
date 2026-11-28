# Statement Of Architecture Work — Skeleton — workspace, tenancy, auth, jobs, SSE

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a1ff84-57c3-7f67-9229-94bf0188732d",
      "approved_at": "2026-11-21T19:10:30.370289+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S02.toml is green against the published snapshot at the recorded SHA, with the S2 gate proven: two tenants isolated including RLS, and login works. The long-job lease/recovery proof under two workers was issue #2's P2; it is proved by `apps/server/tests/jobs_lease_two_workers.rs` at d367a4a9, and issue #2 closes with the lane PR.",
        "assignment": "Rework the delivered S02 skeleton by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526, tranche 1), recording the A\u2013D artefacts, the delivery contract and the Phase-G compliance review, and closing the project once the long-job proof issue #2 named is landed in this batch.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The ten-member workspace and one `conveyor` binary delivered by S02: eleven platform tables in `migrations/0001_platform.sql`, forced RLS on the nine tenant-owned tables, OIDC and development authentication, the fail-closed `app_licence` gate, the fenced job runner, the replaying `/api/events` stream, and the declared `shell/login` surface for S12."
      },
      "created_at": "2026-11-21T19:10:20.569282+00:00",
      "id": "01a1ff84-4425-75d9-8772-08e09cf71069",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S02 Skeleton (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:10:30.370289+00:00",
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
      "approval_id": "01a1ff84-57c3-7f67-9229-94bf0188732d",
      "approved_at": "2026-11-21T19:10:30.370289+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S02.toml is green against the published snapshot at the recorded SHA, with the S2 gate proven: two tenants isolated including RLS, and login works. The long-job lease/recovery proof under two workers was issue #2's P2; it is proved by `apps/server/tests/jobs_lease_two_workers.rs` at d367a4a9, and issue #2 closes with the lane PR.",
        "assignment": "Rework the delivered S02 skeleton by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526, tranche 1), recording the A\u2013D artefacts, the delivery contract and the Phase-G compliance review, and closing the project once the long-job proof issue #2 named is landed in this batch.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The ten-member workspace and one `conveyor` binary delivered by S02: eleven platform tables in `migrations/0001_platform.sql`, forced RLS on the nine tenant-owned tables, OIDC and development authentication, the fail-closed `app_licence` gate, the fenced job runner, the replaying `/api/events` stream, and the declared `shell/login` surface for S12."
      },
      "created_at": "2026-11-21T19:10:20.569282+00:00",
      "id": "01a1ff84-4425-75d9-8772-08e09cf71069",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S02 Skeleton (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:10:30.370289+00:00",
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
      "approval_id": "01a1ff84-57c3-7f67-9229-94bf0188732d",
      "approved_at": "2026-11-21T19:10:30.370289+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S02.toml is green against the published snapshot at the recorded SHA, with the S2 gate proven: two tenants isolated including RLS, and login works. The long-job lease/recovery proof under two workers was issue #2's P2; it is proved by `apps/server/tests/jobs_lease_two_workers.rs` at d367a4a9, and issue #2 closes with the lane PR.",
        "assignment": "Rework the delivered S02 skeleton by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526, tranche 1), recording the A\u2013D artefacts, the delivery contract and the Phase-G compliance review, and closing the project once the long-job proof issue #2 named is landed in this batch.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The ten-member workspace and one `conveyor` binary delivered by S02: eleven platform tables in `migrations/0001_platform.sql`, forced RLS on the nine tenant-owned tables, OIDC and development authentication, the fail-closed `app_licence` gate, the fenced job runner, the replaying `/api/events` stream, and the declared `shell/login` surface for S12."
      },
      "created_at": "2026-11-21T19:10:20.569282+00:00",
      "id": "01a1ff84-4425-75d9-8772-08e09cf71069",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S02 Skeleton (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:10:30.370289+00:00",
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
      "approval_id": "01a1ff84-57c3-7f67-9229-94bf0188732d",
      "approved_at": "2026-11-21T19:10:30.370289+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S02.toml is green against the published snapshot at the recorded SHA, with the S2 gate proven: two tenants isolated including RLS, and login works. The long-job lease/recovery proof under two workers was issue #2's P2; it is proved by `apps/server/tests/jobs_lease_two_workers.rs` at d367a4a9, and issue #2 closes with the lane PR.",
        "assignment": "Rework the delivered S02 skeleton by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526, tranche 1), recording the A\u2013D artefacts, the delivery contract and the Phase-G compliance review, and closing the project once the long-job proof issue #2 named is landed in this batch.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The ten-member workspace and one `conveyor` binary delivered by S02: eleven platform tables in `migrations/0001_platform.sql`, forced RLS on the nine tenant-owned tables, OIDC and development authentication, the fail-closed `app_licence` gate, the fenced job runner, the replaying `/api/events` stream, and the declared `shell/login` surface for S12."
      },
      "created_at": "2026-11-21T19:10:20.569282+00:00",
      "id": "01a1ff84-4425-75d9-8772-08e09cf71069",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S02 Skeleton (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:10:30.370289+00:00",
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
      "approval_id": "01a1ff84-57c3-7f67-9229-94bf0188732d",
      "approved_at": "2026-11-21T19:10:30.370289+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/S02.toml is green against the published snapshot at the recorded SHA, with the S2 gate proven: two tenants isolated including RLS, and login works. The long-job lease/recovery proof under two workers was issue #2's P2; it is proved by `apps/server/tests/jobs_lease_two_workers.rs` at d367a4a9, and issue #2 closes with the lane PR.",
        "assignment": "Rework the delivered S02 skeleton by the reconstituted MSP/TOGAF lifecycle (U5a, issue #526, tranche 1), recording the A\u2013D artefacts, the delivery contract and the Phase-G compliance review, and closing the project once the long-job proof issue #2 named is landed in this batch.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The ten-member workspace and one `conveyor` binary delivered by S02: eleven platform tables in `migrations/0001_platform.sql`, forced RLS on the nine tenant-owned tables, OIDC and development authentication, the fail-closed `app_licence` gate, the fenced job runner, the replaying `/api/events` stream, and the declared `shell/login` surface for S12."
      },
      "created_at": "2026-11-21T19:10:20.569282+00:00",
      "id": "01a1ff84-4425-75d9-8772-08e09cf71069",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S02 Skeleton (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:10:30.370289+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

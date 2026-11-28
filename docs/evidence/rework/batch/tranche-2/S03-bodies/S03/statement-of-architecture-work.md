# Statement Of Architecture Work — Domain and schema — aggregates, state machines, baseline domain migration

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a20061-d19f-7269-9f5c-113f3aeb0001",
      "approved_at": "2026-11-21T23:12:26.539242+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC \u00a78 S3 gate: proptest on transitions and schema review. Property (a) \u2014 no transition leaves list.kind and archived_at contradictory, proved by board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error. Property (b) \u2014 claim not granted twice without release or expiry, proved as tested by claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic, with the lease-exclusivity half recorded as the P1 package v1-s03-1. Property (c) \u2014 one pending approval per (run, gate), proved by approval_race.rs::sixteen_racers_create_exactly_one_pending_approval. Property (d) \u2014 terminal run and dense revision, proved by transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense. Tenant isolation is proved by rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema review against data-arch \u00a72.3/\u00a72.4 admits the P2 package v1-s03-2. The rework's acceptance instrument is acceptance/programme/S03.toml; the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V1 slice S03 (SPEC \u00a76, \u00a78 S3) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate on the P0-only gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the baseline domain of SPEC \u00a75.3 and \u00a76: the fourteen tables of migrations/0002_domain.sql (programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state), the may_move matrix and the run/approval/asset state machines, the store `_tx` modules with TenantId, and the catalogue_entry.kind vocabulary. Out of scope: every route and action (S04\u2013S06), the board satellites (S05), channels and delivery (S06), outlines, quality and project_request (S07), models, views and portfolio registers (S08), documents and secrets (S09), conversations (S10), procedures (S11), workflow (S13) and the capability columns (S15)."
      },
      "created_at": "2026-11-21T23:12:15.079018+00:00",
      "id": "01a20061-bd73-7a89-b08c-bd8b867eb061",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S03 Domain and schema (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T23:12:26.539242+00:00",
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
      "approval_id": "01a20061-d19f-7269-9f5c-113f3aeb0001",
      "approved_at": "2026-11-21T23:12:26.539242+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC \u00a78 S3 gate: proptest on transitions and schema review. Property (a) \u2014 no transition leaves list.kind and archived_at contradictory, proved by board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error. Property (b) \u2014 claim not granted twice without release or expiry, proved as tested by claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic, with the lease-exclusivity half recorded as the P1 package v1-s03-1. Property (c) \u2014 one pending approval per (run, gate), proved by approval_race.rs::sixteen_racers_create_exactly_one_pending_approval. Property (d) \u2014 terminal run and dense revision, proved by transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense. Tenant isolation is proved by rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema review against data-arch \u00a72.3/\u00a72.4 admits the P2 package v1-s03-2. The rework's acceptance instrument is acceptance/programme/S03.toml; the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V1 slice S03 (SPEC \u00a76, \u00a78 S3) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate on the P0-only gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the baseline domain of SPEC \u00a75.3 and \u00a76: the fourteen tables of migrations/0002_domain.sql (programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state), the may_move matrix and the run/approval/asset state machines, the store `_tx` modules with TenantId, and the catalogue_entry.kind vocabulary. Out of scope: every route and action (S04\u2013S06), the board satellites (S05), channels and delivery (S06), outlines, quality and project_request (S07), models, views and portfolio registers (S08), documents and secrets (S09), conversations (S10), procedures (S11), workflow (S13) and the capability columns (S15)."
      },
      "created_at": "2026-11-21T23:12:15.079018+00:00",
      "id": "01a20061-bd73-7a89-b08c-bd8b867eb061",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S03 Domain and schema (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T23:12:26.539242+00:00",
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
      "approval_id": "01a20061-d19f-7269-9f5c-113f3aeb0001",
      "approved_at": "2026-11-21T23:12:26.539242+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC \u00a78 S3 gate: proptest on transitions and schema review. Property (a) \u2014 no transition leaves list.kind and archived_at contradictory, proved by board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error. Property (b) \u2014 claim not granted twice without release or expiry, proved as tested by claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic, with the lease-exclusivity half recorded as the P1 package v1-s03-1. Property (c) \u2014 one pending approval per (run, gate), proved by approval_race.rs::sixteen_racers_create_exactly_one_pending_approval. Property (d) \u2014 terminal run and dense revision, proved by transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense. Tenant isolation is proved by rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema review against data-arch \u00a72.3/\u00a72.4 admits the P2 package v1-s03-2. The rework's acceptance instrument is acceptance/programme/S03.toml; the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V1 slice S03 (SPEC \u00a76, \u00a78 S3) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate on the P0-only gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the baseline domain of SPEC \u00a75.3 and \u00a76: the fourteen tables of migrations/0002_domain.sql (programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state), the may_move matrix and the run/approval/asset state machines, the store `_tx` modules with TenantId, and the catalogue_entry.kind vocabulary. Out of scope: every route and action (S04\u2013S06), the board satellites (S05), channels and delivery (S06), outlines, quality and project_request (S07), models, views and portfolio registers (S08), documents and secrets (S09), conversations (S10), procedures (S11), workflow (S13) and the capability columns (S15)."
      },
      "created_at": "2026-11-21T23:12:15.079018+00:00",
      "id": "01a20061-bd73-7a89-b08c-bd8b867eb061",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S03 Domain and schema (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T23:12:26.539242+00:00",
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
      "approval_id": "01a20061-d19f-7269-9f5c-113f3aeb0001",
      "approved_at": "2026-11-21T23:12:26.539242+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC \u00a78 S3 gate: proptest on transitions and schema review. Property (a) \u2014 no transition leaves list.kind and archived_at contradictory, proved by board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error. Property (b) \u2014 claim not granted twice without release or expiry, proved as tested by claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic, with the lease-exclusivity half recorded as the P1 package v1-s03-1. Property (c) \u2014 one pending approval per (run, gate), proved by approval_race.rs::sixteen_racers_create_exactly_one_pending_approval. Property (d) \u2014 terminal run and dense revision, proved by transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense. Tenant isolation is proved by rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema review against data-arch \u00a72.3/\u00a72.4 admits the P2 package v1-s03-2. The rework's acceptance instrument is acceptance/programme/S03.toml; the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V1 slice S03 (SPEC \u00a76, \u00a78 S3) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate on the P0-only gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the baseline domain of SPEC \u00a75.3 and \u00a76: the fourteen tables of migrations/0002_domain.sql (programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state), the may_move matrix and the run/approval/asset state machines, the store `_tx` modules with TenantId, and the catalogue_entry.kind vocabulary. Out of scope: every route and action (S04\u2013S06), the board satellites (S05), channels and delivery (S06), outlines, quality and project_request (S07), models, views and portfolio registers (S08), documents and secrets (S09), conversations (S10), procedures (S11), workflow (S13) and the capability columns (S15)."
      },
      "created_at": "2026-11-21T23:12:15.079018+00:00",
      "id": "01a20061-bd73-7a89-b08c-bd8b867eb061",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S03 Domain and schema (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T23:12:26.539242+00:00",
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
      "approval_id": "01a20061-d19f-7269-9f5c-113f3aeb0001",
      "approved_at": "2026-11-21T23:12:26.539242+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC \u00a78 S3 gate: proptest on transitions and schema review. Property (a) \u2014 no transition leaves list.kind and archived_at contradictory, proved by board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error. Property (b) \u2014 claim not granted twice without release or expiry, proved as tested by claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic, with the lease-exclusivity half recorded as the P1 package v1-s03-1. Property (c) \u2014 one pending approval per (run, gate), proved by approval_race.rs::sixteen_racers_create_exactly_one_pending_approval. Property (d) \u2014 terminal run and dense revision, proved by transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense. Tenant isolation is proved by rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema review against data-arch \u00a72.3/\u00a72.4 admits the P2 package v1-s03-2. The rework's acceptance instrument is acceptance/programme/S03.toml; the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V1 slice S03 (SPEC \u00a76, \u00a78 S3) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate on the P0-only gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the baseline domain of SPEC \u00a75.3 and \u00a76: the fourteen tables of migrations/0002_domain.sql (programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state), the may_move matrix and the run/approval/asset state machines, the store `_tx` modules with TenantId, and the catalogue_entry.kind vocabulary. Out of scope: every route and action (S04\u2013S06), the board satellites (S05), channels and delivery (S06), outlines, quality and project_request (S07), models, views and portfolio registers (S08), documents and secrets (S09), conversations (S10), procedures (S11), workflow (S13) and the capability columns (S15)."
      },
      "created_at": "2026-11-21T23:12:15.079018+00:00",
      "id": "01a20061-bd73-7a89-b08c-bd8b867eb061",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S03 Domain and schema (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T23:12:26.539242+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

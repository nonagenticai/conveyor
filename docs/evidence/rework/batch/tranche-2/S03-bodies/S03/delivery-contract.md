# Delivery Contract — Domain and schema — aggregates, state machines, baseline domain migration

## Commitments

The record the section derives from, or an explicit absence.

_Explicit absence — the contract's commitment list: a commitments field on the delivery contract body._

## Compliance method

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a20063-3a7d-7e67-a437-2e0884f6c46a",
      "field": "compliance",
      "kind": "contract_field",
      "title": "compliance: S03 is assessed clause by clause against the SPEC \u00a78 S3 gate and the slice plan \u00a77 gate. The four gate properties are carried by named tests: board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error (a), claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic (b), approval_race.rs::sixteen_racers_create_exactly_one_pending_approval (c), transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense (d). Tenant isolation is rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema-review clause is met with two admitted packages: v1-s03-1 (P1) records that property (b)'s lease-exclusivity half is not exercised, and v1-s03-2 (P2) records that the archive-exclusion claim covers only the two board-scan indexes. The Phase-G review records the compliance level against this contract; the rework's acceptance instrument is acceptance/programme/S03.toml, and the closure gate is decided by an approver other than its requester on the P0-only gate.",
      "value": "S03 is assessed clause by clause against the SPEC \u00a78 S3 gate and the slice plan \u00a77 gate. The four gate properties are carried by named tests: board.rs::archived_implies_the_card_sits_in_an_archive_list and ::restore_off_a_non_archive_list_is_an_error (a), claim_race.rs::thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic (b), approval_race.rs::sixteen_racers_create_exactly_one_pending_approval (c), transitions.rs::a_terminal_run_rejects_every_transition_and_a_revision_is_dense (d). Tenant isolation is rls_domain.rs::the_fourteen_domain_tables_isolate_runtime_owner_and_superuser. The schema-review clause is met with two admitted packages: v1-s03-1 (P1) records that property (b)'s lease-exclusivity half is not exercised, and v1-s03-2 (P2) records that the archive-exclusion claim covers only the two board-scan indexes. The Phase-G review records the compliance level against this contract; the rework's acceptance instrument is acceptance/programme/S03.toml, and the closure gate is decided by an approver other than its requester on the P0-only gate."
    }
  ],
  "source": "computed:contract_field"
}
```

## Approval

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_at": "2026-11-21T23:14:03.417876+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a20063-5009-7a66-aaa2-5f2876187e88",
      "kind": "contract_approval",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "approved",
      "title": "approval 01a20063-5009-7a66-aaa2-5f2876187e88 \u2014 approved"
    }
  ],
  "source": "computed:contract_approval"
}
```

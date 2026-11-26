# Compliance Review — Skeleton — workspace, tenancy, auth, jobs, SSE

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 2,
  "rows": [
    {
      "contract_id": "01a1ff85-bda3-7671-a5b1-42eae3b0e6ee",
      "created_at": "2026-11-21T19:13:26.122519+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S02.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "local_run": "nextest run -p conveyor-server --test jobs_lease_two_workers -> PASS (11.878 s)",
        "manifest": "acceptance/programme/S02.toml",
        "preflight": "44938697512 (preflight.yml, success)",
        "proof": "apps/server/tests/jobs_lease_two_workers.rs",
        "sha": "d367a4a992ca5338c82466e9d6ef308e62b01ba6"
      },
      "findings": "Recorded retrospectively on 2026-11-21 under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. Level compliant: S02's Statement of Architecture Work, delivery contract and A\u2013D artefacts are recorded against the delivered tree (migrations/0001_platform.sql; crates/store, crates/auth, crates/api; compose.yaml). The S2 gate's two-tenant isolation and login proofs are carried by apps/server/tests/rls.rs and the dev principal path; the append-only audit, fail-closed licence, SSE replay and SIGKILL/lease-reclaim proofs are carried by the named tests and modules. The long-job lease/recovery proof under two concurrent workers was outstanding: issue #2 named a test that did not exist on main or the branch. This batch lands apps/server/tests/jobs_lease_two_workers.rs at d367a4a9; two real `conveyor worker` children poll the same tenant while a stub holds one licence.sync handler for four leases, and P2.1\u2013P2.3 are asserted from the row and the stub counter (attempts stays 1, the handler runs once, the finish is succeeded and the lease is cleared). Local run: `nextest run -p conveyor-server --test jobs_lease_two_workers` -> PASS (11.878 s); preflight 44938697512 (success). The acceptance is met at the branch head; issue #2 remains open only as the delivery gate and closes with the lane PR (`Closes #2`).",
      "id": "01a1ff87-19a4-783f-85c5-033d5f38cedf",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "reviewed_at": "2026-11-21T19:13:26.122519+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-21T19:13:26.122519+00:00",
      "version": 1
    },
    {
      "contract_id": "01a217e8-45db-7e4e-92b4-b41cde590b15",
      "created_at": "2026-11-26T12:50:33.931575+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S02.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "delivery_contract": "01a1ff85-bda3-7671-a5b1-42eae3b0e6ee",
        "live_review": "01a1ff87-19a4-783f-85c5-033d5f38cedf",
        "manifest": "acceptance/programme/S02.toml",
        "rework_note": "docs/evidence/rework/S02/transcript.jsonl",
        "scope_baseline": "01a1ff85-faae-7c2e-b897-fe957985ca78",
        "sha": "b0449db045a18004c19bbfbd87dd39acedad85a8"
      },
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live Phase-G review 01a1ff87-19a4-783f-85c5-033d5f38cedf is compliant against the delivery contract 01a1ff85-bda3-7671-a5b1-42eae3b0e6ee, the contract is approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545 rather than the requester 01a1e607-17d9-77d3-98d3-a43a87c4633d, and the scope baseline 01a1ff85-faae-7c2e-b897-fe957985ca78 is approved. The skeleton's own gate is the plan's \u00a77: two tenants isolated under forced RLS, login, the kill and two-worker lease proofs, SSE replay, the append-only audit and the fail-closed licence. The rework adds the seven A\u2013D artefacts, the registers and the closure review that the earlier attempt left unretained; it does not re-open the boundary. scripts/verify-project.py --project S02 --snapshot <fresh> is the receipt that must be green with zero failures.",
      "id": "01a217e8-5d50-7b9c-b76a-4dc0eee5d6e8",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "reviewed_at": "2026-11-26T12:50:33.931575+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T12:50:33.931575+00:00",
      "version": 1
    }
  ],
  "source": "architecture.json#/compliance_reviews"
}
```

## Findings

The evidence for each finding, against the contract.

```json
{
  "category": "compliance-review",
  "count": 2,
  "rows": [
    {
      "contract_id": "01a1ff85-bda3-7671-a5b1-42eae3b0e6ee",
      "created_at": "2026-11-21T19:13:26.122519+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S02.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "local_run": "nextest run -p conveyor-server --test jobs_lease_two_workers -> PASS (11.878 s)",
        "manifest": "acceptance/programme/S02.toml",
        "preflight": "44938697512 (preflight.yml, success)",
        "proof": "apps/server/tests/jobs_lease_two_workers.rs",
        "sha": "d367a4a992ca5338c82466e9d6ef308e62b01ba6"
      },
      "findings": "Recorded retrospectively on 2026-11-21 under delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. Level compliant: S02's Statement of Architecture Work, delivery contract and A\u2013D artefacts are recorded against the delivered tree (migrations/0001_platform.sql; crates/store, crates/auth, crates/api; compose.yaml). The S2 gate's two-tenant isolation and login proofs are carried by apps/server/tests/rls.rs and the dev principal path; the append-only audit, fail-closed licence, SSE replay and SIGKILL/lease-reclaim proofs are carried by the named tests and modules. The long-job lease/recovery proof under two concurrent workers was outstanding: issue #2 named a test that did not exist on main or the branch. This batch lands apps/server/tests/jobs_lease_two_workers.rs at d367a4a9; two real `conveyor worker` children poll the same tenant while a stub holds one licence.sync handler for four leases, and P2.1\u2013P2.3 are asserted from the row and the stub counter (attempts stays 1, the handler runs once, the finish is succeeded and the lease is cleared). Local run: `nextest run -p conveyor-server --test jobs_lease_two_workers` -> PASS (11.878 s); preflight 44938697512 (success). The acceptance is met at the branch head; issue #2 remains open only as the delivery gate and closes with the lane PR (`Closes #2`).",
      "id": "01a1ff87-19a4-783f-85c5-033d5f38cedf",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "reviewed_at": "2026-11-21T19:13:26.122519+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-21T19:13:26.122519+00:00",
      "version": 1
    },
    {
      "contract_id": "01a217e8-45db-7e4e-92b4-b41cde590b15",
      "created_at": "2026-11-26T12:50:33.931575+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S02.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "delivery_contract": "01a1ff85-bda3-7671-a5b1-42eae3b0e6ee",
        "live_review": "01a1ff87-19a4-783f-85c5-033d5f38cedf",
        "manifest": "acceptance/programme/S02.toml",
        "rework_note": "docs/evidence/rework/S02/transcript.jsonl",
        "scope_baseline": "01a1ff85-faae-7c2e-b897-fe957985ca78",
        "sha": "b0449db045a18004c19bbfbd87dd39acedad85a8"
      },
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live Phase-G review 01a1ff87-19a4-783f-85c5-033d5f38cedf is compliant against the delivery contract 01a1ff85-bda3-7671-a5b1-42eae3b0e6ee, the contract is approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545 rather than the requester 01a1e607-17d9-77d3-98d3-a43a87c4633d, and the scope baseline 01a1ff85-faae-7c2e-b897-fe957985ca78 is approved. The skeleton's own gate is the plan's \u00a77: two tenants isolated under forced RLS, login, the kill and two-worker lease proofs, SSE replay, the append-only audit and the fail-closed licence. The rework adds the seven A\u2013D artefacts, the registers and the closure review that the earlier attempt left unretained; it does not re-open the boundary. scripts/verify-project.py --project S02 --snapshot <fresh> is the receipt that must be green with zero failures.",
      "id": "01a217e8-5d50-7b9c-b76a-4dc0eee5d6e8",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d27-7b45-abb5-bf851a4c3d4a",
      "reviewed_at": "2026-11-26T12:50:33.931575+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T12:50:33.931575+00:00",
      "version": 1
    }
  ],
  "source": "architecture.json#/compliance_reviews"
}
```

## Waivers

Each accepted deviation with its owner, reason, cost, risk and reassessment date.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "kind": "waiver_status",
      "title": "None recorded \u2014 0 waivers for this project",
      "waivers": 0
    }
  ],
  "source": "computed:waiver_status"
}
```

## Decision

The outcome and the conditions that attach to it.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "conditions": null,
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live Phase-G review 01a1ff87-19a4-783f-85c5-033d5f38cedf is compliant against the delivery contract 01a1ff85-bda3-7671-a5b1-42eae3b0e6ee, the contract is approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545 rather than the requester 01a1e607-17d9-77d3-98d3-a43a87c4633d, and the scope baseline 01a1ff85-faae-7c2e-b897-fe957985ca78 is approved. The skeleton's own gate is the plan's \u00a77: two tenants isolated under forced RLS, login, the kill and two-worker lease proofs, SSE replay, the append-only audit and the fail-closed licence. The rework adds the seven A\u2013D artefacts, the registers and the closure review that the earlier attempt left unretained; it does not re-open the boundary. scripts/verify-project.py --project S02 --snapshot <fresh> is the receipt that must be green with zero failures.",
      "id": "01a217e8-5d50-7b9c-b76a-4dc0eee5d6e8",
      "kind": "governing_review",
      "level": "compliant",
      "outcome": null,
      "reviewed_at": "2026-11-26T12:50:33.931575+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a217e8-5d50-7b9c-b76a-4dc0eee5d6e8: compliant / no outcome"
    }
  ],
  "source": "computed:governing_review"
}
```

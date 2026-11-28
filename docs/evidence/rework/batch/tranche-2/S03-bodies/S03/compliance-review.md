# Compliance Review — Domain and schema — aggregates, state machines, baseline domain migration

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "contract_id": "01a20063-3a7d-7e67-a437-2e0884f6c46a",
      "created_at": "2026-11-21T23:17:06.610436+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S03.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/S03.toml",
        "sha": "7ff27efd65146630e62d9c235458be789fb68c14"
      },
      "findings": "This retrospective Phase-G review records the compliance level compliant for S03 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 (the published decision 'Standing delegation extension \u2014 the U5 programme rework (2026-11-21)'). The close carried a mandatory_artefacts gap: at the 2026-11-21T19:24:46Z rework snapshot the seven Phase A\u2013D artefacts, the Statement of Architecture Work and the delivery contract were absent, and the architecture compliance-review list was empty. This rework produces and publishes stakeholder_map, driver_goal_objective, requirements_catalog and solution_concept (Phase A), business_capability_map (Phase B), data_entity_catalog (Phase C) and technology_standards (Phase D), and records this review. Two findings are admitted by this batch. v1-s03-1 (P1): property (b) claims claim-exclusivity, but claim_card_tx lets the same assignee re-claim an unexpired lease through its third arm (OR assignee_id = $3), and the 32-racer test in crates/store/tests/claim_race.rs shares one user (let who = seed.user) and one version (Version::INITIAL), so the one-winner assertion is decided by the version predicate and the test would pass with the lease arm deleted. v1-s03-2 (P2): the plan's claim that every hot index filters archived cards is true only of the two board-scan indexes, card_board_list_position_idx and card_board_position_idx; card_assignee_lease_idx and card_tsv_idx include stamped cards, and the later card_name_trgm_idx and card_wbs_node_idx add no predicate. Both findings are packages owned by S03. A P1 does not block closure under this programme's manifest: the closure is on the P0-only gate of acceptance/programme/S03.toml, and neither finding is a P0. No original Phase-G review existed; this one is retrospective and does not attest contemporaneous intent. The live project had no WBS node and no project baseline at the snapshot; this rework takes the scope baseline from the one WBS deliverable, and the frozen-snapshot behaviour is proved elsewhere by T1's baseline test.",
      "id": "01a20066-3107-7b16-8835-6b6cd2aa8c84",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "reviewed_at": "2026-11-21T23:17:06.610436+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-21T23:17:06.610436+00:00",
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
  "count": 1,
  "rows": [
    {
      "contract_id": "01a20063-3a7d-7e67-a437-2e0884f6c46a",
      "created_at": "2026-11-21T23:17:06.610436+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S03.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/S03.toml",
        "sha": "7ff27efd65146630e62d9c235458be789fb68c14"
      },
      "findings": "This retrospective Phase-G review records the compliance level compliant for S03 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 (the published decision 'Standing delegation extension \u2014 the U5 programme rework (2026-11-21)'). The close carried a mandatory_artefacts gap: at the 2026-11-21T19:24:46Z rework snapshot the seven Phase A\u2013D artefacts, the Statement of Architecture Work and the delivery contract were absent, and the architecture compliance-review list was empty. This rework produces and publishes stakeholder_map, driver_goal_objective, requirements_catalog and solution_concept (Phase A), business_capability_map (Phase B), data_entity_catalog (Phase C) and technology_standards (Phase D), and records this review. Two findings are admitted by this batch. v1-s03-1 (P1): property (b) claims claim-exclusivity, but claim_card_tx lets the same assignee re-claim an unexpired lease through its third arm (OR assignee_id = $3), and the 32-racer test in crates/store/tests/claim_race.rs shares one user (let who = seed.user) and one version (Version::INITIAL), so the one-winner assertion is decided by the version predicate and the test would pass with the lease arm deleted. v1-s03-2 (P2): the plan's claim that every hot index filters archived cards is true only of the two board-scan indexes, card_board_list_position_idx and card_board_position_idx; card_assignee_lease_idx and card_tsv_idx include stamped cards, and the later card_name_trgm_idx and card_wbs_node_idx add no predicate. Both findings are packages owned by S03. A P1 does not block closure under this programme's manifest: the closure is on the P0-only gate of acceptance/programme/S03.toml, and neither finding is a P0. No original Phase-G review existed; this one is retrospective and does not attest contemporaneous intent. The live project had no WBS node and no project baseline at the snapshot; this rework takes the scope baseline from the one WBS deliverable, and the frozen-snapshot behaviour is proved elsewhere by T1's baseline test.",
      "id": "01a20066-3107-7b16-8835-6b6cd2aa8c84",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d28-743b-86c7-0d0698ed69c1",
      "reviewed_at": "2026-11-21T23:17:06.610436+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-21T23:17:06.610436+00:00",
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
      "conditions": "none recorded",
      "decision": "compliant",
      "findings": "This retrospective Phase-G review records the compliance level compliant for S03 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 (the published decision 'Standing delegation extension \u2014 the U5 programme rework (2026-11-21)'). The close carried a mandatory_artefacts gap: at the 2026-11-21T19:24:46Z rework snapshot the seven Phase A\u2013D artefacts, the Statement of Architecture Work and the delivery contract were absent, and the architecture compliance-review list was empty. This rework produces and publishes stakeholder_map, driver_goal_objective, requirements_catalog and solution_concept (Phase A), business_capability_map (Phase B), data_entity_catalog (Phase C) and technology_standards (Phase D), and records this review. Two findings are admitted by this batch. v1-s03-1 (P1): property (b) claims claim-exclusivity, but claim_card_tx lets the same assignee re-claim an unexpired lease through its third arm (OR assignee_id = $3), and the 32-racer test in crates/store/tests/claim_race.rs shares one user (let who = seed.user) and one version (Version::INITIAL), so the one-winner assertion is decided by the version predicate and the test would pass with the lease arm deleted. v1-s03-2 (P2): the plan's claim that every hot index filters archived cards is true only of the two board-scan indexes, card_board_list_position_idx and card_board_position_idx; card_assignee_lease_idx and card_tsv_idx include stamped cards, and the later card_name_trgm_idx and card_wbs_node_idx add no predicate. Both findings are packages owned by S03. A P1 does not block closure under this programme's manifest: the closure is on the P0-only gate of acceptance/programme/S03.toml, and neither finding is a P0. No original Phase-G review existed; this one is retrospective and does not attest contemporaneous intent. The live project had no WBS node and no project baseline at the snapshot; this rework takes the scope baseline from the one WBS deliverable, and the frozen-snapshot behaviour is proved elsewhere by T1's baseline test.",
      "id": "01a20066-3107-7b16-8835-6b6cd2aa8c84",
      "kind": "governing_review",
      "level": "compliant",
      "phase_h_outcome": null,
      "reviewed_at": "2026-11-21T23:17:06.610436+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a20066-3107-7b16-8835-6b6cd2aa8c84: compliant governs; phase-H outcome not recorded (a phase-G decision is its level)"
    }
  ],
  "source": "computed:governing_review"
}
```

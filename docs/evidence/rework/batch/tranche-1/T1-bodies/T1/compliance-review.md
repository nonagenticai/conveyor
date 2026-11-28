# Compliance Review — Plan spine

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 2,
  "rows": [
    {
      "contract_id": "01a1ff8a-754f-70de-a63a-fa8d00596433",
      "created_at": "2026-11-21T19:18:27.698069+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T1.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/T1.toml",
        "sha": "d367a4a992ca5338c82466e9d6ef308e62b01ba6"
      },
      "findings": "This retrospective Phase-G review records the compliance level compliant for T1 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The close carried a mandatory_artefacts gap: at the 2026-11-21T18:51:39Z rework snapshot the seven Phase A\u2013D artefacts, the Statement of Architecture Work and the delivery contract were absent, and the architecture compliance-review list was empty. This rework produces and publishes stakeholder_map, driver_goal_objective, requirements_catalog and solution_concept (Phase A), business_capability_map (Phase B), data_entity_catalog (Phase C) and technology_standards (Phase D), and records this review. The gate's clauses are proved by named tests: plan_spine.rs::a_baseline_does_not_move_when_rows_change, ::a_change_request_without_impact_is_refused, ::wsjf_recomputes_when_an_input_changes, ::the_portfolio_lists_only_the_in_flight_set, ::a_two_node_dependency_cycle_is_refused, ::a_deep_dependency_cycle_leaves_no_row, ::a_benefit_without_baseline_target_method_or_owner_is_refused, ::a_benefit_map_edge_names_its_five_levels and ::a_tranche_review_without_evidence_is_refused, with approval_dual.rs::the_requester_cannot_decide_their_own_gate and programme_baseline.rs::the_taker_cannot_approve_their_own_baseline for the refused self-approval. No original Phase-G review existed; this one is retrospective and does not attest contemporaneous intent. The live project had no baseline row at the snapshot; this rework takes the scope baseline from the one WBS deliverable, and the frozen-snapshot behaviour is the P1.1 test.",
      "id": "01a1ff8b-b3ac-7082-bc16-df2f97bd7f8d",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "reviewed_at": "2026-11-21T19:18:27.698069+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-21T19:18:27.698069+00:00",
      "version": 1
    },
    {
      "contract_id": "01a217e8-ee60-725f-8c10-676c401569be",
      "created_at": "2026-11-26T12:51:17.635037+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T1.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "delivery_contract": "01a1ff8a-754f-70de-a63a-fa8d00596433",
        "live_review": "01a1ff8b-b3ac-7082-bc16-df2f97bd7f8d",
        "manifest": "acceptance/programme/T1.toml",
        "rework_note": "docs/evidence/rework/T1.json",
        "scope_baseline": "01a1ff8a-b4f0-7e6e-9cc7-4231076e1039",
        "sha": "b0449db045a18004c19bbfbd87dd39acedad85a8",
        "statement_of_architecture_work": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5"
      },
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live Phase-G review 01a1ff8b-b3ac-7082-bc16-df2f97bd7f8d is compliant against the delivery contract 01a1ff8a-754f-70de-a63a-fa8d00596433, the scope baseline 01a1ff8a-b4f0-7e6e-9cc7-4231076e1039 is approved, and the seven Phase A\u2013D artefacts in this pack are authored against the plan spine. The gate clauses are proved by named tests: plan_spine.rs::a_baseline_does_not_move_when_rows_change, ::a_change_request_without_impact_is_refused, ::wsjf_recomputes_when_an_input_changes, ::the_portfolio_lists_only_the_in_flight_set, ::a_two_node_dependency_cycle_is_refused, ::a_deep_dependency_cycle_leaves_no_row, ::a_benefit_without_baseline_target_method_or_owner_is_refused, ::a_benefit_map_edge_names_its_five_levels and ::a_tranche_review_without_evidence_is_refused, with approval_dual.rs::the_requester_cannot_decide_their_own_gate and programme_baseline.rs::the_taker_cannot_approve_their_own_baseline for the refused self-approval. Against that, three limits are stated rather than waived: no contemporaneous governance record existed at the 2026-11-17 close, cadence materialisation and PI planning and the benefits-realization procedure are T10, and the console surfaces beyond portfolio.in_flight remain fixtures. Those are scope boundaries of the delivery, not non-compliances.",
      "id": "01a217e9-0808-7378-8e60-e34b690c2385",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "reviewed_at": "2026-11-26T12:51:17.635037+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T12:51:17.635037+00:00",
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
      "contract_id": "01a1ff8a-754f-70de-a63a-fa8d00596433",
      "created_at": "2026-11-21T19:18:27.698069+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T1.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/T1.toml",
        "sha": "d367a4a992ca5338c82466e9d6ef308e62b01ba6"
      },
      "findings": "This retrospective Phase-G review records the compliance level compliant for T1 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The close carried a mandatory_artefacts gap: at the 2026-11-21T18:51:39Z rework snapshot the seven Phase A\u2013D artefacts, the Statement of Architecture Work and the delivery contract were absent, and the architecture compliance-review list was empty. This rework produces and publishes stakeholder_map, driver_goal_objective, requirements_catalog and solution_concept (Phase A), business_capability_map (Phase B), data_entity_catalog (Phase C) and technology_standards (Phase D), and records this review. The gate's clauses are proved by named tests: plan_spine.rs::a_baseline_does_not_move_when_rows_change, ::a_change_request_without_impact_is_refused, ::wsjf_recomputes_when_an_input_changes, ::the_portfolio_lists_only_the_in_flight_set, ::a_two_node_dependency_cycle_is_refused, ::a_deep_dependency_cycle_leaves_no_row, ::a_benefit_without_baseline_target_method_or_owner_is_refused, ::a_benefit_map_edge_names_its_five_levels and ::a_tranche_review_without_evidence_is_refused, with approval_dual.rs::the_requester_cannot_decide_their_own_gate and programme_baseline.rs::the_taker_cannot_approve_their_own_baseline for the refused self-approval. No original Phase-G review existed; this one is retrospective and does not attest contemporaneous intent. The live project had no baseline row at the snapshot; this rework takes the scope baseline from the one WBS deliverable, and the frozen-snapshot behaviour is the P1.1 test.",
      "id": "01a1ff8b-b3ac-7082-bc16-df2f97bd7f8d",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "reviewed_at": "2026-11-21T19:18:27.698069+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-21T19:18:27.698069+00:00",
      "version": 1
    },
    {
      "contract_id": "01a217e8-ee60-725f-8c10-676c401569be",
      "created_at": "2026-11-26T12:51:17.635037+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T1.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "delivery_contract": "01a1ff8a-754f-70de-a63a-fa8d00596433",
        "live_review": "01a1ff8b-b3ac-7082-bc16-df2f97bd7f8d",
        "manifest": "acceptance/programme/T1.toml",
        "rework_note": "docs/evidence/rework/T1.json",
        "scope_baseline": "01a1ff8a-b4f0-7e6e-9cc7-4231076e1039",
        "sha": "b0449db045a18004c19bbfbd87dd39acedad85a8",
        "statement_of_architecture_work": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5"
      },
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live Phase-G review 01a1ff8b-b3ac-7082-bc16-df2f97bd7f8d is compliant against the delivery contract 01a1ff8a-754f-70de-a63a-fa8d00596433, the scope baseline 01a1ff8a-b4f0-7e6e-9cc7-4231076e1039 is approved, and the seven Phase A\u2013D artefacts in this pack are authored against the plan spine. The gate clauses are proved by named tests: plan_spine.rs::a_baseline_does_not_move_when_rows_change, ::a_change_request_without_impact_is_refused, ::wsjf_recomputes_when_an_input_changes, ::the_portfolio_lists_only_the_in_flight_set, ::a_two_node_dependency_cycle_is_refused, ::a_deep_dependency_cycle_leaves_no_row, ::a_benefit_without_baseline_target_method_or_owner_is_refused, ::a_benefit_map_edge_names_its_five_levels and ::a_tranche_review_without_evidence_is_refused, with approval_dual.rs::the_requester_cannot_decide_their_own_gate and programme_baseline.rs::the_taker_cannot_approve_their_own_baseline for the refused self-approval. Against that, three limits are stated rather than waived: no contemporaneous governance record existed at the 2026-11-17 close, cadence materialisation and PI planning and the benefits-realization procedure are T10, and the console surfaces beyond portfolio.in_flight remain fixtures. Those are scope boundaries of the delivery, not non-compliances.",
      "id": "01a217e9-0808-7378-8e60-e34b690c2385",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "reviewed_at": "2026-11-26T12:51:17.635037+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T12:51:17.635037+00:00",
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
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live Phase-G review 01a1ff8b-b3ac-7082-bc16-df2f97bd7f8d is compliant against the delivery contract 01a1ff8a-754f-70de-a63a-fa8d00596433, the scope baseline 01a1ff8a-b4f0-7e6e-9cc7-4231076e1039 is approved, and the seven Phase A\u2013D artefacts in this pack are authored against the plan spine. The gate clauses are proved by named tests: plan_spine.rs::a_baseline_does_not_move_when_rows_change, ::a_change_request_without_impact_is_refused, ::wsjf_recomputes_when_an_input_changes, ::the_portfolio_lists_only_the_in_flight_set, ::a_two_node_dependency_cycle_is_refused, ::a_deep_dependency_cycle_leaves_no_row, ::a_benefit_without_baseline_target_method_or_owner_is_refused, ::a_benefit_map_edge_names_its_five_levels and ::a_tranche_review_without_evidence_is_refused, with approval_dual.rs::the_requester_cannot_decide_their_own_gate and programme_baseline.rs::the_taker_cannot_approve_their_own_baseline for the refused self-approval. Against that, three limits are stated rather than waived: no contemporaneous governance record existed at the 2026-11-17 close, cadence materialisation and PI planning and the benefits-realization procedure are T10, and the console surfaces beyond portfolio.in_flight remain fixtures. Those are scope boundaries of the delivery, not non-compliances.",
      "id": "01a217e9-0808-7378-8e60-e34b690c2385",
      "kind": "governing_review",
      "level": "compliant",
      "outcome": null,
      "reviewed_at": "2026-11-26T12:51:17.635037+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a217e9-0808-7378-8e60-e34b690c2385: compliant / no outcome"
    }
  ],
  "source": "computed:governing_review"
}
```

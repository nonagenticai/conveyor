# Delivery Contract — Plan spine

## Commitments

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a217e8-ee60-725f-8c10-676c401569be",
      "field": "commitments",
      "kind": "contract_field",
      "title": "commitments: Deliver the plan spine of SPEC_V2 \u00a712: `wbs_node`, `project_baseline`, `project_register_item`, `project_dependency`, the programme `shared_config` and cadence, `programme_tranche`, `benefit_profile`, `benefit_link`, `benefit_transition`, `stakeholder_profile`, `programme_baseline` and the widened `governed_asset` kinds, with the project-type selection of phase template and gate set, the DoD gate on `card_type.definition`, and the requester and executor exclusion in `approvals.decide`. The build is one WBS deliverable accepted with WSJF inputs business value 8, time criticality 5, risk reduction 8 and job size 8.",
      "value": "Deliver the plan spine of SPEC_V2 \u00a712: `wbs_node`, `project_baseline`, `project_register_item`, `project_dependency`, the programme `shared_config` and cadence, `programme_tranche`, `benefit_profile`, `benefit_link`, `benefit_transition`, `stakeholder_profile`, `programme_baseline` and the widened `governed_asset` kinds, with the project-type selection of phase template and gate set, the DoD gate on `card_type.definition`, and the requester and executor exclusion in `approvals.decide`. The build is one WBS deliverable accepted with WSJF inputs business value 8, time criticality 5, risk reduction 8 and job size 8."
    }
  ],
  "source": "computed:contract_field"
}
```

## Compliance method

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a217e8-ee60-725f-8c10-676c401569be",
      "field": "compliance",
      "kind": "contract_field",
      "title": "compliance: T1 is assessed clause by clause against the SPEC_V2 \u00a712 gate and the ten issue-#72 scenarios. Each scenario has one named real-Postgres test in `crates/actions/tests/plan_spine.rs`, `crates/api/tests/approval_dual.rs` or `crates/actions/tests/programme_baseline.rs`, and the refusal slugs are stable: 409 dependency_cycle, 409 self_approval, 422 change_impact_missing, 422 benefit_profile_incomplete, 422 benefit_map_incomplete, 422 tranche_evidence_missing and 422 self_dependency. The Phase-G review 01a1ff8b-b3ac-7082-bc16-df2f97bd7f8d records the level compliant against this contract, and the rework's acceptance instrument is `acceptance/programme/T1.toml`.",
      "value": "T1 is assessed clause by clause against the SPEC_V2 \u00a712 gate and the ten issue-#72 scenarios. Each scenario has one named real-Postgres test in `crates/actions/tests/plan_spine.rs`, `crates/api/tests/approval_dual.rs` or `crates/actions/tests/programme_baseline.rs`, and the refusal slugs are stable: 409 dependency_cycle, 409 self_approval, 422 change_impact_missing, 422 benefit_profile_incomplete, 422 benefit_map_incomplete, 422 tranche_evidence_missing and 422 self_dependency. The Phase-G review 01a1ff8b-b3ac-7082-bc16-df2f97bd7f8d records the level compliant against this contract, and the rework's acceptance instrument is `acceptance/programme/T1.toml`."
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
      "approved_at": "2026-11-26T12:51:11.341178+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a217e8-eee8-761a-9c2e-255e19ad612c",
      "kind": "contract_approval",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "approved",
      "title": "approval 01a217e8-eee8-761a-9c2e-255e19ad612c \u2014 approved"
    }
  ],
  "source": "computed:contract_approval"
}
```

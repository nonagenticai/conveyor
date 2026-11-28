# Delivery Contract — Decision ladder

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
      "contract_id": "01a2180f-1c53-7d7d-ac50-60cdaa37717a",
      "field": "compliance",
      "kind": "contract_field",
      "title": "compliance: T2 is assessed clause by clause against the SPEC_V2 \u00a712 gate and the issue-#73 scenarios P1.1, P2.1, P3.1 and P4.1/P4.2. The first three gate clauses are proved by named real-Postgres tests: `crates/actions/tests/decision_ladder.rs::a_rule_a_laya_and_a_human_decision_share_one_audit_shape`, `::a_refinement_changes_a_threshold_without_a_code_change` and `::every_decision_pins_the_refinement_and_laya_config_versions`. The fourth clause is proved on the apply and update paths by `::a_generated_draft_cannot_transition_a_card` and `crates/actions/tests/ladder_human_gate.rs::only_a_human_draft_apply_decision_can_gate_a_draft`, but is refuted on the create path by v1-t2-1: `documents.compose` accepts `card_id` and `meta.generated = true` and attaches the document with no gate. The batch admitted six findings \u2014 v1-t2-1 (P0, blocking), v1-t2-2 (L1 never calls laya), v1-t2-3 (`refinements.publish` ungated), v1-t2-4 (inline options and questions), v1-t2-5 (budget code spelling) and v1-t2-6 (a human decision cites any approved approval, not its gate). The compliance level is non_conformant; the project stays ACTIVE and is not closed in this pass. The rework's acceptance instrument is `acceptance/programme/T2.toml`.",
      "value": "T2 is assessed clause by clause against the SPEC_V2 \u00a712 gate and the issue-#73 scenarios P1.1, P2.1, P3.1 and P4.1/P4.2. The first three gate clauses are proved by named real-Postgres tests: `crates/actions/tests/decision_ladder.rs::a_rule_a_laya_and_a_human_decision_share_one_audit_shape`, `::a_refinement_changes_a_threshold_without_a_code_change` and `::every_decision_pins_the_refinement_and_laya_config_versions`. The fourth clause is proved on the apply and update paths by `::a_generated_draft_cannot_transition_a_card` and `crates/actions/tests/ladder_human_gate.rs::only_a_human_draft_apply_decision_can_gate_a_draft`, but is refuted on the create path by v1-t2-1: `documents.compose` accepts `card_id` and `meta.generated = true` and attaches the document with no gate. The batch admitted six findings \u2014 v1-t2-1 (P0, blocking), v1-t2-2 (L1 never calls laya), v1-t2-3 (`refinements.publish` ungated), v1-t2-4 (inline options and questions), v1-t2-5 (budget code spelling) and v1-t2-6 (a human decision cites any approved approval, not its gate). The compliance level is non_conformant; the project stays ACTIVE and is not closed in this pass. The rework's acceptance instrument is `acceptance/programme/T2.toml`."
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
      "approved_at": "2026-11-26T13:32:53.474747+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a2180f-1cdc-7d7e-940b-7a09f77274bf",
      "kind": "contract_approval",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "approved",
      "title": "approval 01a2180f-1cdc-7d7e-940b-7a09f77274bf \u2014 approved"
    }
  ],
  "source": "computed:contract_approval"
}
```

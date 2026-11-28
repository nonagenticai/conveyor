# Compliance Review — Usage inventory → parity/parity.yaml

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 2,
  "rows": [
    {
      "contract_id": "01a1ff39-4ee0-7ecc-93cc-eeaf21fb9738",
      "created_at": "2026-11-21T17:49:40.576704+00:00",
      "evidence": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/S00.toml",
        "result": "green",
        "sha": "d7293f96082dbf2f99f53bb8fd765834dddf6858"
      },
      "findings": [
        {
          "code": "S00-1",
          "note": "the inventory is one signed file of 1,300 rows; the 244 dropped rows are explicit"
        },
        {
          "code": "S00-2",
          "note": "each row states its evidence class; class D rows are marked source-manifest only"
        }
      ],
      "id": "01a1ff3a-6a97-7c12-841c-1c4f803cffb6",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "reviewed_at": "2026-11-21T17:49:40.576704+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-21T17:49:40.576704+00:00",
      "version": 1
    },
    {
      "contract_id": "01a2173a-ed43-78cd-8d1d-5c98fba37ed9",
      "created_at": "2026-11-26T09:41:13.259263+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S00.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "delivery_contract": "01a1ff39-4ee0-7ecc-93cc-eeaf21fb9738",
        "live_review": "01a1ff3a-6a97-7c12-841c-1c4f803cffb6",
        "manifest": "acceptance/programme/S00.toml",
        "rework_note": "docs/evidence/rework/S00.md",
        "scope_baseline": "01a1ff39-8d6e-7270-b2ba-9d235eb28b83",
        "sha": "5a7f590cad36ae7512106dabbc8fbd4204be1759"
      },
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live Phase-G review 01a1ff3a-6a97-7c12-841c-1c4f803cffb6 is compliant against the delivery contract 01a1ff39-4ee0-7ecc-93cc-eeaf21fb9738, the contract is approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545 rather than the requester 01a1e607-17d9-77d3-98d3-a43a87c4633d, and the scope baseline 01a1ff39-8d6e-7270-b2ba-9d235eb28b83 is approved. The project is closed and the rework note records the reconstituted lifecycle completed through the same gate, with the 38 acts in transcript.jsonl read back one write at a time by programme-record.py. The inventory itself is the signed contract: parity/parity.yaml carries 1,300 rows \u2014 218 used, 295 rare, 543 unused and 244 dropped \u2014 sampled on 2026-11-07 and 2026-11-08, every used row carries a test:, e2e: or probe: acceptance string, every dropped row carries a decision: or replacement: reason, and the gate G1 to G7 plus check-jsonschema passed at sign-off. scripts/verify-project.py --project S00 --snapshot <fresh> is green with 11 checks and 0 failures at d7293f96, and the strict audit on the same snapshot carries no P0/P1 entry. Against that, one limit is stated rather than waived: the samples are dated and a re-measurement is not recoverable, so the counts describe those two windows and nothing later. That limit is a residual of the measurement, not a non-compliance of the delivery.",
      "id": "01a2173b-03b1-7754-acda-14fdd2f8773c",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "reviewed_at": "2026-11-26T09:41:13.259263+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T09:41:13.259263+00:00",
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
      "contract_id": "01a1ff39-4ee0-7ecc-93cc-eeaf21fb9738",
      "created_at": "2026-11-21T17:49:40.576704+00:00",
      "evidence": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/S00.toml",
        "result": "green",
        "sha": "d7293f96082dbf2f99f53bb8fd765834dddf6858"
      },
      "findings": [
        {
          "code": "S00-1",
          "note": "the inventory is one signed file of 1,300 rows; the 244 dropped rows are explicit"
        },
        {
          "code": "S00-2",
          "note": "each row states its evidence class; class D rows are marked source-manifest only"
        }
      ],
      "id": "01a1ff3a-6a97-7c12-841c-1c4f803cffb6",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "reviewed_at": "2026-11-21T17:49:40.576704+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-21T17:49:40.576704+00:00",
      "version": 1
    },
    {
      "contract_id": "01a2173a-ed43-78cd-8d1d-5c98fba37ed9",
      "created_at": "2026-11-26T09:41:13.259263+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/S00.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "delivery_contract": "01a1ff39-4ee0-7ecc-93cc-eeaf21fb9738",
        "live_review": "01a1ff3a-6a97-7c12-841c-1c4f803cffb6",
        "manifest": "acceptance/programme/S00.toml",
        "rework_note": "docs/evidence/rework/S00.md",
        "scope_baseline": "01a1ff39-8d6e-7270-b2ba-9d235eb28b83",
        "sha": "5a7f590cad36ae7512106dabbc8fbd4204be1759"
      },
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live Phase-G review 01a1ff3a-6a97-7c12-841c-1c4f803cffb6 is compliant against the delivery contract 01a1ff39-4ee0-7ecc-93cc-eeaf21fb9738, the contract is approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545 rather than the requester 01a1e607-17d9-77d3-98d3-a43a87c4633d, and the scope baseline 01a1ff39-8d6e-7270-b2ba-9d235eb28b83 is approved. The project is closed and the rework note records the reconstituted lifecycle completed through the same gate, with the 38 acts in transcript.jsonl read back one write at a time by programme-record.py. The inventory itself is the signed contract: parity/parity.yaml carries 1,300 rows \u2014 218 used, 295 rare, 543 unused and 244 dropped \u2014 sampled on 2026-11-07 and 2026-11-08, every used row carries a test:, e2e: or probe: acceptance string, every dropped row carries a decision: or replacement: reason, and the gate G1 to G7 plus check-jsonschema passed at sign-off. scripts/verify-project.py --project S00 --snapshot <fresh> is green with 11 checks and 0 failures at d7293f96, and the strict audit on the same snapshot carries no P0/P1 entry. Against that, one limit is stated rather than waived: the samples are dated and a re-measurement is not recoverable, so the counts describe those two windows and nothing later. That limit is a residual of the measurement, not a non-compliance of the delivery.",
      "id": "01a2173b-03b1-7754-acda-14fdd2f8773c",
      "level": "compliant",
      "outcome": null,
      "project_id": "01a1d039-0d26-7573-9b0c-e0f60af92003",
      "reviewed_at": "2026-11-26T09:41:13.259263+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T09:41:13.259263+00:00",
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
      "findings": "This retrospective Phase-G review, dated 2026-11-26, carries the level the record supports: the live Phase-G review 01a1ff3a-6a97-7c12-841c-1c4f803cffb6 is compliant against the delivery contract 01a1ff39-4ee0-7ecc-93cc-eeaf21fb9738, the contract is approved by 01a1e8a7-8b23-76a9-9114-aafb95f7a545 rather than the requester 01a1e607-17d9-77d3-98d3-a43a87c4633d, and the scope baseline 01a1ff39-8d6e-7270-b2ba-9d235eb28b83 is approved. The project is closed and the rework note records the reconstituted lifecycle completed through the same gate, with the 38 acts in transcript.jsonl read back one write at a time by programme-record.py. The inventory itself is the signed contract: parity/parity.yaml carries 1,300 rows \u2014 218 used, 295 rare, 543 unused and 244 dropped \u2014 sampled on 2026-11-07 and 2026-11-08, every used row carries a test:, e2e: or probe: acceptance string, every dropped row carries a decision: or replacement: reason, and the gate G1 to G7 plus check-jsonschema passed at sign-off. scripts/verify-project.py --project S00 --snapshot <fresh> is green with 11 checks and 0 failures at d7293f96, and the strict audit on the same snapshot carries no P0/P1 entry. Against that, one limit is stated rather than waived: the samples are dated and a re-measurement is not recoverable, so the counts describe those two windows and nothing later. That limit is a residual of the measurement, not a non-compliance of the delivery.",
      "id": "01a2173b-03b1-7754-acda-14fdd2f8773c",
      "kind": "governing_review",
      "level": "compliant",
      "outcome": null,
      "reviewed_at": "2026-11-26T09:41:13.259263+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a2173b-03b1-7754-acda-14fdd2f8773c: compliant / no outcome"
    }
  ],
  "source": "computed:governing_review"
}
```

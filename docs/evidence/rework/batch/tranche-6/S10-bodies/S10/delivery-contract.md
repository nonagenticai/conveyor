# Delivery Contract — Copilot / AG-UI — one action surface, proposals, conversations

## Commitments

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a2210f-1047-7899-bd0d-9eb6926acb0b",
      "field": "commitments",
      "kind": "contract_field",
      "title": "commitments: S10 delivers the copilot's one action surface: `/api/copilot/actions` and `/mcp` `tools/list` are projections of the same `ActionSpec` set; a must-decide tool call becomes one `approval(kind='copilot.action')` in the same transaction as the terminal turn; an approved proposal executes exactly once under a claim, a scope re-check and a proposal idempotency key; the conversation is two tables plus an outbox event with `Last-Event-ID` replay; `completions.create` fails closed; and `copilot.chat` is the only declared coverage exemption. The project also records what it does not deliver: the chat UI, a deployed copilot run and a production router proof stay S12 and S14 work.",
      "value": "S10 delivers the copilot's one action surface: `/api/copilot/actions` and `/mcp` `tools/list` are projections of the same `ActionSpec` set; a must-decide tool call becomes one `approval(kind='copilot.action')` in the same transaction as the terminal turn; an approved proposal executes exactly once under a claim, a scope re-check and a proposal idempotency key; the conversation is two tables plus an outbox event with `Last-Event-ID` replay; `completions.create` fails closed; and `copilot.chat` is the only declared coverage exemption. The project also records what it does not deliver: the chat UI, a deployed copilot run and a production router proof stay S12 and S14 work."
    }
  ],
  "source": "computed:contract_field"
}
```

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a2210f-10d5-7ff5-8573-c8d933cdb52b",
      "approved_at": "2026-11-28T07:29:25.345257+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_at": "2026-11-27",
        "approval": "The contract is approved by the board decision the factory records for this pack, under the standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3; the Phase-G review is compliant for the record set at the 2026-11-27 snapshot, and the closure decision is taken by an approver other than the requester.",
        "commitments": "S10 delivers the copilot's one action surface: `/api/copilot/actions` and `/mcp` `tools/list` are projections of the same `ActionSpec` set; a must-decide tool call becomes one `approval(kind='copilot.action')` in the same transaction as the terminal turn; an approved proposal executes exactly once under a claim, a scope re-check and a proposal idempotency key; the conversation is two tables plus an outbox event with `Last-Event-ID` replay; `completions.create` fails closed; and `copilot.chat` is the only declared coverage exemption. The project also records what it does not deliver: the chat UI, a deployed copilot run and a production router proof stay S12 and S14 work.",
        "compliance": "Compliance is evidenced by `crates/api/tests/copilot_*.rs` (fourteen suites), `crates/api/tests/agui_parity.rs`, `crates/actions/tests/copilot_coverage.rs`, `migrations/0018_copilot.sql` and the modules the artefact set names. The retrospective Phase-G review reads the same evidence and records the level compliant for the record set, while naming the pre-pack receipt `docs/evidence/rework/S10.json` as red on lifecycle checks. The pack is content-checked at the pinned snapshot; the repository's preflight and CI are the compile-and-run gate, and this retrospective does not claim a fresh local run.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "manifest": "acceptance/programme/S10.toml \u2014 17 checks over the project row, the baseline, the charter, the closure outputs, the contract, the Phase-G review, the closure gate, the waivers, the PI cadence, the PI plan, the PI objectives, the WSJF, the demo credits, the lessons, the product findings, the artefact standard and the rework resolution.",
        "retrospective": "This delivery contract is authored retrospectively for the tranche-6 rework. The S10 row was created on 2026-11-12 and closed without a contemporaneous contract; the pre-pack receipt was generated on 2026-11-25. The commitments are read from the tree and the suites at the 2026-11-27 snapshot and are not presented as an earlier agreement."
      },
      "created_at": "2026-11-28T07:29:25.060895+00:00",
      "id": "01a2210f-1047-7899-bd0d-9eb6926acb0b",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery Contract \u2014 S10 Copilot / AG-UI (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:29:25.345257+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
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
      "contract_id": "01a2210f-1047-7899-bd0d-9eb6926acb0b",
      "field": "compliance",
      "kind": "contract_field",
      "title": "compliance: Compliance is evidenced by `crates/api/tests/copilot_*.rs` (fourteen suites), `crates/api/tests/agui_parity.rs`, `crates/actions/tests/copilot_coverage.rs`, `migrations/0018_copilot.sql` and the modules the artefact set names. The retrospective Phase-G review reads the same evidence and records the level compliant for the record set, while naming the pre-pack receipt `docs/evidence/rework/S10.json` as red on lifecycle checks. The pack is content-checked at the pinned snapshot; the repository's preflight and CI are the compile-and-run gate, and this retrospective does not claim a fresh local run.",
      "value": "Compliance is evidenced by `crates/api/tests/copilot_*.rs` (fourteen suites), `crates/api/tests/agui_parity.rs`, `crates/actions/tests/copilot_coverage.rs`, `migrations/0018_copilot.sql` and the modules the artefact set names. The retrospective Phase-G review reads the same evidence and records the level compliant for the record set, while naming the pre-pack receipt `docs/evidence/rework/S10.json` as red on lifecycle checks. The pack is content-checked at the pinned snapshot; the repository's preflight and CI are the compile-and-run gate, and this retrospective does not claim a fresh local run."
    }
  ],
  "source": "computed:contract_field"
}
```

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a2210f-10d5-7ff5-8573-c8d933cdb52b",
      "approved_at": "2026-11-28T07:29:25.345257+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_at": "2026-11-27",
        "approval": "The contract is approved by the board decision the factory records for this pack, under the standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3; the Phase-G review is compliant for the record set at the 2026-11-27 snapshot, and the closure decision is taken by an approver other than the requester.",
        "commitments": "S10 delivers the copilot's one action surface: `/api/copilot/actions` and `/mcp` `tools/list` are projections of the same `ActionSpec` set; a must-decide tool call becomes one `approval(kind='copilot.action')` in the same transaction as the terminal turn; an approved proposal executes exactly once under a claim, a scope re-check and a proposal idempotency key; the conversation is two tables plus an outbox event with `Last-Event-ID` replay; `completions.create` fails closed; and `copilot.chat` is the only declared coverage exemption. The project also records what it does not deliver: the chat UI, a deployed copilot run and a production router proof stay S12 and S14 work.",
        "compliance": "Compliance is evidenced by `crates/api/tests/copilot_*.rs` (fourteen suites), `crates/api/tests/agui_parity.rs`, `crates/actions/tests/copilot_coverage.rs`, `migrations/0018_copilot.sql` and the modules the artefact set names. The retrospective Phase-G review reads the same evidence and records the level compliant for the record set, while naming the pre-pack receipt `docs/evidence/rework/S10.json` as red on lifecycle checks. The pack is content-checked at the pinned snapshot; the repository's preflight and CI are the compile-and-run gate, and this retrospective does not claim a fresh local run.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "manifest": "acceptance/programme/S10.toml \u2014 17 checks over the project row, the baseline, the charter, the closure outputs, the contract, the Phase-G review, the closure gate, the waivers, the PI cadence, the PI plan, the PI objectives, the WSJF, the demo credits, the lessons, the product findings, the artefact standard and the rework resolution.",
        "retrospective": "This delivery contract is authored retrospectively for the tranche-6 rework. The S10 row was created on 2026-11-12 and closed without a contemporaneous contract; the pre-pack receipt was generated on 2026-11-25. The commitments are read from the tree and the suites at the 2026-11-27 snapshot and are not presented as an earlier agreement."
      },
      "created_at": "2026-11-28T07:29:25.060895+00:00",
      "id": "01a2210f-1047-7899-bd0d-9eb6926acb0b",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery Contract \u2014 S10 Copilot / AG-UI (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:29:25.345257+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
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
      "approved_at": "2026-11-28T07:29:25.345257+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a2210f-10d5-7ff5-8573-c8d933cdb52b",
      "kind": "contract_approval",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "approved",
      "title": "approval 01a2210f-10d5-7ff5-8573-c8d933cdb52b \u2014 approved"
    }
  ],
  "source": "computed:contract_approval"
}
```

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a2210f-10d5-7ff5-8573-c8d933cdb52b",
      "approved_at": "2026-11-28T07:29:25.345257+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_at": "2026-11-27",
        "approval": "The contract is approved by the board decision the factory records for this pack, under the standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3; the Phase-G review is compliant for the record set at the 2026-11-27 snapshot, and the closure decision is taken by an approver other than the requester.",
        "commitments": "S10 delivers the copilot's one action surface: `/api/copilot/actions` and `/mcp` `tools/list` are projections of the same `ActionSpec` set; a must-decide tool call becomes one `approval(kind='copilot.action')` in the same transaction as the terminal turn; an approved proposal executes exactly once under a claim, a scope re-check and a proposal idempotency key; the conversation is two tables plus an outbox event with `Last-Event-ID` replay; `completions.create` fails closed; and `copilot.chat` is the only declared coverage exemption. The project also records what it does not deliver: the chat UI, a deployed copilot run and a production router proof stay S12 and S14 work.",
        "compliance": "Compliance is evidenced by `crates/api/tests/copilot_*.rs` (fourteen suites), `crates/api/tests/agui_parity.rs`, `crates/actions/tests/copilot_coverage.rs`, `migrations/0018_copilot.sql` and the modules the artefact set names. The retrospective Phase-G review reads the same evidence and records the level compliant for the record set, while naming the pre-pack receipt `docs/evidence/rework/S10.json` as red on lifecycle checks. The pack is content-checked at the pinned snapshot; the repository's preflight and CI are the compile-and-run gate, and this retrospective does not claim a fresh local run.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "manifest": "acceptance/programme/S10.toml \u2014 17 checks over the project row, the baseline, the charter, the closure outputs, the contract, the Phase-G review, the closure gate, the waivers, the PI cadence, the PI plan, the PI objectives, the WSJF, the demo credits, the lessons, the product findings, the artefact standard and the rework resolution.",
        "retrospective": "This delivery contract is authored retrospectively for the tranche-6 rework. The S10 row was created on 2026-11-12 and closed without a contemporaneous contract; the pre-pack receipt was generated on 2026-11-25. The commitments are read from the tree and the suites at the 2026-11-27 snapshot and are not presented as an earlier agreement."
      },
      "created_at": "2026-11-28T07:29:25.060895+00:00",
      "id": "01a2210f-1047-7899-bd0d-9eb6926acb0b",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a1d039-0d2d-787e-853e-fef542ec8815",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery Contract \u2014 S10 Copilot / AG-UI (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:29:25.345257+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

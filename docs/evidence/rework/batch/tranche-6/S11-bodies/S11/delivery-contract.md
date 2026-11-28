# Delivery Contract — Processes, steps and SOPs — executable documentation

## Commitments

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a22110-33b2-718d-8253-3ad6bd4e4f50",
      "field": "commitments",
      "kind": "contract_field",
      "title": "commitments: S11 delivers executable documentation on a card: a versioned process of ordered steps, each naming a registry action and a resolved input template, each judged against a versioned SOP, executed as a run with one immutable run_step per attempt, with gates, retry, continue and cancel. The project also records what it does not deliver \u2014 the default procedure seed, the plan's \u00a77 live two-process rehearsal and the S15 executor-scoping arm \u2014 as deferred or open items.",
      "value": "S11 delivers executable documentation on a card: a versioned process of ordered steps, each naming a registry action and a resolved input template, each judged against a versioned SOP, executed as a run with one immutable run_step per attempt, with gates, retry, continue and cancel. The project also records what it does not deliver \u2014 the default procedure seed, the plan's \u00a77 live two-process rehearsal and the S15 executor-scoping arm \u2014 as deferred or open items."
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
      "approval_id": "01a22110-3443-744b-b8b9-97e93d4ea6b3",
      "approved_at": "2026-11-28T07:30:39.949554+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_at": "2026-11-27",
        "approval": "The contract is approved by the board decision the factory records for this pack, under the standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3; the Phase-G review is compliant for the record set at the 2026-11-27 snapshot, and the closure decision is taken by an approver other than the requester.",
        "commitments": "S11 delivers executable documentation on a card: a versioned process of ordered steps, each naming a registry action and a resolved input template, each judged against a versioned SOP, executed as a run with one immutable run_step per attempt, with gates, retry, continue and cancel. The project also records what it does not deliver \u2014 the default procedure seed, the plan's \u00a77 live two-process rehearsal and the S15 executor-scoping arm \u2014 as deferred or open items.",
        "compliance": "Compliance is evidenced by the suites in crates/actions/tests (procedures.rs, process_start.rs, process_step.rs, process_cancel.rs and process_maps.rs), the resolver and SOP unit tests in crates/domain/src/template.rs and crates/domain/src/procedure.rs, and migrations 0015 through 0035. The receipt docs/evidence/rework/S11.json at the pinned snapshot is red on 14 of 19 checks because the lifecycle set did not exist; after this pack is published the factory re-runs the 17-check manifest in acceptance/programme/S11.toml. The retrospective Phase-G review reads the same evidence and records the level compliant for the record set, with the deferred items carried as open rather than as passes.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "manifest": "acceptance/programme/S11.toml \u2014 17 checks over the project row, the scope baseline, the charter, the closure outputs, the delivery contract, the Phase-G review, the closure gate, the waivers, the PI cadence, the PI plan, the PI objectives, the WSJF, the demo credits, the lessons, the product findings, the artefact standard and the rework resolution.",
        "retrospective": "This delivery contract is authored retrospectively for the tranche-6 rework. The 2026-11-09 build carried no contemporaneous contract, so the commitments are read from the plan, the migrations and the suites at the 2026-11-27 snapshot and are not presented as a 2026-11-09 agreement."
      },
      "created_at": "2026-11-28T07:30:39.663675+00:00",
      "id": "01a22110-33b2-718d-8253-3ad6bd4e4f50",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery Contract \u2014 S11 Processes, Steps and SOPs (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:30:39.949554+00:00",
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
      "contract_id": "01a22110-33b2-718d-8253-3ad6bd4e4f50",
      "field": "compliance",
      "kind": "contract_field",
      "title": "compliance: Compliance is evidenced by the suites in crates/actions/tests (procedures.rs, process_start.rs, process_step.rs, process_cancel.rs and process_maps.rs), the resolver and SOP unit tests in crates/domain/src/template.rs and crates/domain/src/procedure.rs, and migrations 0015 through 0035. The receipt docs/evidence/rework/S11.json at the pinned snapshot is red on 14 of 19 checks because the lifecycle set did not exist; after this pack is published the factory re-runs the 17-check manifest in acceptance/programme/S11.toml. The retrospective Phase-G review reads the same evidence and records the level compliant for the record set, with the deferred items carried as open rather than as passes.",
      "value": "Compliance is evidenced by the suites in crates/actions/tests (procedures.rs, process_start.rs, process_step.rs, process_cancel.rs and process_maps.rs), the resolver and SOP unit tests in crates/domain/src/template.rs and crates/domain/src/procedure.rs, and migrations 0015 through 0035. The receipt docs/evidence/rework/S11.json at the pinned snapshot is red on 14 of 19 checks because the lifecycle set did not exist; after this pack is published the factory re-runs the 17-check manifest in acceptance/programme/S11.toml. The retrospective Phase-G review reads the same evidence and records the level compliant for the record set, with the deferred items carried as open rather than as passes."
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
      "approval_id": "01a22110-3443-744b-b8b9-97e93d4ea6b3",
      "approved_at": "2026-11-28T07:30:39.949554+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_at": "2026-11-27",
        "approval": "The contract is approved by the board decision the factory records for this pack, under the standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3; the Phase-G review is compliant for the record set at the 2026-11-27 snapshot, and the closure decision is taken by an approver other than the requester.",
        "commitments": "S11 delivers executable documentation on a card: a versioned process of ordered steps, each naming a registry action and a resolved input template, each judged against a versioned SOP, executed as a run with one immutable run_step per attempt, with gates, retry, continue and cancel. The project also records what it does not deliver \u2014 the default procedure seed, the plan's \u00a77 live two-process rehearsal and the S15 executor-scoping arm \u2014 as deferred or open items.",
        "compliance": "Compliance is evidenced by the suites in crates/actions/tests (procedures.rs, process_start.rs, process_step.rs, process_cancel.rs and process_maps.rs), the resolver and SOP unit tests in crates/domain/src/template.rs and crates/domain/src/procedure.rs, and migrations 0015 through 0035. The receipt docs/evidence/rework/S11.json at the pinned snapshot is red on 14 of 19 checks because the lifecycle set did not exist; after this pack is published the factory re-runs the 17-check manifest in acceptance/programme/S11.toml. The retrospective Phase-G review reads the same evidence and records the level compliant for the record set, with the deferred items carried as open rather than as passes.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "manifest": "acceptance/programme/S11.toml \u2014 17 checks over the project row, the scope baseline, the charter, the closure outputs, the delivery contract, the Phase-G review, the closure gate, the waivers, the PI cadence, the PI plan, the PI objectives, the WSJF, the demo credits, the lessons, the product findings, the artefact standard and the rework resolution.",
        "retrospective": "This delivery contract is authored retrospectively for the tranche-6 rework. The 2026-11-09 build carried no contemporaneous contract, so the commitments are read from the plan, the migrations and the suites at the 2026-11-27 snapshot and are not presented as a 2026-11-09 agreement."
      },
      "created_at": "2026-11-28T07:30:39.663675+00:00",
      "id": "01a22110-33b2-718d-8253-3ad6bd4e4f50",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery Contract \u2014 S11 Processes, Steps and SOPs (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:30:39.949554+00:00",
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
      "approved_at": "2026-11-28T07:30:39.949554+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a22110-3443-744b-b8b9-97e93d4ea6b3",
      "kind": "contract_approval",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "approved",
      "title": "approval 01a22110-3443-744b-b8b9-97e93d4ea6b3 \u2014 approved"
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
      "approval_id": "01a22110-3443-744b-b8b9-97e93d4ea6b3",
      "approved_at": "2026-11-28T07:30:39.949554+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_at": "2026-11-27",
        "approval": "The contract is approved by the board decision the factory records for this pack, under the standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3; the Phase-G review is compliant for the record set at the 2026-11-27 snapshot, and the closure decision is taken by an approver other than the requester.",
        "commitments": "S11 delivers executable documentation on a card: a versioned process of ordered steps, each naming a registry action and a resolved input template, each judged against a versioned SOP, executed as a run with one immutable run_step per attempt, with gates, retry, continue and cancel. The project also records what it does not deliver \u2014 the default procedure seed, the plan's \u00a77 live two-process rehearsal and the S15 executor-scoping arm \u2014 as deferred or open items.",
        "compliance": "Compliance is evidenced by the suites in crates/actions/tests (procedures.rs, process_start.rs, process_step.rs, process_cancel.rs and process_maps.rs), the resolver and SOP unit tests in crates/domain/src/template.rs and crates/domain/src/procedure.rs, and migrations 0015 through 0035. The receipt docs/evidence/rework/S11.json at the pinned snapshot is red on 14 of 19 checks because the lifecycle set did not exist; after this pack is published the factory re-runs the 17-check manifest in acceptance/programme/S11.toml. The retrospective Phase-G review reads the same evidence and records the level compliant for the record set, with the deferred items carried as open rather than as passes.",
        "delegation_ref": "01a1fb9f-ac2d-732d-b274-1079be3dd2b3",
        "manifest": "acceptance/programme/S11.toml \u2014 17 checks over the project row, the scope baseline, the charter, the closure outputs, the delivery contract, the Phase-G review, the closure gate, the waivers, the PI cadence, the PI plan, the PI objectives, the WSJF, the demo credits, the lessons, the product findings, the artefact standard and the rework resolution.",
        "retrospective": "This delivery contract is authored retrospectively for the tranche-6 rework. The 2026-11-09 build carried no contemporaneous contract, so the commitments are read from the plan, the migrations and the suites at the 2026-11-27 snapshot and are not presented as a 2026-11-09 agreement."
      },
      "created_at": "2026-11-28T07:30:39.663675+00:00",
      "id": "01a22110-33b2-718d-8253-3ad6bd4e4f50",
      "kind": "delivery_contract",
      "links": {},
      "meta": {},
      "phase_code": "f",
      "project_id": "01a1d039-0d2d-7ec6-aef0-4096ea66e7a4",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Delivery Contract \u2014 S11 Processes, Steps and SOPs (retrospective, 2026-11-27)",
      "updated_at": "2026-11-28T07:30:39.949554+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

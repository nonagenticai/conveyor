# Statement Of Architecture Work — Decision ladder

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 2,
  "rows": [
    {
      "approval_id": "01a2005a-8c6e-708a-84df-ab16d175072e",
      "approved_at": "2026-11-21T23:04:28.797665+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-21T23:04:18.681795+00:00",
      "id": "01a2005a-7886-7f2d-9b49-53d174544ee9",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:14.119814+00:00",
      "version": 3
    },
    {
      "approval_id": "01a2180f-168b-79af-af7f-e4cc18948e7b",
      "approved_at": "2026-11-26T13:32:51.863706+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-26T13:32:51.58027+00:00",
      "id": "01a2180f-15fe-794d-aba4-395c2f06a32f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:51.863706+00:00",
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
  "count": 2,
  "rows": [
    {
      "approval_id": "01a2005a-8c6e-708a-84df-ab16d175072e",
      "approved_at": "2026-11-21T23:04:28.797665+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-21T23:04:18.681795+00:00",
      "id": "01a2005a-7886-7f2d-9b49-53d174544ee9",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:14.119814+00:00",
      "version": 3
    },
    {
      "approval_id": "01a2180f-168b-79af-af7f-e4cc18948e7b",
      "approved_at": "2026-11-26T13:32:51.863706+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-26T13:32:51.58027+00:00",
      "id": "01a2180f-15fe-794d-aba4-395c2f06a32f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:51.863706+00:00",
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
  "count": 2,
  "rows": [
    {
      "approval_id": "01a2005a-8c6e-708a-84df-ab16d175072e",
      "approved_at": "2026-11-21T23:04:28.797665+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-21T23:04:18.681795+00:00",
      "id": "01a2005a-7886-7f2d-9b49-53d174544ee9",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:14.119814+00:00",
      "version": 3
    },
    {
      "approval_id": "01a2180f-168b-79af-af7f-e4cc18948e7b",
      "approved_at": "2026-11-26T13:32:51.863706+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-26T13:32:51.58027+00:00",
      "id": "01a2180f-15fe-794d-aba4-395c2f06a32f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:51.863706+00:00",
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
  "count": 2,
  "rows": [
    {
      "approval_id": "01a2005a-8c6e-708a-84df-ab16d175072e",
      "approved_at": "2026-11-21T23:04:28.797665+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-21T23:04:18.681795+00:00",
      "id": "01a2005a-7886-7f2d-9b49-53d174544ee9",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:14.119814+00:00",
      "version": 3
    },
    {
      "approval_id": "01a2180f-168b-79af-af7f-e4cc18948e7b",
      "approved_at": "2026-11-26T13:32:51.863706+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-26T13:32:51.58027+00:00",
      "id": "01a2180f-15fe-794d-aba4-395c2f06a32f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:51.863706+00:00",
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
  "count": 2,
  "rows": [
    {
      "approval_id": "01a2005a-8c6e-708a-84df-ab16d175072e",
      "approved_at": "2026-11-21T23:04:28.797665+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-21T23:04:18.681795+00:00",
      "id": "01a2005a-7886-7f2d-9b49-53d174544ee9",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "superseded",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:14.119814+00:00",
      "version": 3
    },
    {
      "approval_id": "01a2180f-168b-79af-af7f-e4cc18948e7b",
      "approved_at": "2026-11-26T13:32:51.863706+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T2 gate: one audit shape across a rule, a laya and a human decision; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya-config versions it read; and a generated draft cannot transition a card. The first three are proved by named tests in `crates/actions/tests/decision_ladder.rs`; the fourth is proved on the `ladder.apply_draft` and `documents.update` paths only, and is refuted on the `documents.compose` create path by v1-t2-1. The rework's acceptance instrument is `acceptance/programme/T2.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T2 (SPEC_V2 \u00a77 and \u00a712, issue #73) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the \u00a712 gate, and publish the project's expected outputs (`project-brief`, `evidence`). The project remains ACTIVE in this pass: v1-t2-1 (a generated draft can be attached by `documents.compose` with no gate) blocks the fourth gate clause, so no closure is attempted.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the decision ladder and generation split of SPEC_V2 \u00a77: the `decision` write shape and its three rungs; `decision_option`, `decision_policy`, `decision_policy_refinement` and `laya_inference_config`; the `ladder.*`, `decision_policies.*`, `refinements.*` and `laya_configs.*` actions; the human gate and the `draft.apply` gate; and the real-Postgres acceptance suites `decision_ladder.rs` and `ladder_human_gate.rs`. Out of scope: the PM job (T3), the tool platform (T13), metrics and metering (T11), T10 cadence work, the S15 judge's laya call, and the ADR board's `decisions.*` family."
      },
      "created_at": "2026-11-26T13:32:51.58027+00:00",
      "id": "01a2180f-15fe-794d-aba4-395c2f06a32f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7282-b7b0-6dde32c38ab1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T2 Decision ladder (retrospective, U5a)",
      "updated_at": "2026-11-26T13:32:51.863706+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

# Statement Of Architecture Work — Plan spine

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a1ff89-1794-79d0-8707-f7614987ee7c",
      "approved_at": "2026-11-21T19:15:42.114015+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T1 gate: one project with a baseline, a change request and a refused self-approval; WSJF recomputes from inputs; the portfolio view lists the in-flight set; a dependency cycle is refused; a benefit profile without baseline, target, method or owner is refused; a benefit-map edge names its five levels. The ten issue-#72 scenarios are proved by the named real-Postgres tests in crates/actions/tests/plan_spine.rs and crates/api/tests/approval_dual.rs; the rework's acceptance instrument is acceptance/programme/T1.toml. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T1 (SPEC_V2 \u00a712, issue #72) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the plan spine and programme-definition rows of SPEC_V2 \u00a712: wbs_node, project_baseline, project_register_item, project_dependency, programme_cadence, programme_tranche, benefit_profile, benefit_link, benefit_transition, stakeholder_profile, programme_baseline, and the widened governed_asset kinds; the spine action families; and the requester/executor exclusion in approvals.decide. Out of scope: the decision ladder (T2), the PM job (T3), cadence materialisation and PI planning and the benefits-realization procedure (T10), metrics (T11), and any migration of V1's outline tree."
      },
      "created_at": "2026-11-21T19:15:31.506201+00:00",
      "id": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T1 Plan spine (retrospective, U5a)",
      "updated_at": "2026-11-21T19:15:42.114015+00:00",
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
  "count": 1,
  "rows": [
    {
      "approval_id": "01a1ff89-1794-79d0-8707-f7614987ee7c",
      "approved_at": "2026-11-21T19:15:42.114015+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T1 gate: one project with a baseline, a change request and a refused self-approval; WSJF recomputes from inputs; the portfolio view lists the in-flight set; a dependency cycle is refused; a benefit profile without baseline, target, method or owner is refused; a benefit-map edge names its five levels. The ten issue-#72 scenarios are proved by the named real-Postgres tests in crates/actions/tests/plan_spine.rs and crates/api/tests/approval_dual.rs; the rework's acceptance instrument is acceptance/programme/T1.toml. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T1 (SPEC_V2 \u00a712, issue #72) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the plan spine and programme-definition rows of SPEC_V2 \u00a712: wbs_node, project_baseline, project_register_item, project_dependency, programme_cadence, programme_tranche, benefit_profile, benefit_link, benefit_transition, stakeholder_profile, programme_baseline, and the widened governed_asset kinds; the spine action families; and the requester/executor exclusion in approvals.decide. Out of scope: the decision ladder (T2), the PM job (T3), cadence materialisation and PI planning and the benefits-realization procedure (T10), metrics (T11), and any migration of V1's outline tree."
      },
      "created_at": "2026-11-21T19:15:31.506201+00:00",
      "id": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T1 Plan spine (retrospective, U5a)",
      "updated_at": "2026-11-21T19:15:42.114015+00:00",
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
  "count": 1,
  "rows": [
    {
      "approval_id": "01a1ff89-1794-79d0-8707-f7614987ee7c",
      "approved_at": "2026-11-21T19:15:42.114015+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T1 gate: one project with a baseline, a change request and a refused self-approval; WSJF recomputes from inputs; the portfolio view lists the in-flight set; a dependency cycle is refused; a benefit profile without baseline, target, method or owner is refused; a benefit-map edge names its five levels. The ten issue-#72 scenarios are proved by the named real-Postgres tests in crates/actions/tests/plan_spine.rs and crates/api/tests/approval_dual.rs; the rework's acceptance instrument is acceptance/programme/T1.toml. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T1 (SPEC_V2 \u00a712, issue #72) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the plan spine and programme-definition rows of SPEC_V2 \u00a712: wbs_node, project_baseline, project_register_item, project_dependency, programme_cadence, programme_tranche, benefit_profile, benefit_link, benefit_transition, stakeholder_profile, programme_baseline, and the widened governed_asset kinds; the spine action families; and the requester/executor exclusion in approvals.decide. Out of scope: the decision ladder (T2), the PM job (T3), cadence materialisation and PI planning and the benefits-realization procedure (T10), metrics (T11), and any migration of V1's outline tree."
      },
      "created_at": "2026-11-21T19:15:31.506201+00:00",
      "id": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T1 Plan spine (retrospective, U5a)",
      "updated_at": "2026-11-21T19:15:42.114015+00:00",
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
  "count": 1,
  "rows": [
    {
      "approval_id": "01a1ff89-1794-79d0-8707-f7614987ee7c",
      "approved_at": "2026-11-21T19:15:42.114015+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T1 gate: one project with a baseline, a change request and a refused self-approval; WSJF recomputes from inputs; the portfolio view lists the in-flight set; a dependency cycle is refused; a benefit profile without baseline, target, method or owner is refused; a benefit-map edge names its five levels. The ten issue-#72 scenarios are proved by the named real-Postgres tests in crates/actions/tests/plan_spine.rs and crates/api/tests/approval_dual.rs; the rework's acceptance instrument is acceptance/programme/T1.toml. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T1 (SPEC_V2 \u00a712, issue #72) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the plan spine and programme-definition rows of SPEC_V2 \u00a712: wbs_node, project_baseline, project_register_item, project_dependency, programme_cadence, programme_tranche, benefit_profile, benefit_link, benefit_transition, stakeholder_profile, programme_baseline, and the widened governed_asset kinds; the spine action families; and the requester/executor exclusion in approvals.decide. Out of scope: the decision ladder (T2), the PM job (T3), cadence materialisation and PI planning and the benefits-realization procedure (T10), metrics (T11), and any migration of V1's outline tree."
      },
      "created_at": "2026-11-21T19:15:31.506201+00:00",
      "id": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T1 Plan spine (retrospective, U5a)",
      "updated_at": "2026-11-21T19:15:42.114015+00:00",
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
  "count": 1,
  "rows": [
    {
      "approval_id": "01a1ff89-1794-79d0-8707-f7614987ee7c",
      "approved_at": "2026-11-21T19:15:42.114015+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T1 gate: one project with a baseline, a change request and a refused self-approval; WSJF recomputes from inputs; the portfolio view lists the in-flight set; a dependency cycle is refused; a benefit profile without baseline, target, method or owner is refused; a benefit-map edge names its five levels. The ten issue-#72 scenarios are proved by the named real-Postgres tests in crates/actions/tests/plan_spine.rs and crates/api/tests/approval_dual.rs; the rework's acceptance instrument is acceptance/programme/T1.toml. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T1 (SPEC_V2 \u00a712, issue #72) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the project baseline with the one WBS deliverable, record the Phase-G compliance review, publish the project's expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the plan spine and programme-definition rows of SPEC_V2 \u00a712: wbs_node, project_baseline, project_register_item, project_dependency, programme_cadence, programme_tranche, benefit_profile, benefit_link, benefit_transition, stakeholder_profile, programme_baseline, and the widened governed_asset kinds; the spine action families; and the requester/executor exclusion in approvals.decide. Out of scope: the decision ladder (T2), the PM job (T3), cadence materialisation and PI planning and the benefits-realization procedure (T10), metrics (T11), and any migration of V1's outline tree."
      },
      "created_at": "2026-11-21T19:15:31.506201+00:00",
      "id": "01a1ff89-02bd-7b6d-902e-ffa5902cc7f5",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d32-7c81-8365-371217d16ef7",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T1 Plan spine (retrospective, U5a)",
      "updated_at": "2026-11-21T19:15:42.114015+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

# Statement Of Architecture Work — SPEC V2 — the organisational operating model

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a1ff41-2c98-7646-b835-7c7a96e1ca6e",
      "approved_at": "2026-11-21T17:57:08.762631+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest `acceptance/programme/SPEC-V2.toml` is green: the closure review exists, every declared closure output (`evidence`, `closure-review`) is published, and the project re-closes through the one gate decided by an approver other than its requester. The Phase-G compliance review records exactly one standard level against this contract and cites the published standing delegation. The seven A\u2013D artefacts are approved rows with their entries; no outcome-level claim substitutes for them.",
        "assignment": "Produce and publish SPEC V2 as the programme's adopted operating model, and prove it through the programme's executable acceptance. The work is the document and its decision set \u2014 `docs/SPEC_V2.md`, decisions D28\u2013D63 \u2014 with the ownership, organisational, work, decision, tool and rigour models it defines. This Statement of Architecture Work is recorded retrospectively on 2026-11-21 for the tranche-0 rework (issue #526) under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the project's real, contemporaneous statement was never recorded and is not recoverable.",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The V2 document and its decisions, and the phase A\u2013D architecture artefacts this contract governs: the stakeholder map, the driver/goal/objective catalog, the requirements catalog and the solution concept (phase A); the business capability map (phase B); the data entity/data component catalog (phase C); the technology standards catalog (phase D). Not in scope: V1's S-slices (not re-opened), the V2 delivery slices T0\u2013T17 (their own projects), and `docs/SPEC_LEGACY_KAG.md` (D54)."
      },
      "created_at": "2026-11-21T17:56:58.859998+00:00",
      "id": "01a1ff41-19f9-77bb-aa4a-f416f0d758e3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 SPEC-V2, the organisational operating model (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:57:08.762631+00:00",
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
      "approval_id": "01a1ff41-2c98-7646-b835-7c7a96e1ca6e",
      "approved_at": "2026-11-21T17:57:08.762631+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest `acceptance/programme/SPEC-V2.toml` is green: the closure review exists, every declared closure output (`evidence`, `closure-review`) is published, and the project re-closes through the one gate decided by an approver other than its requester. The Phase-G compliance review records exactly one standard level against this contract and cites the published standing delegation. The seven A\u2013D artefacts are approved rows with their entries; no outcome-level claim substitutes for them.",
        "assignment": "Produce and publish SPEC V2 as the programme's adopted operating model, and prove it through the programme's executable acceptance. The work is the document and its decision set \u2014 `docs/SPEC_V2.md`, decisions D28\u2013D63 \u2014 with the ownership, organisational, work, decision, tool and rigour models it defines. This Statement of Architecture Work is recorded retrospectively on 2026-11-21 for the tranche-0 rework (issue #526) under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the project's real, contemporaneous statement was never recorded and is not recoverable.",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The V2 document and its decisions, and the phase A\u2013D architecture artefacts this contract governs: the stakeholder map, the driver/goal/objective catalog, the requirements catalog and the solution concept (phase A); the business capability map (phase B); the data entity/data component catalog (phase C); the technology standards catalog (phase D). Not in scope: V1's S-slices (not re-opened), the V2 delivery slices T0\u2013T17 (their own projects), and `docs/SPEC_LEGACY_KAG.md` (D54)."
      },
      "created_at": "2026-11-21T17:56:58.859998+00:00",
      "id": "01a1ff41-19f9-77bb-aa4a-f416f0d758e3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 SPEC-V2, the organisational operating model (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:57:08.762631+00:00",
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
      "approval_id": "01a1ff41-2c98-7646-b835-7c7a96e1ca6e",
      "approved_at": "2026-11-21T17:57:08.762631+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest `acceptance/programme/SPEC-V2.toml` is green: the closure review exists, every declared closure output (`evidence`, `closure-review`) is published, and the project re-closes through the one gate decided by an approver other than its requester. The Phase-G compliance review records exactly one standard level against this contract and cites the published standing delegation. The seven A\u2013D artefacts are approved rows with their entries; no outcome-level claim substitutes for them.",
        "assignment": "Produce and publish SPEC V2 as the programme's adopted operating model, and prove it through the programme's executable acceptance. The work is the document and its decision set \u2014 `docs/SPEC_V2.md`, decisions D28\u2013D63 \u2014 with the ownership, organisational, work, decision, tool and rigour models it defines. This Statement of Architecture Work is recorded retrospectively on 2026-11-21 for the tranche-0 rework (issue #526) under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the project's real, contemporaneous statement was never recorded and is not recoverable.",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The V2 document and its decisions, and the phase A\u2013D architecture artefacts this contract governs: the stakeholder map, the driver/goal/objective catalog, the requirements catalog and the solution concept (phase A); the business capability map (phase B); the data entity/data component catalog (phase C); the technology standards catalog (phase D). Not in scope: V1's S-slices (not re-opened), the V2 delivery slices T0\u2013T17 (their own projects), and `docs/SPEC_LEGACY_KAG.md` (D54)."
      },
      "created_at": "2026-11-21T17:56:58.859998+00:00",
      "id": "01a1ff41-19f9-77bb-aa4a-f416f0d758e3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 SPEC-V2, the organisational operating model (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:57:08.762631+00:00",
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
      "approval_id": "01a1ff41-2c98-7646-b835-7c7a96e1ca6e",
      "approved_at": "2026-11-21T17:57:08.762631+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest `acceptance/programme/SPEC-V2.toml` is green: the closure review exists, every declared closure output (`evidence`, `closure-review`) is published, and the project re-closes through the one gate decided by an approver other than its requester. The Phase-G compliance review records exactly one standard level against this contract and cites the published standing delegation. The seven A\u2013D artefacts are approved rows with their entries; no outcome-level claim substitutes for them.",
        "assignment": "Produce and publish SPEC V2 as the programme's adopted operating model, and prove it through the programme's executable acceptance. The work is the document and its decision set \u2014 `docs/SPEC_V2.md`, decisions D28\u2013D63 \u2014 with the ownership, organisational, work, decision, tool and rigour models it defines. This Statement of Architecture Work is recorded retrospectively on 2026-11-21 for the tranche-0 rework (issue #526) under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the project's real, contemporaneous statement was never recorded and is not recoverable.",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The V2 document and its decisions, and the phase A\u2013D architecture artefacts this contract governs: the stakeholder map, the driver/goal/objective catalog, the requirements catalog and the solution concept (phase A); the business capability map (phase B); the data entity/data component catalog (phase C); the technology standards catalog (phase D). Not in scope: V1's S-slices (not re-opened), the V2 delivery slices T0\u2013T17 (their own projects), and `docs/SPEC_LEGACY_KAG.md` (D54)."
      },
      "created_at": "2026-11-21T17:56:58.859998+00:00",
      "id": "01a1ff41-19f9-77bb-aa4a-f416f0d758e3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 SPEC-V2, the organisational operating model (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:57:08.762631+00:00",
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
      "approval_id": "01a1ff41-2c98-7646-b835-7c7a96e1ca6e",
      "approved_at": "2026-11-21T17:57:08.762631+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The manifest `acceptance/programme/SPEC-V2.toml` is green: the closure review exists, every declared closure output (`evidence`, `closure-review`) is published, and the project re-closes through the one gate decided by an approver other than its requester. The Phase-G compliance review records exactly one standard level against this contract and cites the published standing delegation. The seven A\u2013D artefacts are approved rows with their entries; no outcome-level claim substitutes for them.",
        "assignment": "Produce and publish SPEC V2 as the programme's adopted operating model, and prove it through the programme's executable acceptance. The work is the document and its decision set \u2014 `docs/SPEC_V2.md`, decisions D28\u2013D63 \u2014 with the ownership, organisational, work, decision, tool and rigour models it defines. This Statement of Architecture Work is recorded retrospectively on 2026-11-21 for the tranche-0 rework (issue #526) under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`; the project's real, contemporaneous statement was never recorded and is not recoverable.",
        "manifest": "acceptance/programme/SPEC-V2.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The V2 document and its decisions, and the phase A\u2013D architecture artefacts this contract governs: the stakeholder map, the driver/goal/objective catalog, the requirements catalog and the solution concept (phase A); the business capability map (phase B); the data entity/data component catalog (phase C); the technology standards catalog (phase D). Not in scope: V1's S-slices (not re-opened), the V2 delivery slices T0\u2013T17 (their own projects), and `docs/SPEC_LEGACY_KAG.md` (D54)."
      },
      "created_at": "2026-11-21T17:56:58.859998+00:00",
      "id": "01a1ff41-19f9-77bb-aa4a-f416f0d758e3",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7f3b-bd04-211f7dfe1e50",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 SPEC-V2, the organisational operating model (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T17:57:08.762631+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

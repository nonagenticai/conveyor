# Statement Of Architecture Work — The real V2 UI

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a219de-7e7e-72db-b751-a17bdd9b013d",
      "approved_at": "2026-11-26T21:59:01.639909+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T18 gate, judged by the fifteen checks of acceptance/programme/T18.toml: every destination route exists with its spec, each spec asserts empty, error, happy and one refusal, the coverage record reports exists only from a real route and a present spec with a *Gate reported as extend naming its owner, no production screen renders a fixture, and a signed-in tenant drives portfolio \u2192 programme \u2192 project \u2192 work package, organisation \u2192 release \u2192 binding, procedures intake \u2192 publish, tools call \u2192 ledger, improve \u2192 kata \u2192 adoption, operations incident \u2192 postmortem and the ADM phase rail from the app, with the T12 loop as the final spec.",
        "assignment": "Rework the V2 delivery slice T18 (SPEC_V2 \u00a710.5 and \u00a712 T18) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter, the Statement of Architecture Work and the delivery contract, take the scope baseline with the one WBS deliverable, record the retrospective Phase-G compliance review, publish the expected outputs project-brief, inception-analysis and architecture-design plus the evidence output, and leave the project active because T18-4 and T18-5 are not delivered. The work is retrospective: it reconstructs the governance record from the project row, the plan, the route table and the issue registry, and it attests no contemporaneous intent.",
        "contract": "The delivery contract record is requested and approved live by scripts/rework-project.py under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 and is titled 'Delivery contract \u2014 T18 The real V2 UI (retrospective, 2026-11-24)'. It commits the lane to the \u00a712 gate and to the two open units as typed dependencies, and it records that this retrospective neither closes the project nor waives any check. The contract's approval is retrospective; the historical snapshot has no live contract, which is one of the failed checks in the prior acceptance record.",
        "responsibilities": "The T18 lane under the programme architect (conveyor-e2e) is responsible for the route table, the screen modules, the specs and the coverage instrument. The Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract. The sponsoring group is accountable for the tranche-4 authorisation and for keeping the project active. The owning slices T4, T5, T7, T9, T10, T11, T13, T16 and T17 are responsible for the actions each destination calls, and T12 owns the final organisation loop.",
        "retrospective": true,
        "scope": "the shell, route registry and navigation; the ten V2 destinations of SPEC_V2 \u00a710.5 and the execution room; the L1\u2013L4 delivery view; one Playwright spec per destination; the V2 coverage instrument measured from apps/web/src/app.rs and the named specs; and the T12 organisation loop as the final acceptance. Out of scope: new tables, actions, events or ports; SSR, a second router or frontend, or a mobile app; retiring or rebuilding the design pack; the V1 routes, which are linked rather than re-implemented; and any production fixture or live tenant write inside the retrospective."
      },
      "created_at": "2026-11-26T21:59:01.357241+00:00",
      "id": "01a219de-7def-7827-8a21-98428640f502",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T18 The real V2 UI (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:59:01.639909+00:00",
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
      "approval_id": "01a219de-7e7e-72db-b751-a17bdd9b013d",
      "approved_at": "2026-11-26T21:59:01.639909+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T18 gate, judged by the fifteen checks of acceptance/programme/T18.toml: every destination route exists with its spec, each spec asserts empty, error, happy and one refusal, the coverage record reports exists only from a real route and a present spec with a *Gate reported as extend naming its owner, no production screen renders a fixture, and a signed-in tenant drives portfolio \u2192 programme \u2192 project \u2192 work package, organisation \u2192 release \u2192 binding, procedures intake \u2192 publish, tools call \u2192 ledger, improve \u2192 kata \u2192 adoption, operations incident \u2192 postmortem and the ADM phase rail from the app, with the T12 loop as the final spec.",
        "assignment": "Rework the V2 delivery slice T18 (SPEC_V2 \u00a710.5 and \u00a712 T18) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter, the Statement of Architecture Work and the delivery contract, take the scope baseline with the one WBS deliverable, record the retrospective Phase-G compliance review, publish the expected outputs project-brief, inception-analysis and architecture-design plus the evidence output, and leave the project active because T18-4 and T18-5 are not delivered. The work is retrospective: it reconstructs the governance record from the project row, the plan, the route table and the issue registry, and it attests no contemporaneous intent.",
        "contract": "The delivery contract record is requested and approved live by scripts/rework-project.py under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 and is titled 'Delivery contract \u2014 T18 The real V2 UI (retrospective, 2026-11-24)'. It commits the lane to the \u00a712 gate and to the two open units as typed dependencies, and it records that this retrospective neither closes the project nor waives any check. The contract's approval is retrospective; the historical snapshot has no live contract, which is one of the failed checks in the prior acceptance record.",
        "responsibilities": "The T18 lane under the programme architect (conveyor-e2e) is responsible for the route table, the screen modules, the specs and the coverage instrument. The Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract. The sponsoring group is accountable for the tranche-4 authorisation and for keeping the project active. The owning slices T4, T5, T7, T9, T10, T11, T13, T16 and T17 are responsible for the actions each destination calls, and T12 owns the final organisation loop.",
        "retrospective": true,
        "scope": "the shell, route registry and navigation; the ten V2 destinations of SPEC_V2 \u00a710.5 and the execution room; the L1\u2013L4 delivery view; one Playwright spec per destination; the V2 coverage instrument measured from apps/web/src/app.rs and the named specs; and the T12 organisation loop as the final acceptance. Out of scope: new tables, actions, events or ports; SSR, a second router or frontend, or a mobile app; retiring or rebuilding the design pack; the V1 routes, which are linked rather than re-implemented; and any production fixture or live tenant write inside the retrospective."
      },
      "created_at": "2026-11-26T21:59:01.357241+00:00",
      "id": "01a219de-7def-7827-8a21-98428640f502",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T18 The real V2 UI (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:59:01.639909+00:00",
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
      "approval_id": "01a219de-7e7e-72db-b751-a17bdd9b013d",
      "approved_at": "2026-11-26T21:59:01.639909+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T18 gate, judged by the fifteen checks of acceptance/programme/T18.toml: every destination route exists with its spec, each spec asserts empty, error, happy and one refusal, the coverage record reports exists only from a real route and a present spec with a *Gate reported as extend naming its owner, no production screen renders a fixture, and a signed-in tenant drives portfolio \u2192 programme \u2192 project \u2192 work package, organisation \u2192 release \u2192 binding, procedures intake \u2192 publish, tools call \u2192 ledger, improve \u2192 kata \u2192 adoption, operations incident \u2192 postmortem and the ADM phase rail from the app, with the T12 loop as the final spec.",
        "assignment": "Rework the V2 delivery slice T18 (SPEC_V2 \u00a710.5 and \u00a712 T18) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter, the Statement of Architecture Work and the delivery contract, take the scope baseline with the one WBS deliverable, record the retrospective Phase-G compliance review, publish the expected outputs project-brief, inception-analysis and architecture-design plus the evidence output, and leave the project active because T18-4 and T18-5 are not delivered. The work is retrospective: it reconstructs the governance record from the project row, the plan, the route table and the issue registry, and it attests no contemporaneous intent.",
        "contract": "The delivery contract record is requested and approved live by scripts/rework-project.py under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 and is titled 'Delivery contract \u2014 T18 The real V2 UI (retrospective, 2026-11-24)'. It commits the lane to the \u00a712 gate and to the two open units as typed dependencies, and it records that this retrospective neither closes the project nor waives any check. The contract's approval is retrospective; the historical snapshot has no live contract, which is one of the failed checks in the prior acceptance record.",
        "responsibilities": "The T18 lane under the programme architect (conveyor-e2e) is responsible for the route table, the screen modules, the specs and the coverage instrument. The Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract. The sponsoring group is accountable for the tranche-4 authorisation and for keeping the project active. The owning slices T4, T5, T7, T9, T10, T11, T13, T16 and T17 are responsible for the actions each destination calls, and T12 owns the final organisation loop.",
        "retrospective": true,
        "scope": "the shell, route registry and navigation; the ten V2 destinations of SPEC_V2 \u00a710.5 and the execution room; the L1\u2013L4 delivery view; one Playwright spec per destination; the V2 coverage instrument measured from apps/web/src/app.rs and the named specs; and the T12 organisation loop as the final acceptance. Out of scope: new tables, actions, events or ports; SSR, a second router or frontend, or a mobile app; retiring or rebuilding the design pack; the V1 routes, which are linked rather than re-implemented; and any production fixture or live tenant write inside the retrospective."
      },
      "created_at": "2026-11-26T21:59:01.357241+00:00",
      "id": "01a219de-7def-7827-8a21-98428640f502",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T18 The real V2 UI (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:59:01.639909+00:00",
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
      "approval_id": "01a219de-7e7e-72db-b751-a17bdd9b013d",
      "approved_at": "2026-11-26T21:59:01.639909+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T18 gate, judged by the fifteen checks of acceptance/programme/T18.toml: every destination route exists with its spec, each spec asserts empty, error, happy and one refusal, the coverage record reports exists only from a real route and a present spec with a *Gate reported as extend naming its owner, no production screen renders a fixture, and a signed-in tenant drives portfolio \u2192 programme \u2192 project \u2192 work package, organisation \u2192 release \u2192 binding, procedures intake \u2192 publish, tools call \u2192 ledger, improve \u2192 kata \u2192 adoption, operations incident \u2192 postmortem and the ADM phase rail from the app, with the T12 loop as the final spec.",
        "assignment": "Rework the V2 delivery slice T18 (SPEC_V2 \u00a710.5 and \u00a712 T18) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter, the Statement of Architecture Work and the delivery contract, take the scope baseline with the one WBS deliverable, record the retrospective Phase-G compliance review, publish the expected outputs project-brief, inception-analysis and architecture-design plus the evidence output, and leave the project active because T18-4 and T18-5 are not delivered. The work is retrospective: it reconstructs the governance record from the project row, the plan, the route table and the issue registry, and it attests no contemporaneous intent.",
        "contract": "The delivery contract record is requested and approved live by scripts/rework-project.py under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 and is titled 'Delivery contract \u2014 T18 The real V2 UI (retrospective, 2026-11-24)'. It commits the lane to the \u00a712 gate and to the two open units as typed dependencies, and it records that this retrospective neither closes the project nor waives any check. The contract's approval is retrospective; the historical snapshot has no live contract, which is one of the failed checks in the prior acceptance record.",
        "responsibilities": "The T18 lane under the programme architect (conveyor-e2e) is responsible for the route table, the screen modules, the specs and the coverage instrument. The Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract. The sponsoring group is accountable for the tranche-4 authorisation and for keeping the project active. The owning slices T4, T5, T7, T9, T10, T11, T13, T16 and T17 are responsible for the actions each destination calls, and T12 owns the final organisation loop.",
        "retrospective": true,
        "scope": "the shell, route registry and navigation; the ten V2 destinations of SPEC_V2 \u00a710.5 and the execution room; the L1\u2013L4 delivery view; one Playwright spec per destination; the V2 coverage instrument measured from apps/web/src/app.rs and the named specs; and the T12 organisation loop as the final acceptance. Out of scope: new tables, actions, events or ports; SSR, a second router or frontend, or a mobile app; retiring or rebuilding the design pack; the V1 routes, which are linked rather than re-implemented; and any production fixture or live tenant write inside the retrospective."
      },
      "created_at": "2026-11-26T21:59:01.357241+00:00",
      "id": "01a219de-7def-7827-8a21-98428640f502",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T18 The real V2 UI (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:59:01.639909+00:00",
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
      "approval_id": "01a219de-7e7e-72db-b751-a17bdd9b013d",
      "approved_at": "2026-11-26T21:59:01.639909+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T18 gate, judged by the fifteen checks of acceptance/programme/T18.toml: every destination route exists with its spec, each spec asserts empty, error, happy and one refusal, the coverage record reports exists only from a real route and a present spec with a *Gate reported as extend naming its owner, no production screen renders a fixture, and a signed-in tenant drives portfolio \u2192 programme \u2192 project \u2192 work package, organisation \u2192 release \u2192 binding, procedures intake \u2192 publish, tools call \u2192 ledger, improve \u2192 kata \u2192 adoption, operations incident \u2192 postmortem and the ADM phase rail from the app, with the T12 loop as the final spec.",
        "assignment": "Rework the V2 delivery slice T18 (SPEC_V2 \u00a710.5 and \u00a712 T18) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter, the Statement of Architecture Work and the delivery contract, take the scope baseline with the one WBS deliverable, record the retrospective Phase-G compliance review, publish the expected outputs project-brief, inception-analysis and architecture-design plus the evidence output, and leave the project active because T18-4 and T18-5 are not delivered. The work is retrospective: it reconstructs the governance record from the project row, the plan, the route table and the issue registry, and it attests no contemporaneous intent.",
        "contract": "The delivery contract record is requested and approved live by scripts/rework-project.py under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 and is titled 'Delivery contract \u2014 T18 The real V2 UI (retrospective, 2026-11-24)'. It commits the lane to the \u00a712 gate and to the two open units as typed dependencies, and it records that this retrospective neither closes the project nor waives any check. The contract's approval is retrospective; the historical snapshot has no live contract, which is one of the failed checks in the prior acceptance record.",
        "responsibilities": "The T18 lane under the programme architect (conveyor-e2e) is responsible for the route table, the screen modules, the specs and the coverage instrument. The Architecture Board (conveyor-architecture-board) approves the Phase A\u2013D artefacts and the delivery contract. The sponsoring group is accountable for the tranche-4 authorisation and for keeping the project active. The owning slices T4, T5, T7, T9, T10, T11, T13, T16 and T17 are responsible for the actions each destination calls, and T12 owns the final organisation loop.",
        "retrospective": true,
        "scope": "the shell, route registry and navigation; the ten V2 destinations of SPEC_V2 \u00a710.5 and the execution room; the L1\u2013L4 delivery view; one Playwright spec per destination; the V2 coverage instrument measured from apps/web/src/app.rs and the named specs; and the T12 organisation loop as the final acceptance. Out of scope: new tables, actions, events or ports; SSR, a second router or frontend, or a mobile app; retiring or rebuilding the design pack; the V1 routes, which are linked rather than re-implemented; and any production fixture or live tenant write inside the retrospective."
      },
      "created_at": "2026-11-26T21:59:01.357241+00:00",
      "id": "01a219de-7def-7827-8a21-98428640f502",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T18 The real V2 UI (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:59:01.639909+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

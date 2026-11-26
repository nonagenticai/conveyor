# Statement Of Architecture Work — Board — projects, boards, lists, cards, labels, comments, tasks, custom fields, webhooks, SSE

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a219d8-8c6d-715e-ad85-da38d5f3351c",
      "approved_at": "2026-11-26T21:52:31.994413+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S05.toml` and plan \u00a77: (a) replay fidelity \u2014 1,000 cards/10 lists, `admin diff` prints zero deltas; (b) one winner \u2014 two clients with the same `If-Match` get one 200 and one 412 carrying the current representation, `version` +1; (c) 1k load \u2014 `GET /api/boards/{id}/cards?limit=200` p95 \u2264 120 ms at 20 rps, 1,000 cards in five pages \u2264 500 ms; (d) webhook shape \u2014 `{event,data,prevData,user}` exactly, `event` the internal register type and `data.list_id`/`prevData.list_id` the lanes; (e) SSE \u2014 a second client on `board:{id}` gets `card.moved` \u2264 200 ms live and on replay; (f) no second store \u2014 one `comment`, one `board_membership`, no `card_membership`, no `attachment`. The instrument is `acceptance/programme/S05.toml`; the closure gate is decided by an approver other than its requester. The snapshot records no Phase-G review of these properties; this rework records that gap rather than claiming a pass.",
        "assignment": "Rework the V1 slice S05 (SPEC \u00a78 S5) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the acceptance instrument.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the board slice of the plan: the nine tables (board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription), the 39 actions, the read projections and cursors, the SSE topics `board:{id}` and the resolved `card:{id}`, the webhook emitter and its `webhook.deliver` job, and the key-referenced `board-export/v1` CLI. Out of scope: attachments and card-level memberships, claims/assignee/stopwatch writes (S06), webhook delivery rows as actions, and the UI (S12)."
      },
      "created_at": "2026-11-26T21:52:31.682863+00:00",
      "id": "01a219d8-8bc7-71a2-8dbe-33702699eac1",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S05 Board (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:52:31.994413+00:00",
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
      "approval_id": "01a219d8-8c6d-715e-ad85-da38d5f3351c",
      "approved_at": "2026-11-26T21:52:31.994413+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S05.toml` and plan \u00a77: (a) replay fidelity \u2014 1,000 cards/10 lists, `admin diff` prints zero deltas; (b) one winner \u2014 two clients with the same `If-Match` get one 200 and one 412 carrying the current representation, `version` +1; (c) 1k load \u2014 `GET /api/boards/{id}/cards?limit=200` p95 \u2264 120 ms at 20 rps, 1,000 cards in five pages \u2264 500 ms; (d) webhook shape \u2014 `{event,data,prevData,user}` exactly, `event` the internal register type and `data.list_id`/`prevData.list_id` the lanes; (e) SSE \u2014 a second client on `board:{id}` gets `card.moved` \u2264 200 ms live and on replay; (f) no second store \u2014 one `comment`, one `board_membership`, no `card_membership`, no `attachment`. The instrument is `acceptance/programme/S05.toml`; the closure gate is decided by an approver other than its requester. The snapshot records no Phase-G review of these properties; this rework records that gap rather than claiming a pass.",
        "assignment": "Rework the V1 slice S05 (SPEC \u00a78 S5) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the acceptance instrument.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the board slice of the plan: the nine tables (board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription), the 39 actions, the read projections and cursors, the SSE topics `board:{id}` and the resolved `card:{id}`, the webhook emitter and its `webhook.deliver` job, and the key-referenced `board-export/v1` CLI. Out of scope: attachments and card-level memberships, claims/assignee/stopwatch writes (S06), webhook delivery rows as actions, and the UI (S12)."
      },
      "created_at": "2026-11-26T21:52:31.682863+00:00",
      "id": "01a219d8-8bc7-71a2-8dbe-33702699eac1",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S05 Board (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:52:31.994413+00:00",
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
      "approval_id": "01a219d8-8c6d-715e-ad85-da38d5f3351c",
      "approved_at": "2026-11-26T21:52:31.994413+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S05.toml` and plan \u00a77: (a) replay fidelity \u2014 1,000 cards/10 lists, `admin diff` prints zero deltas; (b) one winner \u2014 two clients with the same `If-Match` get one 200 and one 412 carrying the current representation, `version` +1; (c) 1k load \u2014 `GET /api/boards/{id}/cards?limit=200` p95 \u2264 120 ms at 20 rps, 1,000 cards in five pages \u2264 500 ms; (d) webhook shape \u2014 `{event,data,prevData,user}` exactly, `event` the internal register type and `data.list_id`/`prevData.list_id` the lanes; (e) SSE \u2014 a second client on `board:{id}` gets `card.moved` \u2264 200 ms live and on replay; (f) no second store \u2014 one `comment`, one `board_membership`, no `card_membership`, no `attachment`. The instrument is `acceptance/programme/S05.toml`; the closure gate is decided by an approver other than its requester. The snapshot records no Phase-G review of these properties; this rework records that gap rather than claiming a pass.",
        "assignment": "Rework the V1 slice S05 (SPEC \u00a78 S5) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the acceptance instrument.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the board slice of the plan: the nine tables (board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription), the 39 actions, the read projections and cursors, the SSE topics `board:{id}` and the resolved `card:{id}`, the webhook emitter and its `webhook.deliver` job, and the key-referenced `board-export/v1` CLI. Out of scope: attachments and card-level memberships, claims/assignee/stopwatch writes (S06), webhook delivery rows as actions, and the UI (S12)."
      },
      "created_at": "2026-11-26T21:52:31.682863+00:00",
      "id": "01a219d8-8bc7-71a2-8dbe-33702699eac1",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S05 Board (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:52:31.994413+00:00",
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
      "approval_id": "01a219d8-8c6d-715e-ad85-da38d5f3351c",
      "approved_at": "2026-11-26T21:52:31.994413+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S05.toml` and plan \u00a77: (a) replay fidelity \u2014 1,000 cards/10 lists, `admin diff` prints zero deltas; (b) one winner \u2014 two clients with the same `If-Match` get one 200 and one 412 carrying the current representation, `version` +1; (c) 1k load \u2014 `GET /api/boards/{id}/cards?limit=200` p95 \u2264 120 ms at 20 rps, 1,000 cards in five pages \u2264 500 ms; (d) webhook shape \u2014 `{event,data,prevData,user}` exactly, `event` the internal register type and `data.list_id`/`prevData.list_id` the lanes; (e) SSE \u2014 a second client on `board:{id}` gets `card.moved` \u2264 200 ms live and on replay; (f) no second store \u2014 one `comment`, one `board_membership`, no `card_membership`, no `attachment`. The instrument is `acceptance/programme/S05.toml`; the closure gate is decided by an approver other than its requester. The snapshot records no Phase-G review of these properties; this rework records that gap rather than claiming a pass.",
        "assignment": "Rework the V1 slice S05 (SPEC \u00a78 S5) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the acceptance instrument.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the board slice of the plan: the nine tables (board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription), the 39 actions, the read projections and cursors, the SSE topics `board:{id}` and the resolved `card:{id}`, the webhook emitter and its `webhook.deliver` job, and the key-referenced `board-export/v1` CLI. Out of scope: attachments and card-level memberships, claims/assignee/stopwatch writes (S06), webhook delivery rows as actions, and the UI (S12)."
      },
      "created_at": "2026-11-26T21:52:31.682863+00:00",
      "id": "01a219d8-8bc7-71a2-8dbe-33702699eac1",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S05 Board (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:52:31.994413+00:00",
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
      "approval_id": "01a219d8-8c6d-715e-ad85-da38d5f3351c",
      "approved_at": "2026-11-26T21:52:31.994413+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S05.toml` and plan \u00a77: (a) replay fidelity \u2014 1,000 cards/10 lists, `admin diff` prints zero deltas; (b) one winner \u2014 two clients with the same `If-Match` get one 200 and one 412 carrying the current representation, `version` +1; (c) 1k load \u2014 `GET /api/boards/{id}/cards?limit=200` p95 \u2264 120 ms at 20 rps, 1,000 cards in five pages \u2264 500 ms; (d) webhook shape \u2014 `{event,data,prevData,user}` exactly, `event` the internal register type and `data.list_id`/`prevData.list_id` the lanes; (e) SSE \u2014 a second client on `board:{id}` gets `card.moved` \u2264 200 ms live and on replay; (f) no second store \u2014 one `comment`, one `board_membership`, no `card_membership`, no `attachment`. The instrument is `acceptance/programme/S05.toml`; the closure gate is decided by an approver other than its requester. The snapshot records no Phase-G review of these properties; this rework records that gap rather than claiming a pass.",
        "assignment": "Rework the V1 slice S05 (SPEC \u00a78 S5) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure review, and re-close through the acceptance instrument.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the board slice of the plan: the nine tables (board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription), the 39 actions, the read projections and cursors, the SSE topics `board:{id}` and the resolved `card:{id}`, the webhook emitter and its `webhook.deliver` job, and the key-referenced `board-export/v1` CLI. Out of scope: attachments and card-level memberships, claims/assignee/stopwatch writes (S06), webhook delivery rows as actions, and the UI (S12)."
      },
      "created_at": "2026-11-26T21:52:31.682863+00:00",
      "id": "01a219d8-8bc7-71a2-8dbe-33702699eac1",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d29-796d-967b-1e819a4dd40a",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S05 Board (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:52:31.994413+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

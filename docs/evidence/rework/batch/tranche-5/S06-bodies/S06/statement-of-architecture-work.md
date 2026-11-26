# Statement Of Architecture Work — Control plane — claims, runs, approvals, andon, comms and notifications

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a21a39-aae9-7d90-9069-b0576ed463d4",
      "approved_at": "2026-11-26T23:38:36.773008+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S06.toml` and plan \u00a77: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 `cards.move(done)` yields `run.succeeded`, `approval.approved` and one audit, invocation and event row each; (b) dual control \u2014 `required_decisions=2`, one approve stays pending, the same approver again is 409, a second approver closes; (c) retry \u2014 `kill -9` mid-run leads to `run.reap`, a crashed run, then `attempt=2` with two rows and never two live runs; (d) andon \u2014 engaged gives 409 and a pending `run.execute` for 60 s, released resumes within one poll; (e) delivery retry \u2014 a stub 500 gives `attempts=5`, `state=failed`, one row; (f) no timers \u2014 `rg 'setInterval' crates apps` is empty and every cadence is a `schedule` row; (g) human vs agent \u2014 `decided_by_kind` is `human` for one decision and `agent` for the other. The instrument is `acceptance/programme/S06.toml`; the close gate is decided by an approver other than its requester. The snapshot records no run of these properties and no Phase-G review; this rework records the level `non_conformant` rather than claiming a pass.",
        "assignment": "Rework the V1 slice S06 (SPEC \u00a78 S6) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure decision, and re-close through the acceptance instrument once its properties run.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the control-plane slice of the plan: the six tables of `02-DATA-ARCHITECTURE \u00a72.6` (`channel_binding`, `channel_preference`, `message`, `delivery`, `audience`, `audience_member` \u2014 the last two PENDING-S0), the 17 actions, the 17 events, the SSE topics `run:{id}`, `approval:{id}` and `tenant`, the Telegram in/out path with its sender-identity resolution, the andon gauge and poller pause, and the job kinds that replace 11 of fr's 14 timers (three deleted). Out of scope: the signed mcp_server channel (S06a), MCP tool surfacing (S10), `project_request.timeout` (S07), the UI (S12) and the audience screens (PENDING-S0)."
      },
      "created_at": "2026-11-26T23:38:36.518039+00:00",
      "id": "01a21a39-aa69-7167-8696-e615aa3b012f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06 Control Plane (retrospective, 2026-11-27)",
      "updated_at": "2026-11-26T23:38:36.773008+00:00",
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
      "approval_id": "01a21a39-aae9-7d90-9069-b0576ed463d4",
      "approved_at": "2026-11-26T23:38:36.773008+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S06.toml` and plan \u00a77: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 `cards.move(done)` yields `run.succeeded`, `approval.approved` and one audit, invocation and event row each; (b) dual control \u2014 `required_decisions=2`, one approve stays pending, the same approver again is 409, a second approver closes; (c) retry \u2014 `kill -9` mid-run leads to `run.reap`, a crashed run, then `attempt=2` with two rows and never two live runs; (d) andon \u2014 engaged gives 409 and a pending `run.execute` for 60 s, released resumes within one poll; (e) delivery retry \u2014 a stub 500 gives `attempts=5`, `state=failed`, one row; (f) no timers \u2014 `rg 'setInterval' crates apps` is empty and every cadence is a `schedule` row; (g) human vs agent \u2014 `decided_by_kind` is `human` for one decision and `agent` for the other. The instrument is `acceptance/programme/S06.toml`; the close gate is decided by an approver other than its requester. The snapshot records no run of these properties and no Phase-G review; this rework records the level `non_conformant` rather than claiming a pass.",
        "assignment": "Rework the V1 slice S06 (SPEC \u00a78 S6) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure decision, and re-close through the acceptance instrument once its properties run.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the control-plane slice of the plan: the six tables of `02-DATA-ARCHITECTURE \u00a72.6` (`channel_binding`, `channel_preference`, `message`, `delivery`, `audience`, `audience_member` \u2014 the last two PENDING-S0), the 17 actions, the 17 events, the SSE topics `run:{id}`, `approval:{id}` and `tenant`, the Telegram in/out path with its sender-identity resolution, the andon gauge and poller pause, and the job kinds that replace 11 of fr's 14 timers (three deleted). Out of scope: the signed mcp_server channel (S06a), MCP tool surfacing (S10), `project_request.timeout` (S07), the UI (S12) and the audience screens (PENDING-S0)."
      },
      "created_at": "2026-11-26T23:38:36.518039+00:00",
      "id": "01a21a39-aa69-7167-8696-e615aa3b012f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06 Control Plane (retrospective, 2026-11-27)",
      "updated_at": "2026-11-26T23:38:36.773008+00:00",
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
      "approval_id": "01a21a39-aae9-7d90-9069-b0576ed463d4",
      "approved_at": "2026-11-26T23:38:36.773008+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S06.toml` and plan \u00a77: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 `cards.move(done)` yields `run.succeeded`, `approval.approved` and one audit, invocation and event row each; (b) dual control \u2014 `required_decisions=2`, one approve stays pending, the same approver again is 409, a second approver closes; (c) retry \u2014 `kill -9` mid-run leads to `run.reap`, a crashed run, then `attempt=2` with two rows and never two live runs; (d) andon \u2014 engaged gives 409 and a pending `run.execute` for 60 s, released resumes within one poll; (e) delivery retry \u2014 a stub 500 gives `attempts=5`, `state=failed`, one row; (f) no timers \u2014 `rg 'setInterval' crates apps` is empty and every cadence is a `schedule` row; (g) human vs agent \u2014 `decided_by_kind` is `human` for one decision and `agent` for the other. The instrument is `acceptance/programme/S06.toml`; the close gate is decided by an approver other than its requester. The snapshot records no run of these properties and no Phase-G review; this rework records the level `non_conformant` rather than claiming a pass.",
        "assignment": "Rework the V1 slice S06 (SPEC \u00a78 S6) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure decision, and re-close through the acceptance instrument once its properties run.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the control-plane slice of the plan: the six tables of `02-DATA-ARCHITECTURE \u00a72.6` (`channel_binding`, `channel_preference`, `message`, `delivery`, `audience`, `audience_member` \u2014 the last two PENDING-S0), the 17 actions, the 17 events, the SSE topics `run:{id}`, `approval:{id}` and `tenant`, the Telegram in/out path with its sender-identity resolution, the andon gauge and poller pause, and the job kinds that replace 11 of fr's 14 timers (three deleted). Out of scope: the signed mcp_server channel (S06a), MCP tool surfacing (S10), `project_request.timeout` (S07), the UI (S12) and the audience screens (PENDING-S0)."
      },
      "created_at": "2026-11-26T23:38:36.518039+00:00",
      "id": "01a21a39-aa69-7167-8696-e615aa3b012f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06 Control Plane (retrospective, 2026-11-27)",
      "updated_at": "2026-11-26T23:38:36.773008+00:00",
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
      "approval_id": "01a21a39-aae9-7d90-9069-b0576ed463d4",
      "approved_at": "2026-11-26T23:38:36.773008+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S06.toml` and plan \u00a77: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 `cards.move(done)` yields `run.succeeded`, `approval.approved` and one audit, invocation and event row each; (b) dual control \u2014 `required_decisions=2`, one approve stays pending, the same approver again is 409, a second approver closes; (c) retry \u2014 `kill -9` mid-run leads to `run.reap`, a crashed run, then `attempt=2` with two rows and never two live runs; (d) andon \u2014 engaged gives 409 and a pending `run.execute` for 60 s, released resumes within one poll; (e) delivery retry \u2014 a stub 500 gives `attempts=5`, `state=failed`, one row; (f) no timers \u2014 `rg 'setInterval' crates apps` is empty and every cadence is a `schedule` row; (g) human vs agent \u2014 `decided_by_kind` is `human` for one decision and `agent` for the other. The instrument is `acceptance/programme/S06.toml`; the close gate is decided by an approver other than its requester. The snapshot records no run of these properties and no Phase-G review; this rework records the level `non_conformant` rather than claiming a pass.",
        "assignment": "Rework the V1 slice S06 (SPEC \u00a78 S6) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure decision, and re-close through the acceptance instrument once its properties run.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the control-plane slice of the plan: the six tables of `02-DATA-ARCHITECTURE \u00a72.6` (`channel_binding`, `channel_preference`, `message`, `delivery`, `audience`, `audience_member` \u2014 the last two PENDING-S0), the 17 actions, the 17 events, the SSE topics `run:{id}`, `approval:{id}` and `tenant`, the Telegram in/out path with its sender-identity resolution, the andon gauge and poller pause, and the job kinds that replace 11 of fr's 14 timers (three deleted). Out of scope: the signed mcp_server channel (S06a), MCP tool surfacing (S10), `project_request.timeout` (S07), the UI (S12) and the audience screens (PENDING-S0)."
      },
      "created_at": "2026-11-26T23:38:36.518039+00:00",
      "id": "01a21a39-aa69-7167-8696-e615aa3b012f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06 Control Plane (retrospective, 2026-11-27)",
      "updated_at": "2026-11-26T23:38:36.773008+00:00",
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
      "approval_id": "01a21a39-aae9-7d90-9069-b0576ed463d4",
      "approved_at": "2026-11-26T23:38:36.773008+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The slice gate in `acceptance/programme/S06.toml` and plan \u00a77: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 `cards.move(done)` yields `run.succeeded`, `approval.approved` and one audit, invocation and event row each; (b) dual control \u2014 `required_decisions=2`, one approve stays pending, the same approver again is 409, a second approver closes; (c) retry \u2014 `kill -9` mid-run leads to `run.reap`, a crashed run, then `attempt=2` with two rows and never two live runs; (d) andon \u2014 engaged gives 409 and a pending `run.execute` for 60 s, released resumes within one poll; (e) delivery retry \u2014 a stub 500 gives `attempts=5`, `state=failed`, one row; (f) no timers \u2014 `rg 'setInterval' crates apps` is empty and every cadence is a `schedule` row; (g) human vs agent \u2014 `decided_by_kind` is `human` for one decision and `agent` for the other. The instrument is `acceptance/programme/S06.toml`; the close gate is decided by an approver other than its requester. The snapshot records no run of these properties and no Phase-G review; this rework records the level `non_conformant` rather than claiming a pass.",
        "assignment": "Rework the V1 slice S06 (SPEC \u00a78 S6) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, take the scope baseline with the one WBS deliverable, record the Phase-G compliance review, publish the expected outputs (`project-brief`, `evidence`) and the closure decision, and re-close through the acceptance instrument once its properties run.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the control-plane slice of the plan: the six tables of `02-DATA-ARCHITECTURE \u00a72.6` (`channel_binding`, `channel_preference`, `message`, `delivery`, `audience`, `audience_member` \u2014 the last two PENDING-S0), the 17 actions, the 17 events, the SSE topics `run:{id}`, `approval:{id}` and `tenant`, the Telegram in/out path with its sender-identity resolution, the andon gauge and poller pause, and the job kinds that replace 11 of fr's 14 timers (three deleted). Out of scope: the signed mcp_server channel (S06a), MCP tool surfacing (S10), `project_request.timeout` (S07), the UI (S12) and the audience screens (PENDING-S0)."
      },
      "created_at": "2026-11-26T23:38:36.518039+00:00",
      "id": "01a21a39-aa69-7167-8696-e615aa3b012f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d2a-7007-bcd8-29bb24f36b54",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 S06 Control Plane (retrospective, 2026-11-27)",
      "updated_at": "2026-11-26T23:38:36.773008+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

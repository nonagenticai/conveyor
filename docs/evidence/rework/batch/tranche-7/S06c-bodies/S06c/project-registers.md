# Project Registers — MCP tasks — a long-running project asks for input

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "`uq_run_live` permits two live rows when the discriminating column is null, so a check-then-insert in application code would let two live executions of the same project coexist under load. Migration 0023 adds a partial unique index. The risk that remains is a later migration dropping or widening it without knowing why it is partial.",
      "code": "S06c-R1",
      "created_at": "2026-11-28T14:54:11.524831+00:00",
      "id": "01a222a6-4448-79f1-ac11-3a908678ecee",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S06c-mcp-tasks.md#8"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "low",
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": "Keep the index's comment and this risk row as the record of why it is partial, and refuse a migration that replaces it with a full unique index on the same columns.",
      "review_at": "2026-12-12",
      "severity": "medium",
      "status": "open",
      "title": "The live one-live-run property depends on a partial index, because the existing unique index does not dedupe nulls",
      "updated_at": "2026-11-28T14:54:11.524831+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "`reap_dead_runs_tx` now requires `card_id IS NOT NULL`, which is what keeps the reaper from killing a task that is legitimately waiting on a human for a day. If a later change relaxes the predicate, long-running tasks are reaped and the surface reports a cancellation nobody asked for.",
      "code": "S06c-R2",
      "created_at": "2026-11-28T14:54:11.665609+00:00",
      "id": "01a222a6-44d4-7638-85f5-f132b8e0bd72",
      "impact": 4,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S06c-mcp-tasks.md#8"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": "Cover the reaper's project-execution exclusion with a test that fails if the predicate is widened, and treat the predicate as part of this project's contract rather than as an implementation detail.",
      "review_at": "2026-12-12",
      "severity": "medium",
      "status": "open",
      "title": "The reaper must skip project executions, and its correctness depends on that predicate",
      "updated_at": "2026-11-28T14:54:11.665609+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A terminal run is surfaced as a completed result carrying `isError`, and the terminal check runs before the approval check. Reverse the order and a failed run with a stale pending approval reports `inputRequests` for a task that will never accept an answer; drop the terminal check entirely and the client polls a dead task forever.",
      "code": "S06c-R3",
      "created_at": "2026-11-28T14:54:11.806622+00:00",
      "id": "01a222a6-4562-7421-8a21-73815103546c",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S06c-mcp-tasks.md#8"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "low",
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": "Assert both orderings in the contract suite: a failed run with a stale approval must terminate, not block.",
      "review_at": "2026-12-12",
      "severity": "medium",
      "status": "open",
      "title": "The `failed` state must never reach the wire, which makes the terminal check ordering load-bearing",
      "updated_at": "2026-11-28T14:54:11.806622+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Issues

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 4,
  "rows": [
    {
      "baseline_ref": null,
      "body": "Closed in an unauthorised tranche before the tranche review; accepted by the delegate as a retrospective exception under the standing delegation. The project is reworked in its batch and re-closed through the gates.",
      "code": "U3-EXC-S06c",
      "created_at": "2026-11-21T01:17:54.134569+00:00",
      "id": "01a1fbae-6b35-7b52-a47a-43c0bf7199a3",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": "P0",
      "status": "accepted",
      "title": "Out-of-sequence closure accepted as a retrospective exception (S06c)",
      "updated_at": "2026-11-21T01:17:54.134569+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "At the 2026-11-28 snapshot the project row is closed and its product is in the tree, but the snapshot carries no charter asset, no approved baseline, no delivery contract, no Phase A\u2013D set and no compliance review. The gap is a record gap rather than a product gap; this pack supplies the set and the receipt proves it.",
      "code": "S06c-I1",
      "created_at": "2026-11-28T14:54:11.951703+00:00",
      "id": "01a222a6-45f2-7cf2-93de-3d0d10bce1fa",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": "Supplied by this pack and proved by the project's receipt at the pinned snapshot.",
      "review_at": null,
      "severity": "P1",
      "status": "open",
      "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review",
      "updated_at": "2026-11-28T14:54:11.951703+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The plan's compiled-registry assertion originally grepped for `task_capable: true`, a flag carried by neither the admin action dump nor the committed action list. A filter that selects nothing exits 0, so a check that proved nothing read as a pass until the plan corrected it to read the compiled registry directly.",
      "code": "S06c-I2",
      "created_at": "2026-11-28T14:54:12.094425+00:00",
      "id": "01a222a6-4681-7353-9df6-3e5ee3f01bb8",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": "The corrected assertion is the one the receipt runs; the mistake is recorded so the pattern is not reintroduced by a later zero-match filter.",
      "review_at": null,
      "severity": "P2",
      "status": "open",
      "title": "The gate's first form could never match, and its zero-match run exited green",
      "updated_at": "2026-11-28T14:54:12.094425+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The task surface reports state transitions but no percentage and no server push; the client polls. Both absences are deliberate per the slice plan's boundary, but an MCP client author reading the protocol spec may expect them, so the absence needs a record rather than a silence.",
      "code": "S06c-I3",
      "created_at": "2026-11-28T14:54:12.235715+00:00",
      "id": "01a222a6-470e-7d10-ad6c-f6062b3e728a",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": "Document the polling-only contract in the project brief and treat a push or percentage request as a new unit rather than a defect in this one.",
      "review_at": null,
      "severity": "P2",
      "status": "open",
      "title": "A progress percentage and push delivery are absent by scope, not by omission",
      "updated_at": "2026-11-28T14:54:12.235715+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Assumptions

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 4,
  "rows": [
    {
      "baseline_ref": null,
      "body": "Every judgement in this pack is read from the pinned snapshot, and the content checker refuses any identifier that appears nowhere in it. A row that changed after the snapshot is out of scope for this pack by construction.",
      "code": "S06c-A1",
      "created_at": "2026-11-28T14:54:12.376066+00:00",
      "id": "01a222a6-479b-7003-be9f-b9c09a148caa",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The 2026-11-28 rework snapshot is the authoritative record for S06c",
      "updated_at": "2026-11-28T14:54:12.376066+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The protocol allows push; this implementation does not provide it. The assumption is that a conforming client treats push as optional, which the slice plan's decisions assert when they keep polling as the fallback.",
      "code": "S06c-A2",
      "created_at": "2026-11-28T14:54:12.514844+00:00",
      "id": "01a222a6-4825-7ce9-8ff4-1d6c4e084e09",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A conforming MCP client accepts polling as the only delivery mechanism",
      "updated_at": "2026-11-28T14:54:12.514844+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The partial unique index enforces it. The assumption is that a project is a single unit of concurrent work; if a project ever needs two live executions the index is the thing that will say so, loudly, at the second start.",
      "code": "S06c-A3",
      "created_at": "2026-11-28T14:54:12.659124+00:00",
      "id": "01a222a6-48b6-720b-9a6a-8d01448f2048",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "One live execution per project is the right cardinality",
      "updated_at": "2026-11-28T14:54:12.659124+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The task's `inputRequests` are the project's pending approvals, and there is no other kind of input the surface accepts. A future input type \u2014 a form, a selection \u2014 would need its own mapping rather than a widening of the approval path.",
      "code": "S06c-A4",
      "created_at": "2026-11-28T14:54:12.797087+00:00",
      "id": "01a222a6-4940-7157-b0be-46c3bb6c14c8",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The approval is a sufficient input request",
      "updated_at": "2026-11-28T14:54:12.797087+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Dependencies

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The project row records the edges. S04 supplies the action registry and the `/mcp` transport, S05 the project and card actions, and S06 the run and approval state machines together with the events the task surface reads.",
      "code": "S06c-D1",
      "created_at": "2026-11-28T14:54:12.938383+00:00",
      "id": "01a222a6-49cd-72c8-bae0-e16625b9f1ac",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S06c depends on S04, S05 and S06",
      "updated_at": "2026-11-28T14:54:12.938383+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A task-capable call is still a licence-principal call; S06b's machine-auth resolution and tool list are what make the call attributable. Without that principal the task would be anonymous and the ownership failures would have nothing to compare.",
      "code": "S06c-D2",
      "created_at": "2026-11-28T14:54:13.085386+00:00",
      "id": "01a222a6-4a61-710f-be32-91a0f176862a",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S06b supplies the licence principal the task surface authenticates",
      "updated_at": "2026-11-28T14:54:13.085386+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The task status text is S13's to fill and the agent's client-side task support against external servers is a separate plan with its own gate. Recording both stops a reader treating an empty status message as an S06c defect.",
      "code": "S06c-D3",
      "created_at": "2026-11-28T14:54:13.2363+00:00",
      "id": "01a222a6-4af7-715d-8cb9-485c4962babf",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S13 populates `statusMessage` and the agent's own task support is a separate plan",
      "updated_at": "2026-11-28T14:54:13.2363+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Benefits

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 4,
  "rows": [
    {
      "baseline_ref": null,
      "body": "Before the task protocol a client that disconnected lost the ability to follow the project without re-deriving its state from the run table. After it, the run is the task and the client polls a standard surface. The benefit is structural and is proved by the contract suite rather than by a measurement.",
      "code": "S06c-B1",
      "created_at": "2026-11-28T14:54:13.382553+00:00",
      "id": "01a222a6-4b89-79d3-8a5e-27f3356984f7",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S06c-mcp-tasks.md#1"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A long-running project becomes a durable task with no second store",
      "updated_at": "2026-11-28T14:54:13.382553+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The `inputRequests` listing and the `tasks/update` answer mean a client needs one surface, not two, to start work and to unblock it. The benefit is observed in the single round-trip test that starts a project, receives a task, answers the approval and reads a terminal result.",
      "code": "S06c-B2",
      "created_at": "2026-11-28T14:54:13.521987+00:00",
      "id": "01a222a6-4c15-71e8-8025-de6e156a2276",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S06c-mcp-tasks.md#7"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A blocked project is answerable through the protocol that started it",
      "updated_at": "2026-11-28T14:54:13.521987+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "`projects.execute` is the only action that returns a task, asserted against the compiled registry. A surface where every action could be a task would make the client's lifecycle handling universal and the server's contract unprovable; one action keeps both bounded.",
      "code": "S06c-B3",
      "created_at": "2026-11-28T14:54:13.662722+00:00",
      "id": "01a222a6-4ca2-7021-b6ae-76f82a1be795",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S06c-mcp-tasks.md#7"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "One task-capable action keeps the surface small",
      "updated_at": "2026-11-28T14:54:13.662722+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Every ownership failure returns the same `-32602`, so the endpoint cannot be used to probe which project ids exist. The benefit is a security property and is measured by the suite that asserts one code across the failure cases.",
      "code": "S06c-B4",
      "created_at": "2026-11-28T14:54:13.805909+00:00",
      "id": "01a222a6-4d31-72d7-b58e-aa2a86424c68",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S06c-mcp-tasks.md#8"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Ownership failures are indistinguishable",
      "updated_at": "2026-11-28T14:54:13.805909+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Changes

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 1,
  "rows": [
    {
      "baseline_ref": "01a222a6-4dd7-71e2-b652-66d5f1b7a071",
      "body": "The retrospective MSP/TOGAF/SAFe rework changed this project's record: its artefacts were re-versioned with documents and links, its register was written, and its schedule and cost baselines were taken. This change records that rework against the frozen scope baseline.",
      "code": "CH-S06c-03386b",
      "created_at": "2026-11-28T14:54:27.125374+00:00",
      "id": "01a222a6-813c-736e-9930-8c3d2203386b",
      "impact": null,
      "kind": "change",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {
        "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
        "impact": {
          "scope": "the retrospective rework's record acts"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2b-7542-9574-e961ffd1b597",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "The retrospective rework of S06c",
      "updated_at": "2026-11-28T14:54:27.4928+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```

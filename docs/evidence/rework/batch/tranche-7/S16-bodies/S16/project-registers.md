# Project Registers — Inference ledger and prompt audit

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The shipped capture test asserts the stored row against the router's usage schema, but it runs against a stub transport. The slice plan's live rehearsal, which needs a real router token, is marked ignored and has not been run. Until it is, the field-for-field property is evidenced for the schema and the seam, not for a live provider response.",
      "code": "S16-R1",
      "created_at": "2026-11-28T14:56:04.236539+00:00",
      "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S16-inference-audit.md#7"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": "Run the ignored live rehearsal against the estate router and attach its output to the project's evidence before any tranche review claims live capture.",
      "review_at": "2026-12-12",
      "severity": "medium",
      "status": "open",
      "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub",
      "updated_at": "2026-11-28T14:56:04.236539+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The capture is written inside the call's own transaction so that it can never fail the call. The consequence is the mirror image: a genuine database failure rolls the audit row back with the rest of the transaction, and a call that happened leaves no ledger trace. The trade is deliberate but it is not free, and a silent under-count is harder to notice than a failed call.",
      "code": "S16-R2",
      "created_at": "2026-11-28T14:56:04.399981+00:00",
      "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S16-inference-audit.md#8"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": "Alert on a router call count that exceeds the ledger row count over a window, so the rollback case surfaces as a discrepancy rather than as an absence.",
      "review_at": "2026-12-12",
      "severity": "medium",
      "status": "open",
      "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row",
      "updated_at": "2026-11-28T14:56:04.399981+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The joins to `procedure`, `procedure_step`, `run_step` and `conversation_turn` are integers rather than foreign keys, because database-level referential integrity runs outside row-level security and would let one tenant's check read another's rows. An orphaned ledger row is therefore permitted by construction, and the grouping report is the only thing between the design and a silently unattributed cost.",
      "code": "S16-R3",
      "created_at": "2026-11-28T14:56:04.551344+00:00",
      "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
      "impact": 2,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S16-inference-audit.md#9"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "low",
      "programme_id": null,
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": "Keep the report's group-by on the prompt sha, and treat an unattributed row count above zero as a finding rather than as noise.",
      "review_at": "2026-12-12",
      "severity": "low",
      "status": "open",
      "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it",
      "updated_at": "2026-11-28T14:56:04.551344+00:00",
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
      "code": "U3-EXC-S16",
      "created_at": "2026-11-21T01:18:01.784553+00:00",
      "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": "P0",
      "status": "accepted",
      "title": "Out-of-sequence closure accepted as a retrospective exception (S16)",
      "updated_at": "2026-11-21T01:18:01.784553+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "At the 2026-11-28 snapshot the project row is closed and its product is in the tree, but the snapshot carries no charter asset, no approved baseline, no delivery contract, no Phase A\u2013D set and no compliance review. The gap is a record gap, not a product gap: this pack supplies the set and the receipt proves it.",
      "code": "S16-I1",
      "created_at": "2026-11-28T14:56:04.698046+00:00",
      "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": "Supplied by this pack and proved by the project's receipt at the pinned snapshot.",
      "review_at": null,
      "severity": "P1",
      "status": "open",
      "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review",
      "updated_at": "2026-11-28T14:56:04.698046+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The retention decision is ninety days and the report groups by prompt sha from day one, but the two prune schedule rows the decision needs are deferred to S17 and are not in the record. A declared retention with no executing schedule is a policy statement, not a control, and the tranche review should read it that way.",
      "code": "S16-I2",
      "created_at": "2026-11-28T14:56:04.841432+00:00",
      "id": "01a222a7-feec-7963-937f-34fa00815231",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": "Carry the deferral into S17's acceptance and record it in the tranche-7 review's risk section rather than treating retention as delivered.",
      "review_at": null,
      "severity": "P2",
      "status": "open",
      "title": "Retention is declared at ninety days but its prune schedules are S17's",
      "updated_at": "2026-11-28T14:56:04.841432+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The cache status is derived from the response, never the request, and falls to `unknown` when the provider omits the class. That is the right call for honesty and the wrong one for a savings report: an under-reported cache benefit looks like a system that caches badly. The row is correct; the report needs to separate `unknown` from `miss` before anyone reads a trend from it.",
      "code": "S16-I3",
      "created_at": "2026-11-28T14:56:04.989063+00:00",
      "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": "Keep `unknown` as its own report bucket and refuse to fold it into `miss` in any downstream aggregate.",
      "review_at": null,
      "severity": "P2",
      "status": "open",
      "title": "A provider that omits the cache class records unknown rather than a guess",
      "updated_at": "2026-11-28T14:56:04.989063+00:00",
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
      "body": "Every judgement in this pack is read from the pinned snapshot, and the content checker refuses any identifier that appears nowhere in it. A row that changed after the snapshot is therefore out of scope for this pack by construction.",
      "code": "S16-A1",
      "created_at": "2026-11-28T14:56:05.141453+00:00",
      "id": "01a222a8-0018-7780-904d-a8f750abec69",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The 2026-11-28 rework snapshot is the authoritative record for S16",
      "updated_at": "2026-11-28T14:56:05.141453+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The ledger stores what the provider returned rather than what the conveyor thinks it sent, so a disagreement between the two is a bug in the estimate and not a reason to prefer the local number. The field-for-field test encodes that preference.",
      "code": "S16-A2",
      "created_at": "2026-11-28T14:56:05.285793+00:00",
      "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The router's usage object is the single source of token truth",
      "updated_at": "2026-11-28T14:56:05.285793+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The retention decision assumes a review that starts within a quarter. A longer investigation would need the rows the prune removes, so the decision is revisited before the schedule is enabled rather than after the first gap.",
      "code": "S16-A3",
      "created_at": "2026-11-28T14:56:05.431588+00:00",
      "id": "01a222a8-013b-726c-bcaa-b162576f318f",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Ninety days is the horizon the incident review needs",
      "updated_at": "2026-11-28T14:56:05.431588+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The audit grades findings and blocks only the high ones, with an explicit, logged acknowledgement as the override. The assumption is that medium findings are advice; if a medium class proves to be a real defect, the level moves and the publish path changes with it.",
      "code": "S16-A4",
      "created_at": "2026-11-28T14:56:05.583971+00:00",
      "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A high finding is the only severity that should block a publish",
      "updated_at": "2026-11-28T14:56:05.583971+00:00",
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
      "body": "The project row records the dependency edges. S02 and S03 supply the schema and the card model, S04 the action registry and the transport, S10 the conversation turn the audit joins to, and S11 the procedure and step tables the publish path reads.",
      "code": "S16-D1",
      "created_at": "2026-11-28T14:56:05.731659+00:00",
      "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S16 depends on S02, S03, S04, S10 and S11",
      "updated_at": "2026-11-28T14:56:05.731659+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The chat action is S13's; until it declares the capture flag the ledger records the calls that exist but not the surface that will make most of them. The dependency is a declaration, not a build, so it does not block this pack.",
      "code": "S16-D2",
      "created_at": "2026-11-28T14:56:05.872574+00:00",
      "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S13 declares captures_usage when it lands",
      "updated_at": "2026-11-28T14:56:05.872574+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The two read surfaces and the two schedule rows are other projects' deliverables by the slice plan's own boundary. Recording them here stops a later session reading their absence as an S16 gap.",
      "code": "S16-D3",
      "created_at": "2026-11-28T14:56:06.012582+00:00",
      "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S12 builds both screens and S17 carries the prune schedules",
      "updated_at": "2026-11-28T14:56:06.012582+00:00",
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
      "body": "Before the ledger, a run's spend was inferred from a billing rollup; after it, each call carries its own token counts and the sum is a read. The benefit is declared at the capability level and is observable in the report, not in a saving that a retrospective pack could claim to have measured.",
      "code": "S16-B1",
      "created_at": "2026-11-28T14:56:06.149623+00:00",
      "id": "01a222a8-0409-7017-a365-ba86b7c60129",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S16-inference-audit.md#1"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Cost becomes a query rather than a guess",
      "updated_at": "2026-11-28T14:56:06.149623+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A hit, a write and a miss spend differently, and the request does not say which happened. Recording the response's class turns cache efficiency from an assumption into a column, with `unknown` kept separate so a provider that omits the class cannot inflate the hit rate.",
      "code": "S16-B2",
      "created_at": "2026-11-28T14:56:06.286692+00:00",
      "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S16-inference-audit.md#9"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Cache behaviour is recorded in three states plus unknown",
      "updated_at": "2026-11-28T14:56:06.286692+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The audit runs at publish rather than on a cadence, so a prompt that would fail the check never reaches a tenant without a logged acknowledgement. The benefit is preventive: it is measured by the refusals the publish path records, not by a finding count.",
      "code": "S16-B3",
      "created_at": "2026-11-28T14:56:06.425256+00:00",
      "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S16-inference-audit.md#9"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A high prompt-audit finding blocks the publish",
      "updated_at": "2026-11-28T14:56:06.425256+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Every external call is recorded by the same code path, so the ledger cannot be half-adopted: a new action that calls a model is captured or it does not call. The benefit is structural and is proved by the registry check that no prompt action holds a process write.",
      "code": "S16-B4",
      "created_at": "2026-11-28T14:56:06.580232+00:00",
      "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S16-inference-audit.md#7"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "One engine seam writes every call",
      "updated_at": "2026-11-28T14:56:06.580232+00:00",
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
      "baseline_ref": "01a222a8-0653-7520-a710-ad7f75d13e25",
      "body": "The retrospective MSP/TOGAF/SAFe rework changed this project's record: its artefacts were re-versioned with documents and links, its register was written, and its schedule and cost baselines were taken. This change records that rework against the frozen scope baseline.",
      "code": "CH-S16-5ed8d6",
      "created_at": "2026-11-28T14:56:19.887058+00:00",
      "id": "01a222a8-39b4-7c91-9ca8-8d5ead5ed8d6",
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
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "The retrospective rework of S16",
      "updated_at": "2026-11-28T14:56:20.256602+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```

# Project Registers — Capability, knowledge and memory

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The consolidation store prunes document versions, and a document ingested by two cards is one document by content. A prune driven by one card's source can therefore remove a version another card's run cited. The card-id header on ingested documents is the defence: it makes the ingest attributable, so the prune can be scoped to the sources this card actually introduced.",
      "code": "S15-R1",
      "created_at": "2026-11-28T14:55:12.378914+00:00",
      "id": "01a222a7-31fd-7e85-970e-59537fed5f01",
      "impact": 4,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S15-capability-knowledge-memory.md#8"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": "Keep the card-id header on every ingest and refuse a consolidation change that prunes by content without it.",
      "review_at": "2026-12-12",
      "severity": "high",
      "status": "open",
      "title": "Cross-source dedupe on prune can delete a document two cards share",
      "updated_at": "2026-11-28T14:55:12.378914+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The slice plan's build record closes steps 1\u20134 and leaves step 5's seven actions open. The model, the tables and the seed exist; the action surface that exposes them does not yet. A pack that claimed completion would be describing a product the tree does not hold, so the risk is recorded at the level the plan states.",
      "code": "S15-R2",
      "created_at": "2026-11-28T14:55:12.526021+00:00",
      "id": "01a222a7-3291-73f3-82f0-1684422ef2dd",
      "impact": 4,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S15-capability-knowledge-memory.md#11"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "high",
      "programme_id": null,
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": "Drive step 5 to completion on the slice's own lane (#9) and re-read the receipt before any tranche review treats S15 as green on its live scenarios.",
      "review_at": "2026-12-12",
      "severity": "high",
      "status": "open",
      "title": "Seven actions of step 5 are still open, so the surface the gate names is partly unbuilt",
      "updated_at": "2026-11-28T14:55:12.526021+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Two properties the gate depends on are not demonstrated: that a retrieval is bound to a pinned pack rather than to whatever the store currently holds, and that `memory.remember` joins the run's own transaction. Both are named as unproved in the build record, and an unproved property in a gate is a hypothesis.",
      "code": "S15-R3",
      "created_at": "2026-11-28T14:55:12.668459+00:00",
      "id": "01a222a7-331f-7c58-a06f-d242b168041f",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S15-capability-knowledge-memory.md#7"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": "Prove both on the live strata path, which the `strata_live` suite carries as an ignored test, and record the output before the tranche review.",
      "review_at": "2026-12-12",
      "severity": "medium",
      "status": "open",
      "title": "Pack-bound retrieval and the run-transactional memory write are unproved",
      "updated_at": "2026-11-28T14:55:12.668459+00:00",
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
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "At the 2026-11-28 snapshot the project row is active and its model is in the tree, but the snapshot carries no charter asset, no approved baseline, no delivery contract, no Phase A\u2013D set and no compliance review. The gap is a record gap rather than a product gap; this pack supplies the set.",
      "code": "S15-I1",
      "created_at": "2026-11-28T14:55:12.807263+00:00",
      "id": "01a222a7-33aa-73b8-93b7-c4c9939b3ab8",
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
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": "Supplied by this pack and proved by the project's receipt at the pinned snapshot.",
      "review_at": null,
      "severity": "P1",
      "status": "open",
      "title": "The record carried no charter, baseline, contract or Phase-G review",
      "updated_at": "2026-11-28T14:55:12.807263+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The project declares outputs it had not published, which is why the audit reports an unpublished declared output against S15 and not against its two siblings. The pack authors and publishes the declared outputs; the finding clears on the receipt rather than on an allowlist edit.",
      "code": "S15-I2",
      "created_at": "2026-11-28T14:55:12.949773+00:00",
      "id": "01a222a7-3438-7c8e-98b9-31015859d742",
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
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": "Author and publish the declared outputs in this pack and read the finding off the receipt.",
      "review_at": null,
      "severity": "P1",
      "status": "open",
      "title": "A declared output was unpublished, so the pack surface and the record disagreed",
      "updated_at": "2026-11-28T14:55:12.949773+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Step 4's check excluded `capability.rs` by name, which is the very file the step creates, so the exclusion removed the evidence rather than the noise. The build record names it; the corrected check is what the manifest runs.",
      "code": "S15-I3",
      "created_at": "2026-11-28T14:55:13.093004+00:00",
      "id": "01a222a7-34c8-71a3-9d16-8b80b59e8bc0",
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
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": "Record the exclusion's correction in the evidence output so a later session does not restore the wider pattern.",
      "review_at": null,
      "severity": "P2",
      "status": "open",
      "title": "The build record's grep exclusion was wrong and hid capability files",
      "updated_at": "2026-11-28T14:55:13.093004+00:00",
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
      "body": "Every judgement in this pack is read from the pinned snapshot, and the content checker refuses any identifier that appears nowhere in it. A row that changes when the slice lane merges is out of scope until the pack is re-preflighted.",
      "code": "S15-A1",
      "created_at": "2026-11-28T14:55:13.24653+00:00",
      "id": "01a222a7-3561-7817-a37a-2029b644b9f7",
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
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The 2026-11-28 rework snapshot is the authoritative record for S15",
      "updated_at": "2026-11-28T14:55:13.24653+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The model assumes one capability pack per tenant with immutable versions, so a run's binding names a version rather than a mutable row. The assumption is what makes the cited passage reproducible after the capability is edited.",
      "code": "S15-A2",
      "created_at": "2026-11-28T14:55:13.390464+00:00",
      "id": "01a222a7-35f1-7c07-a17d-b64461249755",
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
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A capability is tenant-scoped and versioned immutably",
      "updated_at": "2026-11-28T14:55:13.390464+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The conveyor writes the binding and strata answers the query. If a second store ever appeared, the pinned-pack guarantee would be per-store and the requirement would need restating rather than extending.",
      "code": "S15-A3",
      "created_at": "2026-11-28T14:55:13.533667+00:00",
      "id": "01a222a7-3680-79a1-99d3-853bb224190d",
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
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Strata is the only retrieval store and the conveyor owns only the binding",
      "updated_at": "2026-11-28T14:55:13.533667+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Consolidation derives digests from the hot rows, so a failed consolidation loses a digest rather than the memory itself. The assumption is that a rebuild is always possible, which is why the hot rows are never pruned by consolidation.",
      "code": "S15-A4",
      "created_at": "2026-11-28T14:55:13.670209+00:00",
      "id": "01a222a7-3709-74d5-b676-b1144b76b191",
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
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Hot rows are the record and the digests are derived",
      "updated_at": "2026-11-28T14:55:13.670209+00:00",
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
      "body": "S13 supplies `agent.run`, which is the loop a capability merely configures; S10 the copilot surface that reads it; S09 the strata ingest and retrieval actions; and S11 the start seam whose `ensure_start_ready` is where readiness is decided.",
      "code": "S15-D1",
      "created_at": "2026-11-28T14:55:13.809629+00:00",
      "id": "01a222a7-3794-79d5-90be-5bf4ca73a4b6",
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
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S15 depends on S13, S10, S09 and S11",
      "updated_at": "2026-11-28T14:55:13.809629+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A capability's prompt reference is audited by S16's procedure, so the binding's provenance depends on the ledger and the audit existing. S16 is in this tranche, which is why the two packs are authored together rather than a tranche apart.",
      "code": "S15-D2",
      "created_at": "2026-11-28T14:55:13.948298+00:00",
      "id": "01a222a7-381f-731c-ae3c-8997724b9ac5",
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
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S16 supplies the prompt-audit provenance the binding cites",
      "updated_at": "2026-11-28T14:55:13.948298+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The parity suite takes its lease from S19-B, and S20's reserved migration number moves to 0028 because this project's 0020 and 0027 land first. Both are ordering facts rather than build dependencies.",
      "code": "S15-D3",
      "created_at": "2026-11-28T14:55:14.107817+00:00",
      "id": "01a222a7-38be-7c2d-942f-ddc75a8e16b6",
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
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "S19-B's lease and S20's reserved migration number constrain the lane",
      "updated_at": "2026-11-28T14:55:14.107817+00:00",
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
      "body": "Before the aggregate, a capability was a vocabulary row that a run could name but not resolve. After it, the run reads an executor spec and a versioned binding, so what a run is about to do is a fact in the record rather than a convention in code.",
      "code": "S15-B1",
      "created_at": "2026-11-28T14:55:14.247907+00:00",
      "id": "01a222a7-394a-7a33-9988-39e5bd5aa09d",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S15-capability-knowledge-memory.md#1"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A capability is a record rather than a catalogue word",
      "updated_at": "2026-11-28T14:55:14.247907+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "A pre-run readiness check turns a broken capability into a refusal the caller can act on, instead of a half-built run whose error names the wrong cause. The benefit is measured by the two distinct refusals the gate keeps apart.",
      "code": "S15-B2",
      "created_at": "2026-11-28T14:55:14.389172+00:00",
      "id": "01a222a7-39d7-7c55-bd26-018e9b8f317a",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S15-capability-knowledge-memory.md#7"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Readiness fails before the run rather than during it",
      "updated_at": "2026-11-28T14:55:14.389172+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The binding names a pack version and documents are immutable, so the passage a run cited can be re-read after the pack is edited. The benefit is a provenance property, and it is the reason the pack-bound retrieval is recorded as unproved rather than assumed.",
      "code": "S15-B3",
      "created_at": "2026-11-28T14:55:14.526717+00:00",
      "id": "01a222a7-3a61-70ac-bbb7-cdb06629bd7f",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S15-capability-knowledge-memory.md#7"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "A cited passage is reproducible from a pinned pack",
      "updated_at": "2026-11-28T14:55:14.526717+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Hot rows hold the working memory and consolidation derives digests into the retrieval store, so a card's notes are recallable by `source_id` after the card is done. The benefit is what makes a later run on the same work cheaper.",
      "code": "S15-B4",
      "created_at": "2026-11-28T14:55:14.664432+00:00",
      "id": "01a222a7-3aeb-7f2e-845c-135e797b86e9",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/slices/S15-capability-knowledge-memory.md#1"
        ]
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Work-package memory outlives the card",
      "updated_at": "2026-11-28T14:55:14.664432+00:00",
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
      "baseline_ref": "01a222a7-3b8a-7019-b55c-353661b57c53",
      "body": "The retrospective MSP/TOGAF/SAFe rework changed this project's record: its artefacts were re-versioned with documents and links, its register was written, and its schedule and cost baselines were taken. This change records that rework against the frozen scope baseline.",
      "code": "CH-S15-799016",
      "created_at": "2026-11-28T14:55:27.741056+00:00",
      "id": "01a222a7-6e02-7840-b7bc-8f0716799016",
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
      "project_id": "01a1d039-0d2f-7ca0-9d3d-534ca7d673fb",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "The retrospective rework of S15",
      "updated_at": "2026-11-28T14:55:28.113735+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```

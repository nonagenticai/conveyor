# Project Registers — Estate toolkits

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The charter, the delivery contract and the Phase-G review all record that the 2026-11-24 snapshot carries no probe or live-call receipt for any toolkit, no scientific_processor scale-up and scale-back receipt, no zero-use report and no backend-outage unbind run. meta.measured_status says built, but a built status is a record claim, not a gate pass; until the SPEC_V2 \u00a712 T14 gate runs on the live estate through acceptance/programme/T14.toml the four clauses stay open verification and no closure is possible.",
      "code": "T14-R1",
      "created_at": "2026-11-26T21:57:48.615381+00:00",
      "id": "01a219dd-61ca-7176-adf9-57ef0693e4b2",
      "impact": 4,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R1 Probe and live call per published toolkit",
          "R2 scientific_processor scale receipt",
          "R3 Zero-use report",
          "R4 Backend-outage unbind test",
          "acceptance/programme/T14.toml"
        ]
      },
      "meta": {
        "owner": "the programme architect (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "high",
      "programme_id": null,
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": "Run the four gate clauses on the live estate and keep one receipt per clause; keep the project active until every receipt exists rather than inferring a pass from the built status.",
      "review_at": "2026-12-15",
      "severity": "high",
      "status": "open",
      "title": "The four gate clauses have no executed receipt, so the project's built status is unproven",
      "updated_at": "2026-11-26T21:57:48.615381+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "D3/G3 and requirement R2 make the scale discipline a gate clause: scientific_processor is scaled up before its probe and live call and scaled back afterwards, with both edges recorded. The snapshot holds no scale receipt at all, so neither the scale-up nor the return to the prior size is evidenced; an unrecorded scale or a left-up host is a gate failure, not an optimisation.",
      "code": "T14-R2",
      "created_at": "2026-11-26T21:57:48.755559+00:00",
      "id": "01a219dd-6256-7a17-92c6-5947b65035ff",
      "impact": 4,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "R2 scientific_processor scale receipt",
          "acceptance/programme/T14.toml"
        ]
      },
      "meta": {
        "owner": "the scientific compute owners \u2014 scientific_processor and lean4_prover"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": "Wrap the probe and the live call in the scale controller, record the scale-up and scale-back edges, and confirm the host returns to its prior size before counting the clause.",
      "review_at": "2026-12-08",
      "severity": "high",
      "status": "open",
      "title": "scientific_processor could be left scaled up or scaled without a receipt, consuming scarce host compute",
      "updated_at": "2026-11-26T21:57:48.755559+00:00",
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
      "code": "U3-EXC-T14",
      "created_at": "2026-11-21T01:16:57.10209+00:00",
      "id": "01a1fbad-8c5f-7086-b569-28e8f0cf6fac",
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
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": "P0",
      "status": "accepted",
      "title": "Out-of-sequence closure accepted as a retrospective exception (T14)",
      "updated_at": "2026-11-21T01:16:57.10209+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The review findings and the evidence output record that the facts return no artefact, contract, review, baseline or output rows for T14 and no gate receipt, and organisation.json carries no toolkit rows, so publication cannot be counted from the record. The Business Capability Map therefore records governed estate import and publication proof as not evidenced rather than delivered.",
      "code": "T14-I1",
      "created_at": "2026-11-26T21:57:48.899332+00:00",
      "id": "01a219dd-62e6-734c-9e4f-8936ed8890ef",
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
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": "high",
      "status": "open",
      "title": "No probe or live-call receipt exists for any published toolkit at the 2026-11-24 snapshot",
      "updated_at": "2026-11-26T21:57:48.899332+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Issues 558 and 560 are closed without receipts and issue 561 is open at 2026-11-22T08:25:44Z; requirement R8 records the research and Lean wave as outstanding and not delivered. A built status alongside an open import issue is the record's own evidence that the wave is incomplete.",
      "code": "T14-I2",
      "created_at": "2026-11-26T21:57:49.04003+00:00",
      "id": "01a219dd-6373-71a9-aa63-1937b2d39a2c",
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
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": "medium",
      "status": "open",
      "title": "Issue 561 (research and Lean toolkits, first wave) remains open while the record claims the slice built",
      "updated_at": "2026-11-26T21:57:49.04003+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The Phase-G review records that the SPEC_V2 \u00a712 plan fragment is draft and that no standalone T14 slice plan exists, and the project meta carries plan_status: draft; the WBS deliverable is therefore held to a draft fragment. A plan that is not accepted cannot anchor a closure, so this stays open while the project stays active.",
      "code": "T14-I3",
      "created_at": "2026-11-26T21:57:49.186614+00:00",
      "id": "01a219dd-6405-759d-a762-cf1081e4330c",
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
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": "medium",
      "status": "open",
      "title": "The plan fragment docs/SPEC_V2.md#12 is status draft and there is no standalone T14 slice plan",
      "updated_at": "2026-11-26T21:57:49.186614+00:00",
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
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The charter states this retrospective reconstructs the lifecycle record, does not attest contemporaneous intent and records the gap truthfully, and the review is a new verdict written at 2026-11-24 rather than a copy of a prior one. Every judgement in this pack is read from that snapshot, and the content checker refuses any id that appears nowhere in it.",
      "code": "T14-A1",
      "created_at": "2026-11-26T21:57:49.328545+00:00",
      "id": "01a219dd-6493-7b54-af39-8e34bb1cf412",
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
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The 2026-11-24 facts snapshot is the authoritative record for T14",
      "updated_at": "2026-11-26T21:57:49.328545+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The Statement of Architecture Work and the review both reason that the record supports consistent \u2014 the seven A\u2013D artefacts, the Statement of Architecture Work, the delivery contract and the WSJF inputs 10/9/10/8 exist \u2014 and does not support non_conformant because nothing in the record shows a refused gate, the missing receipts being the project's open verification item. The programme has no waiver for T14, so the review relies on that distinction staying true.",
      "code": "T14-A2",
      "created_at": "2026-11-26T21:57:49.470178+00:00",
      "id": "01a219dd-6521-7496-9c02-a66f1eec4925",
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
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "The level consistent stands without a waiver because the record shows no refused gate",
      "updated_at": "2026-11-26T21:57:49.470178+00:00",
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
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "meta.depends_on names T13 and P0, and the project brief carries the blocked-by T13 edge for tool, toolkit, tool_binding and tool_call, the broker tools.call, the effect classes, the credential profiles and the schema-valid-input refusal. T13's compliance review records the tool platform passing at lane head 582e2dc6 against acceptance/programme/T13.toml; T14 adds estate rows and receipts and creates no second registry.",
      "code": "T14-D1",
      "created_at": "2026-11-26T21:57:49.61185+00:00",
      "id": "01a219dd-65af-755f-a766-75e18690dfba",
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
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "satisfied",
      "title": "T13 supplies the tool family, the broker, the effect classes and the credential profiles",
      "updated_at": "2026-11-26T21:57:49.61185+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The charter and the delivery contract name the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 as the authority for this rework, and the accepted P0 register item U3-EXC-T14 01a1fbad-8c5f-7086-b569-28e8f0cf6fac records the earlier close in an unauthorised tranche as a retrospective exception. With no live T14 contract or review before this pack, P0's record is what the rework stands on.",
      "code": "T14-D2",
      "created_at": "2026-11-26T21:57:49.785861+00:00",
      "id": "01a219dd-665c-7aac-9f4e-83b616436b1b",
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
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "satisfied",
      "title": "P0 supplies the standing delegation and the accepted exception under which this retrospective runs",
      "updated_at": "2026-11-26T21:57:49.785861+00:00",
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
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "D1/G1 and requirement R9 make the governed call path the slice's first objective: each live service enters as tool, toolkit and tool_binding rows and is invoked through tools.call, while a direct backend call is not a T14 toolkit. Broker-only execution is the one capability the Business Capability Map records as evidenced, inherited from T13's compliance review at lane head 582e2dc6.",
      "code": "T14-B1",
      "created_at": "2026-11-26T21:57:49.932219+00:00",
      "id": "01a219dd-66ef-7544-b165-f234cd5a34ad",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "baselined",
      "title": "Every estate tool is called through the broker and leaves one tool_call row, so no call escapes the governed path",
      "updated_at": "2026-11-26T21:57:49.932219+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "D2/G2 and requirement R1 make proof the act the gate guards: each published toolkit carries a probe receipt and one live-call receipt, and a draft is never counted as published. The zero-use report and the retirement gate then read the same tool_call rows, so the catalogue's published entries are the ones that demonstrably work.",
      "code": "T14-B2",
      "created_at": "2026-11-26T21:57:50.083196+00:00",
      "id": "01a219dd-6785-7e7d-9796-71f25cf5194d",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "baselined",
      "title": "Publication requires a probe and one live call, so an unproven toolkit cannot be counted as published",
      "updated_at": "2026-11-26T21:57:50.083196+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "D4/G4 and requirements R3 and R7 make unused toolkits visible: a zero-use report over tool_call names the toolkits with no calls and feeds the executable provenance and retirement gate delivered by issue 558, so an unused toolkit is offered for retirement rather than left silently bound and costing money.",
      "code": "T14-B3",
      "created_at": "2026-11-26T21:57:50.227233+00:00",
      "id": "01a219dd-6816-734b-b2fb-f68d0e57423c",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "baselined",
      "title": "An unused toolkit has a recorded exit path through the zero-use report and the provenance and retirement gate",
      "updated_at": "2026-11-26T21:57:50.227233+00:00",
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
      "baseline_ref": "01a219dd-68b3-7467-b150-49c5b261bfb9",
      "body": "The retrospective MSP/TOGAF/SAFe rework changed this project's record: its artefacts were re-versioned with documents and links, its register was written, and its schedule and cost baselines were taken. This change records that rework against the frozen scope baseline.",
      "code": "CH-T14-357957",
      "created_at": "2026-11-26T21:57:59.274953+00:00",
      "id": "01a219dd-8b70-7bc4-adaa-ce2d98357957",
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
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "The retrospective rework of T14",
      "updated_at": "2026-11-26T22:47:25.742661+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```

# Statement Of Architecture Work — Estate toolkits

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a219dd-4701-7342-9adf-f2c47331c4af",
      "approved_at": "2026-11-26T21:57:41.900644+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T14 gate, executed on the live estate through `acceptance/programme/T14.toml`: (a) each published toolkit has a probe and one live call; (b) `scientific_processor` is scaled up for its probe and one live call, then scaled back, with a receipt; (c) a zero-use report exists; (d) a backend outage unbinds new runs while in-flight runs finish. The snapshot carries no executed receipt for any of the four clauses, so this rework records them as the project's open verification item rather than a pass; the level written is consistent, never non_conformant without a waiver, and no waiver exists.",
        "assignment": "Rework the V2 slice T14 (SPEC_V2 \u00a712) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, record the Phase-G compliance review at the level the record supports, publish the expected outputs (`project-brief`, `evidence`) and leave the project active with its gate recorded as open verification rather than closing it.",
        "contract": "No live Statement of Architecture Work and no live delivery contract exist for T14 at the snapshot; this pair is supplied retrospectively by the 2026-11-24 rework and is the contract of record until a Board-approved one supersedes it. It binds the project to the four gate clauses above, to broker-only calls inherited from T13, to draft status for the named draft services, and to a receipt per claim; it does not authorise closure. The standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 is the authority under which the rework acts.",
        "responsibilities": "the conveyor-approver approves the Phase A\u2013D set and the delivery contract; the conveyor-architecture-board governs the standards; the programme architect (conveyor-e2e) owns T14 and this retrospective pack; the T13 tool-platform team owns the tool family, the broker, the effect classes and the credential profiles; the estate service owners grant the live-call window and the outage drill; the SRO is accountable for the tranche outcome.",
        "retrospective": "written by the 2026-11-24 tranche-4 rework; it reconstructs the record and does not attest contemporaneous intent.",
        "scope": "the estate-toolkit import wave on T13's tool platform: the `tool`, `toolkit`, `tool_binding` and `tool_call` rows for the coding fleet, strata, billing and licence reads, webscraper, media_discovery, scientific_processor, lean4_prover, video-compiler, the w2v orchestrator and the ops read tools; `evidence_review`, `discovery` :8807 and the planned AHTAPol services as `draft`; the probe and the one live call per published toolkit; the `scientific_processor` scale-up and scale-back receipt; the zero-use report; and the backend-outage unbind test. Out of scope: the broker itself (T13), tool authoring and BYO (T15), and the UI."
      },
      "created_at": "2026-11-26T21:57:41.6166+00:00",
      "id": "01a219dd-4673-70da-b701-3f43675b4fa6",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T14 Estate toolkits (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:57:41.900644+00:00",
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
      "approval_id": "01a219dd-4701-7342-9adf-f2c47331c4af",
      "approved_at": "2026-11-26T21:57:41.900644+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T14 gate, executed on the live estate through `acceptance/programme/T14.toml`: (a) each published toolkit has a probe and one live call; (b) `scientific_processor` is scaled up for its probe and one live call, then scaled back, with a receipt; (c) a zero-use report exists; (d) a backend outage unbinds new runs while in-flight runs finish. The snapshot carries no executed receipt for any of the four clauses, so this rework records them as the project's open verification item rather than a pass; the level written is consistent, never non_conformant without a waiver, and no waiver exists.",
        "assignment": "Rework the V2 slice T14 (SPEC_V2 \u00a712) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, record the Phase-G compliance review at the level the record supports, publish the expected outputs (`project-brief`, `evidence`) and leave the project active with its gate recorded as open verification rather than closing it.",
        "contract": "No live Statement of Architecture Work and no live delivery contract exist for T14 at the snapshot; this pair is supplied retrospectively by the 2026-11-24 rework and is the contract of record until a Board-approved one supersedes it. It binds the project to the four gate clauses above, to broker-only calls inherited from T13, to draft status for the named draft services, and to a receipt per claim; it does not authorise closure. The standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 is the authority under which the rework acts.",
        "responsibilities": "the conveyor-approver approves the Phase A\u2013D set and the delivery contract; the conveyor-architecture-board governs the standards; the programme architect (conveyor-e2e) owns T14 and this retrospective pack; the T13 tool-platform team owns the tool family, the broker, the effect classes and the credential profiles; the estate service owners grant the live-call window and the outage drill; the SRO is accountable for the tranche outcome.",
        "retrospective": "written by the 2026-11-24 tranche-4 rework; it reconstructs the record and does not attest contemporaneous intent.",
        "scope": "the estate-toolkit import wave on T13's tool platform: the `tool`, `toolkit`, `tool_binding` and `tool_call` rows for the coding fleet, strata, billing and licence reads, webscraper, media_discovery, scientific_processor, lean4_prover, video-compiler, the w2v orchestrator and the ops read tools; `evidence_review`, `discovery` :8807 and the planned AHTAPol services as `draft`; the probe and the one live call per published toolkit; the `scientific_processor` scale-up and scale-back receipt; the zero-use report; and the backend-outage unbind test. Out of scope: the broker itself (T13), tool authoring and BYO (T15), and the UI."
      },
      "created_at": "2026-11-26T21:57:41.6166+00:00",
      "id": "01a219dd-4673-70da-b701-3f43675b4fa6",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T14 Estate toolkits (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:57:41.900644+00:00",
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
      "approval_id": "01a219dd-4701-7342-9adf-f2c47331c4af",
      "approved_at": "2026-11-26T21:57:41.900644+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T14 gate, executed on the live estate through `acceptance/programme/T14.toml`: (a) each published toolkit has a probe and one live call; (b) `scientific_processor` is scaled up for its probe and one live call, then scaled back, with a receipt; (c) a zero-use report exists; (d) a backend outage unbinds new runs while in-flight runs finish. The snapshot carries no executed receipt for any of the four clauses, so this rework records them as the project's open verification item rather than a pass; the level written is consistent, never non_conformant without a waiver, and no waiver exists.",
        "assignment": "Rework the V2 slice T14 (SPEC_V2 \u00a712) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, record the Phase-G compliance review at the level the record supports, publish the expected outputs (`project-brief`, `evidence`) and leave the project active with its gate recorded as open verification rather than closing it.",
        "contract": "No live Statement of Architecture Work and no live delivery contract exist for T14 at the snapshot; this pair is supplied retrospectively by the 2026-11-24 rework and is the contract of record until a Board-approved one supersedes it. It binds the project to the four gate clauses above, to broker-only calls inherited from T13, to draft status for the named draft services, and to a receipt per claim; it does not authorise closure. The standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 is the authority under which the rework acts.",
        "responsibilities": "the conveyor-approver approves the Phase A\u2013D set and the delivery contract; the conveyor-architecture-board governs the standards; the programme architect (conveyor-e2e) owns T14 and this retrospective pack; the T13 tool-platform team owns the tool family, the broker, the effect classes and the credential profiles; the estate service owners grant the live-call window and the outage drill; the SRO is accountable for the tranche outcome.",
        "retrospective": "written by the 2026-11-24 tranche-4 rework; it reconstructs the record and does not attest contemporaneous intent.",
        "scope": "the estate-toolkit import wave on T13's tool platform: the `tool`, `toolkit`, `tool_binding` and `tool_call` rows for the coding fleet, strata, billing and licence reads, webscraper, media_discovery, scientific_processor, lean4_prover, video-compiler, the w2v orchestrator and the ops read tools; `evidence_review`, `discovery` :8807 and the planned AHTAPol services as `draft`; the probe and the one live call per published toolkit; the `scientific_processor` scale-up and scale-back receipt; the zero-use report; and the backend-outage unbind test. Out of scope: the broker itself (T13), tool authoring and BYO (T15), and the UI."
      },
      "created_at": "2026-11-26T21:57:41.6166+00:00",
      "id": "01a219dd-4673-70da-b701-3f43675b4fa6",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T14 Estate toolkits (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:57:41.900644+00:00",
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
      "approval_id": "01a219dd-4701-7342-9adf-f2c47331c4af",
      "approved_at": "2026-11-26T21:57:41.900644+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T14 gate, executed on the live estate through `acceptance/programme/T14.toml`: (a) each published toolkit has a probe and one live call; (b) `scientific_processor` is scaled up for its probe and one live call, then scaled back, with a receipt; (c) a zero-use report exists; (d) a backend outage unbinds new runs while in-flight runs finish. The snapshot carries no executed receipt for any of the four clauses, so this rework records them as the project's open verification item rather than a pass; the level written is consistent, never non_conformant without a waiver, and no waiver exists.",
        "assignment": "Rework the V2 slice T14 (SPEC_V2 \u00a712) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, record the Phase-G compliance review at the level the record supports, publish the expected outputs (`project-brief`, `evidence`) and leave the project active with its gate recorded as open verification rather than closing it.",
        "contract": "No live Statement of Architecture Work and no live delivery contract exist for T14 at the snapshot; this pair is supplied retrospectively by the 2026-11-24 rework and is the contract of record until a Board-approved one supersedes it. It binds the project to the four gate clauses above, to broker-only calls inherited from T13, to draft status for the named draft services, and to a receipt per claim; it does not authorise closure. The standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 is the authority under which the rework acts.",
        "responsibilities": "the conveyor-approver approves the Phase A\u2013D set and the delivery contract; the conveyor-architecture-board governs the standards; the programme architect (conveyor-e2e) owns T14 and this retrospective pack; the T13 tool-platform team owns the tool family, the broker, the effect classes and the credential profiles; the estate service owners grant the live-call window and the outage drill; the SRO is accountable for the tranche outcome.",
        "retrospective": "written by the 2026-11-24 tranche-4 rework; it reconstructs the record and does not attest contemporaneous intent.",
        "scope": "the estate-toolkit import wave on T13's tool platform: the `tool`, `toolkit`, `tool_binding` and `tool_call` rows for the coding fleet, strata, billing and licence reads, webscraper, media_discovery, scientific_processor, lean4_prover, video-compiler, the w2v orchestrator and the ops read tools; `evidence_review`, `discovery` :8807 and the planned AHTAPol services as `draft`; the probe and the one live call per published toolkit; the `scientific_processor` scale-up and scale-back receipt; the zero-use report; and the backend-outage unbind test. Out of scope: the broker itself (T13), tool authoring and BYO (T15), and the UI."
      },
      "created_at": "2026-11-26T21:57:41.6166+00:00",
      "id": "01a219dd-4673-70da-b701-3f43675b4fa6",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T14 Estate toolkits (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:57:41.900644+00:00",
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
      "approval_id": "01a219dd-4701-7342-9adf-f2c47331c4af",
      "approved_at": "2026-11-26T21:57:41.900644+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 \u00a712 T14 gate, executed on the live estate through `acceptance/programme/T14.toml`: (a) each published toolkit has a probe and one live call; (b) `scientific_processor` is scaled up for its probe and one live call, then scaled back, with a receipt; (c) a zero-use report exists; (d) a backend outage unbinds new runs while in-flight runs finish. The snapshot carries no executed receipt for any of the four clauses, so this rework records them as the project's open verification item rather than a pass; the level written is consistent, never non_conformant without a waiver, and no waiver exists.",
        "assignment": "Rework the V2 slice T14 (SPEC_V2 \u00a712) by the reconstituted TOGAF/MSP lifecycle: author and publish the seven mandatory Phase A\u2013D artefacts, the charter and the delivery contract, record the Phase-G compliance review at the level the record supports, publish the expected outputs (`project-brief`, `evidence`) and leave the project active with its gate recorded as open verification rather than closing it.",
        "contract": "No live Statement of Architecture Work and no live delivery contract exist for T14 at the snapshot; this pair is supplied retrospectively by the 2026-11-24 rework and is the contract of record until a Board-approved one supersedes it. It binds the project to the four gate clauses above, to broker-only calls inherited from T13, to draft status for the named draft services, and to a receipt per claim; it does not authorise closure. The standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 is the authority under which the rework acts.",
        "responsibilities": "the conveyor-approver approves the Phase A\u2013D set and the delivery contract; the conveyor-architecture-board governs the standards; the programme architect (conveyor-e2e) owns T14 and this retrospective pack; the T13 tool-platform team owns the tool family, the broker, the effect classes and the credential profiles; the estate service owners grant the live-call window and the outage drill; the SRO is accountable for the tranche outcome.",
        "retrospective": "written by the 2026-11-24 tranche-4 rework; it reconstructs the record and does not attest contemporaneous intent.",
        "scope": "the estate-toolkit import wave on T13's tool platform: the `tool`, `toolkit`, `tool_binding` and `tool_call` rows for the coding fleet, strata, billing and licence reads, webscraper, media_discovery, scientific_processor, lean4_prover, video-compiler, the w2v orchestrator and the ops read tools; `evidence_review`, `discovery` :8807 and the planned AHTAPol services as `draft`; the probe and the one live call per published toolkit; the `scientific_processor` scale-up and scale-back receipt; the zero-use report; and the backend-outage unbind test. Out of scope: the broker itself (T13), tool authoring and BYO (T15), and the UI."
      },
      "created_at": "2026-11-26T21:57:41.6166+00:00",
      "id": "01a219dd-4673-70da-b701-3f43675b4fa6",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d37-7896-adfb-fa7bd2279dfd",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T14 Estate toolkits (retrospective, 2026-11-24)",
      "updated_at": "2026-11-26T21:57:41.900644+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

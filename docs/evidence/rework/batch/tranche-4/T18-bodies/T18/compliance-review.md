# Compliance Review — The real V2 UI

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "contract_id": "01a219de-96f5-743b-803d-2002a4b7c2af",
      "created_at": "2026-11-26T21:59:14.733778+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T18.toml",
        "coverage_record": "docs/evidence/v2-coverage.json (stale; build-v2-coverage.py --check exits 1)",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "e2e_specs": "apps/web/e2e/",
        "manifest": "acceptance/programme/T18.toml",
        "old_review": "docs/evidence/rework/T18.json (verdict red, generated 2026-11-24T21:07:28Z)",
        "route_table": "apps/web/src/app.rs",
        "sha": "df4a511718af8c52f04ab30f7f4a66c5ff4b1b06",
        "snapshot": "/tmp/conv526-w1-snap/data"
      },
      "findings": "This retrospective Phase-G review records the level consistent for T18 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The project has no prior Phase-G review row in the snapshot, so the old verdict is the project-acceptance record docs/evidence/rework/T18.json generated 2026-11-24T21:07:28Z, verdict red: its fifteen-check manifest acceptance/programme/T18.toml failed on the baseline, contract, review, WSJF, demo-credit, product-finding, coverage, artefact-standard and rework-resolution checks and passed only the project-active, waiver, PI-cadence, PI-plan, PI-objective and lessons checks. That red record measures the pre-rework state and is the reason this pack exists; it is not evidence that the delivered screens are wrong. The delivered construction is real: apps/web/src/app.rs registers /portfolio, /portfolio/:programme_id, /programme, /organisation, /procedures, /authoring, /plan, /improve, /operations, /execution, /design/adm and /tools, with /tools and /design/adm rendering ToolsGate and AdmGate; apps/web/e2e carries shell, portfolio, programme, organisation, procedures, authoring, plan, improve, operations, execution and admin specs; and issues #559, #562, #563, #564 and #629 are closed. The remaining limits are explicit: issue #565 (T18-4) and issue #566 (T18-5) are open and issue #630 (T18-3c) is blocked; Tools has no destination spec and the architecture destination has no `adm.spec.ts` (the existing `admin.spec.ts` covers /settings/admin, not the ADM rail); docs/evidence/v2-coverage.json is stale, reporting seven destination rows as exists and four as extend (Architecture, Improve, Operations, Tools) while build-v2-coverage.py --check exits 1; the T12 organisation loop has no spec; and no project baseline, delivery contract, Phase-G review, project-brief, inception-analysis, architecture-design or evidence output was published before this pack. The level is consistent, not compliant or conformant: the evidence supports that the project is active and its construction has advanced, but it does not support the \u00a712 gate, which is why this pack supplies the governance record and closes nothing.",
      "id": "01a219de-b233-7c79-9cf9-5c16a4f05765",
      "level": "consistent",
      "outcome": null,
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "reviewed_at": "2026-11-26T21:59:14.733778+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T21:59:14.733778+00:00",
      "version": 1
    }
  ],
  "source": "architecture.json#/compliance_reviews"
}
```

## Findings

The evidence for each finding, against the contract.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "contract_id": "01a219de-96f5-743b-803d-2002a4b7c2af",
      "created_at": "2026-11-26T21:59:14.733778+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T18.toml",
        "coverage_record": "docs/evidence/v2-coverage.json (stale; build-v2-coverage.py --check exits 1)",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "e2e_specs": "apps/web/e2e/",
        "manifest": "acceptance/programme/T18.toml",
        "old_review": "docs/evidence/rework/T18.json (verdict red, generated 2026-11-24T21:07:28Z)",
        "route_table": "apps/web/src/app.rs",
        "sha": "df4a511718af8c52f04ab30f7f4a66c5ff4b1b06",
        "snapshot": "/tmp/conv526-w1-snap/data"
      },
      "findings": "This retrospective Phase-G review records the level consistent for T18 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The project has no prior Phase-G review row in the snapshot, so the old verdict is the project-acceptance record docs/evidence/rework/T18.json generated 2026-11-24T21:07:28Z, verdict red: its fifteen-check manifest acceptance/programme/T18.toml failed on the baseline, contract, review, WSJF, demo-credit, product-finding, coverage, artefact-standard and rework-resolution checks and passed only the project-active, waiver, PI-cadence, PI-plan, PI-objective and lessons checks. That red record measures the pre-rework state and is the reason this pack exists; it is not evidence that the delivered screens are wrong. The delivered construction is real: apps/web/src/app.rs registers /portfolio, /portfolio/:programme_id, /programme, /organisation, /procedures, /authoring, /plan, /improve, /operations, /execution, /design/adm and /tools, with /tools and /design/adm rendering ToolsGate and AdmGate; apps/web/e2e carries shell, portfolio, programme, organisation, procedures, authoring, plan, improve, operations, execution and admin specs; and issues #559, #562, #563, #564 and #629 are closed. The remaining limits are explicit: issue #565 (T18-4) and issue #566 (T18-5) are open and issue #630 (T18-3c) is blocked; Tools has no destination spec and the architecture destination has no `adm.spec.ts` (the existing `admin.spec.ts` covers /settings/admin, not the ADM rail); docs/evidence/v2-coverage.json is stale, reporting seven destination rows as exists and four as extend (Architecture, Improve, Operations, Tools) while build-v2-coverage.py --check exits 1; the T12 organisation loop has no spec; and no project baseline, delivery contract, Phase-G review, project-brief, inception-analysis, architecture-design or evidence output was published before this pack. The level is consistent, not compliant or conformant: the evidence supports that the project is active and its construction has advanced, but it does not support the \u00a712 gate, which is why this pack supplies the governance record and closes nothing.",
      "id": "01a219de-b233-7c79-9cf9-5c16a4f05765",
      "level": "consistent",
      "outcome": null,
      "project_id": "01a20c80-b734-748a-b299-879ea19be681",
      "reviewed_at": "2026-11-26T21:59:14.733778+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-26T21:59:14.733778+00:00",
      "version": 1
    }
  ],
  "source": "architecture.json#/compliance_reviews"
}
```

## Waivers

Each accepted deviation with its owner, reason, cost, risk and reassessment date.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "kind": "waiver_status",
      "title": "None recorded \u2014 0 waivers for this project",
      "waivers": 0
    }
  ],
  "source": "computed:waiver_status"
}
```

## Decision

The outcome and the conditions that attach to it.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "conditions": "none recorded",
      "decision": "consistent",
      "findings": "This retrospective Phase-G review records the level consistent for T18 under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14. The project has no prior Phase-G review row in the snapshot, so the old verdict is the project-acceptance record docs/evidence/rework/T18.json generated 2026-11-24T21:07:28Z, verdict red: its fifteen-check manifest acceptance/programme/T18.toml failed on the baseline, contract, review, WSJF, demo-credit, product-finding, coverage, artefact-standard and rework-resolution checks and passed only the project-active, waiver, PI-cadence, PI-plan, PI-objective and lessons checks. That red record measures the pre-rework state and is the reason this pack exists; it is not evidence that the delivered screens are wrong. The delivered construction is real: apps/web/src/app.rs registers /portfolio, /portfolio/:programme_id, /programme, /organisation, /procedures, /authoring, /plan, /improve, /operations, /execution, /design/adm and /tools, with /tools and /design/adm rendering ToolsGate and AdmGate; apps/web/e2e carries shell, portfolio, programme, organisation, procedures, authoring, plan, improve, operations, execution and admin specs; and issues #559, #562, #563, #564 and #629 are closed. The remaining limits are explicit: issue #565 (T18-4) and issue #566 (T18-5) are open and issue #630 (T18-3c) is blocked; Tools has no destination spec and the architecture destination has no `adm.spec.ts` (the existing `admin.spec.ts` covers /settings/admin, not the ADM rail); docs/evidence/v2-coverage.json is stale, reporting seven destination rows as exists and four as extend (Architecture, Improve, Operations, Tools) while build-v2-coverage.py --check exits 1; the T12 organisation loop has no spec; and no project baseline, delivery contract, Phase-G review, project-brief, inception-analysis, architecture-design or evidence output was published before this pack. The level is consistent, not compliant or conformant: the evidence supports that the project is active and its construction has advanced, but it does not support the \u00a712 gate, which is why this pack supplies the governance record and closes nothing.",
      "id": "01a219de-b233-7c79-9cf9-5c16a4f05765",
      "kind": "governing_review",
      "level": "consistent",
      "phase_h_outcome": null,
      "reviewed_at": "2026-11-26T21:59:14.733778+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a219de-b233-7c79-9cf9-5c16a4f05765: consistent governs; phase-H outcome not recorded (a phase-G decision is its level)"
    }
  ],
  "source": "computed:governing_review"
}
```

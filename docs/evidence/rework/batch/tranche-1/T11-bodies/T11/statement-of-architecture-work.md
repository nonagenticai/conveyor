# Statement Of Architecture Work — Metrics, metering and maturity

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a1ff8d-b4cd-74f9-9f32-2b9f6cae33cd",
      "approved_at": "2026-11-21T19:20:43.741951+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T11.toml is green against the published snapshot at the re-close: every metric and every metering level recomputes from rows; the reconciliation with billing_service is exact at six decimals or opens an incident; a C0\u2013C5 level moves only when the measured evidence crosses its recorded threshold, naming that threshold version; a gate that never refused is flagged. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T11 (SPEC_V2 \u00a77.5, \u00a710.4, \u00a712; issues #84 and #458) by the reconstituted TOGAF/MSP lifecycle: produce and approve the Phase A\u2013D mandatory artefacts, this Statement of Architecture Work, the delivery contract, the Phase-G compliance review and the closure review; publish the declared outputs; and re-close the project through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The metrics, metering and maturity capability delivered by #84 (commit 183f5111, 2026-11-19) and the live Metering screen delivered by #458 (commit 0a8a6e95, 2026-11-20): migration 0059_metrics.sql, crates/domain/src/metrics.rs, crates/store/src/metrics.rs, crates/actions/src/metrics/mod.rs, the eight metrics actions, the billing.summary reconciliation input, the incident card type, the served-spec lock and the console screen. Charging, eval/benchmark-ledger quality metrics, knowledge metrics without ledgers and per-tool health are out of scope."
      },
      "created_at": "2026-11-21T19:20:34.079029+00:00",
      "id": "01a1ff8d-a0ad-7a91-b026-d9a2575daa3f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T11 Metrics, metering and maturity (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:20:43.741951+00:00",
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
      "approval_id": "01a1ff8d-b4cd-74f9-9f32-2b9f6cae33cd",
      "approved_at": "2026-11-21T19:20:43.741951+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T11.toml is green against the published snapshot at the re-close: every metric and every metering level recomputes from rows; the reconciliation with billing_service is exact at six decimals or opens an incident; a C0\u2013C5 level moves only when the measured evidence crosses its recorded threshold, naming that threshold version; a gate that never refused is flagged. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T11 (SPEC_V2 \u00a77.5, \u00a710.4, \u00a712; issues #84 and #458) by the reconstituted TOGAF/MSP lifecycle: produce and approve the Phase A\u2013D mandatory artefacts, this Statement of Architecture Work, the delivery contract, the Phase-G compliance review and the closure review; publish the declared outputs; and re-close the project through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The metrics, metering and maturity capability delivered by #84 (commit 183f5111, 2026-11-19) and the live Metering screen delivered by #458 (commit 0a8a6e95, 2026-11-20): migration 0059_metrics.sql, crates/domain/src/metrics.rs, crates/store/src/metrics.rs, crates/actions/src/metrics/mod.rs, the eight metrics actions, the billing.summary reconciliation input, the incident card type, the served-spec lock and the console screen. Charging, eval/benchmark-ledger quality metrics, knowledge metrics without ledgers and per-tool health are out of scope."
      },
      "created_at": "2026-11-21T19:20:34.079029+00:00",
      "id": "01a1ff8d-a0ad-7a91-b026-d9a2575daa3f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T11 Metrics, metering and maturity (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:20:43.741951+00:00",
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
      "approval_id": "01a1ff8d-b4cd-74f9-9f32-2b9f6cae33cd",
      "approved_at": "2026-11-21T19:20:43.741951+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T11.toml is green against the published snapshot at the re-close: every metric and every metering level recomputes from rows; the reconciliation with billing_service is exact at six decimals or opens an incident; a C0\u2013C5 level moves only when the measured evidence crosses its recorded threshold, naming that threshold version; a gate that never refused is flagged. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T11 (SPEC_V2 \u00a77.5, \u00a710.4, \u00a712; issues #84 and #458) by the reconstituted TOGAF/MSP lifecycle: produce and approve the Phase A\u2013D mandatory artefacts, this Statement of Architecture Work, the delivery contract, the Phase-G compliance review and the closure review; publish the declared outputs; and re-close the project through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The metrics, metering and maturity capability delivered by #84 (commit 183f5111, 2026-11-19) and the live Metering screen delivered by #458 (commit 0a8a6e95, 2026-11-20): migration 0059_metrics.sql, crates/domain/src/metrics.rs, crates/store/src/metrics.rs, crates/actions/src/metrics/mod.rs, the eight metrics actions, the billing.summary reconciliation input, the incident card type, the served-spec lock and the console screen. Charging, eval/benchmark-ledger quality metrics, knowledge metrics without ledgers and per-tool health are out of scope."
      },
      "created_at": "2026-11-21T19:20:34.079029+00:00",
      "id": "01a1ff8d-a0ad-7a91-b026-d9a2575daa3f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T11 Metrics, metering and maturity (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:20:43.741951+00:00",
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
      "approval_id": "01a1ff8d-b4cd-74f9-9f32-2b9f6cae33cd",
      "approved_at": "2026-11-21T19:20:43.741951+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T11.toml is green against the published snapshot at the re-close: every metric and every metering level recomputes from rows; the reconciliation with billing_service is exact at six decimals or opens an incident; a C0\u2013C5 level moves only when the measured evidence crosses its recorded threshold, naming that threshold version; a gate that never refused is flagged. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T11 (SPEC_V2 \u00a77.5, \u00a710.4, \u00a712; issues #84 and #458) by the reconstituted TOGAF/MSP lifecycle: produce and approve the Phase A\u2013D mandatory artefacts, this Statement of Architecture Work, the delivery contract, the Phase-G compliance review and the closure review; publish the declared outputs; and re-close the project through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The metrics, metering and maturity capability delivered by #84 (commit 183f5111, 2026-11-19) and the live Metering screen delivered by #458 (commit 0a8a6e95, 2026-11-20): migration 0059_metrics.sql, crates/domain/src/metrics.rs, crates/store/src/metrics.rs, crates/actions/src/metrics/mod.rs, the eight metrics actions, the billing.summary reconciliation input, the incident card type, the served-spec lock and the console screen. Charging, eval/benchmark-ledger quality metrics, knowledge metrics without ledgers and per-tool health are out of scope."
      },
      "created_at": "2026-11-21T19:20:34.079029+00:00",
      "id": "01a1ff8d-a0ad-7a91-b026-d9a2575daa3f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T11 Metrics, metering and maturity (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:20:43.741951+00:00",
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
      "approval_id": "01a1ff8d-b4cd-74f9-9f32-2b9f6cae33cd",
      "approved_at": "2026-11-21T19:20:43.741951+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "acceptance/programme/T11.toml is green against the published snapshot at the re-close: every metric and every metering level recomputes from rows; the reconciliation with billing_service is exact at six decimals or opens an incident; a C0\u2013C5 level moves only when the measured evidence crosses its recorded threshold, naming that threshold version; a gate that never refused is flagged. The A\u2013D set is approved, the delivery contract is live and board-approved, the Phase-G review cites the published standing delegation, and the closure gate is decided by an approver other than its requester.",
        "assignment": "Rework the V2 slice T11 (SPEC_V2 \u00a77.5, \u00a710.4, \u00a712; issues #84 and #458) by the reconstituted TOGAF/MSP lifecycle: produce and approve the Phase A\u2013D mandatory artefacts, this Statement of Architecture Work, the delivery contract, the Phase-G compliance review and the closure review; publish the declared outputs; and re-close the project through the one gate.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The metrics, metering and maturity capability delivered by #84 (commit 183f5111, 2026-11-19) and the live Metering screen delivered by #458 (commit 0a8a6e95, 2026-11-20): migration 0059_metrics.sql, crates/domain/src/metrics.rs, crates/store/src/metrics.rs, crates/actions/src/metrics/mod.rs, the eight metrics actions, the billing.summary reconciliation input, the incident card type, the served-spec lock and the console screen. Charging, eval/benchmark-ledger quality metrics, knowledge metrics without ledgers and per-tool health are out of scope."
      },
      "created_at": "2026-11-21T19:20:34.079029+00:00",
      "id": "01a1ff8d-a0ad-7a91-b026-d9a2575daa3f",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d36-782b-8b18-ff07a64dedbe",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T11 Metrics, metering and maturity (retrospective, 2026-11-21)",
      "updated_at": "2026-11-21T19:20:43.741951+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

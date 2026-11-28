# Project Baseline — Inference ledger and prompt audit

## Scope

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-baseline",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:06.729867+00:00",
      "id": "01a222a8-0653-7520-a710-ad7f75d13e25",
      "kind": "scope",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:06.729867+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

```json
{
  "category": "project-baseline",
  "count": 3,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:06.729867+00:00",
      "id": "01a222a8-0653-7520-a710-ad7f75d13e25",
      "kind": "scope",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:06.729867+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:06.883946+00:00",
      "id": "01a222a8-06ec-701a-8380-ce10a4bf6481",
      "kind": "schedule",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:06.883946+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:07.19389+00:00",
      "id": "01a222a8-0821-7f9e-8718-f236fc124474",
      "kind": "cost",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:07.19389+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

## Schedule

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-baseline",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:06.883946+00:00",
      "id": "01a222a8-06ec-701a-8380-ce10a4bf6481",
      "kind": "schedule",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:06.883946+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

```json
{
  "category": "project-baseline",
  "count": 3,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:06.729867+00:00",
      "id": "01a222a8-0653-7520-a710-ad7f75d13e25",
      "kind": "scope",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:06.729867+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:06.883946+00:00",
      "id": "01a222a8-06ec-701a-8380-ce10a4bf6481",
      "kind": "schedule",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:06.883946+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:07.19389+00:00",
      "id": "01a222a8-0821-7f9e-8718-f236fc124474",
      "kind": "cost",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:07.19389+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

## Cost

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-baseline",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:07.19389+00:00",
      "id": "01a222a8-0821-7f9e-8718-f236fc124474",
      "kind": "cost",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:07.19389+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

```json
{
  "category": "project-baseline",
  "count": 3,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:06.729867+00:00",
      "id": "01a222a8-0653-7520-a710-ad7f75d13e25",
      "kind": "scope",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:06.729867+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:06.883946+00:00",
      "id": "01a222a8-06ec-701a-8380-ce10a4bf6481",
      "kind": "schedule",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:06.883946+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:07.19389+00:00",
      "id": "01a222a8-0821-7f9e-8718-f236fc124474",
      "kind": "cost",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:07.19389+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

## Approval

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-baseline",
  "count": 3,
  "rows": [
    {
      "card_id": null,
      "created_at": "2026-11-28T14:56:06.729867+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a222a8-0655-779a-94f1-2c1b455984a6",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a222a8-0653-7520-a710-ad7f75d13e25",
        "description": "Approve the scope baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T14:56:19.563291+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T14:56:06.883946+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a222a8-06ee-7ace-8e72-bcfeb499ca3d",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a222a8-06ec-701a-8380-ce10a4bf6481",
        "description": "Approve the schedule baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T14:56:07.030406+00:00",
      "version": 2
    },
    {
      "card_id": null,
      "created_at": "2026-11-28T14:56:07.19389+00:00",
      "expires_at": null,
      "gate": "baseline",
      "id": "01a222a8-0827-7658-90f9-8e32ed45aaba",
      "kind": "pm",
      "meta": {
        "approver_kind": "human",
        "baseline_id": "01a222a8-0821-7f9e-8718-f236fc124474",
        "description": "Approve the cost baseline",
        "gate_policy": {
          "floor": 1,
          "source": "default"
        }
      },
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "required_decisions": 1,
      "run_id": null,
      "status": "approved",
      "updated_at": "2026-11-28T14:56:07.33995+00:00",
      "version": 2
    }
  ],
  "source": "work.json#/approvals"
}
```

```json
{
  "category": "project-baseline",
  "count": 3,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:06.729867+00:00",
      "id": "01a222a8-0653-7520-a710-ad7f75d13e25",
      "kind": "scope",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:06.729867+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:06.883946+00:00",
      "id": "01a222a8-06ec-701a-8380-ce10a4bf6481",
      "kind": "schedule",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:06.883946+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    },
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "approved_by_name": "conveyor-approver",
      "created_at": "2026-11-28T14:56:07.19389+00:00",
      "id": "01a222a8-0821-7f9e-8718-f236fc124474",
      "kind": "cost",
      "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
      "snapshot": {
        "frozen": true,
        "project_id": "01a1d039-0d30-72a1-9bba-29fadf6170cf",
        "register": [
          {
            "code": "S16-B4",
            "id": "01a222a8-05b7-7bd2-bd56-22e8c7c24ad0",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "One engine seam writes every call"
          },
          {
            "code": "S16-B3",
            "id": "01a222a8-051c-79bd-9f31-3ace87713f6c",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "A high prompt-audit finding blocks the publish"
          },
          {
            "code": "S16-B2",
            "id": "01a222a8-0491-7aed-9f04-5c7a6304b635",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cache behaviour is recorded in three states plus unknown"
          },
          {
            "code": "S16-B1",
            "id": "01a222a8-0409-7017-a365-ba86b7c60129",
            "impact": null,
            "kind": "benefit",
            "severity": null,
            "status": "open",
            "title": "Cost becomes a query rather than a guess"
          },
          {
            "code": "S16-D3",
            "id": "01a222a8-037f-7b83-8508-75c44b1c5663",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S12 builds both screens and S17 carries the prune schedules"
          },
          {
            "code": "S16-D2",
            "id": "01a222a8-02f3-7bd5-9ef5-8b9bd9f2df45",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S13 declares captures_usage when it lands"
          },
          {
            "code": "S16-D1",
            "id": "01a222a8-0266-786f-81ea-7736e0d20b7f",
            "impact": null,
            "kind": "dependency",
            "severity": null,
            "status": "open",
            "title": "S16 depends on S02, S03, S04, S10 and S11"
          },
          {
            "code": "S16-A4",
            "id": "01a222a8-01d3-700e-a6ef-43b1c0bd49d4",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "A high finding is the only severity that should block a publish"
          },
          {
            "code": "S16-A3",
            "id": "01a222a8-013b-726c-bcaa-b162576f318f",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "Ninety days is the horizon the incident review needs"
          },
          {
            "code": "S16-A2",
            "id": "01a222a8-00a8-7e2a-965b-ed15d5d8c767",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The router's usage object is the single source of token truth"
          },
          {
            "code": "S16-A1",
            "id": "01a222a8-0018-7780-904d-a8f750abec69",
            "impact": null,
            "kind": "assumption",
            "severity": null,
            "status": "open",
            "title": "The 2026-11-28 rework snapshot is the authoritative record for S16"
          },
          {
            "code": "S16-I3",
            "id": "01a222a7-ff80-7859-b6d7-70d05d0c3372",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "A provider that omits the cache class records unknown rather than a guess"
          },
          {
            "code": "S16-I2",
            "id": "01a222a7-feec-7963-937f-34fa00815231",
            "impact": null,
            "kind": "issue",
            "severity": "P2",
            "status": "open",
            "title": "Retention is declared at ninety days but its prune schedules are S17's"
          },
          {
            "code": "S16-I1",
            "id": "01a222a7-fe5d-76a9-a85c-a33a1a57e297",
            "impact": null,
            "kind": "issue",
            "severity": "P1",
            "status": "open",
            "title": "The record claimed built while carrying no charter, baseline, contract or Phase-G review"
          },
          {
            "code": "S16-R3",
            "id": "01a222a7-fdca-7606-98c0-3ddca78900ef",
            "impact": 2,
            "kind": "risk",
            "severity": "low",
            "status": "open",
            "title": "Join keys carry no foreign keys, so an orphaned row is possible and only the report catches it"
          },
          {
            "code": "S16-R2",
            "id": "01a222a7-fd32-7d8c-8210-020fdb25865c",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "Capture shares the call's transaction, so a database failure can roll back the ledger row"
          },
          {
            "code": "S16-R1",
            "id": "01a222a7-fc8f-74b3-a2e5-ddc3361918f1",
            "impact": 3,
            "kind": "risk",
            "severity": "medium",
            "status": "open",
            "title": "The live-router rehearsal is pending, so the field-for-field claim is proved on a stub"
          },
          {
            "code": "U3-EXC-S16",
            "id": "01a1fbae-890b-76fc-b665-8ad8d56ced7b",
            "impact": null,
            "kind": "issue",
            "severity": "P0",
            "status": "accepted",
            "title": "Out-of-sequence closure accepted as a retrospective exception (S16)"
          }
        ],
        "wbs": [
          {
            "business_value": 9,
            "id": "01a222a8-064f-7641-a92e-8b56891851bd",
            "job_size": 8,
            "kind": "deliverable",
            "parent_id": null,
            "position": 1.0,
            "risk_reduction": 9,
            "status": "open",
            "time_criticality": 7,
            "title": "A queryable cost record \u2014 one ledger row per model call carrying the prompt's identity, the token counts and both cache classes, with a blocking prompt audit"
          }
        ]
      },
      "taken_at": "2026-11-28T14:56:07.19389+00:00",
      "taken_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "taken_by_name": "conveyor-e2e",
      "version": 1
    }
  ],
  "source": "msp.json#/project_baselines"
}
```

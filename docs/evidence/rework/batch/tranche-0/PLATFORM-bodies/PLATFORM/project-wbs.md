# Project Wbs — CI, runners and platform

## Work packages

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-wbs",
  "count": 5,
  "rows": [
    {
      "acceptance": [
        "the ARC runner pool lists a live runner on hp2",
        "the hosted preflight returns a verdict at a pushed head"
      ],
      "body": "",
      "business_value": 3,
      "card_id": "01a21769-e11c-7df6-be83-a4dedc25c763",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "docs/plans/development-process-speed.md",
        "the preflight run at the rework head"
      ],
      "id": "01a1fdd8-cdf3-7108-aac5-0cec790e988d",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 1,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 2,
      "status": "open",
      "time_criticality": 3,
      "title": "CI and runners available",
      "updated_at": "2026-11-26T10:32:24.745122+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "registry.internal:5000 answers a pull",
        "a hand push from worker3 succeeds with the CA in place",
        "the public window answers through the ingress"
      ],
      "body": "",
      "business_value": 3,
      "card_id": "01a21769-e297-7801-90f7-788212c63c49",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "the registry's CA and pull record",
        "the public window probe"
      ],
      "id": "01a1fdd8-ce5c-70c8-87be-2dd12bf61cb0",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 2,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 2,
      "status": "open",
      "time_criticality": 2,
      "title": "Registry and ingress available",
      "updated_at": "2026-11-26T10:32:25.120029+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "the primary reports pg_is_in_recovery false and the standby true",
        "the nightly dump exists with its offsite relay",
        "one restore drill reads the rows back"
      ],
      "body": "",
      "business_value": 3,
      "card_id": "01a21769-e40d-72c8-9348-507a60229366",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "the database recovery check",
        "the restore drill's receipt"
      ],
      "id": "01a1fdd8-cedd-74c2-bad5-a22aa9918c87",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 3,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 3,
      "status": "open",
      "time_criticality": 3,
      "title": "Postgres primary, backups and restore",
      "updated_at": "2026-11-26T10:32:25.494727+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "the service workflow is consumable unchanged",
        "a consuming project's CI runs on it"
      ],
      "body": "",
      "business_value": 2,
      "card_id": "01a21769-e587-714f-8d81-409d734ebcad",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "the shared workflow revision",
        "a consuming project's run"
      ],
      "id": "01a1fdd8-cf24-737d-a364-b0bdc88eac00",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 4,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 2,
      "status": "open",
      "time_criticality": 2,
      "title": "Shared workflows and archetypes",
      "updated_at": "2026-11-26T10:32:25.869863+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "the manifest's checks pass on a fresh snapshot",
        "the deliverable's evidence list resolves",
        "the standing rule is recorded: the platform does not close"
      ],
      "body": "",
      "business_value": 5,
      "card_id": null,
      "created_at": "2026-11-21T17:25:54.271515+00:00",
      "evidence": [
        "docs/evidence/rework/PLATFORM.json",
        "acceptance/programme/PLATFORM.toml",
        "docs/evidence/rework/tranche-0/acts.md"
      ],
      "id": "01a1ff24-a6c6-7504-a811-0fecfbc460f6",
      "job_size": 6,
      "kind": "deliverable",
      "parent_id": null,
      "position": 5,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 3,
      "status": "accepted",
      "time_criticality": 5,
      "title": "The standing platform record \u2014 charter, baseline and the estate verification",
      "updated_at": "2026-11-26T10:32:26.090677+00:00",
      "version": 3
    }
  ],
  "source": "msp.json#/wbs_nodes"
}
```

## Acceptance

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-wbs",
  "count": 5,
  "rows": [
    {
      "acceptance": [
        "the ARC runner pool lists a live runner on hp2",
        "the hosted preflight returns a verdict at a pushed head"
      ],
      "body": "",
      "business_value": 3,
      "card_id": "01a21769-e11c-7df6-be83-a4dedc25c763",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "docs/plans/development-process-speed.md",
        "the preflight run at the rework head"
      ],
      "id": "01a1fdd8-cdf3-7108-aac5-0cec790e988d",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 1,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 2,
      "status": "open",
      "time_criticality": 3,
      "title": "CI and runners available",
      "updated_at": "2026-11-26T10:32:24.745122+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "registry.internal:5000 answers a pull",
        "a hand push from worker3 succeeds with the CA in place",
        "the public window answers through the ingress"
      ],
      "body": "",
      "business_value": 3,
      "card_id": "01a21769-e297-7801-90f7-788212c63c49",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "the registry's CA and pull record",
        "the public window probe"
      ],
      "id": "01a1fdd8-ce5c-70c8-87be-2dd12bf61cb0",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 2,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 2,
      "status": "open",
      "time_criticality": 2,
      "title": "Registry and ingress available",
      "updated_at": "2026-11-26T10:32:25.120029+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "the primary reports pg_is_in_recovery false and the standby true",
        "the nightly dump exists with its offsite relay",
        "one restore drill reads the rows back"
      ],
      "body": "",
      "business_value": 3,
      "card_id": "01a21769-e40d-72c8-9348-507a60229366",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "the database recovery check",
        "the restore drill's receipt"
      ],
      "id": "01a1fdd8-cedd-74c2-bad5-a22aa9918c87",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 3,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 3,
      "status": "open",
      "time_criticality": 3,
      "title": "Postgres primary, backups and restore",
      "updated_at": "2026-11-26T10:32:25.494727+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "the service workflow is consumable unchanged",
        "a consuming project's CI runs on it"
      ],
      "body": "",
      "business_value": 2,
      "card_id": "01a21769-e587-714f-8d81-409d734ebcad",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "the shared workflow revision",
        "a consuming project's run"
      ],
      "id": "01a1fdd8-cf24-737d-a364-b0bdc88eac00",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 4,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 2,
      "status": "open",
      "time_criticality": 2,
      "title": "Shared workflows and archetypes",
      "updated_at": "2026-11-26T10:32:25.869863+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "the manifest's checks pass on a fresh snapshot",
        "the deliverable's evidence list resolves",
        "the standing rule is recorded: the platform does not close"
      ],
      "body": "",
      "business_value": 5,
      "card_id": null,
      "created_at": "2026-11-21T17:25:54.271515+00:00",
      "evidence": [
        "docs/evidence/rework/PLATFORM.json",
        "acceptance/programme/PLATFORM.toml",
        "docs/evidence/rework/tranche-0/acts.md"
      ],
      "id": "01a1ff24-a6c6-7504-a811-0fecfbc460f6",
      "job_size": 6,
      "kind": "deliverable",
      "parent_id": null,
      "position": 5,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 3,
      "status": "accepted",
      "time_criticality": 5,
      "title": "The standing platform record \u2014 charter, baseline and the estate verification",
      "updated_at": "2026-11-26T10:32:26.090677+00:00",
      "version": 3
    }
  ],
  "source": "msp.json#/wbs_nodes"
}
```

## Status

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-wbs",
  "count": 5,
  "rows": [
    {
      "acceptance": [
        "the ARC runner pool lists a live runner on hp2",
        "the hosted preflight returns a verdict at a pushed head"
      ],
      "body": "",
      "business_value": 3,
      "card_id": "01a21769-e11c-7df6-be83-a4dedc25c763",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "docs/plans/development-process-speed.md",
        "the preflight run at the rework head"
      ],
      "id": "01a1fdd8-cdf3-7108-aac5-0cec790e988d",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 1,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 2,
      "status": "open",
      "time_criticality": 3,
      "title": "CI and runners available",
      "updated_at": "2026-11-26T10:32:24.745122+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "registry.internal:5000 answers a pull",
        "a hand push from worker3 succeeds with the CA in place",
        "the public window answers through the ingress"
      ],
      "body": "",
      "business_value": 3,
      "card_id": "01a21769-e297-7801-90f7-788212c63c49",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "the registry's CA and pull record",
        "the public window probe"
      ],
      "id": "01a1fdd8-ce5c-70c8-87be-2dd12bf61cb0",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 2,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 2,
      "status": "open",
      "time_criticality": 2,
      "title": "Registry and ingress available",
      "updated_at": "2026-11-26T10:32:25.120029+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "the primary reports pg_is_in_recovery false and the standby true",
        "the nightly dump exists with its offsite relay",
        "one restore drill reads the rows back"
      ],
      "body": "",
      "business_value": 3,
      "card_id": "01a21769-e40d-72c8-9348-507a60229366",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "the database recovery check",
        "the restore drill's receipt"
      ],
      "id": "01a1fdd8-cedd-74c2-bad5-a22aa9918c87",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 3,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 3,
      "status": "open",
      "time_criticality": 3,
      "title": "Postgres primary, backups and restore",
      "updated_at": "2026-11-26T10:32:25.494727+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "the service workflow is consumable unchanged",
        "a consuming project's CI runs on it"
      ],
      "body": "",
      "business_value": 2,
      "card_id": "01a21769-e587-714f-8d81-409d734ebcad",
      "created_at": "2026-11-21T11:23:26.290608+00:00",
      "evidence": [
        "the shared workflow revision",
        "a consuming project's run"
      ],
      "id": "01a1fdd8-cf24-737d-a364-b0bdc88eac00",
      "job_size": 1,
      "kind": "work_package",
      "parent_id": null,
      "position": 4,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 2,
      "status": "open",
      "time_criticality": 2,
      "title": "Shared workflows and archetypes",
      "updated_at": "2026-11-26T10:32:25.869863+00:00",
      "version": 2
    },
    {
      "acceptance": [
        "the manifest's checks pass on a fresh snapshot",
        "the deliverable's evidence list resolves",
        "the standing rule is recorded: the platform does not close"
      ],
      "body": "",
      "business_value": 5,
      "card_id": null,
      "created_at": "2026-11-21T17:25:54.271515+00:00",
      "evidence": [
        "docs/evidence/rework/PLATFORM.json",
        "acceptance/programme/PLATFORM.toml",
        "docs/evidence/rework/tranche-0/acts.md"
      ],
      "id": "01a1ff24-a6c6-7504-a811-0fecfbc460f6",
      "job_size": 6,
      "kind": "deliverable",
      "parent_id": null,
      "position": 5,
      "project_id": "01a1d039-0d39-79ce-84b6-1173b94bc67f",
      "risk_reduction": 3,
      "status": "accepted",
      "time_criticality": 5,
      "title": "The standing platform record \u2014 charter, baseline and the estate verification",
      "updated_at": "2026-11-26T10:32:26.090677+00:00",
      "version": 3
    }
  ],
  "source": "msp.json#/wbs_nodes"
}
```

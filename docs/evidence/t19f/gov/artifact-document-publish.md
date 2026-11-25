# T19F — Architecture artifact (the typed TOGAF set) — publication

Action `documents.publish` for document 01a210be-6a72-77d4-ae4f-9ab9fa3c7f2a, If-Match 1. Publication moves the output into the public window; the template check covers the four required headings.


```
== write: documents.publish ==
{
  "body": "# Architecture artefact \u2014 T19F forum bindings\n\nThe typed TOGAF artefact set for T19F: seven artefacts, their links, the published\ndocument and the Statement of Architecture Work they sit under.\n\n## Metadata\n\n| Field | Value |\n|---|---|\n| Project | T19F \u2014 Forum bindings (policy authoring, board sitting, cadence) |\n| Artefact type | architecture-artifact (TOGAF, \u00a78.1) |\n| Version | 1 |\n| Owner | programme-architect |\n| Reviewer | Conveyor Architecture Board |\n| Review date | 2026-11-25 |\n| Phase / plateau | A\u2013G / PSA + HLD + capability |\n| Authority | standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3 (published decision output under P0) |\n\n## Entries\n\n| Artefact | Type | Phase / plateau | Entries | Status |\n|---|---|---|---|---|\n| T19F \u2014 Technology standards (01a210bc-b7e1-7a83-b322-81d74e0a903a) | technology_standards | d/hld | 4 | approved |\n| T19F \u2014 Data entity catalog (01a210bc-92bd-71d7-964f-8d9761e78baa) | data_entity_catalog | c/hld | 6 | approved |\n| T19F \u2014 Business capability map (01a210bc-6b09-7785-ab85-5e72fc64997a) | business_capability_map | b/hld | 3 | approved |\n| T19F \u2014 Solution concept (01a210bc-465a-7d83-b9c6-7c7b089d8b02) | solution_concept | a/psa | 4 | approved |\n| T19F \u2014 Requirements catalog (01a210bc-1f7b-7703-bb2d-a0d8695d12bd) | requirements_catalog | a/psa | 8 | approved |\n| T19F \u2014 Drivers, goals and objectives (01a210bb-f6ea-7d99-b23f-4dc3dace459c) | driver_goal_objective | a/psa | 9 | approved |\n| T19F \u2014 Stakeholder map (01a210bb-cfc9-72ec-85f5-619bf730fff9) | stakeholder_map | a/psa | 6 | approved |\n\nEvery entry is a row in `architecture_artifact_entry`; the seven types are the standard's phase A\u2013D mandatory set for a project at phase G.\n\n## Links\n\n| From | Kind | To |\n|---|---|---|\n| T19F \u2014 Drivers, goals and objectives | derivation | T19F \u2014 Requirements catalog |\n| T19F \u2014 Requirements catalog | derivation | T19F \u2014 Solution concept |\n| T19F \u2014 Solution concept | realization | T19F \u2014 Data entity catalog |\n| T19F \u2014 Solution concept | realization | T19F \u2014 Business capability map |\n| T19F \u2014 Stakeholder map | dependency | T19F \u2014 Business capability map |\n| T19F \u2014 Business capability map | realization | T19F \u2014 Technology standards |\n| T19F \u2014 Data entity catalog | realization | T19F \u2014 Technology standards |\n\nThe edges follow the design's \u00a7Links: goals derive requirements, requirements derive the\nsolution concept, the concept realizes the data entities and the capability map, the\nstakeholder map depends on the capability map, and both the capability map and the data\nentities are realized on the technology standards.\n\n## Document\n\nThis output is the published `architecture-artifact` document for T19F. The artefact set sits under the Statement of Architecture Work `Statement of Architecture Work \u2014 T19F forum bindings (2026-11-25)` (01a210bd-ba25-78c1-a363-3eea5aa71335), approved by the Architecture Board under the standing delegation; the board decision is `01a210bd-cdb2-7f2f-ac9a-c314fb1856af`.\n",
  "card_id": null,
  "category": "architecture-artifact",
  "content_hash": "ac2c43b290fb112b3cd6fbe615f7b4b46f38c8a8f4b9f997bfd40c549bbdca04",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:27:24.159798Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210be-6a72-77d4-ae4f-9ab9fa3c7f2a",
  "kind": "document",
  "meta": {
    "category": "architecture-artifact",
    "content_type": "text/markdown",
    "sha256": "ac2c43b290fb112b3cd6fbe615f7b4b46f38c8a8f4b9f997bfd40c549bbdca04",
    "size_bytes": 3027,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T03:27:29.396734Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 3027,
  "status": "active",
  "title": "T19F \u2014 Architecture artifact (the typed TOGAF set)",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:27:29.396734Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
== read-back: documents.get ==
{
  "body": "# Architecture artefact \u2014 T19F forum bindings\n\nThe typed TOGAF artefact set for T19F: seven artefacts, their links, the published\ndocument and the Statement of Architecture Work they sit under.\n\n## Metadata\n\n| Field | Value |\n|---|---|\n| Project | T19F \u2014 Forum bindings (policy authoring, board sitting, cadence) |\n| Artefact type | architecture-artifact (TOGAF, \u00a78.1) |\n| Version | 1 |\n| Owner | programme-architect |\n| Reviewer | Conveyor Architecture Board |\n| Review date | 2026-11-25 |\n| Phase / plateau | A\u2013G / PSA + HLD + capability |\n| Authority | standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3 (published decision output under P0) |\n\n## Entries\n\n| Artefact | Type | Phase / plateau | Entries | Status |\n|---|---|---|---|---|\n| T19F \u2014 Technology standards (01a210bc-b7e1-7a83-b322-81d74e0a903a) | technology_standards | d/hld | 4 | approved |\n| T19F \u2014 Data entity catalog (01a210bc-92bd-71d7-964f-8d9761e78baa) | data_entity_catalog | c/hld | 6 | approved |\n| T19F \u2014 Business capability map (01a210bc-6b09-7785-ab85-5e72fc64997a) | business_capability_map | b/hld | 3 | approved |\n| T19F \u2014 Solution concept (01a210bc-465a-7d83-b9c6-7c7b089d8b02) | solution_concept | a/psa | 4 | approved |\n| T19F \u2014 Requirements catalog (01a210bc-1f7b-7703-bb2d-a0d8695d12bd) | requirements_catalog | a/psa | 8 | approved |\n| T19F \u2014 Drivers, goals and objectives (01a210bb-f6ea-7d99-b23f-4dc3dace459c) | driver_goal_objective | a/psa | 9 | approved |\n| T19F \u2014 Stakeholder map (01a210bb-cfc9-72ec-85f5-619bf730fff9) | stakeholder_map | a/psa | 6 | approved |\n\nEvery entry is a row in `architecture_artifact_entry`; the seven types are the standard's phase A\u2013D mandatory set for a project at phase G.\n\n## Links\n\n| From | Kind | To |\n|---|---|---|\n| T19F \u2014 Drivers, goals and objectives | derivation | T19F \u2014 Requirements catalog |\n| T19F \u2014 Requirements catalog | derivation | T19F \u2014 Solution concept |\n| T19F \u2014 Solution concept | realization | T19F \u2014 Data entity catalog |\n| T19F \u2014 Solution concept | realization | T19F \u2014 Business capability map |\n| T19F \u2014 Stakeholder map | dependency | T19F \u2014 Business capability map |\n| T19F \u2014 Business capability map | realization | T19F \u2014 Technology standards |\n| T19F \u2014 Data entity catalog | realization | T19F \u2014 Technology standards |\n\nThe edges follow the design's \u00a7Links: goals derive requirements, requirements derive the\nsolution concept, the concept realizes the data entities and the capability map, the\nstakeholder map depends on the capability map, and both the capability map and the data\nentities are realized on the technology standards.\n\n## Document\n\nThis output is the published `architecture-artifact` document for T19F. The artefact set sits under the Statement of Architecture Work `Statement of Architecture Work \u2014 T19F forum bindings (2026-11-25)` (01a210bd-ba25-78c1-a363-3eea5aa71335), approved by the Architecture Board under the standing delegation; the board decision is `01a210bd-cdb2-7f2f-ac9a-c314fb1856af`.\n",
  "card_id": null,
  "category": "architecture-artifact",
  "content_hash": "ac2c43b290fb112b3cd6fbe615f7b4b46f38c8a8f4b9f997bfd40c549bbdca04",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:27:24.159798Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210be-6a72-77d4-ae4f-9ab9fa3c7f2a",
  "kind": "document",
  "meta": {
    "category": "architecture-artifact",
    "content_type": "text/markdown",
    "sha256": "ac2c43b290fb112b3cd6fbe615f7b4b46f38c8a8f4b9f997bfd40c549bbdca04",
    "size_bytes": 3027,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T03:27:29.396734Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 3027,
  "status": "active",
  "title": "T19F \u2014 Architecture artifact (the typed TOGAF set)",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:27:29.396734Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
```

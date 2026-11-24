# T19 architecture artifact — the Document section

Action `documents.update` on the published architecture-artifact output, correcting the Document section to name the contract and the board decision rather than the compose's placeholder. The four required headings are unchanged.

```
== write: documents.update ==
{
  "body": "# Architecture artefact \u2014 T19 decision forums\n\nThe typed TOGAF artefact set for T19: seven artefacts, their links, the published \ndocument and the Statement of Architecture Work they sit under.\n\n## Metadata\n\n| Field | Value |\n|---|---|\n| Project | T19 \u2014 Decision forums (meetings) |\n| Artefact type | architecture-artifact (TOGAF, \u00a78.1) |\n| Version | 1 |\n| Owner | programme-architect |\n| Reviewer | Conveyor Architecture Board |\n| Review date | 2026-11-24 |\n| Phase / plateau | A\u2013D / PSA + HLD |\n| Authority | standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3 (published decision output under P0) |\n\n## Entries\n\n| Artefact | Type | Phase / plateau | Entries | Status |\n|---|---|---|---|---|\n| T19 \u2014 Technology standards (01a20cde-d6bd-7cfe-88ad-42a1e6db30a7) | technology_standards | d/hld | 7 | approved |\n| T19 \u2014 Data entity catalog (01a20cde-adbf-7626-9b2b-58faf7164755) | data_entity_catalog | c/hld | 7 | approved |\n| T19 \u2014 Business capability map (01a20cde-86d7-7257-a45e-b4731ab423a4) | business_capability_map | b/hld | 8 | approved |\n| T19 \u2014 Solution concept (01a20cde-5f86-7720-9a16-36a964ef255d) | solution_concept | a/psa | 11 | approved |\n| T19 \u2014 Requirements catalog (01a20cde-371c-7d85-ae2d-780453958bef) | requirements_catalog | a/psa | 14 | approved |\n| T19 \u2014 Drivers, goals and objectives (01a20cde-0f97-7f74-afd8-7cd74e0990e3) | driver_goal_objective | a/psa | 5 | approved |\n| T19 \u2014 Stakeholder map (01a20cdd-e71f-7d06-8ca5-701ff6b7d69e) | stakeholder_map | a/psa | 9 | approved |\n\nEvery entry is a row in `architecture_artifact_entry`; the seven types are the standard's phase A\u2013D mandatory set for a project at phase G.\n\n## Links\n\n| From | Kind | To |\n|---|---|---|\n| T19 \u2014 Drivers, goals and objectives | derivation | T19 \u2014 Requirements catalog |\n| T19 \u2014 Requirements catalog | derivation | T19 \u2014 Solution concept |\n| T19 \u2014 Solution concept | realization | T19 \u2014 Data entity catalog |\n| T19 \u2014 Solution concept | realization | T19 \u2014 Business capability map |\n| T19 \u2014 Stakeholder map | dependency | T19 \u2014 Business capability map |\n| T19 \u2014 Business capability map | realization | T19 \u2014 Technology standards |\n| T19 \u2014 Data entity catalog | realization | T19 \u2014 Technology standards |\n\nThe edges follow the design's \u00a75: goals derive requirements, requirements derive the \nsolution concept, the concept realizes the data entities and the capability map, the \nstakeholder map depends on the capability map, and both the capability map and the data \nentities are realized on the technology standards.\n\n## Document\n\nThis output is the published `architecture-artifact` document for T19. The artefact set sits under the Statement of Architecture Work `Statement of Architecture Work \u2014 T19 decision forums (2026-11-24)` (01a20ce0-5183-776c-84bd-327f2301b19b), approved by the Architecture Board under the standing delegation; the board decision is `01a20ce0-644d-7340-a696-1af10fa8c61e`.\n",
  "card_id": null,
  "category": "architecture-artifact",
  "content_hash": "19cb156ace5e519fbea53c4296a01e39e47a2554b31ec081416e6de554c73830",
  "content_type": "text/markdown",
  "created_at": "2026-11-24T09:30:28.081869Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a20ce4-73e4-7009-9ec7-eb2265e9e84e",
  "kind": "document",
  "meta": {
    "category": "architecture-artifact",
    "content_type": "text/markdown",
    "sha256": "19cb156ace5e519fbea53c4296a01e39e47a2554b31ec081416e6de554c73830",
    "size_bytes": 2961,
    "unit": "T19"
  },
  "parent_document_id": null,
  "project_id": "01a20c80-b9b8-7214-abc6-f2c4f75a1fc1",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-24T09:30:33.176033Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 2961,
  "status": "active",
  "title": "T19 \u2014 Architecture artifact (the typed TOGAF set)",
  "unit": "T19",
  "updated_at": "2026-11-24T09:31:58.766037Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 2
}
== read-back: documents.get ==
{
  "body": "# Architecture artefact \u2014 T19 decision forums\n\nThe typed TOGAF artefact set for T19: seven artefacts, their links, the published \ndocument and the Statement of Architecture Work they sit under.\n\n## Metadata\n\n| Field | Value |\n|---|---|\n| Project | T19 \u2014 Decision forums (meetings) |\n| Artefact type | architecture-artifact (TOGAF, \u00a78.1) |\n| Version | 1 |\n| Owner | programme-architect |\n| Reviewer | Conveyor Architecture Board |\n| Review date | 2026-11-24 |\n| Phase / plateau | A\u2013D / PSA + HLD |\n| Authority | standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3 (published decision output under P0) |\n\n## Entries\n\n| Artefact | Type | Phase / plateau | Entries | Status |\n|---|---|---|---|---|\n| T19 \u2014 Technology standards (01a20cde-d6bd-7cfe-88ad-42a1e6db30a7) | technology_standards | d/hld | 7 | approved |\n| T19 \u2014 Data entity catalog (01a20cde-adbf-7626-9b2b-58faf7164755) | data_entity_catalog | c/hld | 7 | approved |\n| T19 \u2014 Business capability map (01a20cde-86d7-7257-a45e-b4731ab423a4) | business_capability_map | b/hld | 8 | approved |\n| T19 \u2014 Solution concept (01a20cde-5f86-7720-9a16-36a964ef255d) | solution_concept | a/psa | 11 | approved |\n| T19 \u2014 Requirements catalog (01a20cde-371c-7d85-ae2d-780453958bef) | requirements_catalog | a/psa | 14 | approved |\n| T19 \u2014 Drivers, goals and objectives (01a20cde-0f97-7f74-afd8-7cd74e0990e3) | driver_goal_objective | a/psa | 5 | approved |\n| T19 \u2014 Stakeholder map (01a20cdd-e71f-7d06-8ca5-701ff6b7d69e) | stakeholder_map | a/psa | 9 | approved |\n\nEvery entry is a row in `architecture_artifact_entry`; the seven types are the standard's phase A\u2013D mandatory set for a project at phase G.\n\n## Links\n\n| From | Kind | To |\n|---|---|---|\n| T19 \u2014 Drivers, goals and objectives | derivation | T19 \u2014 Requirements catalog |\n| T19 \u2014 Requirements catalog | derivation | T19 \u2014 Solution concept |\n| T19 \u2014 Solution concept | realization | T19 \u2014 Data entity catalog |\n| T19 \u2014 Solution concept | realization | T19 \u2014 Business capability map |\n| T19 \u2014 Stakeholder map | dependency | T19 \u2014 Business capability map |\n| T19 \u2014 Business capability map | realization | T19 \u2014 Technology standards |\n| T19 \u2014 Data entity catalog | realization | T19 \u2014 Technology standards |\n\nThe edges follow the design's \u00a75: goals derive requirements, requirements derive the \nsolution concept, the concept realizes the data entities and the capability map, the \nstakeholder map depends on the capability map, and both the capability map and the data \nentities are realized on the technology standards.\n\n## Document\n\nThis output is the published `architecture-artifact` document for T19. The artefact set sits under the Statement of Architecture Work `Statement of Architecture Work \u2014 T19 decision forums (2026-11-24)` (01a20ce0-5183-776c-84bd-327f2301b19b), approved by the Architecture Board under the standing delegation; the board decision is `01a20ce0-644d-7340-a696-1af10fa8c61e`.\n",
  "card_id": null,
  "category": "architecture-artifact",
  "content_hash": "19cb156ace5e519fbea53c4296a01e39e47a2554b31ec081416e6de554c73830",
  "content_type": "text/markdown",
  "created_at": "2026-11-24T09:30:28.081869Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a20ce4-73e4-7009-9ec7-eb2265e9e84e",
  "kind": "document",
  "meta": {
    "category": "architecture-artifact",
    "content_type": "text/markdown",
    "sha256": "19cb156ace5e519fbea53c4296a01e39e47a2554b31ec081416e6de554c73830",
    "size_bytes": 2961,
    "unit": "T19"
  },
  "parent_document_id": null,
  "project_id": "01a20c80-b9b8-7214-abc6-f2c4f75a1fc1",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-24T09:30:33.176033Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 2961,
  "status": "active",
  "title": "T19 \u2014 Architecture artifact (the typed TOGAF set)",
  "unit": "T19",
  "updated_at": "2026-11-24T09:31:58.766037Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 2
}
```

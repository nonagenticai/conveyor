# T12 evidence — publish

`documents.publish` for `01a2121a-fb41-7110-84df-f0e3499f704a` (admin act, If-Match). The standard's required headings are checked on this transition.

```
== write: documents.publish ==
{
  "body": "---\ndocument: T12/evidence\nversion: 1\nstatus: published\nproject: T12\ncategory: evidence\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: evidence\nphase: G\nplateau: capability\n---\n\n# T12 \u2014 One organisation loop: evidence\n\n## Claim\n\nT12's loop is constructed and proven in the server harness: request \u2192 PM \u2192 delivery jobs \u2192 coach\ninterruption \u2192 kata \u2192 development project \u2192 deploy/live verify \u2192 operations incident \u2192 lesson, with\nevery transition audited and the loop within budget, and one benefit baselined before its\ntransition and read by the acceptance harness. The project's **acceptance is open**: the live half\nof P7 was not exercised (owner ruling 2026-11-24).\n\n## Evidence\n\n- `apps/server/tests/organisation_loop.rs` (12 scenarios), including `request_to_pm`,\n  `development_project_deploy_and_live_verify`, `deploy_failure_to_incident`,\n  `incident_to_promoted_lesson`, `every_loop_transition_audited`, `loop_within_budget` and\n  `architecture_and_benefits_acceptance`.\n- PR #670, merge `64f3d6ec370da089cefa46b4d8b6b80092ac98ad` (head `e65552f0`).\n- The deployed P6 service answered `GET /health` 200 at digest `sha256:84c75723\u2026`, tag\n  `sha-64f3d6e`, 2026-11-24T21:00:07Z.\n\n## Verification\n\n`cargo test -p conveyor-server --test organisation_loop` on a checked-out main; the deployment\nprobe is `curl -fsS https://<deployed-host>/health` at the tag above (recorded in the unit's\nevidence, not repeated here).\n\n## Links\n\n- `docs/plans/inception/T12/00-project-brief.md`, `01-inception-analysis.md`,\n  `02-architecture-design.md`\n- Issue #88 (open); PR #670; `docs/evidence/v2-coverage.md`.\n\n## Gap\n\nThe acceptance is open and this document does not claim it:\n\n- P7's live half is **not exercised** (owner ruling 2026-11-24).\n- The rollback exercise was not run.\n- The budget binding is an open question (tenant, programme or job binding; default tenant with\n  per-binding defaults); the observed refusal is the inference router's `429 budget_exceeded`, not\n  a conveyor-side spend gate.\n- No live benefit delta is claimed; `architecture_and_benefits_acceptance` is a harness test.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "76691699ac3f1cf66eb14252ef8b230de12f1826b7a7e948edb10210b7dd585b",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:48:07.761852Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121a-fb41-7110-84df-f0e3499f704a",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "76691699ac3f1cf66eb14252ef8b230de12f1826b7a7e948edb10210b7dd585b",
    "size_bytes": 2155,
    "unit": "T12"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d36-7de8-891c-4d3dcf110cf5",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T09:48:13.113178Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 2155,
  "status": "active",
  "title": "Evidence \u2014 T12 One organisation loop",
  "unit": "T12",
  "updated_at": "2026-11-25T09:48:13.113178Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
== read-back: documents.get ==
{
  "body": "---\ndocument: T12/evidence\nversion: 1\nstatus: published\nproject: T12\ncategory: evidence\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: evidence\nphase: G\nplateau: capability\n---\n\n# T12 \u2014 One organisation loop: evidence\n\n## Claim\n\nT12's loop is constructed and proven in the server harness: request \u2192 PM \u2192 delivery jobs \u2192 coach\ninterruption \u2192 kata \u2192 development project \u2192 deploy/live verify \u2192 operations incident \u2192 lesson, with\nevery transition audited and the loop within budget, and one benefit baselined before its\ntransition and read by the acceptance harness. The project's **acceptance is open**: the live half\nof P7 was not exercised (owner ruling 2026-11-24).\n\n## Evidence\n\n- `apps/server/tests/organisation_loop.rs` (12 scenarios), including `request_to_pm`,\n  `development_project_deploy_and_live_verify`, `deploy_failure_to_incident`,\n  `incident_to_promoted_lesson`, `every_loop_transition_audited`, `loop_within_budget` and\n  `architecture_and_benefits_acceptance`.\n- PR #670, merge `64f3d6ec370da089cefa46b4d8b6b80092ac98ad` (head `e65552f0`).\n- The deployed P6 service answered `GET /health` 200 at digest `sha256:84c75723\u2026`, tag\n  `sha-64f3d6e`, 2026-11-24T21:00:07Z.\n\n## Verification\n\n`cargo test -p conveyor-server --test organisation_loop` on a checked-out main; the deployment\nprobe is `curl -fsS https://<deployed-host>/health` at the tag above (recorded in the unit's\nevidence, not repeated here).\n\n## Links\n\n- `docs/plans/inception/T12/00-project-brief.md`, `01-inception-analysis.md`,\n  `02-architecture-design.md`\n- Issue #88 (open); PR #670; `docs/evidence/v2-coverage.md`.\n\n## Gap\n\nThe acceptance is open and this document does not claim it:\n\n- P7's live half is **not exercised** (owner ruling 2026-11-24).\n- The rollback exercise was not run.\n- The budget binding is an open question (tenant, programme or job binding; default tenant with\n  per-binding defaults); the observed refusal is the inference router's `429 budget_exceeded`, not\n  a conveyor-side spend gate.\n- No live benefit delta is claimed; `architecture_and_benefits_acceptance` is a harness test.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "76691699ac3f1cf66eb14252ef8b230de12f1826b7a7e948edb10210b7dd585b",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:48:07.761852Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121a-fb41-7110-84df-f0e3499f704a",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "76691699ac3f1cf66eb14252ef8b230de12f1826b7a7e948edb10210b7dd585b",
    "size_bytes": 2155,
    "unit": "T12"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d36-7de8-891c-4d3dcf110cf5",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T09:48:13.113178Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 2155,
  "status": "active",
  "title": "Evidence \u2014 T12 One organisation loop",
  "unit": "T12",
  "updated_at": "2026-11-25T09:48:13.113178Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
```

# Statement Of Architecture Work — Publish the SPEC_V2 studio design pack, live

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a1ff3c-8f0f-7326-b8a7-2de4d2eb6597",
      "approved_at": "2026-11-21T17:52:06.626934+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "Four, each with its check: (1) the public site renders the programme from Postgres \u2014 `data/snapshot.js` carries the programme's rows and the public sidebar names it only when the snapshot does (#470, 7d4e0d27); (2) a published output appears at its stable URL \u2014 165 committed output pages, every one named by `docs/design/docs/outputs.js` (#516, 06ba0fe3); (3) signed out there is no API request and no write control \u2014 `node docs/design/scripts/check-console.cjs`; (4) the pack's claims are checked like code \u2014 `build-coverage.py --check`, `build-docs.py --check` and the surface checks in the `design-pack` job of `.github/workflows/ci-public.yml`. The project's own gate is `acceptance/programme/DESIGN-PUBLISH.toml`.",
        "assignment": "Deliver to the programme owner, the Architecture Board and every outside reader the published studio design pack that makes the SPEC_V2 operating model readable and challengeable from a browser, and keep it live from the application image. This is the Phase A architecture work of a programme-level project in tranche 0, held to `docs/design/STUDIO-SPEC.md`. Recorded retrospectively on 2026-11-21: the pack was built and published by units #32, #33, #36, #37, #89, #97, #103 and #149, all closed.",
        "manifest": "acceptance/programme/DESIGN-PUBLISH.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The Phase A artefact set at plateau psa \u2014 stakeholder map (matrix), driver/goal/objective catalog, requirements catalog and solution concept (diagram) \u2014 with the Phase B\u2013D catalogues: business capability map, data entity catalog and technology standards. The published surfaces: `studio.html` offline, `https://conveyor.nonagentic.ai/design/` from the app image, `/design/data/snapshot.js` read from the tenant's rows, and `/design/docs/output-<id>.html` for every published output. Out of scope: any V2 backend aggregate, any signed-out write path, and the retired homepage mirror."
      },
      "created_at": "2026-11-21T17:51:55.933191+00:00",
      "id": "01a1ff3c-7aad-7dda-820e-1c7d85fe70a8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 DESIGN-PUBLISH: the published studio design pack",
      "updated_at": "2026-11-21T17:52:06.626934+00:00",
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
      "approval_id": "01a1ff3c-8f0f-7326-b8a7-2de4d2eb6597",
      "approved_at": "2026-11-21T17:52:06.626934+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "Four, each with its check: (1) the public site renders the programme from Postgres \u2014 `data/snapshot.js` carries the programme's rows and the public sidebar names it only when the snapshot does (#470, 7d4e0d27); (2) a published output appears at its stable URL \u2014 165 committed output pages, every one named by `docs/design/docs/outputs.js` (#516, 06ba0fe3); (3) signed out there is no API request and no write control \u2014 `node docs/design/scripts/check-console.cjs`; (4) the pack's claims are checked like code \u2014 `build-coverage.py --check`, `build-docs.py --check` and the surface checks in the `design-pack` job of `.github/workflows/ci-public.yml`. The project's own gate is `acceptance/programme/DESIGN-PUBLISH.toml`.",
        "assignment": "Deliver to the programme owner, the Architecture Board and every outside reader the published studio design pack that makes the SPEC_V2 operating model readable and challengeable from a browser, and keep it live from the application image. This is the Phase A architecture work of a programme-level project in tranche 0, held to `docs/design/STUDIO-SPEC.md`. Recorded retrospectively on 2026-11-21: the pack was built and published by units #32, #33, #36, #37, #89, #97, #103 and #149, all closed.",
        "manifest": "acceptance/programme/DESIGN-PUBLISH.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The Phase A artefact set at plateau psa \u2014 stakeholder map (matrix), driver/goal/objective catalog, requirements catalog and solution concept (diagram) \u2014 with the Phase B\u2013D catalogues: business capability map, data entity catalog and technology standards. The published surfaces: `studio.html` offline, `https://conveyor.nonagentic.ai/design/` from the app image, `/design/data/snapshot.js` read from the tenant's rows, and `/design/docs/output-<id>.html` for every published output. Out of scope: any V2 backend aggregate, any signed-out write path, and the retired homepage mirror."
      },
      "created_at": "2026-11-21T17:51:55.933191+00:00",
      "id": "01a1ff3c-7aad-7dda-820e-1c7d85fe70a8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 DESIGN-PUBLISH: the published studio design pack",
      "updated_at": "2026-11-21T17:52:06.626934+00:00",
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
      "approval_id": "01a1ff3c-8f0f-7326-b8a7-2de4d2eb6597",
      "approved_at": "2026-11-21T17:52:06.626934+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "Four, each with its check: (1) the public site renders the programme from Postgres \u2014 `data/snapshot.js` carries the programme's rows and the public sidebar names it only when the snapshot does (#470, 7d4e0d27); (2) a published output appears at its stable URL \u2014 165 committed output pages, every one named by `docs/design/docs/outputs.js` (#516, 06ba0fe3); (3) signed out there is no API request and no write control \u2014 `node docs/design/scripts/check-console.cjs`; (4) the pack's claims are checked like code \u2014 `build-coverage.py --check`, `build-docs.py --check` and the surface checks in the `design-pack` job of `.github/workflows/ci-public.yml`. The project's own gate is `acceptance/programme/DESIGN-PUBLISH.toml`.",
        "assignment": "Deliver to the programme owner, the Architecture Board and every outside reader the published studio design pack that makes the SPEC_V2 operating model readable and challengeable from a browser, and keep it live from the application image. This is the Phase A architecture work of a programme-level project in tranche 0, held to `docs/design/STUDIO-SPEC.md`. Recorded retrospectively on 2026-11-21: the pack was built and published by units #32, #33, #36, #37, #89, #97, #103 and #149, all closed.",
        "manifest": "acceptance/programme/DESIGN-PUBLISH.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The Phase A artefact set at plateau psa \u2014 stakeholder map (matrix), driver/goal/objective catalog, requirements catalog and solution concept (diagram) \u2014 with the Phase B\u2013D catalogues: business capability map, data entity catalog and technology standards. The published surfaces: `studio.html` offline, `https://conveyor.nonagentic.ai/design/` from the app image, `/design/data/snapshot.js` read from the tenant's rows, and `/design/docs/output-<id>.html` for every published output. Out of scope: any V2 backend aggregate, any signed-out write path, and the retired homepage mirror."
      },
      "created_at": "2026-11-21T17:51:55.933191+00:00",
      "id": "01a1ff3c-7aad-7dda-820e-1c7d85fe70a8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 DESIGN-PUBLISH: the published studio design pack",
      "updated_at": "2026-11-21T17:52:06.626934+00:00",
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
      "approval_id": "01a1ff3c-8f0f-7326-b8a7-2de4d2eb6597",
      "approved_at": "2026-11-21T17:52:06.626934+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "Four, each with its check: (1) the public site renders the programme from Postgres \u2014 `data/snapshot.js` carries the programme's rows and the public sidebar names it only when the snapshot does (#470, 7d4e0d27); (2) a published output appears at its stable URL \u2014 165 committed output pages, every one named by `docs/design/docs/outputs.js` (#516, 06ba0fe3); (3) signed out there is no API request and no write control \u2014 `node docs/design/scripts/check-console.cjs`; (4) the pack's claims are checked like code \u2014 `build-coverage.py --check`, `build-docs.py --check` and the surface checks in the `design-pack` job of `.github/workflows/ci-public.yml`. The project's own gate is `acceptance/programme/DESIGN-PUBLISH.toml`.",
        "assignment": "Deliver to the programme owner, the Architecture Board and every outside reader the published studio design pack that makes the SPEC_V2 operating model readable and challengeable from a browser, and keep it live from the application image. This is the Phase A architecture work of a programme-level project in tranche 0, held to `docs/design/STUDIO-SPEC.md`. Recorded retrospectively on 2026-11-21: the pack was built and published by units #32, #33, #36, #37, #89, #97, #103 and #149, all closed.",
        "manifest": "acceptance/programme/DESIGN-PUBLISH.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The Phase A artefact set at plateau psa \u2014 stakeholder map (matrix), driver/goal/objective catalog, requirements catalog and solution concept (diagram) \u2014 with the Phase B\u2013D catalogues: business capability map, data entity catalog and technology standards. The published surfaces: `studio.html` offline, `https://conveyor.nonagentic.ai/design/` from the app image, `/design/data/snapshot.js` read from the tenant's rows, and `/design/docs/output-<id>.html` for every published output. Out of scope: any V2 backend aggregate, any signed-out write path, and the retired homepage mirror."
      },
      "created_at": "2026-11-21T17:51:55.933191+00:00",
      "id": "01a1ff3c-7aad-7dda-820e-1c7d85fe70a8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 DESIGN-PUBLISH: the published studio design pack",
      "updated_at": "2026-11-21T17:52:06.626934+00:00",
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
      "approval_id": "01a1ff3c-8f0f-7326-b8a7-2de4d2eb6597",
      "approved_at": "2026-11-21T17:52:06.626934+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "Four, each with its check: (1) the public site renders the programme from Postgres \u2014 `data/snapshot.js` carries the programme's rows and the public sidebar names it only when the snapshot does (#470, 7d4e0d27); (2) a published output appears at its stable URL \u2014 165 committed output pages, every one named by `docs/design/docs/outputs.js` (#516, 06ba0fe3); (3) signed out there is no API request and no write control \u2014 `node docs/design/scripts/check-console.cjs`; (4) the pack's claims are checked like code \u2014 `build-coverage.py --check`, `build-docs.py --check` and the surface checks in the `design-pack` job of `.github/workflows/ci-public.yml`. The project's own gate is `acceptance/programme/DESIGN-PUBLISH.toml`.",
        "assignment": "Deliver to the programme owner, the Architecture Board and every outside reader the published studio design pack that makes the SPEC_V2 operating model readable and challengeable from a browser, and keep it live from the application image. This is the Phase A architecture work of a programme-level project in tranche 0, held to `docs/design/STUDIO-SPEC.md`. Recorded retrospectively on 2026-11-21: the pack was built and published by units #32, #33, #36, #37, #89, #97, #103 and #149, all closed.",
        "manifest": "acceptance/programme/DESIGN-PUBLISH.toml",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "The Phase A artefact set at plateau psa \u2014 stakeholder map (matrix), driver/goal/objective catalog, requirements catalog and solution concept (diagram) \u2014 with the Phase B\u2013D catalogues: business capability map, data entity catalog and technology standards. The published surfaces: `studio.html` offline, `https://conveyor.nonagentic.ai/design/` from the app image, `/design/data/snapshot.js` read from the tenant's rows, and `/design/docs/output-<id>.html` for every published output. Out of scope: any V2 backend aggregate, any signed-out write path, and the retired homepage mirror."
      },
      "created_at": "2026-11-21T17:51:55.933191+00:00",
      "id": "01a1ff3c-7aad-7dda-820e-1c7d85fe70a8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d39-7414-b43c-450de170ffe1",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 DESIGN-PUBLISH: the published studio design pack",
      "updated_at": "2026-11-21T17:52:06.626934+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

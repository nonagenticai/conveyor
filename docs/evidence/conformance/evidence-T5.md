# T5 evidence — the published output

`documents.compose` of `docs/plans/T5/evidence.md` (category `evidence`, unit `T5`), then `documents.publish`; the audit's declared-output rule reads this row.

```
== write: documents.compose ==
{
  "body": "---\ndocument: T5/evidence\nversion: 1\nstatus: published\nproject: T5\ncategory: evidence\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: evidence\nphase: G\nplateau: capability\n---\n\n# T5 \u2014 Kata: evidence\n\n## Claim\n\nT5 delivers the kata loop: a planned procedure change opens a kata whose prediction is recorded\nbefore its measurement, the measurement runs on a held-out split, a measured regression reverts the\ncandidate, a flat or incomparable result adopts nothing, the kata cannot approve its own adoption,\nand the open/adopt gates refuse the documented cases.\n\n## Evidence\n\n- `crates/actions/tests/kata_gets.rs::the_prediction_is_recorded_before_the_measurement`\n- `crates/actions/tests/kata_measure.rs::a_procedure_change_is_measured_on_the_held_out_split`\n- `crates/actions/tests/kata_revert.rs::a_measured_regression_reverts_the_candidate`,\n  `::a_flat_or_incomparable_result_adopts_nothing`\n- `crates/actions/tests/kata_adopt.rs::the_kata_cannot_approve_its_own_adoption`\n- `crates/actions/tests/kata_open.rs::a_kata_without_an_instrument_is_not_opened`,\n  `::a_second_kata_for_an_open_signature_is_refused`\n- Migrations `0066_kata.sql` and `0067_kata_gate.sql`; merges `7e617673` (PR #636) and\n  `f8c82c68` (PR #654).\n\n## Verification\n\nRun the suite on a checked-out main: `cargo test -p conveyor-actions --test kata_adopt --test\nkata_gets --test kata_measure --test kata_open --test kata_revert`. The CI equivalent is the PR's\n`preflight` and `ci-public` at the merge commits above.\n\n## Links\n\n- `docs/plans/inception/T5/00-project-brief.md`, `01-inception-analysis.md`,\n  `02-architecture-design.md`\n- Issues #77 and #653; PRs #636 and #654.\n\n## Gap\n\nThe tranche-10 authorisation is not evidenced here and is not claimed \u2014 it is the #526 programme\nchain's act. No delivery gap is claimed by the issue record.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "547352fa0926cd8c98b084e84d26ca118aa15d6b1a4c755513a8720dcc7726f3",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:47:03.383099Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a21219-ffc6-7397-8721-61e5aa7e78fc",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "547352fa0926cd8c98b084e84d26ca118aa15d6b1a4c755513a8720dcc7726f3",
    "size_bytes": 1864,
    "unit": "T5"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d34-73db-975e-5d178d729cef",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 1864,
  "status": "active",
  "title": "Evidence \u2014 T5 Kata",
  "unit": "T5",
  "updated_at": "2026-11-25T09:47:03.383099Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
== read-back: documents.list ==
{
  "items": [
    {
      "body": null,
      "card_id": null,
      "category": "evidence",
      "content_hash": "547352fa0926cd8c98b084e84d26ca118aa15d6b1a4c755513a8720dcc7726f3",
      "content_type": "text/markdown",
      "created_at": "2026-11-25T09:47:03.383099Z",
      "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "created_by_name": "FR Test",
      "external_ref": null,
      "id": "01a21219-ffc6-7397-8721-61e5aa7e78fc",
      "kind": "document",
      "meta": {
        "category": "evidence",
        "content_type": "text/markdown",
        "sha256": "547352fa0926cd8c98b084e84d26ca118aa15d6b1a4c755513a8720dcc7726f3",
        "size_bytes": 1864,
        "unit": "T5"
      },
      "parent_document_id": null,
      "project_id": "01a1d039-0d34-73db-975e-5d178d729cef",
      "publish_requested_at": null,
      "publishable": false,
      "published_at": null,
      "published_by": null,
      "published_by_name": null,
      "size_bytes": 1864,
      "status": "active",
      "title": "Evidence \u2014 T5 Kata",
      "unit": "T5",
      "updated_at": "2026-11-25T09:47:03.383099Z",
      "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "updated_by_name": "FR Test",
      "version": 1
    }
  ]
}
```

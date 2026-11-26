# T5 closure-review — the published output

`documents.compose` of `docs/plans/T5/closure-review.md` (category `closure-review`, unit `T5`), then `documents.publish`; the audit's declared-output rule reads this row.

```
== write: documents.compose ==
{
  "body": "---\ndocument: T5/closure-review\nversion: 1\nstatus: published\nproject: T5\ncategory: closure-review\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: closure-review\nphase: G\nplateau: capability\n---\n\n# T5 \u2014 Kata: closure review\n\nWhether the project delivered its acceptance, on its evidence, and the decision that follows.\n\n## Outcome vs acceptance\n\n| Acceptance criterion | Evidence | Result |\n|---|---|---|\n| A planned procedure change opens a kata whose prediction is recorded before the measurement | `crates/actions/tests/kata_gets.rs::the_prediction_is_recorded_before_the_measurement`; the `kata.*` action family in `docs/design/data/actions.json`; migration `0066_kata.sql` | pass |\n| The measurement runs on the held-out split | `crates/actions/tests/kata_measure.rs::a_procedure_change_is_measured_on_the_held_out_split` | pass |\n| A measured regression reverts the candidate; a flat or incomparable result adopts nothing | `crates/actions/tests/kata_revert.rs::a_measured_regression_reverts_the_candidate`, `::a_flat_or_incomparable_result_adopts_nothing` | pass |\n| The kata cannot approve its own adoption | `crates/actions/tests/kata_adopt.rs::the_kata_cannot_approve_its_own_adoption`; migration `0067_kata_gate.sql` | pass |\n| A kata without an instrument is not opened; a second open kata for the same signature is refused | `crates/actions/tests/kata_open.rs::a_kata_without_an_instrument_is_not_opened`, `::a_second_kata_for_an_open_signature_is_refused` | pass |\n| The project's programme row and coverage agree | issue #77 closed by PR #636 (`7e617673`); #653 closed by PR #654 (`f8c82c68`); `build-coverage.py --check` and `build-programme-seed.py --check` green at `7d5ef094` | pass |\n\n## Evidence\n\n- Delivery merges: `7e6176733924b6bc695a7cf10a68dd5df061985c` (PR #636) and\n  `f8c82c689cc4e0a848f68da1fcb520cb42d71f9e` (PR #654).\n- Schema: `migrations/0066_kata.sql`, `migrations/0067_kata_gate.sql`.\n- Behaviour: the `kata.*` action family (`crates/actions/src/kata.rs`,\n  `crates/store/src/kata.rs`, `crates/domain/src/kata.rs`) and its integration tests under\n  `crates/actions/tests/kata_*.rs`.\n- The inception set: `docs/plans/inception/T5/` (brief, analysis, design).\n\n## Open items\n\n- The tranche-10 authorisation is **not** claimed here: the tranche's state is `planned` and its\n  authorisation is the #526 programme chain. This review records delivery, not authority.\n- No open delivery item is named by the issue record.\n\n## Decision\n\nClose. Acceptance is met on the evidence above; the tranche authorisation remains a separate\nprogramme act (#526), and this review does not manufacture it.\n",
  "card_id": null,
  "category": "closure-review",
  "content_hash": "83533f1147ee4602844cfbf2b1e452d507aabff10ce831a36b32d20b28914f49",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:46:50.304474Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a21219-ccba-7349-ad69-53093e798c8c",
  "kind": "document",
  "meta": {
    "category": "closure-review",
    "content_type": "text/markdown",
    "sha256": "83533f1147ee4602844cfbf2b1e452d507aabff10ce831a36b32d20b28914f49",
    "size_bytes": 2667,
    "unit": "T5"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d34-73db-975e-5d178d729cef",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 2667,
  "status": "active",
  "title": "Closure review \u2014 T5 Kata",
  "unit": "T5",
  "updated_at": "2026-11-25T09:46:50.304474Z",
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
      "category": "closure-review",
      "content_hash": "83533f1147ee4602844cfbf2b1e452d507aabff10ce831a36b32d20b28914f49",
      "content_type": "text/markdown",
      "created_at": "2026-11-25T09:46:50.304474Z",
      "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "created_by_name": "FR Test",
      "external_ref": null,
      "id": "01a21219-ccba-7349-ad69-53093e798c8c",
      "kind": "document",
      "meta": {
        "category": "closure-review",
        "content_type": "text/markdown",
        "sha256": "83533f1147ee4602844cfbf2b1e452d507aabff10ce831a36b32d20b28914f49",
        "size_bytes": 2667,
        "unit": "T5"
      },
      "parent_document_id": null,
      "project_id": "01a1d039-0d34-73db-975e-5d178d729cef",
      "publish_requested_at": null,
      "publishable": false,
      "published_at": null,
      "published_by": null,
      "published_by_name": null,
      "size_bytes": 2667,
      "status": "active",
      "title": "Closure review \u2014 T5 Kata",
      "unit": "T5",
      "updated_at": "2026-11-25T09:46:50.304474Z",
      "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "updated_by_name": "FR Test",
      "version": 1
    }
  ]
}
```

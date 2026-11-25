# T19F live proof — T19F — Evidence

Action `documents.compose` then `documents.publish`; the body is `docs/evidence/t19f/evidence-output.md`, written to the output standard's required sections.

```
== write: documents.compose ==
{
  "body": "# T19F \u2014 Evidence\n\nWhat was claimed, what was measured, and what remains a gap.\n\n## Claim\n\nThe T19F forum bindings are delivered on `main` (`eb0deb61`) and proven on the deployed engine: a\nmeeting stays pinned to forum-policy v1 while the v2 route is exercised, a board subject is decided\nin a sitting whose per-seat decision row carries `via.surface='meeting'`, and a cadence row\nmaterialises exactly one meeting. The policy v2 itself awaits one human review decision, which the\nproduct's authoring gate refuses to let a machine supply.\n\n## Evidence\n\n- Construction: PR #700 merged as `eb0deb61`; migration 0073 applied on the live database\n  (`_sqlx_migrations` max 73); the deployed image carries it (pod digest `9c80514d\u2026`).\n- Programme row: project `T19F` `01a210ba-b6cb-76cc-82a3-a2b45cfb498f` on its board, `programmes.refresh`\n  reports `created 0, skipped 52`.\n- Governance chain: `scripts/governance-chain.py --project T19F --check` passes 24/24 on the\n  published snapshot \u2014 seven approved artefacts, seven links, a live Statement of Architecture Work\n  and a live phase-F delivery contract (requester \u2260 approver), B7's baseline read back and not\n  overwritten, one Phase-G compliance review per contract (level `conformant`), and the published\n  `architecture-artifact` document with its Metadata/Entries/Links/Document headings.\n- Live gate: `scripts/t19f-live-gate-check.py --check` passes 11 of 12 invariants:\n  - the meeting `01a210c2-a327-7dc7-bab4-5a402a8497a8` is pinned at policy `t19f_live_gate_forum`\n    version 1; the pinned meeting still resolves v1 after the v2 route was exercised;\n  - the sitting body `01a210c8-579b-7808-bc32-aef8899b0b7a` decided contract\n    `01a210c8-7ea8-7dc9-8104-700ecfe2905b` in meeting `01a210c8-94ee-73ba-9147-88f2b4729ae0`;\n    approval `01a210c8-9273-7964-9b97-0f3dd810bcf3` is `approved` with exactly one decision row\n    carrying `via.surface='meeting'` and naming the chair (`conveyor-approver`); the agenda item\n    closed as `decision`;\n  - cadence `01a210c9-0d47-7473-8339-7ea7648e59e9` materialised twice to the same meeting\n    `01a210c9-2397-73f1-8d07-7e8e807e524f`, carrying the bound forum type and the slot and not\n    cancelled.\n\n## Verification\n\n```\npython3 scripts/governance-chain.py --project T19F --check      # 24/24 PASS\npython3 scripts/t19f-live-gate-check.py --check                  # 11/12 PASS, 1 blocked\n```\n\nThe transports are `scripts/programme-record.py` (the deployed action surface) and, for the\nsnapshot, `docs/design/scripts/publish-data.sh`. Raw read-backs and acts are under\n`docs/evidence/t19f/` (`gate/`, `gov/`, `live-gate.json`, `live-gate-check.txt`,\n`publish-report-only.log`).\n\n## Links\n\n- Plan: `docs/plans/T19F-meeting-bindings.md`; artefacts under `docs/plans/T19F/`.\n- Implementation: PR #700 (`Closes #683`), merge `eb0deb61`.\n- This unit: issue #684.\n\n## Gap\n\n- **The policy v2 publish is blocked on one human review.** `authoring.review` requires the\n  approval's last decision to be `human`; the admin CLI records `service` by design, so the gate\n  refuses it verbatim (`BLOCKED policy v2: \u2026`). A person must approve the review gate in the\n  console, after which re-running `--apply` publishes v2 and the twelfth invariant passes. The\n  asset `01a210c3-7c34-7338-89f2-6c7a6e40bea7` and its approved gate\n  `01a210c3-9007-75f4-927c-d1dc32f54c6b` are in place for that click.\n- The strict conformance publish is red on programme-wide debt that predates this unit (T5, T6,\n  T8, T15 and T17 closed since the accepted baseline; their new/worsened P0s and the S18/S21/\n  SPEC-V2 ledger rows). The T19F finding is `project_in_unauthorised_tranche` P1 \u2014 T19F is active\n  in tranche 6, whose authorisation is #526's unit; the plan requires T19F to stay active until\n  the tranche-6 review, so this is the declared state, not a regression. The pack was therefore\n  published report-only with the exact findings in `publish-report-only.log`.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "9a1500aaaac63e62fe3e84af8b08d6c3f811dfe8639d09352483a831d45733dd",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:41:34.231212Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210cb-6308-7552-861c-dad6bc5a3b9f",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "9a1500aaaac63e62fe3e84af8b08d6c3f811dfe8639d09352483a831d45733dd",
    "size_bytes": 3951,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 3951,
  "status": "active",
  "title": "T19F \u2014 Evidence",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:41:34.231212Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
== read-back: documents.get ==
{
  "body": "# T19F \u2014 Evidence\n\nWhat was claimed, what was measured, and what remains a gap.\n\n## Claim\n\nThe T19F forum bindings are delivered on `main` (`eb0deb61`) and proven on the deployed engine: a\nmeeting stays pinned to forum-policy v1 while the v2 route is exercised, a board subject is decided\nin a sitting whose per-seat decision row carries `via.surface='meeting'`, and a cadence row\nmaterialises exactly one meeting. The policy v2 itself awaits one human review decision, which the\nproduct's authoring gate refuses to let a machine supply.\n\n## Evidence\n\n- Construction: PR #700 merged as `eb0deb61`; migration 0073 applied on the live database\n  (`_sqlx_migrations` max 73); the deployed image carries it (pod digest `9c80514d\u2026`).\n- Programme row: project `T19F` `01a210ba-b6cb-76cc-82a3-a2b45cfb498f` on its board, `programmes.refresh`\n  reports `created 0, skipped 52`.\n- Governance chain: `scripts/governance-chain.py --project T19F --check` passes 24/24 on the\n  published snapshot \u2014 seven approved artefacts, seven links, a live Statement of Architecture Work\n  and a live phase-F delivery contract (requester \u2260 approver), B7's baseline read back and not\n  overwritten, one Phase-G compliance review per contract (level `conformant`), and the published\n  `architecture-artifact` document with its Metadata/Entries/Links/Document headings.\n- Live gate: `scripts/t19f-live-gate-check.py --check` passes 11 of 12 invariants:\n  - the meeting `01a210c2-a327-7dc7-bab4-5a402a8497a8` is pinned at policy `t19f_live_gate_forum`\n    version 1; the pinned meeting still resolves v1 after the v2 route was exercised;\n  - the sitting body `01a210c8-579b-7808-bc32-aef8899b0b7a` decided contract\n    `01a210c8-7ea8-7dc9-8104-700ecfe2905b` in meeting `01a210c8-94ee-73ba-9147-88f2b4729ae0`;\n    approval `01a210c8-9273-7964-9b97-0f3dd810bcf3` is `approved` with exactly one decision row\n    carrying `via.surface='meeting'` and naming the chair (`conveyor-approver`); the agenda item\n    closed as `decision`;\n  - cadence `01a210c9-0d47-7473-8339-7ea7648e59e9` materialised twice to the same meeting\n    `01a210c9-2397-73f1-8d07-7e8e807e524f`, carrying the bound forum type and the slot and not\n    cancelled.\n\n## Verification\n\n```\npython3 scripts/governance-chain.py --project T19F --check      # 24/24 PASS\npython3 scripts/t19f-live-gate-check.py --check                  # 11/12 PASS, 1 blocked\n```\n\nThe transports are `scripts/programme-record.py` (the deployed action surface) and, for the\nsnapshot, `docs/design/scripts/publish-data.sh`. Raw read-backs and acts are under\n`docs/evidence/t19f/` (`gate/`, `gov/`, `live-gate.json`, `live-gate-check.txt`,\n`publish-report-only.log`).\n\n## Links\n\n- Plan: `docs/plans/T19F-meeting-bindings.md`; artefacts under `docs/plans/T19F/`.\n- Implementation: PR #700 (`Closes #683`), merge `eb0deb61`.\n- This unit: issue #684.\n\n## Gap\n\n- **The policy v2 publish is blocked on one human review.** `authoring.review` requires the\n  approval's last decision to be `human`; the admin CLI records `service` by design, so the gate\n  refuses it verbatim (`BLOCKED policy v2: \u2026`). A person must approve the review gate in the\n  console, after which re-running `--apply` publishes v2 and the twelfth invariant passes. The\n  asset `01a210c3-7c34-7338-89f2-6c7a6e40bea7` and its approved gate\n  `01a210c3-9007-75f4-927c-d1dc32f54c6b` are in place for that click.\n- The strict conformance publish is red on programme-wide debt that predates this unit (T5, T6,\n  T8, T15 and T17 closed since the accepted baseline; their new/worsened P0s and the S18/S21/\n  SPEC-V2 ledger rows). The T19F finding is `project_in_unauthorised_tranche` P1 \u2014 T19F is active\n  in tranche 6, whose authorisation is #526's unit; the plan requires T19F to stay active until\n  the tranche-6 review, so this is the declared state, not a regression. The pack was therefore\n  published report-only with the exact findings in `publish-report-only.log`.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "9a1500aaaac63e62fe3e84af8b08d6c3f811dfe8639d09352483a831d45733dd",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:41:34.231212Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210cb-6308-7552-861c-dad6bc5a3b9f",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "9a1500aaaac63e62fe3e84af8b08d6c3f811dfe8639d09352483a831d45733dd",
    "size_bytes": 3951,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 3951,
  "status": "active",
  "title": "T19F \u2014 Evidence",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:41:34.231212Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
```

```
== write: documents.publish ==
{
  "body": "# T19F \u2014 Evidence\n\nWhat was claimed, what was measured, and what remains a gap.\n\n## Claim\n\nThe T19F forum bindings are delivered on `main` (`eb0deb61`) and proven on the deployed engine: a\nmeeting stays pinned to forum-policy v1 while the v2 route is exercised, a board subject is decided\nin a sitting whose per-seat decision row carries `via.surface='meeting'`, and a cadence row\nmaterialises exactly one meeting. The policy v2 itself awaits one human review decision, which the\nproduct's authoring gate refuses to let a machine supply.\n\n## Evidence\n\n- Construction: PR #700 merged as `eb0deb61`; migration 0073 applied on the live database\n  (`_sqlx_migrations` max 73); the deployed image carries it (pod digest `9c80514d\u2026`).\n- Programme row: project `T19F` `01a210ba-b6cb-76cc-82a3-a2b45cfb498f` on its board, `programmes.refresh`\n  reports `created 0, skipped 52`.\n- Governance chain: `scripts/governance-chain.py --project T19F --check` passes 24/24 on the\n  published snapshot \u2014 seven approved artefacts, seven links, a live Statement of Architecture Work\n  and a live phase-F delivery contract (requester \u2260 approver), B7's baseline read back and not\n  overwritten, one Phase-G compliance review per contract (level `conformant`), and the published\n  `architecture-artifact` document with its Metadata/Entries/Links/Document headings.\n- Live gate: `scripts/t19f-live-gate-check.py --check` passes 11 of 12 invariants:\n  - the meeting `01a210c2-a327-7dc7-bab4-5a402a8497a8` is pinned at policy `t19f_live_gate_forum`\n    version 1; the pinned meeting still resolves v1 after the v2 route was exercised;\n  - the sitting body `01a210c8-579b-7808-bc32-aef8899b0b7a` decided contract\n    `01a210c8-7ea8-7dc9-8104-700ecfe2905b` in meeting `01a210c8-94ee-73ba-9147-88f2b4729ae0`;\n    approval `01a210c8-9273-7964-9b97-0f3dd810bcf3` is `approved` with exactly one decision row\n    carrying `via.surface='meeting'` and naming the chair (`conveyor-approver`); the agenda item\n    closed as `decision`;\n  - cadence `01a210c9-0d47-7473-8339-7ea7648e59e9` materialised twice to the same meeting\n    `01a210c9-2397-73f1-8d07-7e8e807e524f`, carrying the bound forum type and the slot and not\n    cancelled.\n\n## Verification\n\n```\npython3 scripts/governance-chain.py --project T19F --check      # 24/24 PASS\npython3 scripts/t19f-live-gate-check.py --check                  # 11/12 PASS, 1 blocked\n```\n\nThe transports are `scripts/programme-record.py` (the deployed action surface) and, for the\nsnapshot, `docs/design/scripts/publish-data.sh`. Raw read-backs and acts are under\n`docs/evidence/t19f/` (`gate/`, `gov/`, `live-gate.json`, `live-gate-check.txt`,\n`publish-report-only.log`).\n\n## Links\n\n- Plan: `docs/plans/T19F-meeting-bindings.md`; artefacts under `docs/plans/T19F/`.\n- Implementation: PR #700 (`Closes #683`), merge `eb0deb61`.\n- This unit: issue #684.\n\n## Gap\n\n- **The policy v2 publish is blocked on one human review.** `authoring.review` requires the\n  approval's last decision to be `human`; the admin CLI records `service` by design, so the gate\n  refuses it verbatim (`BLOCKED policy v2: \u2026`). A person must approve the review gate in the\n  console, after which re-running `--apply` publishes v2 and the twelfth invariant passes. The\n  asset `01a210c3-7c34-7338-89f2-6c7a6e40bea7` and its approved gate\n  `01a210c3-9007-75f4-927c-d1dc32f54c6b` are in place for that click.\n- The strict conformance publish is red on programme-wide debt that predates this unit (T5, T6,\n  T8, T15 and T17 closed since the accepted baseline; their new/worsened P0s and the S18/S21/\n  SPEC-V2 ledger rows). The T19F finding is `project_in_unauthorised_tranche` P1 \u2014 T19F is active\n  in tranche 6, whose authorisation is #526's unit; the plan requires T19F to stay active until\n  the tranche-6 review, so this is the declared state, not a regression. The pack was therefore\n  published report-only with the exact findings in `publish-report-only.log`.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "9a1500aaaac63e62fe3e84af8b08d6c3f811dfe8639d09352483a831d45733dd",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:41:34.231212Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210cb-6308-7552-861c-dad6bc5a3b9f",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "9a1500aaaac63e62fe3e84af8b08d6c3f811dfe8639d09352483a831d45733dd",
    "size_bytes": 3951,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T03:41:39.359669Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 3951,
  "status": "active",
  "title": "T19F \u2014 Evidence",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:41:39.359669Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
== read-back: documents.get ==
{
  "body": "# T19F \u2014 Evidence\n\nWhat was claimed, what was measured, and what remains a gap.\n\n## Claim\n\nThe T19F forum bindings are delivered on `main` (`eb0deb61`) and proven on the deployed engine: a\nmeeting stays pinned to forum-policy v1 while the v2 route is exercised, a board subject is decided\nin a sitting whose per-seat decision row carries `via.surface='meeting'`, and a cadence row\nmaterialises exactly one meeting. The policy v2 itself awaits one human review decision, which the\nproduct's authoring gate refuses to let a machine supply.\n\n## Evidence\n\n- Construction: PR #700 merged as `eb0deb61`; migration 0073 applied on the live database\n  (`_sqlx_migrations` max 73); the deployed image carries it (pod digest `9c80514d\u2026`).\n- Programme row: project `T19F` `01a210ba-b6cb-76cc-82a3-a2b45cfb498f` on its board, `programmes.refresh`\n  reports `created 0, skipped 52`.\n- Governance chain: `scripts/governance-chain.py --project T19F --check` passes 24/24 on the\n  published snapshot \u2014 seven approved artefacts, seven links, a live Statement of Architecture Work\n  and a live phase-F delivery contract (requester \u2260 approver), B7's baseline read back and not\n  overwritten, one Phase-G compliance review per contract (level `conformant`), and the published\n  `architecture-artifact` document with its Metadata/Entries/Links/Document headings.\n- Live gate: `scripts/t19f-live-gate-check.py --check` passes 11 of 12 invariants:\n  - the meeting `01a210c2-a327-7dc7-bab4-5a402a8497a8` is pinned at policy `t19f_live_gate_forum`\n    version 1; the pinned meeting still resolves v1 after the v2 route was exercised;\n  - the sitting body `01a210c8-579b-7808-bc32-aef8899b0b7a` decided contract\n    `01a210c8-7ea8-7dc9-8104-700ecfe2905b` in meeting `01a210c8-94ee-73ba-9147-88f2b4729ae0`;\n    approval `01a210c8-9273-7964-9b97-0f3dd810bcf3` is `approved` with exactly one decision row\n    carrying `via.surface='meeting'` and naming the chair (`conveyor-approver`); the agenda item\n    closed as `decision`;\n  - cadence `01a210c9-0d47-7473-8339-7ea7648e59e9` materialised twice to the same meeting\n    `01a210c9-2397-73f1-8d07-7e8e807e524f`, carrying the bound forum type and the slot and not\n    cancelled.\n\n## Verification\n\n```\npython3 scripts/governance-chain.py --project T19F --check      # 24/24 PASS\npython3 scripts/t19f-live-gate-check.py --check                  # 11/12 PASS, 1 blocked\n```\n\nThe transports are `scripts/programme-record.py` (the deployed action surface) and, for the\nsnapshot, `docs/design/scripts/publish-data.sh`. Raw read-backs and acts are under\n`docs/evidence/t19f/` (`gate/`, `gov/`, `live-gate.json`, `live-gate-check.txt`,\n`publish-report-only.log`).\n\n## Links\n\n- Plan: `docs/plans/T19F-meeting-bindings.md`; artefacts under `docs/plans/T19F/`.\n- Implementation: PR #700 (`Closes #683`), merge `eb0deb61`.\n- This unit: issue #684.\n\n## Gap\n\n- **The policy v2 publish is blocked on one human review.** `authoring.review` requires the\n  approval's last decision to be `human`; the admin CLI records `service` by design, so the gate\n  refuses it verbatim (`BLOCKED policy v2: \u2026`). A person must approve the review gate in the\n  console, after which re-running `--apply` publishes v2 and the twelfth invariant passes. The\n  asset `01a210c3-7c34-7338-89f2-6c7a6e40bea7` and its approved gate\n  `01a210c3-9007-75f4-927c-d1dc32f54c6b` are in place for that click.\n- The strict conformance publish is red on programme-wide debt that predates this unit (T5, T6,\n  T8, T15 and T17 closed since the accepted baseline; their new/worsened P0s and the S18/S21/\n  SPEC-V2 ledger rows). The T19F finding is `project_in_unauthorised_tranche` P1 \u2014 T19F is active\n  in tranche 6, whose authorisation is #526's unit; the plan requires T19F to stay active until\n  the tranche-6 review, so this is the declared state, not a regression. The pack was therefore\n  published report-only with the exact findings in `publish-report-only.log`.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "9a1500aaaac63e62fe3e84af8b08d6c3f811dfe8639d09352483a831d45733dd",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:41:34.231212Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210cb-6308-7552-861c-dad6bc5a3b9f",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "9a1500aaaac63e62fe3e84af8b08d6c3f811dfe8639d09352483a831d45733dd",
    "size_bytes": 3951,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T03:41:39.359669Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 3951,
  "status": "active",
  "title": "T19F \u2014 Evidence",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:41:39.359669Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
```

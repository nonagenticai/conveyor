# T19F live proof — T19F — Project brief

Action `documents.compose` + `documents.publish` for the declared output `project-brief` on project T19F; the body is the plan document `docs/plans/T19F/project-brief.md`.

```
== write: documents.compose ==
{
  "body": "\n# T19F \u2014 Decision-forum governance: project brief\n\n## Purpose\n\nClose the two seams T19 left open: a forum policy can be created but not changed through a\ngoverned route, and the T9 board and T10 cadence rows do not name the meeting types they use. The\nproject makes the policy a versioned release through the tenant's authoring route, and makes the\nboard sitting and the cadence materialisation first-class consumers of T19's meeting arithmetic.\n\n## Outcome\n\nAfter T19F:\n\n- a policy change is a reviewed authoring asset that publishes a new immutable `meeting_type`\n  version; an in-flight meeting keeps the version it pinned;\n- a board subject decision can be taken in a sitting whose per-seat decisions are recorded\n  `via='meeting'`, with the same quorum, department, evidence and conflict rules as T19;\n- a cadence row names its meeting type and materialises exactly one meeting, idempotently;\n- the direct board path and every T19 primitive are unchanged;\n- T19F is a live programme row with its MSP/TOGAF/SAFe artefacts, evidence and a closure review\n  that defers closure to the tranche-6 review.\n\n## Scope boundary\n\nIn: the authoring hook, the two read actions, the binding columns, the board-sitting actions, the\ncadence materialisation, the mock-UI and real-app panels, the programme record and the live proof.\n\nOut: a second decision store, a chat/AV/transcript system, a rebuild of T9/T10 or the approval\nprimitive, a change to the direct `board_decide` path, the T18 shell/nav, the homepage mirror, the\ntranche 2\u21924 retrospective chain (#526).\n\n## Stakeholders\n\n| Role | Interest |\n|---|---|\n| Programme sponsor / SRO | a governed forum policy and a board record that shows its deliberation |\n| Programme manager | T19F's tranche-6 evidence and the B7 realisation increment |\n| Programme architect / Architecture Board | the sitting path, the contract, the artefact set |\n| BCM | B7's measurement across board sittings and materialised cadences |\n| Department heads and job owners | seats that are invited and counted by department |\n| Delivery teams | the unchanged direct paths and the existing meetings |\n\n## Constraints\n\n- The authoring route is T17's; no parallel policy store.\n- `meeting_type` versions are immutable; a meeting pins its version.\n- The board's decision arithmetic stays T19's `close_item_tx`; no second calculation.\n- The cadence instance rule is a database constraint, not an action convention.\n- No local Rust compile; the compile/test verdict is `preflight.yml` and the PR's CI.\n- T19F stays `active` while its tranche is planned; closure is a programme decision.\n\n## Acceptance\n\nThe follow-on plan's P1\u2013P5 scenarios (`docs/plans/T19F-meeting-bindings.md` \u00a75) pass at the PR\nhead or on the deployed engine; the migration ledger reaches 68; the programme record carries T19F\nand B7; the strict conformance publish is green with no new T19F finding.\n\n## Risks\n\n| Risk | Response |\n|---|---|\n| A policy change is smuggled through `meetings.schedule` | the schedule refuses a policy when the key exists; the authoring route is the only version writer |\n| A board sitting lowers dual control | `close_item_tx` applies the linked approval's stricter policy; the sitting adds no second path |\n| A cadence slot gets two meetings | the cadence row lock in `materialise_cadence_tx`; the action returns the existing instance |\n| T17's authoring route is mid-flight (#655) | the hook is additive and signature-stable; rebase or coordinate with the T17 lane |\n| T19F closes in a planned tranche | it stays active; the closure review defers closure to the tranche-6 review |\n\n## Approval\n\nThe brief is reviewed with the inception, architecture, MSP and SAFe artefacts on 2026-12-01; the\nApproval of the plan is the programme manager's, the architecture review is the Architecture\nBoard's.\n",
  "card_id": null,
  "category": "project-brief",
  "content_hash": "cde31e037e4164fddb81c35f07055d4b518765ddfd4835bf62cd11d3e5e51ea4",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:30:37.595010Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210c1-5e0b-73a0-8369-4e02145395e2",
  "kind": "document",
  "meta": {
    "category": "project-brief",
    "content_type": "text/markdown",
    "sha256": "cde31e037e4164fddb81c35f07055d4b518765ddfd4835bf62cd11d3e5e51ea4",
    "size_bytes": 3820,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 3820,
  "status": "active",
  "title": "T19F \u2014 Project brief",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:30:37.595010Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
== read-back: documents.get ==
{
  "body": "\n# T19F \u2014 Decision-forum governance: project brief\n\n## Purpose\n\nClose the two seams T19 left open: a forum policy can be created but not changed through a\ngoverned route, and the T9 board and T10 cadence rows do not name the meeting types they use. The\nproject makes the policy a versioned release through the tenant's authoring route, and makes the\nboard sitting and the cadence materialisation first-class consumers of T19's meeting arithmetic.\n\n## Outcome\n\nAfter T19F:\n\n- a policy change is a reviewed authoring asset that publishes a new immutable `meeting_type`\n  version; an in-flight meeting keeps the version it pinned;\n- a board subject decision can be taken in a sitting whose per-seat decisions are recorded\n  `via='meeting'`, with the same quorum, department, evidence and conflict rules as T19;\n- a cadence row names its meeting type and materialises exactly one meeting, idempotently;\n- the direct board path and every T19 primitive are unchanged;\n- T19F is a live programme row with its MSP/TOGAF/SAFe artefacts, evidence and a closure review\n  that defers closure to the tranche-6 review.\n\n## Scope boundary\n\nIn: the authoring hook, the two read actions, the binding columns, the board-sitting actions, the\ncadence materialisation, the mock-UI and real-app panels, the programme record and the live proof.\n\nOut: a second decision store, a chat/AV/transcript system, a rebuild of T9/T10 or the approval\nprimitive, a change to the direct `board_decide` path, the T18 shell/nav, the homepage mirror, the\ntranche 2\u21924 retrospective chain (#526).\n\n## Stakeholders\n\n| Role | Interest |\n|---|---|\n| Programme sponsor / SRO | a governed forum policy and a board record that shows its deliberation |\n| Programme manager | T19F's tranche-6 evidence and the B7 realisation increment |\n| Programme architect / Architecture Board | the sitting path, the contract, the artefact set |\n| BCM | B7's measurement across board sittings and materialised cadences |\n| Department heads and job owners | seats that are invited and counted by department |\n| Delivery teams | the unchanged direct paths and the existing meetings |\n\n## Constraints\n\n- The authoring route is T17's; no parallel policy store.\n- `meeting_type` versions are immutable; a meeting pins its version.\n- The board's decision arithmetic stays T19's `close_item_tx`; no second calculation.\n- The cadence instance rule is a database constraint, not an action convention.\n- No local Rust compile; the compile/test verdict is `preflight.yml` and the PR's CI.\n- T19F stays `active` while its tranche is planned; closure is a programme decision.\n\n## Acceptance\n\nThe follow-on plan's P1\u2013P5 scenarios (`docs/plans/T19F-meeting-bindings.md` \u00a75) pass at the PR\nhead or on the deployed engine; the migration ledger reaches 68; the programme record carries T19F\nand B7; the strict conformance publish is green with no new T19F finding.\n\n## Risks\n\n| Risk | Response |\n|---|---|\n| A policy change is smuggled through `meetings.schedule` | the schedule refuses a policy when the key exists; the authoring route is the only version writer |\n| A board sitting lowers dual control | `close_item_tx` applies the linked approval's stricter policy; the sitting adds no second path |\n| A cadence slot gets two meetings | the cadence row lock in `materialise_cadence_tx`; the action returns the existing instance |\n| T17's authoring route is mid-flight (#655) | the hook is additive and signature-stable; rebase or coordinate with the T17 lane |\n| T19F closes in a planned tranche | it stays active; the closure review defers closure to the tranche-6 review |\n\n## Approval\n\nThe brief is reviewed with the inception, architecture, MSP and SAFe artefacts on 2026-12-01; the\nApproval of the plan is the programme manager's, the architecture review is the Architecture\nBoard's.\n",
  "card_id": null,
  "category": "project-brief",
  "content_hash": "cde31e037e4164fddb81c35f07055d4b518765ddfd4835bf62cd11d3e5e51ea4",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:30:37.595010Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210c1-5e0b-73a0-8369-4e02145395e2",
  "kind": "document",
  "meta": {
    "category": "project-brief",
    "content_type": "text/markdown",
    "sha256": "cde31e037e4164fddb81c35f07055d4b518765ddfd4835bf62cd11d3e5e51ea4",
    "size_bytes": 3820,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 3820,
  "status": "active",
  "title": "T19F \u2014 Project brief",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:30:37.595010Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
```

```
== write: documents.update ==
{
  "body": "# T19F \u2014 Project brief\n\nOne page: what this project changes, what it reuses, and how it is accepted.\n\n## Outcome\n\nAfter T19F, a forum policy changes only through the reviewed authoring route (publishing a new\nimmutable `meeting_type` version while an in-flight meeting keeps its pinned version), a board\nsubject decision is taken in a sitting whose per-seat decisions are recorded `via='meeting'`, and\na cadence row materialises exactly one meeting \u2014 with T19's primitives and the direct board path\nunchanged.\n\n## Boundary\n\nIn: the authoring publish hook and the two policy reads, the `governance_body` and\n`programme_cadence` bindings, the board-sitting actions, the cadence materialisation, the mock-UI\nand real-app panels, the programme record and the live proof.\n\nOut: a second decision store, a chat/AV/transcript system, a rebuild of T9/T10 or the approval\nprimitive, a change to the direct `board_decide` path, the T18 shell/nav, the homepage mirror, the\ntranche 2\u21924 retrospective chain (#526), and programme-wide conformance debt.\n\n## Reuse sources\n\n- T17's authoring route (`authoring.intake/draft/review/publish`) \u2014 the only policy writer.\n- T19's meeting aggregate and `close_item_tx` \u2014 the only decision arithmetic.\n- T9's `governance_body` \u2014 the sitting's convener and body.\n- T10's `programme_cadence` \u2014 the materialised forum.\n- `docs/SPEC_V2.md` \u00a76.8 and \u00a712 T19F \u2014 the gate.\n\n## Inception units\n\n- `docs/plans/T19F/inception-analysis.md` \u2014 stakeholders, risks and the R1\u2013R8 requirements.\n- `docs/plans/T19/closure-review.md` \u2014 the two open items this project closes.\n- `docs/plans/T19F/architecture-design.md` \u2014 PSA + HLD.\n\n## Dependencies\n\n- blocked-by #683 \u2014 the implementation (merged in #700, `eb0deb61`).\n- acceptance-blocked-by \u2014 the deployed image carrying migration 0073 (rolled out and verified).\n- related #526 \u2014 tranche authorisation; T19F stays `active` until the tranche-6 review.\n- related #655 \u2014 the authoring route's own unit.\n- related #650 \u2014 the T19 close-out this project follows.\n\n## Acceptance\n\n- Given a live meeting pinned to v1, when a reviewed policy asset publishes v2, then the meeting\n  still resolves v1 and the type list carries both versions.\n- Given a board subject and a bound forum type, when the sitting's chair records a position and\n  the item is decided, then the approval carries one `via='meeting'` decision row per seat.\n- Given a cadence row with a type, when materialise runs twice, then exactly one non-cancelled\n  meeting exists with the bound type and the slot.\n- Given T19F's record, when the governance chain checks, then seven artefacts, both contracts,\n  both Phase-G reviews, B7 read-not-overwritten and the published architecture-artifact exist.\n\n## Capabilities\n\n- B1 Governed tenant assets \u2014 T17's route extended with the policy payload.\n- B2 Deliberated decisions \u2014 T19's meeting reused by the board sitting.\n- B3 Cadence-driven forums \u2014 T10's cadence row bound to a meeting type.\n\n## Plan\n\n`docs/plans/T19F-meeting-bindings.md`, delivered as #682 (plan and artefacts), #683\n(implementation, merged `eb0deb61`) and #684 (live proof and publication).\n",
  "card_id": null,
  "category": "project-brief",
  "content_hash": "e91f7cb6e5958aa71e433fc329d446382cb2a6a93e21178ccfcf3b2e34537686",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:30:37.595010Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210c1-5e0b-73a0-8369-4e02145395e2",
  "kind": "document",
  "meta": {
    "category": "project-brief",
    "content_type": "text/markdown",
    "sha256": "e91f7cb6e5958aa71e433fc329d446382cb2a6a93e21178ccfcf3b2e34537686",
    "size_bytes": 3164,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 3164,
  "status": "active",
  "title": "T19F \u2014 Project brief",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:31:39.192369Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 2
}
== read-back: documents.get ==
{
  "body": "# T19F \u2014 Project brief\n\nOne page: what this project changes, what it reuses, and how it is accepted.\n\n## Outcome\n\nAfter T19F, a forum policy changes only through the reviewed authoring route (publishing a new\nimmutable `meeting_type` version while an in-flight meeting keeps its pinned version), a board\nsubject decision is taken in a sitting whose per-seat decisions are recorded `via='meeting'`, and\na cadence row materialises exactly one meeting \u2014 with T19's primitives and the direct board path\nunchanged.\n\n## Boundary\n\nIn: the authoring publish hook and the two policy reads, the `governance_body` and\n`programme_cadence` bindings, the board-sitting actions, the cadence materialisation, the mock-UI\nand real-app panels, the programme record and the live proof.\n\nOut: a second decision store, a chat/AV/transcript system, a rebuild of T9/T10 or the approval\nprimitive, a change to the direct `board_decide` path, the T18 shell/nav, the homepage mirror, the\ntranche 2\u21924 retrospective chain (#526), and programme-wide conformance debt.\n\n## Reuse sources\n\n- T17's authoring route (`authoring.intake/draft/review/publish`) \u2014 the only policy writer.\n- T19's meeting aggregate and `close_item_tx` \u2014 the only decision arithmetic.\n- T9's `governance_body` \u2014 the sitting's convener and body.\n- T10's `programme_cadence` \u2014 the materialised forum.\n- `docs/SPEC_V2.md` \u00a76.8 and \u00a712 T19F \u2014 the gate.\n\n## Inception units\n\n- `docs/plans/T19F/inception-analysis.md` \u2014 stakeholders, risks and the R1\u2013R8 requirements.\n- `docs/plans/T19/closure-review.md` \u2014 the two open items this project closes.\n- `docs/plans/T19F/architecture-design.md` \u2014 PSA + HLD.\n\n## Dependencies\n\n- blocked-by #683 \u2014 the implementation (merged in #700, `eb0deb61`).\n- acceptance-blocked-by \u2014 the deployed image carrying migration 0073 (rolled out and verified).\n- related #526 \u2014 tranche authorisation; T19F stays `active` until the tranche-6 review.\n- related #655 \u2014 the authoring route's own unit.\n- related #650 \u2014 the T19 close-out this project follows.\n\n## Acceptance\n\n- Given a live meeting pinned to v1, when a reviewed policy asset publishes v2, then the meeting\n  still resolves v1 and the type list carries both versions.\n- Given a board subject and a bound forum type, when the sitting's chair records a position and\n  the item is decided, then the approval carries one `via='meeting'` decision row per seat.\n- Given a cadence row with a type, when materialise runs twice, then exactly one non-cancelled\n  meeting exists with the bound type and the slot.\n- Given T19F's record, when the governance chain checks, then seven artefacts, both contracts,\n  both Phase-G reviews, B7 read-not-overwritten and the published architecture-artifact exist.\n\n## Capabilities\n\n- B1 Governed tenant assets \u2014 T17's route extended with the policy payload.\n- B2 Deliberated decisions \u2014 T19's meeting reused by the board sitting.\n- B3 Cadence-driven forums \u2014 T10's cadence row bound to a meeting type.\n\n## Plan\n\n`docs/plans/T19F-meeting-bindings.md`, delivered as #682 (plan and artefacts), #683\n(implementation, merged `eb0deb61`) and #684 (live proof and publication).\n",
  "card_id": null,
  "category": "project-brief",
  "content_hash": "e91f7cb6e5958aa71e433fc329d446382cb2a6a93e21178ccfcf3b2e34537686",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:30:37.595010Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210c1-5e0b-73a0-8369-4e02145395e2",
  "kind": "document",
  "meta": {
    "category": "project-brief",
    "content_type": "text/markdown",
    "sha256": "e91f7cb6e5958aa71e433fc329d446382cb2a6a93e21178ccfcf3b2e34537686",
    "size_bytes": 3164,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 3164,
  "status": "active",
  "title": "T19F \u2014 Project brief",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:31:39.192369Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 2
}
```

```
== write: documents.publish ==
{
  "body": "# T19F \u2014 Project brief\n\nOne page: what this project changes, what it reuses, and how it is accepted.\n\n## Outcome\n\nAfter T19F, a forum policy changes only through the reviewed authoring route (publishing a new\nimmutable `meeting_type` version while an in-flight meeting keeps its pinned version), a board\nsubject decision is taken in a sitting whose per-seat decisions are recorded `via='meeting'`, and\na cadence row materialises exactly one meeting \u2014 with T19's primitives and the direct board path\nunchanged.\n\n## Boundary\n\nIn: the authoring publish hook and the two policy reads, the `governance_body` and\n`programme_cadence` bindings, the board-sitting actions, the cadence materialisation, the mock-UI\nand real-app panels, the programme record and the live proof.\n\nOut: a second decision store, a chat/AV/transcript system, a rebuild of T9/T10 or the approval\nprimitive, a change to the direct `board_decide` path, the T18 shell/nav, the homepage mirror, the\ntranche 2\u21924 retrospective chain (#526), and programme-wide conformance debt.\n\n## Reuse sources\n\n- T17's authoring route (`authoring.intake/draft/review/publish`) \u2014 the only policy writer.\n- T19's meeting aggregate and `close_item_tx` \u2014 the only decision arithmetic.\n- T9's `governance_body` \u2014 the sitting's convener and body.\n- T10's `programme_cadence` \u2014 the materialised forum.\n- `docs/SPEC_V2.md` \u00a76.8 and \u00a712 T19F \u2014 the gate.\n\n## Inception units\n\n- `docs/plans/T19F/inception-analysis.md` \u2014 stakeholders, risks and the R1\u2013R8 requirements.\n- `docs/plans/T19/closure-review.md` \u2014 the two open items this project closes.\n- `docs/plans/T19F/architecture-design.md` \u2014 PSA + HLD.\n\n## Dependencies\n\n- blocked-by #683 \u2014 the implementation (merged in #700, `eb0deb61`).\n- acceptance-blocked-by \u2014 the deployed image carrying migration 0073 (rolled out and verified).\n- related #526 \u2014 tranche authorisation; T19F stays `active` until the tranche-6 review.\n- related #655 \u2014 the authoring route's own unit.\n- related #650 \u2014 the T19 close-out this project follows.\n\n## Acceptance\n\n- Given a live meeting pinned to v1, when a reviewed policy asset publishes v2, then the meeting\n  still resolves v1 and the type list carries both versions.\n- Given a board subject and a bound forum type, when the sitting's chair records a position and\n  the item is decided, then the approval carries one `via='meeting'` decision row per seat.\n- Given a cadence row with a type, when materialise runs twice, then exactly one non-cancelled\n  meeting exists with the bound type and the slot.\n- Given T19F's record, when the governance chain checks, then seven artefacts, both contracts,\n  both Phase-G reviews, B7 read-not-overwritten and the published architecture-artifact exist.\n\n## Capabilities\n\n- B1 Governed tenant assets \u2014 T17's route extended with the policy payload.\n- B2 Deliberated decisions \u2014 T19's meeting reused by the board sitting.\n- B3 Cadence-driven forums \u2014 T10's cadence row bound to a meeting type.\n\n## Plan\n\n`docs/plans/T19F-meeting-bindings.md`, delivered as #682 (plan and artefacts), #683\n(implementation, merged `eb0deb61`) and #684 (live proof and publication).\n",
  "card_id": null,
  "category": "project-brief",
  "content_hash": "e91f7cb6e5958aa71e433fc329d446382cb2a6a93e21178ccfcf3b2e34537686",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:30:37.595010Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210c1-5e0b-73a0-8369-4e02145395e2",
  "kind": "document",
  "meta": {
    "category": "project-brief",
    "content_type": "text/markdown",
    "sha256": "e91f7cb6e5958aa71e433fc329d446382cb2a6a93e21178ccfcf3b2e34537686",
    "size_bytes": 3164,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T03:31:44.524272Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 3164,
  "status": "active",
  "title": "T19F \u2014 Project brief",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:31:44.524272Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 2
}
== read-back: documents.get ==
{
  "body": "# T19F \u2014 Project brief\n\nOne page: what this project changes, what it reuses, and how it is accepted.\n\n## Outcome\n\nAfter T19F, a forum policy changes only through the reviewed authoring route (publishing a new\nimmutable `meeting_type` version while an in-flight meeting keeps its pinned version), a board\nsubject decision is taken in a sitting whose per-seat decisions are recorded `via='meeting'`, and\na cadence row materialises exactly one meeting \u2014 with T19's primitives and the direct board path\nunchanged.\n\n## Boundary\n\nIn: the authoring publish hook and the two policy reads, the `governance_body` and\n`programme_cadence` bindings, the board-sitting actions, the cadence materialisation, the mock-UI\nand real-app panels, the programme record and the live proof.\n\nOut: a second decision store, a chat/AV/transcript system, a rebuild of T9/T10 or the approval\nprimitive, a change to the direct `board_decide` path, the T18 shell/nav, the homepage mirror, the\ntranche 2\u21924 retrospective chain (#526), and programme-wide conformance debt.\n\n## Reuse sources\n\n- T17's authoring route (`authoring.intake/draft/review/publish`) \u2014 the only policy writer.\n- T19's meeting aggregate and `close_item_tx` \u2014 the only decision arithmetic.\n- T9's `governance_body` \u2014 the sitting's convener and body.\n- T10's `programme_cadence` \u2014 the materialised forum.\n- `docs/SPEC_V2.md` \u00a76.8 and \u00a712 T19F \u2014 the gate.\n\n## Inception units\n\n- `docs/plans/T19F/inception-analysis.md` \u2014 stakeholders, risks and the R1\u2013R8 requirements.\n- `docs/plans/T19/closure-review.md` \u2014 the two open items this project closes.\n- `docs/plans/T19F/architecture-design.md` \u2014 PSA + HLD.\n\n## Dependencies\n\n- blocked-by #683 \u2014 the implementation (merged in #700, `eb0deb61`).\n- acceptance-blocked-by \u2014 the deployed image carrying migration 0073 (rolled out and verified).\n- related #526 \u2014 tranche authorisation; T19F stays `active` until the tranche-6 review.\n- related #655 \u2014 the authoring route's own unit.\n- related #650 \u2014 the T19 close-out this project follows.\n\n## Acceptance\n\n- Given a live meeting pinned to v1, when a reviewed policy asset publishes v2, then the meeting\n  still resolves v1 and the type list carries both versions.\n- Given a board subject and a bound forum type, when the sitting's chair records a position and\n  the item is decided, then the approval carries one `via='meeting'` decision row per seat.\n- Given a cadence row with a type, when materialise runs twice, then exactly one non-cancelled\n  meeting exists with the bound type and the slot.\n- Given T19F's record, when the governance chain checks, then seven artefacts, both contracts,\n  both Phase-G reviews, B7 read-not-overwritten and the published architecture-artifact exist.\n\n## Capabilities\n\n- B1 Governed tenant assets \u2014 T17's route extended with the policy payload.\n- B2 Deliberated decisions \u2014 T19's meeting reused by the board sitting.\n- B3 Cadence-driven forums \u2014 T10's cadence row bound to a meeting type.\n\n## Plan\n\n`docs/plans/T19F-meeting-bindings.md`, delivered as #682 (plan and artefacts), #683\n(implementation, merged `eb0deb61`) and #684 (live proof and publication).\n",
  "card_id": null,
  "category": "project-brief",
  "content_hash": "e91f7cb6e5958aa71e433fc329d446382cb2a6a93e21178ccfcf3b2e34537686",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T03:30:37.595010Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a210c1-5e0b-73a0-8369-4e02145395e2",
  "kind": "document",
  "meta": {
    "category": "project-brief",
    "content_type": "text/markdown",
    "sha256": "e91f7cb6e5958aa71e433fc329d446382cb2a6a93e21178ccfcf3b2e34537686",
    "size_bytes": 3164,
    "unit": "T19F"
  },
  "parent_document_id": null,
  "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T03:31:44.524272Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 3164,
  "status": "active",
  "title": "T19F \u2014 Project brief",
  "unit": "T19F",
  "updated_at": "2026-11-25T03:31:44.524272Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 2
}
```

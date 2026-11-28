# Project Registers — Decision-forum governance (follow-on)

## Risks

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The gate resolves the sitting's approval through approvals.list. At the 2026-11-27T00:10Z run the row was on the first page; later the same day 592 approvals had been created after it and the un-paged lookup missed it, although the row is present and approved in the record. This unit fixes the instrument: read_approval pages approvals.list (limit 200, following next_cursor) until the row is found. The residual risk is the paging cap and a record that grows past it, which the instrument reports as an absent row rather than a pass.",
      "code": "T19F-R1",
      "created_at": "2026-11-28T07:33:55.777457+00:00",
      "id": "01a22113-31c5-793c-a5f8-7d82f4cc3c0f",
      "impact": 2,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/T19F-meeting-bindings.md#P5"
        ]
      },
      "meta": {
        "owner": "the quality manager (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "high",
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": "The paging fix lands in this unit and the gate re-runs green (13/13) at the fixed head; if the approval is ever reported absent again, check the page cap and the record identity before touching the assertion.",
      "review_at": "2026-12-11",
      "severity": "medium",
      "status": "open",
      "title": "The live gate's approval lookup does not page, so a busy record can hide the sitting's row",
      "updated_at": "2026-11-28T07:33:55.777457+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The gate asserts rows on the programme record through the recorder's transport. A replayed or restored database carries the rows only if the gate's own apply run wrote them there; a fresh database would show them absent. The evidence file names the run and the state file names the ids, so the dependency is visible, but a future reader must not mistake the state file for a fixture.",
      "code": "T19F-R2",
      "created_at": "2026-11-28T07:33:55.91817+00:00",
      "id": "01a22113-3251-74b7-88d5-83f13973c52b",
      "impact": 3,
      "kind": "risk",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": [
          "docs/plans/T19F-meeting-bindings.md#P5"
        ]
      },
      "meta": {
        "owner": "the release manager (conveyor-e2e)"
      },
      "owner_job_id": null,
      "probability": "medium",
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": "Record the record identity (replay container and database) alongside each gate run, and re-run --apply only when the record is intentionally rebuilt; never copy a state file across records.",
      "review_at": "2026-12-11",
      "severity": "medium",
      "status": "open",
      "title": "The live gate depends on the record the transport reaches, not on a hermetic fixture",
      "updated_at": "2026-11-28T07:33:55.91817+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Issues

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The plan (#682\u2013#684) delivered the capability and its live proof, and the MSP/TOGAF/SAFe documents under docs/plans/T19F are contemporaneous. The programme record, however, carried no charter, baseline, contract, register set or Phase-G review for T19F. This pack adds that set at the 2026-11-27 snapshot without rewriting the delivery documents.",
      "code": "T19F-I1",
      "created_at": "2026-11-28T07:33:56.059867+00:00",
      "id": "01a22113-32de-7be4-a634-330c9c038859",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": "This pack is the repair; the receipt at the pinned snapshot is the proof, and the tranche-6 review is the closure forum.",
      "review_at": null,
      "severity": "medium",
      "status": "open",
      "title": "The record carried no lifecycle set for T19F; this pack supplies it retrospectively",
      "updated_at": "2026-11-28T07:33:56.059867+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The plan records that T19F remains active and that its closure review accepts the delivered scope and defers closure to the tranche-6 review. The project row is therefore active at this snapshot, and the pack carries no closure keys. The tranche-6 review is the decision that closes it.",
      "code": "T19F-I2",
      "created_at": "2026-11-28T07:33:56.198854+00:00",
      "id": "01a22113-336a-7405-81b0-6da2e730db16",
      "impact": null,
      "kind": "issue",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": "Close T19F in the tranche-6 review record and cite this deferral; do not close it in the rework factory, which would pre-empt the review.",
      "review_at": null,
      "severity": "low",
      "status": "open",
      "title": "Closure is deferred to the tranche-6 review by the plan's own \u00a710",
      "updated_at": "2026-11-28T07:33:56.198854+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Assumptions

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "The delivery contract judges acceptance against the pinned snapshot; the checker refuses any id that appears nowhere in it. Every judgement in this pack is read from that snapshot, so if it is incomplete or stale for T19F the pack's facts and any re-close move with it.",
      "code": "T19F-A1",
      "created_at": "2026-11-28T07:33:56.338151+00:00",
      "id": "01a22113-33f5-7261-9a00-6ed14aec7571",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The 2026-11-27 rework snapshot is the authoritative record for T19F",
      "updated_at": "2026-11-28T07:33:56.338151+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The plan's \u00a74.4 records that the direct path, the approval primitive, T19's arithmetic, the minutes gate and the retirement flag are untouched. The assumption is that no later slice silently routes the direct path through a sitting; the suite `the_direct_board_decide_path_still_works` is the guard.",
      "code": "T19F-A2",
      "created_at": "2026-11-28T07:33:56.479775+00:00",
      "id": "01a22113-3482-7c6f-a29f-c2a44555b8be",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "The direct architecture.board_decide path stays unchanged",
      "updated_at": "2026-11-28T07:33:56.479775+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The materialisation adds a cadence row lock rather than a stronger unique index, because T19's uq_meeting_live owns the slot rule and permits a second scheduled meeting. The assumption is that no migration later tightens that index without revisiting the materialisation contract.",
      "code": "T19F-A3",
      "created_at": "2026-11-28T07:33:56.618735+00:00",
      "id": "01a22113-350d-784f-9360-311fda17b38a",
      "impact": null,
      "kind": "assumption",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "held",
      "title": "T19's uq_meeting_live remains the slot rule",
      "updated_at": "2026-11-28T07:33:56.618735+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Dependencies

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 2,
  "rows": [
    {
      "baseline_ref": null,
      "body": "T19 supplies the meeting and the session as the decision forum and the outcome arithmetic; T17 supplies the versioned authoring route. The plan's \u00a79 records these as provenance rather than a frozen order, and the tranche-5 review authorised tranche 6, which carries this project.",
      "code": "T19F-D1",
      "created_at": "2026-11-28T07:33:56.760733+00:00",
      "id": "01a22113-359c-760f-9485-5160dc341238",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "met",
      "title": "T19F depends on T19 and T17 and on the tranche-5 authority",
      "updated_at": "2026-11-28T07:33:56.760733+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The tranche-6 exit needs this project's receipt green; the tranche-6 review is the forum that closes it. Nothing after it in the tranche waits on its construction \u2014 the review waits on its receipt, which is the ordering the batch records.",
      "code": "T19F-D2",
      "created_at": "2026-11-28T07:33:56.90391+00:00",
      "id": "01a22113-362b-7086-92c5-fdcc5d6d3963",
      "impact": null,
      "kind": "dependency",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "T19F is the last delivery member of tranche 6",
      "updated_at": "2026-11-28T07:33:56.90391+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Benefits

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 3,
  "rows": [
    {
      "baseline_ref": null,
      "body": "A policy change is a new immutable meeting_type version through the authoring route, and an in-flight meeting keeps the version it pinned. The benefit is an audit property: the policy a decision was taken under can be reconstructed from the meeting's pinned version, which the live gate asserts.",
      "code": "T19F-B1",
      "created_at": "2026-11-28T07:33:57.043806+00:00",
      "id": "01a22113-36b6-78a1-a4f3-bd86a570eab6",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Forum policy changes without mutating history",
      "updated_at": "2026-11-28T07:33:57.043806+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "The sitting's approval carries one approval_decision per qualifying seat with via='meeting', so the decision can be traced to the forum and the chair seat rather than to an unbound board call. The benefit is declared in the plan and asserted by the sitting checks in the live gate.",
      "code": "T19F-B2",
      "created_at": "2026-11-28T07:33:57.183467+00:00",
      "id": "01a22113-3742-7547-868f-2c85efe8c905",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "Board decisions are bound to the forum that took them",
      "updated_at": "2026-11-28T07:33:57.183467+00:00",
      "version": 1
    },
    {
      "baseline_ref": null,
      "body": "Materialising a cadence twice yields one non-cancelled meeting bound to the cadence's type and slot, and the row lock delivers the promise without changing T19's uq_meeting_live. The benefit is idempotence at the cadence boundary that the plan's P3 scenario records.",
      "code": "T19F-B3",
      "created_at": "2026-11-28T07:33:57.324273+00:00",
      "id": "01a22113-37cf-73e7-92a8-2555eeab4e54",
      "impact": null,
      "kind": "benefit",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {},
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "open",
      "title": "One meeting per cadence slot, without a stronger scheduling contract",
      "updated_at": "2026-11-28T07:33:57.324273+00:00",
      "version": 1
    }
  ],
  "source": "msp.json#/register_items"
}
```

## Changes

The record the section derives from, or an explicit absence.

```json
{
  "category": "project-registers",
  "count": 1,
  "rows": [
    {
      "baseline_ref": "01a22113-386c-72ea-a05a-e4d06902e55f",
      "body": "The retrospective MSP/TOGAF/SAFe rework changed this project's record: its artefacts were re-versioned with documents and links, its register was written, and its schedule and cost baselines were taken. This change records that rework against the frozen scope baseline.",
      "code": "CH-T19F-d38a51",
      "created_at": "2026-11-28T07:34:10.428174+00:00",
      "id": "01a22113-6b01-7977-bb8a-236503d38a51",
      "impact": null,
      "kind": "change",
      "links": {
        "cards": [],
        "decisions": [],
        "requirements": []
      },
      "meta": {
        "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
        "impact": {
          "scope": "the retrospective rework's record acts"
        }
      },
      "owner_job_id": null,
      "probability": null,
      "programme_id": null,
      "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
      "response": null,
      "review_at": null,
      "severity": null,
      "status": "approved",
      "title": "The retrospective rework of T19F",
      "updated_at": "2026-11-28T07:34:10.804207+00:00",
      "version": 2
    }
  ],
  "source": "msp.json#/register_items"
}
```

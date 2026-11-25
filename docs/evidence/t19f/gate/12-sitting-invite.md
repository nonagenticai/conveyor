# T19F live gate — the chair seat

Action `meetings.invite`.


```
== write: meetings.invite ==
{
  "delegated_from": null,
  "department_id": null,
  "id": "01a210c8-abed-7041-bc49-fd6654c430fe",
  "invitation_state": "invited",
  "meeting_id": "01a210c8-94ee-73ba-9147-88f2b4729ae0",
  "participant_id": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
  "participant_kind": "app_user",
  "response_state": "none",
  "seat": "chair",
  "version": 1
}
== read-back: meetings.get ==
{
  "agenda": [
    {
      "closed_at": null,
      "defer_reason": null,
      "evidence_refs": [
        {
          "kind": "document",
          "ref": "docs/plans/T19F/architecture-design.md"
        }
      ],
      "id": "01a210c8-97c5-7850-8270-16b37a07d8ce",
      "meeting_id": "01a210c8-94ee-73ba-9147-88f2b4729ae0",
      "outcome": null,
      "outcome_ref": null,
      "participating_departments": [],
      "resolved_id": "01a210c8-9273-7964-9b97-0f3dd810bcf3",
      "resolved_kind": "approval",
      "sequence": 1,
      "version": 1
    }
  ],
  "attendance": [
    {
      "delegated_from": null,
      "department_id": null,
      "id": "01a210c8-abed-7041-bc49-fd6654c430fe",
      "invitation_state": "invited",
      "meeting_id": "01a210c8-94ee-73ba-9147-88f2b4729ae0",
      "participant_id": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "participant_kind": "app_user",
      "response_state": "none",
      "seat": "chair",
      "version": 1
    }
  ],
  "instruments": {
    "action_closure": 0.0,
    "agenda_conversion": 0.0,
    "commitment_slippage": 0,
    "decision_latency_seconds": null,
    "decisions": 0,
    "decisions_per_meeting": 0.0,
    "items": 1,
    "open_items": 1,
    "participant_hours": 0.0,
    "person_hours_per_decision": 0.0,
    "quorum_miss_rate": 0.0
  },
  "meeting": {
    "cadence_slot": null,
    "card_id": "01a210c8-94b3-7ff9-abc1-0016f67329f0",
    "card_name": "T19F live-gate board sitting",
    "closed_at": null,
    "convener_id": "01a210c8-579b-7808-bc32-aef8899b0b7a",
    "convener_kind": "governance_body",
    "decision_rule": "chair",
    "decision_window_end": "2026-12-02T03:38:29.527298Z",
    "id": "01a210c8-94ee-73ba-9147-88f2b4729ae0",
    "meeting_type": "t19f_live_gate_forum",
    "meeting_type_name": "T19F live-gate forum v1",
    "meeting_type_version": 1,
    "opens_at": "2026-11-25T03:38:29.527298Z",
    "retirement_candidate": false,
    "status": "live",
    "version": 2
  },
  "meeting_type": {
    "agenda_sources": [],
    "allowed_outcomes": [
      "decision",
      "commitment",
      "action",
      "information",
      "deferred"
    ],
    "convener_authority": {},
    "decision_rule": "chair",
    "decision_window_seconds": 604800,
    "evidence_minimum": 0,
    "id": "01a210c2-a0f2-77d6-b5cd-f90e4d257131",
    "key": "t19f_live_gate_forum",
    "name": "T19F live-gate forum v1",
    "purpose": "",
    "quorum": 1,
    "required_participation": [],
    "status": "active",
    "timebox_seconds": 3600,
    "version": 1
  },
  "positions": []
}
```

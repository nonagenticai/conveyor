# T19F live gate — the policy v2 draft

Action `authoring.draft`, kind `policy`, key `t19f-live-gate-policy-v2`, payload carrying meeting_type `t19f_live_gate_forum`.


```
== write: authoring.draft ==
{
  "depends_on": [],
  "id": "01a210c3-7c34-7338-89f2-6c7a6e40bea7",
  "intake_id": "01a210c3-68d9-7caf-b5af-8ea62a0f422d",
  "key": "t19f-live-gate-policy-v2",
  "kind": "policy",
  "payload": {
    "meeting_type": {
      "agenda_sources": [],
      "decision_rule": "majority",
      "evidence_minimum": 0,
      "meeting_type_key": "t19f_live_gate_forum",
      "name": "T19F live-gate forum v2",
      "purpose": "The v2 published through the authoring route by the T19F live gate.",
      "quorum": 1,
      "required_participation": []
    }
  },
  "published_at": null,
  "published_meeting_type_id": null,
  "review_approval_id": null,
  "status": "draft",
  "version": 1
}
== read-back: authoring.assets ==
{
  "items": [
    {
      "depends_on": [],
      "id": "01a210c3-7c34-7338-89f2-6c7a6e40bea7",
      "intake_id": "01a210c3-68d9-7caf-b5af-8ea62a0f422d",
      "key": "t19f-live-gate-policy-v2",
      "kind": "policy",
      "payload": {
        "meeting_type": {
          "agenda_sources": [],
          "decision_rule": "majority",
          "evidence_minimum": 0,
          "meeting_type_key": "t19f_live_gate_forum",
          "name": "T19F live-gate forum v2",
          "purpose": "The v2 published through the authoring route by the T19F live gate.",
          "quorum": 1,
          "required_participation": []
        }
      },
      "published_at": null,
      "published_meeting_type_id": null,
      "review_approval_id": null,
      "status": "draft",
      "version": 1
    }
  ]
}
```

# T19F live gate — publishing the policy v2

Action `authoring.publish` for asset 01a210c3-7c34-7338-89f2-6c7a6e40bea7; the hook writes the next immutable `t19f_live_gate_forum` version.


```
== write: authoring.publish ==
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
  "published_at": "2026-11-25T08:37:59.051034Z",
  "published_meeting_type_id": "01a211da-c391-7867-86dd-d22f9cfed5d5",
  "review_approval_id": "01a211d8-001f-7296-bbc1-ccb13835a3ef",
  "status": "published",
  "version": 3
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
      "published_at": "2026-11-25T08:37:59.051034Z",
      "published_meeting_type_id": "01a211da-c391-7867-86dd-d22f9cfed5d5",
      "review_approval_id": "01a211d8-001f-7296-bbc1-ccb13835a3ef",
      "status": "published",
      "version": 3
    }
  ]
}
```

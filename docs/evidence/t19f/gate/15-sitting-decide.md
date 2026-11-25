# T19F live gate — the sitting decides

Action `architecture.board_sit_decide`; the item closes through T19's close_item_tx.


```
== write: architecture.board_sit_decide ==
{
  "agenda_item_id": "01a210c8-97c5-7850-8270-16b37a07d8ce",
  "approval_id": "01a210c8-9273-7964-9b97-0f3dd810bcf3",
  "approval_status": "approved",
  "counted_positions": 1,
  "decisions_written": 1,
  "meeting_id": "01a210c8-94ee-73ba-9147-88f2b4729ae0",
  "quorum": 1
}
== read-back: architecture.board_get ==
{
  "charter": "Decides the T19F live-gate subject in a sitting.",
  "commissioning_authority": "",
  "decision_limits": {},
  "id": "01a210c8-579b-7808-bc32-aef8899b0b7a",
  "key": "t19f-live-gate-board",
  "kind": "architecture_board",
  "meeting_type_key": "t19f_live_gate_forum",
  "members": [],
  "name": "T19F live-gate board",
  "status": "active",
  "version": 2
}
```

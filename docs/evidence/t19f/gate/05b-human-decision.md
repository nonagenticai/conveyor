# T19F live gate — the human review decision

The review gate was decided by the human `app_user` FR Test in the signed-in console
(`https://conveyor.nonagentic.ai/approvals`) on 2026-11-25. The console posts the same
`approvals.decide` action the recorder reads back; the principal is the human seat, so the
decision row is `decided_by_kind='human'` with `via.channel='web'`. The earlier gate
(`01a210c3-9007…`) stays in the record: its only decision was the admin CLI's `service`,
which `authoring.review` refuses by design, and an approved gate takes no second decision.

`approvals.list` read back through the deployed action surface (filtered to
`gate='authoring.review'`; the full response is the recorder's):

```json
[
  {
    "approvals": 1,
    "card_id": null,
    "created_at": "2026-11-25T08:34:57.958362Z",
    "decisions": [
      {
        "approver": "FR Test",
        "approver_id": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
        "decided_at": "2026-11-25T08:36:58.496360Z",
        "decided_by_kind": "human",
        "decision": "approve",
        "note": "",
        "via": {
          "channel": "web"
        }
      }
    ],
    "description": "Human review of the T19F live-gate policy asset (fresh gate; the prior gate 01a210c3-9007 was service-decided)",
    "expires_at": null,
    "gate": "authoring.review",
    "id": "01a211d8-001f-7296-bbc1-ccb13835a3ef",
    "kind": "gate",
    "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
    "requested_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
    "requested_by_label": "conveyor-approver",
    "required_decisions": 1,
    "run_id": null,
    "status": "approved",
    "target": "01a210c3-7c34-7338-89f2-6c7a6e40bea7",
    "version": 2
  },
  {
    "approvals": 1,
    "card_id": null,
    "created_at": "2026-11-25T03:33:01.322357Z",
    "decisions": [
      {
        "approver": "conveyor-approver",
        "approver_id": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
        "decided_at": "2026-11-25T03:33:06.447890Z",
        "decided_by_kind": "service",
        "decision": "approve",
        "note": "Reviewed against docs/plans/T19F/architecture-design.md",
        "via": {
          "channel": "internal"
        }
      }
    ],
    "description": "Review the T19F live-gate policy asset 01a210c3-7c34-7338-89f2-6c7a6e40bea7",
    "expires_at": null,
    "gate": "authoring.review",
    "id": "01a210c3-9007-75f4-927c-d1dc32f54c6b",
    "kind": "gate",
    "project_id": "01a210ba-b6cb-76cc-82a3-a2b45cfb498f",
    "requested_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
    "requested_by_label": "FR Test",
    "required_decisions": 1,
    "run_id": null,
    "status": "approved",
    "target": "01a210c3-7c34-7338-89f2-6c7a6e40bea7",
    "version": 2
  }
]
```

The accepted gate is `01a211d8-001f-7296-bbc1-ccb13835a3ef`: requested by the distinct
`conveyor-approver` seat, approved by FR Test (`01a1cf1e-09dd-7580-99cc-de395f3e86cc`), one
of one decisions, `decided_by_kind='human'`, `channel='web'`.

# T19 delivery contract — the Board's decision

Action `architecture.board_decide` by the Architecture Board on contract 01a20cea-91bb-77f4-a880-044c90bb5f0e, citing the standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3.


```
== write: architecture.board_decide ==
{
  "approval_id": "01a20cea-a68f-7e08-8d0c-399759ef16e0"
}
== read-back: architecture.board_get ==
{
  "charter": "The board governs the conveyor programme's architecture. It adopts the principles catalogue and the tailored ADM profile, approves architecture contracts, and accepts or rejects waivers. It cannot approve a gate it requested and cannot waive a deviation it owns. It meets at each tranche review and on a contract request. Quorum: the chair. Every decision is recorded with the evidence it was taken on.",
  "commissioning_authority": "The programme owner (the sponsor and decision-maker; P0's governance row)",
  "decision_limits": {
    "approves": [
      "principles_catalog",
      "adm_profile",
      "architecture_contract",
      "architecture_waiver"
    ],
    "cannot": [
      "approve a gate it requested",
      "waive a deviation it owns"
    ],
    "quorum": "the chair"
  },
  "id": "01a1f2c0-b22a-7e27-a6cc-1a8e48db327e",
  "key": "conveyor-architecture-board",
  "kind": "architecture_board",
  "members": {
    "chair": "programme owner",
    "note": "positions, not persons; the programme architect position is held by the maintainer sessions under the owner's authority",
    "positions": [
      "programme architect",
      "domain architect",
      "delivery lead"
    ]
  },
  "name": "Conveyor Architecture Board",
  "status": "active",
  "version": 2
}
```

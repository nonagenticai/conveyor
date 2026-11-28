# Delivery Contract — Board — projects, boards, lists, cards, labels, comments, tasks, custom fields, webhooks, SSE

## Commitments

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a219d8-a412-77fa-8083-8b34f2db44f7",
      "field": "commitments",
      "kind": "contract_field",
      "title": "commitments: S05 commits to the board slice - projects, boards, lists, cards, labels, comments, tasks, custom fields, webhooks and SSE - on one action spine, with replay fidelity at 1,000 cards and 10 lists and zero deltas, one winner under a concurrent If-Match write, the webhook payload shape, SSE delivery live and on replay, and no second store for comments, board memberships or attachments.",
      "value": "S05 commits to the board slice - projects, boards, lists, cards, labels, comments, tasks, custom fields, webhooks and SSE - on one action spine, with replay fidelity at 1,000 cards and 10 lists and zero deltas, one winner under a concurrent If-Match write, the webhook payload shape, SSE delivery live and on replay, and no second store for comments, board memberships or attachments."
    }
  ],
  "source": "computed:contract_field"
}
```

## Compliance method

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a219d8-a412-77fa-8083-8b34f2db44f7",
      "field": "compliance",
      "kind": "contract_field",
      "title": "compliance: S05 is assessed clause by clause against the slice gate of `acceptance/programme/S05.toml` and plan \u00a77. The six properties are: (a) replay fidelity \u2014 1,000 cards/10 lists with zero deltas; (b) one winner \u2014 one 200 and one 412 with the current representation, version +1; (c) 1k load \u2014 p95 \u2264 120 ms at 20 rps, five pages \u2264 500 ms; (d) webhook shape \u2014 {event,data,prevData,user} with the register event and *Out payloads; (e) SSE \u2014 card.moved \u2264 200 ms live and on replay; (f) no second store \u2014 one comment, one board_membership, no card_membership, no attachment. Each property is carried by a named test or command in plan \u00a77. The snapshot records no Phase-G review and no run of these properties; this rework supplies the seven A\u2013D artefacts, this contract, the Phase-G review and the closure review, records the level non_conformant, and re-closes on the P0 checks of the acceptance instrument. The closure gate is decided by an approver other than its requester under the standing delegation.",
      "value": "S05 is assessed clause by clause against the slice gate of `acceptance/programme/S05.toml` and plan \u00a77. The six properties are: (a) replay fidelity \u2014 1,000 cards/10 lists with zero deltas; (b) one winner \u2014 one 200 and one 412 with the current representation, version +1; (c) 1k load \u2014 p95 \u2264 120 ms at 20 rps, five pages \u2264 500 ms; (d) webhook shape \u2014 {event,data,prevData,user} with the register event and *Out payloads; (e) SSE \u2014 card.moved \u2264 200 ms live and on replay; (f) no second store \u2014 one comment, one board_membership, no card_membership, no attachment. Each property is carried by a named test or command in plan \u00a77. The snapshot records no Phase-G review and no run of these properties; this rework supplies the seven A\u2013D artefacts, this contract, the Phase-G review and the closure review, records the level non_conformant, and re-closes on the P0 checks of the acceptance instrument. The closure gate is decided by an approver other than its requester under the standing delegation."
    }
  ],
  "source": "computed:contract_field"
}
```

## Approval

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_at": "2026-11-26T21:52:38.199608+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a219d8-a4a8-74c7-afd1-3112e27027e4",
      "kind": "contract_approval",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "approved",
      "title": "approval 01a219d8-a4a8-74c7-afd1-3112e27027e4 \u2014 approved"
    }
  ],
  "source": "computed:contract_approval"
}
```

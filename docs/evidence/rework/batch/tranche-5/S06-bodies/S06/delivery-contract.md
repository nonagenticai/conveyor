# Delivery Contract — Control plane — claims, runs, approvals, andon, comms and notifications

## Commitments

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a21a39-c074-7cac-a6d0-ca18a59d30b7",
      "field": "commitments",
      "kind": "contract_field",
      "title": "commitments: S06 commits to the control plane \u2014 claims, runs, approvals, andon, comms and notifications \u2014 as one durable boundary: one conditional UPDATE per claim with a monotonic claim_fence, one idempotent job row per period in place of fr's timers, one approvals.decide path with required_decisions and decided_by_kind shared by web and Telegram, an andon_state singleton that pauses the poller and refuses runs, and one message plus N delivery rows with a bounded retry.",
      "value": "S06 commits to the control plane \u2014 claims, runs, approvals, andon, comms and notifications \u2014 as one durable boundary: one conditional UPDATE per claim with a monotonic claim_fence, one idempotent job row per period in place of fr's timers, one approvals.decide path with required_decisions and decided_by_kind shared by web and Telegram, an andon_state singleton that pauses the poller and refuses runs, and one message plus N delivery rows with a bounded retry."
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
      "contract_id": "01a21a39-c074-7cac-a6d0-ca18a59d30b7",
      "field": "compliance",
      "kind": "contract_field",
      "title": "compliance: S06 is assessed clause by clause against the slice gate of `acceptance/programme/S06.toml` and plan \u00a77. The properties are: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 cards.move(done) with run succeeded and one audit, invocation and event row each; (b) dual control \u2014 required_decisions=2, one approve stays pending, the same approver again is 409, a second closes; (c) retry \u2014 a killed run is reaped as failed/crashed and the next create is attempt=2; (d) andon \u2014 engaged gives 409 and a 60 s pending run, released resumes within one poll; (e) delivery retry \u2014 a stub 500 lands attempts=5, failed, one row; (f) no timers \u2014 the setInterval scan is empty and every cadence is a schedule row; (g) human vs agent \u2014 decided_by_kind differs for one human and one agent decision through approvals.decide. Each property is carried by a named test in plan \u00a77. The snapshot records no Phase-G review and no run of these properties; this rework supplies the seven A\u2013D artefacts, this contract, the Phase-G review and the evidence output, records the level non_conformant, and keeps the project active until the instrument runs.",
      "value": "S06 is assessed clause by clause against the slice gate of `acceptance/programme/S06.toml` and plan \u00a77. The properties are: (a) golden path \u2014 card \u2192 claim \u2192 run \u2192 gate \u2192 decide \u2192 cards.move(done) with run succeeded and one audit, invocation and event row each; (b) dual control \u2014 required_decisions=2, one approve stays pending, the same approver again is 409, a second closes; (c) retry \u2014 a killed run is reaped as failed/crashed and the next create is attempt=2; (d) andon \u2014 engaged gives 409 and a 60 s pending run, released resumes within one poll; (e) delivery retry \u2014 a stub 500 lands attempts=5, failed, one row; (f) no timers \u2014 the setInterval scan is empty and every cadence is a schedule row; (g) human vs agent \u2014 decided_by_kind differs for one human and one agent decision through approvals.decide. Each property is carried by a named test in plan \u00a77. The snapshot records no Phase-G review and no run of these properties; this rework supplies the seven A\u2013D artefacts, this contract, the Phase-G review and the evidence output, records the level non_conformant, and keeps the project active until the instrument runs."
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
      "approved_at": "2026-11-26T23:38:42.414513+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a21a39-c0f3-79f7-a78f-5ebc08f411a2",
      "kind": "contract_approval",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "approved",
      "title": "approval 01a21a39-c0f3-79f7-a78f-5ebc08f411a2 \u2014 approved"
    }
  ],
  "source": "computed:contract_approval"
}
```

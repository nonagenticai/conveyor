# Token spend per accepted deliverable — pre-transition baseline

**Measured** from the full-fidelity snapshot at `ec799ee1` (2026-11-21T23:27:35Z, live tenant
`nonagentic`, migration 67), built by the branch binary from the restored production copy. T5 landed
on `issue/526-u5a` on 2026-11-22 but is not deployed, so this is the pre-transition baseline.

## Definition

An **accepted deliverable** is a published output document attached to a project —
`artefacts.json.documents` with `publishable = true`. The snapshot has **145**. The ratio is
portfolio-level, not a per-run causal attribution: a run may produce no output, an output may
predate the inference ledger, and project/card attribution exists for only 3 projects and 5 cards.
The post-transition measurement must use the same definition over a window at least as long.

## Measured

| Measure | Value |
|---|---:|
| Inference calls | 609 |
| Prompt / completion tokens | 9,374,195 / 84,273 |
| **Total tokens** | **9,458,468** |
| Cost (priced calls; 81 unpriced) | $1.318791 |
| **Tokens per published output** | **65,231** |
| Cost per published output | $0.0091 |
| Tokens per closed project (26 closed) | 363,787 |
| Tokens per run (371 runs) | 25,495 |
| Tokens per succeeded run (2 succeeded) | 4,729,234 |
| Run failure rate | **98.65%** (366 / 371) |
| Cache-miss / unknown / non-OK calls | 606 / 3 / 0 |

By caller: `agent_run` 604 calls / 9,457,017 tokens; `other` 5 calls / 1,451 tokens.

## Why this is a ceiling, not a cost

366 of 371 runs failed. The tokens were spent overwhelmingly on work that produced no accepted
deliverable. A per-deliverable improvement that simply rejects more work would look good on the
primary ratio and bad on the secondary one; both are therefore recorded.

The frozen `a33e6562` pack reported 165 outputs; this fresh snapshot reports 145. The fresh
snapshot is authoritative — the frozen pack's outputs projection was incomplete.

## Proposed target (owner confirmation required)

- **≤ 52,185 tokens per published output** (−20% against the baseline).
- **Run failure rate ≤ 20%**, measured over the same post-transition window.

The six live benefit profiles currently say *"not measured — no instrument exists"*. The token KPI
is not yet one of them; U6 must add or amend a profile and record the target and the post-transition
measurement at the same `measured_at` as this baseline. The committed data is
`docs/evidence/benefits/token-spend-baseline.json`.

## Reproduce

```
PROGRAMME_RECORD_BIN=<branch conveyor> scripts/snapshot-live.sh <out>   # read-only live projection
jq '.inference.totals' <out>/data/measure.json
jq '[.documents[] | select(.publishable)] | length' <out>/data/artefacts.json
jq '[.runs[] | .status] | group_by(.) | map({(.[0]): length}) | add' <out>/data/work.json
```

The benefit row and this file must keep the same `measured_at`; the lane owner announces the row
date.

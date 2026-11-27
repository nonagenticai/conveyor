# Tranche 3 — the live gate measurement

Measured 2026-11-27T07:09Z before the review act. This is the evidence the strict-live rule
requires: no tranche-3 member may carry an open P0 on the live register.

## The measurement

- **Snapshot:** the deployed engine's binary, copied to the admin host and run against the primary
  (`127.0.0.1:5432`, read-only), pinned to `git_sha 740725180a17531796d981010ea20b1a860d3026`
  (the #797 head, then #1 in the merge queue). 15 data files, 333 platform outputs,
  `SNAP_EXIT=0`.
- **Register:** `python3 scripts/audit-programme-conformance.py --snapshot <data> --out <audit>
  --findings <ledger> --report --rework-out <rework.json>` → 198 packages (196 open, 2
  suppressed), 25 open P0.
- **Ledger:** `origin/issue/797-tranche3-p0-residuals:docs/evidence/v1-conformance-findings.json`
  at `740725180` — the two tranche-3 receipts are in it.

## Tranche 3 (members S04, T13, T3)

| Finding | Subject | Severity | Status |
|---|---|---|---|
| `v1-w4a-1` | T13 | P0 | **open — owner-deferred**; declared as a condition in the review |
| `v1-w2c-3` | T3 | P0 | resolved by the #797 receipt |
| `v1-w4a-2` | T13 | P0 | resolved by the #797 receipt |
| `closed_without_closure_review` | T3 | P0 | absent — the closure review is live (`documents.get 01a219fb…`, published 2026-11-27T22:31:10Z) |

No other P0 names S04, T13 or T3. **The strict-live gate is clear.**

## Observed beyond tranche 3 (not this review)

The same register carries `closed_without_closure_review:T14` as a P0. T14 is a tranche-4 member,
so the tranche-4 residual unit must publish T14's closure review as well as T16's.

## What this is not

- It is not the review. `tranches.review` runs only after #811 merges; its own receipt
  (`tranche-3.md`) names the review id and every read-back.
- The audit files are not committed. The command and the counts above are what is reproducible.

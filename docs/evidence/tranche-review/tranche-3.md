# Tranche 3 review — receipt

## Unit

- **Issue:** conveyor#852 — the promotion record and the tr3 exit.
- **Lane:** branch `issue/852-promote-record`, worktree `.worktrees/issue-852`; claim commit
  `6697072f9`, claim nonce `3F78F644-1954-4EC6-A35E-68F93D72C173`.
- **Head at skeleton:** `b5cb473d8` (P1 re-applied), on main merged at `e9872c334` — the #921
  migration fix plus the tranche-6 landing `b907f3f9c`.
- A `B778126A-…` nonce also present in this history belongs to #921, not to this lane.

## P1 — the ledger on merged main

- 187 `resolved_by.commit` values, every one an ancestor of merged main.
- The lane's 186 receipts: 171 re-pointed to the #526 squash `fa0ba44ac` (106 distinct commits), 15 kept, 0 reopened (`cb5cb5558`).
- Main's later S10 receipt (the tranche-6 landing) cited a pre-squash commit; re-pointed to `b907f3f9c`, the commit that added its cited test (`b5cb473d8`).

## P2 — the promotion journal and its curation

- main's journal: 498 entries; plus 16 live-card entries = **514**; `--check` green.
- Curated journal `docs/evidence/tranche-review/journal-curated.jsonl`: **476** entries after
  excluding 38 staging-lifecycle acts — the 6 reopens; the T9/T10/T16/T7/T4/S04 close chains
  (including the T16 card's done chain, both the `:` and `/` natural-key forms, and the
  `reconstructed/…-tN` input escapes); the T13 `projects.close`. The T13/T3 closure and
  destructive gates are **kept**; the closes return as direct acts in P2.5.

## P2.5 — the tr3 architecture and lifecycles

Direct acts, ids pending:

| act | id |
|---|---|
| T3 delivery contract | `<action-id>` |
| T3 board decision | `<action-id>` |
| T3 `contracts_approve` | `<action-id>` |
| T3 Phase-G review | `<action-id>` |
| T13 schedule baseline | `<action-id>` |
| T13 schedule-baseline decide | `<action-id>` |
| T3 schedule baseline | `<action-id>` |
| T3 schedule-baseline decide | `<action-id>` |
| T13 close | `<action-id>` |
| T3 close | `<action-id>` |

## P3 — the evidence

- Inputs: `docs/evidence/tranche-review/inputs/` — six files.
- Commands: `/tmp/conv852-verify.sh` — `verify-project`, `check-tranche-exit`, `audit`. Their
  outputs are recorded at the live pass.

## P4 — the review

- Decision: **`continue`** for tranche 3.
- Made under delegation `01a2021d-cca2-757c-bfc2-21bd4e4b6d85`, authorised by the owner
  2026-11-28.
- Review row `<action-id>`; gate `<action-id>`; published document `<action-id>`.

## Deviations

- Skeleton: live-act ids are `<action-id>` placeholders, filled after the acts; no id here is
  invented.
- The curated journal and the six input files are committed to this lane with the receipt.

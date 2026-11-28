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
- Curated journal `docs/evidence/tranche-review/journal-curated.jsonl`: **472** entries after
  excluding 42 staging-lifecycle acts — the 6 reopens; the T9/T10/T16/T7/T4/S04 close chains
  (including the T16 card's done chain, both the `:` and `/` natural-key forms, and the
  `reconstructed/…-tN` input escapes); the T13 `projects.close`; and the four tr3 review acts
  (`tranche-review-gate:3`, `tranche-review-decide:3`, `tranche-review:3`,
  `tranche-authorise:4`), whose input cites the staging snapshot `9d5404d1…` — the review is a
  live decision on the P3 evidence and must not be replayed from the staging record. The T13/T3
  closure and destructive gates are **kept**; the closes and the review return as direct acts in
  P2.5/P4.

## P2.5 — the tr3 architecture, lifecycles and v4 rework

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
| T13 cost baseline | `<action-id>` |
| T13 cost-baseline decide | `<action-id>` |
| T3 cost baseline | `<action-id>` |
| T3 cost-baseline decide | `<action-id>` |
| T13 registers (risk/assumption/dependency) | `<action-ids>` |
| T3 registers (risk/assumption/dependency) | `<action-ids>` |
| T3 retrospective ADM artefacts (7, Phase A–D) | `<action-ids>` |
| T13 close | `<action-id>` |
| T3 close | `<action-id>` |

Owner decision 2026-11-28 (this session): the tr3 rework is delivered **inside this lane** —
the 12 v4 delivery documents (user-stories, definition-of-ready, definition-of-done,
acceptance-criteria for S04/T13/T3), the T3 Phase A–D retrospective artefacts, the cost
baselines and register rows the close gate requires, and S04's closure-outputs declaration —
because standards v4 (main `232c0339d`) made the tranche exit red for all three members and a
red exit records `pause`, not `continue`. The bodies are in
`docs/evidence/tranche-review/inputs/` (`t3-artefacts.json`, `delivery/*.md`).

## P2.4 — the replay proof on a pristine restore

The restore was re-created from the 81 dump (`/tmp/conv852-conveyor-restore.sql`, the 02:41Z
`pg_dumpall` section) and migrated to schema **110** with the post-fix binary, then replayed
twice through `promote-record.py --apply --local-bin /tmp/conv852-bin.sh`:

- pass 1: **268 created, 204 skipped, 0 failed** (the 204 are rows the 81 dump already carries);
- pass 2: **0 created, 472 skipped, 0 failed** — idempotent.

Three natural keys (`project:closure:<T13>`, `project:closure:<T3>`, `retro:tranche-3`) record the
action's output id on creation and the resolved row id on a skip, so their `production_id`
differs between passes; no row is duplicated (one closure gate per project; five distinct retro
findings). Recorded, not a defect.

## P3 — the evidence

- Inputs: `docs/evidence/tranche-review/inputs/` — eight files (the T3 contract, board decision,
  contract approval, Phase-G review, the two schedule baselines, the two closes).
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

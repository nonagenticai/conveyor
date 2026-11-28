# Tranche-4 review receipt — 2026-11-26

- **Decision:** `continue`; tranche 4 (`01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce`) is **closed** and
  tranche 5 (`01a1e9e0-fe45-77e0-993a-d571fecc5ec1`) is **authorised**.
- **Review row:** `01a21a10-ecb6-70e7-87d5-daa40f5154ac`, reviewed by `01a1e8a7-8b23-76a9-9114-aafb95f7a545`
  as a rule decision under the delegation `01a2021d-cca2-757c-bfc2-21bd4e4b6d85`.
- **Gate:** approval `01a21a10-ec32-7400-a452-d6930b7231a5` (`tranches.review`, target
  `tranche/01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce/review`), requested by
  `01a1e607-17d9-77d3-98d3-a43a87c4633d` and decided by `01a1e8a7-8b23-76a9-9114-aafb95f7a545`.
- **Evidence (at the pinned head `18276de5`):**
  - Ten green member receipts — `check-tranche-exit.py --tranche 4 --snapshot …` exit 0:
    S05 16 checks, S07 18, S09 18, T10 18, T14 16, T16 18, T18 16, T19 18, T4 19, T9 18; zero
    failures each (`docs/evidence/rework/<key>.json`, batch journal
    `docs/evidence/rework/batch/tranche-4c.jsonl`).
  - The three approved programme baselines, seven benefit profiles (B1–B7), and the ten members'
    22 risk rows, cited as ids in the review evidence.
  - The tranche-4 cadences (PI `01a202a1-dbb9…`, refinement `01a202a2-2705…`, demo
    `01a202a1-f4ee…`, retro `01a202a2-0e3f…`) and the published PI plan/PI objectives documents;
    the PI gate approval `01a216d0-e05d-79e9-b3db-0e3539030094`.
  - T14's live gate executed 2026-11-26: six published toolkits with a probe and a live call each,
    the scientific scale receipt, the zero-use report and the outage-unbind receipt
    (`docs/evidence/t14/live-gate.md`); finding `v1-w3c-2` resolved at `e8bb527d1`.
- **Document:** `Tranche review — tranche 4 (2026-11-26)`, `01a21a10-da03-717a-82b5-c20f5885dad6`,
  category `tranche-review`, unit `tranche-4`, published 2026-11-26T22:54:06Z.
- **Raw acts:** this directory — `01-compose-review-doc.json`, `02-publish-review-doc.json`,
  `03-approval-request.json`, `04-approval-decide.json`, `05-tranches-review.json`.

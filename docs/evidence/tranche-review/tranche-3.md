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

Direct acts (recorded live at the deployed head `e6c36e66`, 2026-11-28):

| act | id |
|---|---|
| T3 delivery contract | `01a2221c-64a0-7be3-ad8a-592bed225d11` |
| T3 board decision | `01a2221c-687a-72b4-83af-85eeeff5685a` |
| T3 `contracts_approve` | the contract `01a2221c-64a0-…` went live; the approve acts on the contract row, so no separate id |
| T3 Phase-G review | `01a2221c-78ad-75bf-8fb7-69678a50f6b8` |
| T13 schedule baseline / decide | `01a2221c-7d0b-7bf2-8546-a4e768b13dac` / `01a2221c-7d15-72b7-9a13-146fb0589fd7` |
| T13 cost baseline / decide | `01a2221c-853d-7198-bbaa-24fbe0a1ed77` / `01a2221c-8543-72fd-86ab-d116681b5e78` |
| T3 schedule baseline / decide | `01a2221c-8cdc-7acd-926d-a98ed7c8d8a6` / `01a2221c-8ce5-7d61-955a-54ad92a58026` |
| T3 cost baseline / decide | `01a2221c-9555-7b2a-9fdf-61dbf4798087` / `01a2221c-9563-7c57-9e38-0f031e399ee0` |
| T13 registers (risk/assumption/dependency and two more) | `01a2221c-a0d3-787e-b484-8d0b8231b14f`, `01a2221c-a426-7138-9a9e-0aa9ccc65fc7`, `01a2221c-a800-7903-84ad-ba5e1ce3a037`, `01a2221c-abdf-70e7-9268-0c4c69dad37e`, `01a2221c-af74-7c69-8b71-bb08b0698386` |
| T3 registers (risk/assumption/dependency) | `01a2221c-b67f-774e-a2fa-4809d6cc636f`, `01a2221c-ba87-7fbc-9919-e52cd58b75d4`, `01a2221c-bdf9-710f-bcd0-f79c34168aa4` |
| T3 retrospective ADM artefacts (7, Phase A–D) | `01a2221c-c33a-787f-b445-db3585b868a5`, `01a2221c-cdb2-74d3-9f96-154c0acf2833`, `01a2221c-d8fa-7fc3-b564-0e38fe0224d6`, `01a2221c-e4e8-7d6a-82dd-ba6a75afacaa`, `01a2221c-f152-765b-a8cb-bba1608565f7`, `01a2221c-fd26-7492-9d5b-e17939da5c46`, `01a2221d-0873-79d6-bc89-ffab6fc8ddfc` |
| T3 scope baseline (the rework's) | `01a2220d-cf67-7aeb-ac42-16d238a32b24` / decide `01a2220d-cf72-71b1-8489-9e8f45a81883` |
| S04 closure gate / `projects.close` gate | `01a22223-959b-79db-8043-235858b5e36a` / `01a22223-a497-7097-ad1f-f269c1c532cf` |
| T13 closure gate / `projects.close` gate | `01a2220e-5c02-7e23-9229-1c4aa60c0df9` / `01a2220e-6f43-7016-b5e7-24d5e08ca3ca` |
| T3 closure gate / `projects.close` gate | `01a22214-f70a-7747-bbe9-0f712f7cf144` / `01a22215-0a1f-75bf-ab85-9e0d3c1786f0` |

The v4 documents the close gate reads, recorded live:

| set | ids |
|---|---|
| The 7 authored analysis/design documents (`runbook/conv852-supplement.sh`) | S04 `01a2221d-284f-702d-93a2-7fcadb05abfa` (inception), `01a2221d-36b2-72cd-9963-3263127c57e2` (analysis), `01a2221d-4614-71fa-94a8-1e4541f8f6b1` (design); T13 `01a2221d-5519-7656-9b83-a8512262dc58` (analysis); T3 `01a2221d-64f6-75fc-8446-6482eba2c0e7` (inception), `01a2221d-7417-7254-9c92-3043456f3e55` (analysis), `01a2221d-8373-7582-9602-4d647510b420` (design) |
| The 12 delivery documents (`runbook/conv852-delivery.sh`) | S04 `01a2221d-abb1-7522-bcdf-f5031d58de2e`, `01a2221d-ba59-7538-95e1-f0fd8ac043cd`, `01a2221d-c709-7327-bd86-bbc382819379`, `01a2221d-d3a3-70bc-bcfd-a5a6f84e7219`; T13 `01a2221d-e039-7003-8a06-658b815cfa9e`, `01a2221d-ee68-75c4-9d20-d3370b30ad4c`, `01a2221d-fea3-71c9-8cb6-fc86cd9f2817`, `01a2221e-0dce-75a2-88e7-5e428ebadfca`; T3 `01a2221e-1dcc-705d-ad8b-2b5166349c9b`, `01a2221e-2ed7-717a-8dc6-ca4468c2bfc3`, `01a2221e-3cb5-71c7-98bf-5fb3005fc4c1`, `01a2221e-4a4f-72a2-a772-c67907b5523f` |
| The v4 lifecycle rework (`scripts/rework-project.py`) | S04 **127 acts** (reopened and re-closed), T13 **183 acts**, T3 **104 acts**; the acts are in `docs/evidence/rework/<KEY>/transcript.jsonl` |
| The conformance baseline acceptances | `01a2222f-6014-7137-910d-2021dc0ce59a` (175 entries at the pre-review register) then `01a22231-f3ba-720c-b0e2-59b1ae1f4bd7` (165 entries after the review resolved 10 tranche-4 P0s) |
| The tranche-3 review | approval `01a22231-3f22-734a-b99f-f4f2f9fe1fc2`; tr3 row `01a1e9e0-eb54-74d8-a2b5-44fb2b87eb52` closed with `review_at=2026-11-28T12:46:24.430740Z`; tr4 authorised under review `01a22231-474d-7ce9-bc57-8bb7c99c2dbb`; the review document `01a22231-50dc-7622-b366-62d26d796dd6` published at `2026-11-28T12:46:29.116452Z` |

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

The fresh restore was rebuilt a second time with the live role shape (`conveyor_migrate` without
SUPERUSER/BYPASSRLS, exactly as production): migrations then behave as they do live — 0108's
backfill no-ops under forced RLS, which is why live tranches 1 and 2 carry a null `review_at`.
Replaying on it reproduced **268 / 204 / 0** and **0 created** exactly.

## P2.6 — the live replay at the deployed head

The rehearsal CLI was recreated from the deployed image (`e6c36e66`, digest `31f94da4…`); #928
changed no compiled surface (only CI, scripts and data), so the rehearsal and the live engine are
the same code path. Passes against the live record:

- pass 1: **267 created, 204 skipped, 1 failed** — `cadence:tranche-5/pi/meta` refused
  `400 If-Match and _version name different preconditions`: the payload carried the staging
  row's `_version` while the replay sends the live row's version as the If-Match. Fixed in
  `scripts/promote-record.py` (`strip_stale_version`, applied to every If-Match action, with a
  self-test); restore pass 1 had not exposed it because the live row's version happened to equal
  the staged one.
- pass 2 (after the fix): **1 created, 471 skipped, 0 failed**, `complete=true`.

## P3 — the evidence

- Inputs: `docs/evidence/tranche-review/inputs/` — the T3 contract, board decision, contract
  approval, Phase-G review, the schedule and cost baselines, the two closes, the two register
  sets, `t3-artefacts.json`, and `delivery/` (the 12 authored v4 delivery documents).
- Tooling defect found and fixed on this lane (`f20389980`): `compose-artefact-bodies.py` and
  `publish-artefact-bodies.py` called `gen.applies()` without `class_by_key`, so every
  class-declared category answered `NOT_APPLICABLE` while the generator listed the instance —
  `--check-category` could never pass for the four v4 delivery categories. A self-test fails if
  the argument is dropped. The composer's self-test fixture was additionally red on main since
  standards v4 removed the blanket it relied on; the fixture now declares its category.
- Commands: `runbook/conv852-verify.sh` — `verify-project`, `check-tranche-exit`, `audit`; the
  exit check reads the receipts the verify pass writes (`--evidence-dir`; a fix made here). Their
  outputs are recorded at the live pass.
- **The v4 lifecycle rework, measured.** Standards v4 (main `232c0339d`, deployed as
  `e6c36e66`) closes a project only when its class's authored documents are published: for the
  three members that was 14 documents each (project-brief, evidence, inception-analysis,
  architecture-analysis, architecture-design, architecture-artifact, statement-of-architecture-work,
  project-wbs, project-registers, the four delivery categories and closure-review), plus one
  typed `architecture-artifact-instance` page per ADM artefact. The order that closes them — and
  the one rehearsed on a pristine restore, then run live — is: replay → acts (closes deferred) →
  the 7 authored analysis/design documents (`runbook/conv852-supplement.sh`) → the 12 delivery
  documents (`runbook/conv852-delivery.sh`, S04's `closure_outputs` written through
  `projects.programme_meta`) → `scripts/rework-project.py` for S04, T13 and T3 → the conformance
  baseline acceptance → the publish.
- **Live result.** S04 127 acts (reopened from its earlier close and re-closed), T13 183 acts,
  T3 104 acts; all three `projects.close` through the full gate. The publish ran strict-green,
  and the final verification at the deployed head `e6c36e66` (post-review snapshot) is
  `S04/T13/T3 green — 18 checks, 0 failures`; `check-tranche-exit --tranche 3` exits 0; the
  audit reports **zero open packages naming S04, T13 or T3 (any severity)**.
- Defects found and fixed on this lane while doing it: the staging-era ids in the tranche-3
  rework content (de-referenced; the content check now passes against the live snapshot); the
  runner's artefact repair loop reading a pre-run view and re-superseding documented rows
  (`412 the architecture artefact moved`); `projects.update` refusing the programme-owned
  `closure_outputs`; the review evidence citing programme-level risks the validator cannot
  resolve (now the members' project risks); the T13/T3 acceptance manifests never pinning
  `status = "closed"` (so a legitimate close read as seed drift); and the replay's `_version`
  contradiction above.
- **The evidence ratchet's first truthful base.** The publish's record reads `required_absent`
  222 → 301, so #928's ratchet refused the PR. Measured cause: the committed pack at
  `origin/main` carried **582 documents the live record does not hold** — staging-derived
  evidence (e.g. S05's "Project Brief — S05 (retrospective, 2026-11-26)" `01a219d8-b0ef-7561-a551-1402074e92a7`;
  the live S05 brief `01a1e007-50ba-71e6-84d1-fb09d88cfe6c` is an unpublished 2026-11-15 draft).
  The +79 is 84 overstated rows on that pack, 5 resolved by the tr3 rework, and 4 genuinely new
  GEMBA-WALK delivery rows. The owner decided (2026-11-28) to re-base the ratchet on the live
  reading: `docs/evidence/conformance-ratchet-baseline.json` pins the exact published record's
  sha256 and counts under the recorded decision `01a22276-4e34-7292-9ae9-2912e3056c60`, and
  `scripts/check-evidence-ratchet.py` accepts only a record that matches it — any later growth
  must be evidenced or re-based by a fresh decision. The 301 is #930's reduction target under the
  #928 rule.

## P4 — the review

- Decision: **`continue`** for tranche 3.
- Made under delegation `01a2021d-cca2-757c-bfc2-21bd4e4b6d85`, authorised by the owner
  2026-11-28.
- Review recorded 2026-11-28T12:46:24Z: the tr3 row `01a1e9e0-eb54-74d8-a2b5-44fb2b87eb52` is
  `closed` with a non-null `review_at`; tranche 4 `01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce` is
  `authorised` with `authorisation_review_id` `01a22231-474d-7ce9-bc57-8bb7c99c2dbb`.
- Gate: approval `01a22231-3f22-734a-b99f-f4f2f9fe1fc2`, decided by the distinct approver under
  the published delegation (`decided_by_kind=rule`).
- Published document: `01a22231-50dc-7622-b366-62d26d796dd6`, published at
  `2026-11-28T12:46:29.116452Z`, carrying Evidence, Benefit status, Risk and issue register and
  Decision. The pack was re-published after the act, so `docs/design/data/msp.json` shows tr3
  `closed` (with `review_at`) and tr4 `authorised` (with the review id).

## Deviations

- The evidence was derived at the **deployed** head `e6c36e66` (not the originally planned
  `232c0339d`): #928 merged while the unit was in flight and the deployment rolled. #928 changed
  no compiled surface, so the rehearsal (built from the deployed image) and the live engine are
  the same code path.
- The live publish's strict gate refused the v4 register until the conformance baseline was
  accepted with the owner's decision of 2026-11-28: 89 new P0/P1 entries plus 2 severity
  corrections across other projects — the v4 evidence debt named to #930 under the #928 ratchet,
  every entry `rework` with a named owner and expiry 2027-02-18. After the review authorised
  tranche 4, the ledger was re-accepted (165 entries): the review resolved 10 tranche-4
  `project_in_unauthorised_tranche` P0s, and the pack was published from the post-review state.
- The curations: the curated journal is 472 entries (the issue's "514" described main's journal
  plus the live-card entries before the 42 staging-lifecycle exclusions); the S04/T13/T3 rework
  content was de-referenced from staging ids before it was runnable.
- The curated journal and the input files are committed to this lane with the receipt; every id
  above is read back from the live record, none is invented.
